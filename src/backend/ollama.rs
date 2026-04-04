//! Ollama backend - HTTP API for local LLMs

use super::Backend;
use crate::config::BackendConfig;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

pub struct OllamaBackend {
    client: Client,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Option<ChatMessage>,
}

impl OllamaBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let base_url = config
            .command
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string());

        let timeout_secs = config.timeout.unwrap_or(300);
        let timeout_secs = if timeout_secs == 0 {
            365 * 24 * 60 * 60 // 1 year = effectively no timeout
        } else {
            timeout_secs
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()?;

        Ok(Self {
            client,
            base_url,
            model,
        })
    }

    async fn chat(
        &self,
        prompt: &str,
        model_override: Option<&str>,
    ) -> std::result::Result<String, super::BackendError> {
        let effective_model = model_override
            .filter(|m| !m.is_empty())
            .unwrap_or(&self.model);
        let request = ChatRequest {
            model: effective_model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            stream: false,
        };

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                if e.is_timeout() {
                    super::BackendError::Timeout {
                        message: format!("Ollama request timed out: {}", e),
                        elapsed_ms,
                    }
                } else if e.is_connect() {
                    super::BackendError::Network {
                        message: format!("Ollama connection failed: {}", e),
                    }
                } else {
                    super::BackendError::Network {
                        message: format!("Ollama request failed: {}", e),
                    }
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            let msg = format!("Ollama error {}: {}", status, error_text);
            return Err(match status.as_u16() {
                429 => super::BackendError::RateLimit {
                    message: msg,
                    retry_after_ms: None,
                },
                _ => super::BackendError::ExecutionFailed {
                    message: msg,
                    exit_code: None,
                },
            });
        }

        let chat_response: ChatResponse =
            response
                .json()
                .await
                .map_err(|e| super::BackendError::Parse {
                    message: format!("Failed to parse Ollama response: {}", e),
                })?;

        match chat_response.message {
            Some(msg) => Ok(msg.content),
            None => Ok(String::new()),
        }
    }
}

#[async_trait]
impl Backend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn query(
        &self,
        prompt: &str,
        _cwd: &Path,
        model: Option<&str>,
    ) -> std::result::Result<super::QueryOutput, super::BackendError> {
        let text = self.chat(prompt, model).await?;
        Ok(super::QueryOutput::from_text(text))
    }

    fn is_available(&self) -> bool {
        // Ollama is a server, not a CLI. Can't easily check synchronously.
        // Return true and let runtime connection fail if not running.
        true
    }
}
