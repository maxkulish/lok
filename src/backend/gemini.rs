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

/// Private representation of the Gemini CLI JSON envelope emitted when
/// `--output-format json` is passed.
#[derive(serde::Deserialize)]
pub(crate) struct GeminiEnvelope {
    response: String,
    #[serde(default)]
    stats: Option<serde_json::Value>,
}

impl GeminiBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let command = config.command.clone().unwrap_or_else(|| "npx".to_string());

        let args = if config.args.is_empty() {
            vec![
                "@google/gemini-cli".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
            ]
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

    pub(crate) fn parse_gemini_envelope(stdout: &str) -> Option<GeminiEnvelope> {
        serde_json::from_str(stdout).ok()
    }

    pub(crate) fn envelope_to_usage(stats: Option<serde_json::Value>) -> Option<super::TokenUsage> {
        let stats = stats?;
        Self::extract_usage_from_stats(&stats)
    }

    fn extract_usage_from_stats(stats: &serde_json::Value) -> Option<super::TokenUsage> {
        // Real Gemini CLI (≥0.42) shape: stats.models.<model>.tokens { prompt, candidates, cached, thoughts }
        if let Some(models) = stats.get("models").and_then(|m| m.as_object()) {
            let mut prompt: u32 = 0;
            let mut candidates: u32 = 0;
            let mut cached: u32 = 0;
            let mut thoughts: u32 = 0;
            let mut prompt_seen = false;
            let mut candidates_seen = false;

            for (_, model) in models {
                if let Some(tokens) = model.get("tokens").and_then(|t| t.as_object()) {
                    if let Some(v) = tokens.get("prompt").and_then(|v| v.as_u64()) {
                        prompt = prompt.saturating_add(v as u32);
                        prompt_seen = true;
                    }
                    if let Some(v) = tokens.get("candidates").and_then(|v| v.as_u64()) {
                        candidates = candidates.saturating_add(v as u32);
                        candidates_seen = true;
                    }
                    if let Some(v) = tokens.get("cached").and_then(|v| v.as_u64()) {
                        cached = cached.saturating_add(v as u32);
                    }
                    if let Some(v) = tokens.get("thoughts").and_then(|v| v.as_u64()) {
                        thoughts = thoughts.saturating_add(v as u32);
                    }
                }
            }

            if prompt_seen && candidates_seen {
                return Some(
                    super::TokenUsage::new(prompt, candidates)
                        .with_cached(if cached > 0 { Some(cached) } else { None })
                        .with_reasoning(if thoughts > 0 { Some(thoughts) } else { None }),
                );
            }
        }

        // Fallback: PRD-assumed flat shape (stats.promptTokenCount / candidatesTokenCount).
        // Kept for forward-compatibility in case a future CLI version flattens the schema.
        if let Some(prompt) = stats
            .get("promptTokenCount")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
        {
            if let Some(candidates) = stats
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32)
            {
                let cached = stats
                    .get("cachedContentTokenCount")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                return Some(super::TokenUsage::new(prompt, candidates).with_cached(cached));
            }
        }

        None
    }

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

    /// Build the shell command string that `query()` executes. Centralises sandbox/approval-mode
    /// mapping so tests exercise the same code path as production.
    fn build_shell_cmd(
        command: &str,
        args: &[String],
        model: Option<&str>,
        sandbox: Option<super::SandboxMode>,
        apply_edits: bool,
        prompt: &str,
    ) -> String {
        let effective = Self::resolve_effective_sandbox(sandbox, apply_edits);
        // Shell-escape each component: wrap in single quotes, escape internal single quotes
        let escape = |s: &str| s.replace("'", "'\\\''");
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
        let approval_flag = match effective {
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
            ctx.apply_edits,
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

        let (response_text, usage) = match Self::parse_gemini_envelope(&stdout) {
            Some(env) => (env.response, Self::envelope_to_usage(env.stats)),
            None => (self.parse_output(&stdout), None),
        };
        Ok(super::QueryOutput::from_process(
            response_text,
            stderr_str,
            exit_code,
            "gemini",
            start.elapsed(),
        )
        .with_model(effective_model)
        .with_usage(usage))
    }

    fn is_available(&self) -> bool {
        super::Engine::is_backend_available(self.name())
    }

    async fn health_check(&self) -> std::result::Result<super::HealthStatus, super::BackendError> {
        if which::which(&self.command).is_ok() {
            Ok(super::HealthStatus::new_available())
        } else {
            Err(super::BackendError::Unavailable {
                message: format!("Gemini CLI command '{}' not found on PATH", self.command),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SandboxMode;
    use super::GeminiBackend;

    fn args() -> Vec<String> {
        vec!["@google/gemini-cli".to_string()]
    }

    fn default_args_with_flag() -> Vec<String> {
        vec![
            "@google/gemini-cli".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
        ]
    }

    #[test]
    fn gemini_default_config_includes_output_format_json() {
        let cfg = crate::config::Config::default();
        let gemini_cfg = cfg
            .backends
            .get("gemini")
            .expect("default gemini backend exists");
        let backend = GeminiBackend::new(gemini_cfg).expect("backend constructs");
        assert!(
            backend.args.contains(&"--output-format".to_string()),
            "default args must include --output-format; got {:?}",
            backend.args
        );
        assert!(
            backend.args.contains(&"json".to_string()),
            "default args must include json; got {:?}",
            backend.args
        );
    }

    #[test]
    fn gemini_custom_args_without_flag_preserved() {
        let mut cfg = crate::config::Config::default();
        let gemini_cfg = cfg
            .backends
            .get_mut("gemini")
            .expect("default gemini backend exists");
        gemini_cfg.args = vec!["@google/gemini-cli".to_string(), "--skip-trust".to_string()];
        let backend = GeminiBackend::new(gemini_cfg).expect("backend constructs");
        assert!(
            !backend.args.contains(&"--output-format".to_string()),
            "custom args should not auto-inject flag; got {:?}",
            backend.args
        );
    }

    #[test]
    fn gemini_build_shell_cmd_preserves_output_format_flag() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &default_args_with_flag(),
            None,
            None,
            false,
            "hello",
        );
        assert!(cmd.contains("--output-format"), "cmd: {}", cmd);
        assert!(cmd.contains("json"), "cmd: {}", cmd);
    }

    #[test]
    fn gemini_sandbox_none_no_approval_flag() {
        let cmd = GeminiBackend::build_shell_cmd("npx", &args(), None, None, false, "hello");
        assert!(!cmd.contains("--approval-mode"));
    }

    #[test]
    fn gemini_sandbox_read_only_adds_plan() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::ReadOnly),
            false,
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
            false,
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
            false,
            "hello",
        );
        assert!(cmd.contains("--approval-mode yolo"));
    }

    #[test]
    fn gemini_sandbox_prompt_is_escaped() {
        let cmd = GeminiBackend::build_shell_cmd("npx", &args(), None, None, false, "it's fine");
        assert!(
            cmd.contains("'\\\''"),
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
            false,
            "hello",
        );
        assert!(cmd.contains("--model 'gemini-2.5-pro'"));
        assert!(cmd.contains("--approval-mode plan"));
    }

    #[test]
    fn gemini_parse_envelope_extracts_usage() {
        let json = r#"{"response":"ok.","stats":{"models":{"gemini-test":{"tokens":{"prompt":100,"candidates":50,"cached":10,"thoughts":5}}}}}"#;
        let env = GeminiBackend::parse_gemini_envelope(json).expect("valid envelope");
        assert_eq!(env.response, "ok.");
        let usage = GeminiBackend::envelope_to_usage(env.stats).expect("usage extracted");
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.cached_tokens, Some(10));
        assert_eq!(usage.reasoning_tokens, Some(5));
    }

    #[test]
    fn gemini_parse_envelope_without_stats_returns_no_usage() {
        let json = r#"{"response":"ok."}"#;
        let env = GeminiBackend::parse_gemini_envelope(json).expect("valid envelope");
        assert_eq!(env.response, "ok.");
        assert!(GeminiBackend::envelope_to_usage(env.stats).is_none());
    }

    #[test]
    fn gemini_parse_envelope_malformed_returns_none() {
        let json = r#"{"response": "ok.", "stats": {"#;
        assert!(GeminiBackend::parse_gemini_envelope(json).is_none());
    }

    #[test]
    fn gemini_parse_envelope_missing_response_returns_none() {
        let json = r#"{"stats":{"promptTokenCount":10,"candidatesTokenCount":5}}"#;
        assert!(GeminiBackend::parse_gemini_envelope(json).is_none());
    }

    #[test]
    fn gemini_parse_envelope_flat_stats_fallback() {
        let json = r#"{"response":"hi","stats":{"promptTokenCount":42,"candidatesTokenCount":7,"cachedContentTokenCount":3}}"#;
        let env = GeminiBackend::parse_gemini_envelope(json).expect("valid envelope");
        let usage = GeminiBackend::envelope_to_usage(env.stats).expect("usage extracted");
        assert_eq!(usage.prompt_tokens, 42);
        assert_eq!(usage.completion_tokens, 7);
        assert_eq!(usage.cached_tokens, Some(3));
    }

    #[test]
    fn gemini_parse_envelope_sums_across_models() {
        let json = r#"{"response":"ok.","stats":{"models":{"m1":{"tokens":{"prompt":30,"candidates":20}},"m2":{"tokens":{"prompt":70,"candidates":30,"cached":5,"thoughts":2}}}}}"#;
        let env = GeminiBackend::parse_gemini_envelope(json).expect("valid envelope");
        let usage = GeminiBackend::envelope_to_usage(env.stats).expect("usage extracted");
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.cached_tokens, Some(5));
        assert_eq!(usage.reasoning_tokens, Some(2));
    }

    #[test]
    fn gemini_parse_envelope_ignores_zero_cached_and_thoughts() {
        let json = r#"{"response":"ok.","stats":{"models":{"m1":{"tokens":{"prompt":10,"candidates":5,"cached":0,"thoughts":0}}}}}"#;
        let env = GeminiBackend::parse_gemini_envelope(json).expect("valid envelope");
        let usage = GeminiBackend::envelope_to_usage(env.stats).expect("usage extracted");
        assert_eq!(usage.cached_tokens, None);
        assert_eq!(usage.reasoning_tokens, None);
    }

    #[test]
    fn gemini_apply_edits_true_no_sandbox_emits_auto_edit() {
        let cmd = GeminiBackend::build_shell_cmd("npx", &args(), None, None, true, "hello");
        assert!(cmd.contains("--approval-mode auto_edit"));
    }

    #[test]
    fn gemini_apply_edits_true_explicit_plan_preserved() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::ReadOnly),
            true,
            "hello",
        );
        assert!(cmd.contains("--approval-mode plan"));
    }

    #[test]
    fn gemini_apply_edits_true_explicit_auto_edit_preserved() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::WorkspaceWrite),
            true,
            "hello",
        );
        assert!(cmd.contains("--approval-mode auto_edit"));
    }

    #[test]
    fn gemini_apply_edits_true_explicit_yolo_preserved() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::DangerFullAccess),
            true,
            "hello",
        );
        assert!(cmd.contains("--approval-mode yolo"));
    }

    #[test]
    fn gemini_apply_edits_false_no_sandbox_omits_approval_flag() {
        let cmd = GeminiBackend::build_shell_cmd("npx", &args(), None, None, false, "hello");
        assert!(!cmd.contains("--approval-mode"));
    }

    #[test]
    fn gemini_apply_edits_false_explicit_auto_edit_preserved() {
        let cmd = GeminiBackend::build_shell_cmd(
            "npx",
            &args(),
            None,
            Some(SandboxMode::WorkspaceWrite),
            false,
            "hello",
        );
        assert!(cmd.contains("--approval-mode auto_edit"));
    }
}
