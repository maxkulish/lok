use crate::config::BackendConfig;
use anyhow::Result;
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

pub struct GeminiBackend {
    command: String,
    args: Vec<String>,
    skip_lines: usize,
    default_model: Option<String>,
}

impl GeminiBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let command = config.command.clone().unwrap_or_else(|| "npx".to_string());

        let args = if config.args.is_empty() {
            vec!["@google/gemini-cli".to_string()]
        } else {
            config.args.clone()
        };

        Ok(Self {
            command,
            args,
            skip_lines: config.skip_lines,
            default_model: config.model.clone(),
        })
    }

    fn parse_output(&self, output: &str) -> String {
        output
            .lines()
            .skip(self.skip_lines)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Build the shell command string that `query()` executes. Centralises sandbox/approval-mode
    /// mapping so tests exercise the same code path as production.
    fn build_shell_cmd(
        command: &str,
        args: &[String],
        model: Option<&str>,
        sandbox: Option<super::SandboxMode>,
        prompt: &str,
    ) -> String {
        // Shell-escape each component: wrap in single quotes, escape internal single quotes
        let escape = |s: &str| s.replace("'", "'\\''");
        let escaped_command = format!("'{}'", escape(command));
        let escaped_args = args
            .iter()
            .map(|a| format!("'{}'", escape(a)))
            .collect::<Vec<_>>()
            .join(" ");
        let escaped_prompt = escape(prompt);
        let model_flag = model
            .map(|m| format!(" --model '{}'", escape(m)))
            .unwrap_or_default();
        let approval_flag = match sandbox {
            Some(super::SandboxMode::ReadOnly) => " --approval-mode plan",
            Some(super::SandboxMode::WorkspaceWrite) => " --approval-mode auto_edit",
            Some(super::SandboxMode::DangerFullAccess) => " --approval-mode yolo",
            None => "",
        };
        format!(
            "echo '' | {} {} {}{} '{}'",
            escaped_command, escaped_args, model_flag, approval_flag, escaped_prompt
        )
    }
}

#[async_trait]
impl super::Backend for GeminiBackend {
    fn name(&self) -> &str {
        "gemini"
    }

    async fn query(
        &self,
        ctx: super::StepContext<'_>,
    ) -> std::result::Result<super::QueryOutput, super::BackendError> {
        let start = Instant::now();

        let prompt = ctx.prompt;
        let cwd = ctx.cwd;
        let model = ctx.model;

        // Gemini CLI requires stdin to be a pipe (not null/tty), so we wrap in `sh -c`
        // and pipe empty stdin: echo '' | npx @google/gemini-cli 'prompt'
        let effective_model: Option<String> = model
            .filter(|m| !m.is_empty())
            .map(String::from)
            .or_else(|| self.default_model.clone());

        let shell_cmd = Self::build_shell_cmd(
            &self.command,
            &self.args,
            effective_model.as_deref(),
            ctx.sandbox,
            prompt,
        );

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&shell_cmd)
            .current_dir(cwd)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| super::BackendError::Unavailable {
                message: format!("Failed to execute gemini command: {}", e),
            })?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let msg = format!("Gemini failed: {}", stderr_str);
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

        let parsed_stdout = self.parse_output(&stdout);
        Ok(super::QueryOutput::from_process(
            parsed_stdout,
            stderr_str,
            exit_code,
            "gemini",
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
    use super::super::SandboxMode;
    use super::GeminiBackend;

    fn args() -> Vec<String> {
        vec!["@google/gemini-cli".to_string()]
    }

    #[test]
    fn gemini_sandbox_none_no_approval_flag() {
        let cmd = GeminiBackend::build_shell_cmd("npx", &args(), None, None, "hello");
        assert!(!cmd.contains("--approval-mode"));
    }

    #[test]
    fn gemini_sandbox_read_only_adds_plan() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::ReadOnly),
            "hello",
        );
        assert!(cmd.contains("--approval-mode plan"));
    }

    #[test]
    fn gemini_sandbox_workspace_write_adds_auto_edit() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::WorkspaceWrite),
            "hello",
        );
        assert!(cmd.contains("--approval-mode auto_edit"));
    }

    #[test]
    fn gemini_sandbox_danger_adds_yolo() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::DangerFullAccess),
            "hello",
        );
        assert!(cmd.contains("--approval-mode yolo"));
    }

    #[test]
    fn gemini_sandbox_prompt_is_escaped() {
        let cmd = GeminiBackend::build_shell_cmd("npx", &args(), None, None, "it's fine");
        assert!(
            cmd.contains("'\\''"),
            "expected shell-escaped single quote in: {}",
            cmd
        );
    }

    #[test]
    fn gemini_model_flag_appended_and_quoted() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            Some("gemini-2.5-pro"),
            Some(SandboxMode::ReadOnly),
            "hello",
        );
        assert!(cmd.contains("--model 'gemini-2.5-pro'"));
        assert!(cmd.contains("--approval-mode plan"));
    }
}
