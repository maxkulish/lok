use crate::config::BackendConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::env;
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

use super::{BackendError, TokenUsage};

/// Claude backend mode - API or CLI
#[derive(Clone)]
pub enum ClaudeMode {
    /// Use Claude API directly (requires ANTHROPIC_API_KEY)
    Api {
        api_key: SecretString,
        model: String,
        client: reqwest::Client,
    },
    /// Use Claude CLI (`claude -p`)
    Cli {
        command: String,
        model: Option<String>,
    },
}

pub struct ClaudeBackend {
    mode: ClaudeMode,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<ClaudeUsage>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl ClaudeBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        // Check if we have a command configured (CLI mode) or API key (API mode)
        if let Some(ref cmd) = config.command {
            // CLI mode - use claude command
            Ok(Self {
                mode: ClaudeMode::Cli {
                    command: cmd.clone(),
                    model: config.model.clone(),
                },
            })
        } else {
            // API mode - requires API key
            let api_key_env = config
                .api_key_env
                .clone()
                .unwrap_or_else(|| "ANTHROPIC_API_KEY".to_string());

            let api_key: SecretString = env::var(&api_key_env)
                .with_context(|| format!("Missing environment variable: {}", api_key_env))?
                .into();

            let model = config
                .model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

            let client = reqwest::Client::new();

            Ok(Self {
                mode: ClaudeMode::Api {
                    api_key,
                    model,
                    client,
                },
            })
        }
    }

    /// Get API mode details (for conductor)
    pub fn api_details(&self) -> Option<(&SecretString, &str, &reqwest::Client)> {
        match &self.mode {
            ClaudeMode::Api {
                api_key,
                model,
                client,
            } => Some((api_key, model, client)),
            ClaudeMode::Cli { .. } => None,
        }
    }

    async fn query_api(
        &self,
        system: &str,
        prompt: &str,
        model_override: Option<&str>,
    ) -> std::result::Result<super::QueryOutput, BackendError> {
        let start = Instant::now();

        let (api_key, default_model, client) = match &self.mode {
            ClaudeMode::Api {
                api_key,
                model,
                client,
            } => (api_key, model, client),
            ClaudeMode::Cli { .. } => {
                return Err(BackendError::Config {
                    message: "API mode required for this operation".to_string(),
                })
            }
        };

        let effective_model = model_override
            .filter(|m| !m.is_empty())
            .unwrap_or(default_model);

        let request = serde_json::json!({
            "model": effective_model,
            "max_tokens": 4096,
            "system": system,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        });

        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key.expose_secret())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    BackendError::Timeout {
                        message: format!("Claude API request timed out: {}", e),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                    }
                } else if e.is_connect() {
                    BackendError::Network {
                        message: format!("Claude API connection failed: {}", e),
                    }
                } else {
                    BackendError::Network {
                        message: format!("Failed to send request to Claude API: {}", e),
                    }
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(classify_http_error(status, &body));
        }

        let parsed: ClaudeResponse = response.json().await.map_err(|e| BackendError::Parse {
            message: format!("Failed to parse Claude response: {}", e),
        })?;

        let text = parsed
            .content
            .into_iter()
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n");

        let usage = parsed
            .usage
            .map(|u| TokenUsage::new(u.input_tokens, u.output_tokens));
        // Fall back to requested effective model if the API omits it (unlikely, but keeps
        // the output's model field populated with something meaningful).
        let model = parsed.model.or_else(|| Some(effective_model.to_string()));

        Ok(
            super::QueryOutput::from_text(text, "claude", start.elapsed())
                .with_model(model)
                .with_usage(usage),
        )
    }

    async fn query_cli(
        &self,
        prompt: &str,
        cwd: &Path,
        model_override: Option<&str>,
    ) -> Result<super::QueryOutput> {
        let start = Instant::now();

        let (command, default_model) = match &self.mode {
            ClaudeMode::Cli { command, model } => (command, model),
            ClaudeMode::Api { .. } => anyhow::bail!("CLI mode required for this operation"),
        };

        let effective_model = model_override
            .filter(|m| !m.is_empty())
            .map(String::from)
            .or_else(|| default_model.clone());

        let mut cmd = Command::new(command);
        cmd.arg("-p") // print mode
            .arg("--output-format")
            .arg("text");

        if let Some(m) = effective_model.as_ref() {
            cmd.arg("--model").arg(m);
        }

        cmd.arg("--") // Prevent prompt from being interpreted as flags
            .arg(prompt)
            .current_dir(cwd)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .context("Failed to execute claude command")?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!("Claude CLI failed: {}", stderr_str);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(super::QueryOutput::from_process(
            stdout.trim().to_string(),
            stderr_str,
            exit_code,
            "claude",
            start.elapsed(),
        )
        .with_model(effective_model))
    }
}

fn classify_http_error(status: reqwest::StatusCode, body: &str) -> BackendError {
    let msg = format!("Claude API error {}: {}", status, body);
    match status.as_u16() {
        401 | 403 => BackendError::Auth { message: msg },
        429 => BackendError::RateLimit {
            message: msg,
            retry_after_ms: None,
        },
        529 => BackendError::RateLimit {
            message: msg,
            retry_after_ms: None,
        },
        500..=599 => BackendError::ExecutionFailed {
            message: msg,
            exit_code: None,
        },
        _ => BackendError::ExecutionFailed {
            message: msg,
            exit_code: None,
        },
    }
}

#[async_trait]
impl super::Backend for ClaudeBackend {
    fn name(&self) -> &str {
        "claude"
    }

    async fn query(
        &self,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
    ) -> std::result::Result<super::QueryOutput, BackendError> {
        match &self.mode {
            ClaudeMode::Api { .. } => {
                self.query_api("You are a helpful assistant.", prompt, model)
                    .await
            }
            ClaudeMode::Cli { .. } => self
                .query_cli(prompt, cwd, model)
                .await
                .map_err(BackendError::from),
        }
    }

    fn is_available(&self) -> bool {
        match &self.mode {
            ClaudeMode::Api { api_key, .. } => !api_key.expose_secret().is_empty(),
            ClaudeMode::Cli { command, .. } => which::which(command).is_ok(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_response_deserialize_with_usage() {
        let json = r#"{
            "content": [{"type": "text", "text": "hello"}],
            "model": "claude-sonnet-4-20250514",
            "usage": {"input_tokens": 12, "output_tokens": 34}
        }"#;
        let parsed: ClaudeResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(parsed.content.len(), 1);
        assert_eq!(parsed.content[0].text.as_deref(), Some("hello"));
        assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-20250514"));
        let usage = parsed.usage.expect("usage should be present");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 34);
    }

    #[test]
    fn test_claude_response_deserialize_without_usage() {
        let json = r#"{
            "content": [{"type": "text", "text": "hello"}]
        }"#;
        let parsed: ClaudeResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(parsed.content.len(), 1);
        assert!(parsed.model.is_none());
        assert!(parsed.usage.is_none());
    }
}
