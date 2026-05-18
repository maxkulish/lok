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

        // Store custom args as-is; sandbox is injected dynamically at query time.
        // Default args no longer include -s (it's set per-query from StepContext).
        let args = if config.args.is_empty() {
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "--ephemeral".to_string(),
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

        // Base args: use config args if set, otherwise recommended defaults
        if self.args.is_empty() {
            cmd.args(["exec", "--json", "--ephemeral"]);
        } else {
            cmd.args(&self.args);
        }

        // Sandbox flag: per-step ctx.sandbox overrides, fallback to read-only
        let sandbox = ctx.sandbox.unwrap_or(super::SandboxMode::ReadOnly);
        match sandbox {
            super::SandboxMode::ReadOnly => cmd.args(["-s", "read-only"]),
            super::SandboxMode::WorkspaceWrite => cmd.args(["-s", "workspace-write"]),
            super::SandboxMode::DangerFullAccess => cmd.args(["-s", "danger-full-access"]),
        };

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

#[cfg(test)]
mod tests {
    use crate::backend::SandboxMode;

    /// Helper: builds the effective Codex argv for a given sandbox setting.
    /// Mirrors the query-time logic without spawning a process.
    fn codex_sandbox_argv(base_args: Vec<String>, sandbox: Option<SandboxMode>) -> Vec<String> {
        let mut args: Vec<String> = if base_args.is_empty() {
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "--ephemeral".to_string(),
            ]
        } else {
            base_args
        };

        let mode = sandbox.unwrap_or(SandboxMode::ReadOnly);
        match mode {
            SandboxMode::ReadOnly => {
                args.push("-s".to_string());
                args.push("read-only".to_string());
            }
            SandboxMode::WorkspaceWrite => {
                args.push("-s".to_string());
                args.push("workspace-write".to_string());
            }
            SandboxMode::DangerFullAccess => {
                args.push("-s".to_string());
                args.push("danger-full-access".to_string());
            }
        }
        args
    }

    #[test]
    fn codex_sandbox_default_none_uses_read_only() {
        let argv = codex_sandbox_argv(vec![], None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "read-only");
    }

    #[test]
    fn codex_sandbox_workspace_write() {
        let argv = codex_sandbox_argv(vec![], Some(SandboxMode::WorkspaceWrite));
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "workspace-write");
    }

    #[test]
    fn codex_sandbox_danger_full_access() {
        let argv = codex_sandbox_argv(vec![], Some(SandboxMode::DangerFullAccess));
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "danger-full-access");
    }

    #[test]
    fn codex_defaults_include_ephemeral() {
        let argv = codex_sandbox_argv(vec![], None);
        assert!(argv.contains(&"--ephemeral".to_string()));
    }

    #[test]
    fn codex_custom_args_preserved_before_sandbox() {
        let custom = vec!["exec".to_string(), "--json".to_string()];
        let argv = codex_sandbox_argv(custom.clone(), Some(SandboxMode::WorkspaceWrite));
        // Custom args come first
        assert_eq!(argv[0], "exec");
        assert_eq!(argv[1], "--json");
        // Then sandbox
        let s_idx = argv.iter().position(|a| a == "-s").unwrap();
        assert!(s_idx >= 2);
        assert_eq!(argv[s_idx + 1], "workspace-write");
    }

    #[test]
    fn codex_no_ephemeral_with_custom_args() {
        let custom = vec!["exec".to_string(), "--json".to_string()];
        let argv = codex_sandbox_argv(custom, None);
        // --ephemeral should NOT be injected when custom args are used
        assert!(!argv.contains(&"--ephemeral".to_string()));
    }
}
