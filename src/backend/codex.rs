use crate::config::BackendConfig;
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

pub struct CodexBackend {
    command: String,
    args: Vec<String>,
    default_model: Option<String>,
}

impl CodexBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let command = config
            .command
            .clone()
            .unwrap_or_else(|| "codex".to_string());

        let args = if config.args.is_empty() {
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
            ]
        } else {
            config.args.clone()
        };

        Ok(Self {
            command,
            args,
            default_model: config.model.clone(),
        })
    }

    fn parse_output(&self, output: &str) -> String {
        // Parse JSON output from codex
        // Look for agent_message in item.completed events
        for line in output.lines() {
            if line.contains("\"type\":\"item.completed\"") && line.contains("agent_message") {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(text) = json
                        .get("item")
                        .and_then(|i| i.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        return text.to_string();
                    }
                }
            }
        }

        // Fallback: return raw output
        output.to_string()
    }
}

#[async_trait]
impl super::Backend for CodexBackend {
    fn name(&self) -> &str {
        "codex"
    }

    async fn query(
        &self,
        ctx: super::StepContext<'_>,
    ) -> std::result::Result<super::QueryOutput, super::BackendError> {
        let prompt = ctx.prompt;
        let cwd = ctx.cwd;
        let model = ctx.model;
        let start = Instant::now();

        let effective_model: Option<String> = model
            .filter(|m| !m.is_empty())
            .map(String::from)
            .or_else(|| self.default_model.clone());

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args);

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
            .map_err(|e| super::BackendError::Unavailable {
                message: format!("Failed to execute codex command: {}", e),
            })?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let msg = format!("Codex failed: {}", stderr_str);
            let err = super::BackendError::from(anyhow::anyhow!("{}", msg));
            // Preserve exit_code that From<anyhow::Error> would discard
            let err = if let super::BackendError::ExecutionFailed { message, .. } = err {
                super::BackendError::ExecutionFailed {
                    message,
                    exit_code: Some(exit_code),
                }
            } else {
                err
            };
            return Err(err);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed_stdout = self.parse_output(&stdout);
        Ok(super::QueryOutput::from_process(
            parsed_stdout,
            stderr_str,
            exit_code,
            "codex",
            start.elapsed(),
        )
        .with_model(effective_model))
    }

    fn is_available(&self) -> bool {
        which::which(&self.command).is_ok()
    }
}
