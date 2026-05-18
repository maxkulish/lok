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

    /// Build the argv prefix for a Codex invocation, up to but not including the `--` separator
    /// and the prompt itself. Centralises the sandbox-injection logic so tests exercise the
    /// same code path that `query()` runs.
    fn build_argv_prefix(
        base_args: &[String],
        sandbox: Option<super::SandboxMode>,
        model: Option<&str>,
    ) -> Vec<String> {
        let mut argv: Vec<String> = if base_args.is_empty() {
            vec![
                "exec".to_string(),
                "--json".to_string(),
                "--ephemeral".to_string(),
            ]
        } else {
            base_args.to_vec()
        };

        let mode = sandbox.unwrap_or(super::SandboxMode::ReadOnly);
        let mode_str = match mode {
            super::SandboxMode::ReadOnly => "read-only",
            super::SandboxMode::WorkspaceWrite => "workspace-write",
            super::SandboxMode::DangerFullAccess => "danger-full-access",
        };
        argv.push("-s".to_string());
        argv.push(mode_str.to_string());

        if let Some(m) = model {
            argv.push("--model".to_string());
            argv.push(m.to_string());
        }
        argv
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
        let argv = Self::build_argv_prefix(&self.args, ctx.sandbox, effective_model.as_deref());
        cmd.args(&argv);

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
    use super::CodexBackend;
    use crate::backend::SandboxMode;

    #[test]
    fn codex_sandbox_default_none_uses_read_only() {
        let argv = CodexBackend::build_argv_prefix(&[], None, None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "read-only");
    }

    #[test]
    fn codex_sandbox_workspace_write() {
        let argv = CodexBackend::build_argv_prefix(&[], Some(SandboxMode::WorkspaceWrite), None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "workspace-write");
    }

    #[test]
    fn codex_sandbox_danger_full_access() {
        let argv = CodexBackend::build_argv_prefix(&[], Some(SandboxMode::DangerFullAccess), None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "danger-full-access");
    }

    #[test]
    fn codex_defaults_include_ephemeral() {
        let argv = CodexBackend::build_argv_prefix(&[], None, None);
        assert!(argv.contains(&"--ephemeral".to_string()));
    }

    #[test]
    fn codex_custom_args_preserved_before_sandbox() {
        let custom = vec!["exec".to_string(), "--json".to_string()];
        let argv =
            CodexBackend::build_argv_prefix(&custom, Some(SandboxMode::WorkspaceWrite), None);
        assert_eq!(argv[0], "exec");
        assert_eq!(argv[1], "--json");
        let s_idx = argv.iter().position(|a| a == "-s").unwrap();
        assert!(s_idx >= 2);
        assert_eq!(argv[s_idx + 1], "workspace-write");
    }

    #[test]
    fn codex_no_ephemeral_with_custom_args() {
        let custom = vec!["exec".to_string(), "--json".to_string()];
        let argv = CodexBackend::build_argv_prefix(&custom, None, None);
        assert!(!argv.contains(&"--ephemeral".to_string()));
    }

    #[test]
    fn codex_exactly_one_sandbox_flag_with_defaults() {
        let argv = CodexBackend::build_argv_prefix(&[], Some(SandboxMode::WorkspaceWrite), None);
        let count = argv.iter().filter(|a| *a == "-s").count();
        assert_eq!(
            count, 1,
            "expected exactly one -s flag; got argv {:?}",
            argv
        );
    }

    #[test]
    fn codex_default_config_yields_ephemeral_and_one_sandbox_flag() {
        // Regression: Config::default() previously hardcoded `-s read-only` in args,
        // which (a) dropped `--ephemeral` and (b) emitted a duplicate `-s` at query time.
        let cfg = crate::config::Config::default();
        let codex_cfg = cfg.backends.get("codex").expect("default codex backend");
        let backend = CodexBackend::new(codex_cfg).expect("backend constructs");
        let argv =
            CodexBackend::build_argv_prefix(&backend.args, Some(SandboxMode::WorkspaceWrite), None);
        assert!(
            argv.contains(&"--ephemeral".to_string()),
            "default config must yield --ephemeral; got {:?}",
            argv
        );
        let count = argv.iter().filter(|a| *a == "-s").count();
        assert_eq!(
            count, 1,
            "default config must yield exactly one -s flag; got {:?}",
            argv
        );
    }

    #[test]
    fn codex_model_flag_appended() {
        let argv = CodexBackend::build_argv_prefix(&[], None, Some("gpt-5"));
        let idx = argv.iter().position(|a| a == "--model").unwrap();
        assert_eq!(argv[idx + 1], "gpt-5");
    }
}
