use crate::config::BackendConfig;
use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use std::time::Instant;
use tempfile::NamedTempFile;
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

    /// Resolve effective sandbox mode, applying FR-22 defaulting:
    ///   apply_edits=true + sandbox=None => WorkspaceWrite.
    ///   apply_edits=true + explicit ReadOnly => preserve (with warning).
    fn resolve_effective_sandbox(
        sandbox: Option<super::SandboxMode>,
        apply_edits: bool,
    ) -> Option<super::SandboxMode> {
        match (apply_edits, sandbox) {
            (true, None) => Some(super::SandboxMode::WorkspaceWrite),
            (true, Some(super::SandboxMode::ReadOnly)) => {
                println!(
                    "[WARN] apply_edits=true but sandbox is read-only; edits will be parsed but the sandbox prevents writes"
                );
                Some(super::SandboxMode::ReadOnly)
            }
            (_, other) => other,
        }
    }

    /// Build the argv prefix for a Codex invocation, up to but not including the `--` separator
    /// and the prompt itself. Centralises the sandbox-injection logic so tests exercise the
    /// same code path that `query()` runs.
    fn build_argv_prefix(
        base_args: &[String],
        sandbox: Option<super::SandboxMode>,
        apply_edits: bool,
        model: Option<&str>,
        output_last_message_path: Option<&Path>,
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

        let effective = Self::resolve_effective_sandbox(sandbox, apply_edits);
        let mode = effective.unwrap_or(super::SandboxMode::ReadOnly);
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

        if let Some(path) = output_last_message_path {
            argv.push("-o".to_string());
            argv.push(path.to_string_lossy().into_owned());
        }

        argv
    }

    fn create_last_message_file() -> Result<NamedTempFile, super::BackendError> {
        tempfile::Builder::new()
            .prefix("lok-codex-last-")
            .rand_bytes(12)
            .tempfile()
            .map_err(|error| super::BackendError::Unavailable {
                message: format!("failed to allocate codex output-last-message file: {error}"),
            })
    }

    async fn read_last_message(path: &Path) -> Option<String> {
        let content = tokio::fs::read_to_string(path).await.ok()?;
        let message = content.trim_end_matches(&['\n', '\r'][..]);

        if message.trim().is_empty() {
            return None;
        }

        Some(message.to_string())
    }

    fn with_exit_code(error: super::BackendError, exit_code: i32) -> super::BackendError {
        match error {
            super::BackendError::ExecutionFailed { message, .. } => {
                super::BackendError::ExecutionFailed {
                    message,
                    exit_code: Some(exit_code),
                }
            }
            other => other,
        }
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

        let last_message_file = Self::create_last_message_file()?;

        let mut cmd = Command::new(&self.command);
        let argv = Self::build_argv_prefix(
            &self.args,
            ctx.sandbox,
            ctx.apply_edits,
            effective_model.as_deref(),
            Some(last_message_file.path()),
        );
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
        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = super::codex_event::parse_jsonl_diagnostics(&stdout);

        if let Some(err) = diagnostics.terminal_error {
            return Err(Self::with_exit_code(err, exit_code));
        }

        if !output.status.success() {
            if diagnostics.parse_error.is_some() && !stderr_str.trim().is_empty() {
                // Non-zero exit with readable stderr should surface CLI/system failure even when
                // JSONL parsing fails (for example, older codex versions rejecting -o).
                let msg = format!("Codex failed: {}", stderr_str);
                let err = super::BackendError::from(anyhow::anyhow!("{}", msg));
                return Err(Self::with_exit_code(err, exit_code));
            }

            if let Some(parse_err) = diagnostics.parse_error {
                return Err(parse_err);
            }

            // JSONL parsed successfully but process still exited non-zero — fall back to stderr
            let msg = format!("Codex failed: {}", stderr_str);
            let err = super::BackendError::from(anyhow::anyhow!("{}", msg));
            return Err(Self::with_exit_code(err, exit_code));
        }

        let text = Self::read_last_message(last_message_file.path())
            .await
            .or(diagnostics.agent_message)
            .ok_or_else(|| {
                diagnostics
                    .parse_error
                    .unwrap_or_else(|| super::BackendError::Parse {
                        message:
                            "Codex completed without output-last-message or JSONL agent_message"
                                .into(),
                    })
            })?;

        Ok(
            super::QueryOutput::from_process(text, stderr_str, exit_code, "codex", start.elapsed())
                .with_model(effective_model)
                .with_usage(diagnostics.usage),
        )
    }

    fn is_available(&self) -> bool {
        which::which(&self.command).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::CodexBackend;
    use crate::backend::SandboxMode;
    use std::io::{self, Write};
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn codex_sandbox_default_none_uses_read_only() {
        let argv = CodexBackend::build_argv_prefix(&[], None, false, None, None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "read-only");
    }

    #[test]
    fn codex_sandbox_workspace_write() {
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::WorkspaceWrite),
            false,
            None,
            None,
        );
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "workspace-write");
    }

    #[test]
    fn codex_sandbox_danger_full_access() {
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::DangerFullAccess),
            false,
            None,
            None,
        );
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "danger-full-access");
    }

    #[test]
    fn codex_defaults_include_ephemeral() {
        let argv = CodexBackend::build_argv_prefix(&[], None, false, None, None);
        assert!(argv.contains(&"--ephemeral".to_string()));
    }

    #[test]
    fn codex_custom_args_preserved_before_sandbox() {
        let custom = vec!["exec".to_string(), "--json".to_string()];
        let argv = CodexBackend::build_argv_prefix(
            &custom,
            Some(SandboxMode::WorkspaceWrite),
            false,
            None,
            None,
        );
        assert_eq!(argv[0], "exec");
        assert_eq!(argv[1], "--json");
        let s_idx = argv.iter().position(|a| a == "-s").unwrap();
        assert!(s_idx >= 2);
        assert_eq!(argv[s_idx + 1], "workspace-write");
    }

    #[test]
    fn codex_no_ephemeral_with_custom_args() {
        let custom = vec!["exec".to_string(), "--json".to_string()];
        let argv = CodexBackend::build_argv_prefix(&custom, None, false, None, None);
        assert!(!argv.contains(&"--ephemeral".to_string()));
    }

    #[test]
    fn codex_exactly_one_sandbox_flag_with_defaults() {
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::WorkspaceWrite),
            false,
            None,
            None,
        );
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
        let argv = CodexBackend::build_argv_prefix(
            &backend.args,
            Some(SandboxMode::WorkspaceWrite),
            false,
            None,
            None,
        );
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
        let argv = CodexBackend::build_argv_prefix(&[], None, false, Some("gpt-5"), None);
        let idx = argv.iter().position(|a| a == "--model").unwrap();
        assert_eq!(argv[idx + 1], "gpt-5");
    }

    #[test]
    fn codex_argv_includes_output_last_message_when_path_given() {
        let path = PathBuf::from("/tmp/example-last-message.txt");
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::ReadOnly),
            false,
            None,
            Some(&path),
        );
        let o_idx = argv
            .iter()
            .position(|a| a == "-o")
            .expect("-o should be present");
        assert_eq!(argv[o_idx + 1], path.to_string_lossy());
        assert_eq!(argv.iter().filter(|a| *a == "-o").count(), 1);
    }

    #[test]
    fn codex_argv_omits_output_last_message_when_path_none() {
        let argv =
            CodexBackend::build_argv_prefix(&[], Some(SandboxMode::ReadOnly), false, None, None);
        assert!(
            !argv.contains(&"-o".to_string()),
            "-o should be omitted when path is None"
        );
    }

    #[test]
    fn codex_argv_orders_output_last_message_after_sandbox_and_model() {
        let path = PathBuf::from("/tmp/example-last-message.txt");
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::WorkspaceWrite),
            false,
            Some("gpt-5"),
            Some(&path),
        );

        let s_idx = argv
            .iter()
            .position(|a| a == "-s")
            .expect("sandbox present");
        assert_eq!(argv[s_idx + 2], "--model");
        assert_eq!(argv[s_idx + 3], "gpt-5");
        let o_idx = argv.iter().position(|a| a == "-o").expect("-o present");
        assert_eq!(argv[o_idx - 1], "gpt-5");
        assert!(o_idx > s_idx + 3);
    }

    #[tokio::test]
    async fn read_last_message_returns_none_for_missing_file() {
        let missing = {
            let file = NamedTempFile::new().expect("temp file available");
            let path = file.path().to_path_buf();
            drop(file);
            path
        };
        let result = CodexBackend::read_last_message(&missing).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn read_last_message_returns_none_for_empty_or_whitespace_file() {
        let mut file = NamedTempFile::new().expect("temp file available");
        writeln!(file, "   \n\t ").expect("writes whitespace");
        let path = file.path();

        let result = CodexBackend::read_last_message(path).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn read_last_message_preserves_leading_whitespace_and_trims_only_trailing_newlines() {
        let mut file = NamedTempFile::new().expect("temp file available");
        write!(file, "  leading indentation\n\n").expect("writes message");
        file.flush().expect("flush message");

        let result = CodexBackend::read_last_message(file.path())
            .await
            .expect("non-empty message expected");

        assert_eq!(result, "  leading indentation");
    }

    #[test]
    fn named_tempfile_cleanup_removes_last_message_path() {
        let path = {
            let file = CodexBackend::create_last_message_file()
                .expect("create temporary last-message file");
            let path = file.path().to_path_buf();
            assert!(path.exists());
            path
        };
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn codex_last_message_file_mode_is_private() -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let file =
            CodexBackend::create_last_message_file().expect("create temporary last-message file");
        let path = file.path().to_path_buf();

        let metadata = std::fs::metadata(&path)?;
        let mode = metadata.permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "tempfile permissions are {mode:o}");

        drop(file);
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn codex_apply_edits_true_no_sandbox_defaults_workspace_write() {
        let argv = CodexBackend::build_argv_prefix(&[], None, true, None, None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "workspace-write");
    }

    #[test]
    fn codex_apply_edits_true_explicit_workspace_write_preserved() {
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::WorkspaceWrite),
            true,
            None,
            None,
        );
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "workspace-write");
    }

    #[test]
    fn codex_apply_edits_true_explicit_danger_preserved() {
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::DangerFullAccess),
            true,
            None,
            None,
        );
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "danger-full-access");
    }

    #[test]
    fn codex_apply_edits_true_explicit_read_only_preserved() {
        let argv =
            CodexBackend::build_argv_prefix(&[], Some(SandboxMode::ReadOnly), true, None, None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "read-only");
    }

    #[test]
    fn codex_apply_edits_false_no_sandbox_keeps_read_only_default() {
        let argv = CodexBackend::build_argv_prefix(&[], None, false, None, None);
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "read-only");
    }

    #[test]
    fn codex_apply_edits_false_explicit_workspace_write_preserved() {
        let argv = CodexBackend::build_argv_prefix(
            &[],
            Some(SandboxMode::WorkspaceWrite),
            false,
            None,
            None,
        );
        let idx = argv.iter().position(|a| a == "-s").unwrap();
        assert_eq!(argv[idx + 1], "workspace-write");
    }

    #[test]
    fn codex_exactly_one_sandbox_flag_with_apply_edits_default() {
        let argv = CodexBackend::build_argv_prefix(&[], None, true, None, None);
        let count = argv.iter().filter(|a| *a == "-s").count();
        assert_eq!(
            count, 1,
            "expected exactly one -s flag with apply_edits default; got argv {:?}",
            argv
        );
    }
}
