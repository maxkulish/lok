use crate::config::BackendConfig;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

pub struct GeminiBackend {
    command: String,
    args: Vec<String>,
    default_model: Option<String>,
}

/// Private representation of the legacy Gemini CLI JSON envelope emitted when
/// `--output-format json` is passed.
#[derive(serde::Deserialize)]
pub(crate) struct GeminiEnvelope {
    response: String,
    #[serde(default)]
    stats: Option<serde_json::Value>,
}

impl GeminiBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let command = config
            .command
            .clone()
            .unwrap_or_else(|| "opencode".to_string());

        let args = if config.args.is_empty() {
            vec![
                "run".to_string(),
                "--format".to_string(),
                "json".to_string(),
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

    fn normalize_model(model: &str) -> String {
        if model.contains('/') {
            model.to_string()
        } else {
            format!("google/{}", model)
        }
    }

    fn pick_u32(value: &serde_json::Value, keys: &[&str]) -> Option<u32> {
        keys.iter()
            .find_map(|key| value.get(key).and_then(|v| v.as_u64()).map(|n| n as u32))
    }

    pub(crate) fn parse_gemini_envelope(stdout: &str) -> Option<GeminiEnvelope> {
        serde_json::from_str(stdout).ok()
    }

    pub(crate) fn envelope_to_usage(stats: Option<serde_json::Value>) -> Option<super::TokenUsage> {
        let stats = stats?;
        Self::parse_usage_from_stats(&stats)
    }

    fn parse_usage_value(value: &serde_json::Value) -> Option<super::TokenUsage> {
        if let Some(v) = value.get("tokens").and_then(Self::parse_usage_value) {
            return Some(v);
        }

        let prompt = Self::pick_u32(
            value,
            &[
                "prompt_tokens",
                "promptTokens",
                "input_tokens",
                "input_tokens_count",
                "inputTokens",
                "promptTokenCount",
                "prompt_token_count",
                "input",
            ],
        )?;
        let completion = Self::pick_u32(
            value,
            &[
                "completion_tokens",
                "completionTokens",
                "output_tokens",
                "outputTokens",
                "completionTokenCount",
                "candidates",
                "candidatesTokenCount",
                "output",
            ],
        )?;
        let mut cached = Self::pick_u32(
            value,
            &[
                "cached_tokens",
                "cached_tokens_count",
                "cachedContentTokenCount",
                "cached",
                "cache_read_input_tokens",
            ],
        );
        if cached.is_none() {
            cached = Self::pick_u32(
                value.get("cache").unwrap_or(&Value::Null),
                &["read", "read_tokens", "write", "write_tokens"],
            );
        }
        let reasoning = Self::pick_u32(
            value,
            &[
                "reasoning_tokens",
                "reasoningTokens",
                "reasoning_token_count",
                "reasoning",
                "thoughts",
            ],
        );

        Some(
            super::TokenUsage::new(prompt, completion)
                .with_cached(cached)
                .with_reasoning(reasoning),
        )
    }

    fn parse_usage_from_stats(stats: &serde_json::Value) -> Option<super::TokenUsage> {
        // Legacy Gemini CLI shape: stats.models.<model>.tokens { prompt, candidates, cached, thoughts }
        if let Some(models) = stats.get("models").and_then(|m| m.as_object()) {
            let mut prompt = 0u32;
            let mut candidates = 0u32;
            let mut cached = 0u32;
            let mut thoughts = 0u32;
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

        // Flat legacy shape fallback.
        if let Some(prompt) = Self::pick_u32(
            stats,
            &[
                "promptTokenCount",
                "prompt_token_count",
                "prompt_tokens",
                "prompt",
            ],
        ) {
            if let Some(completion) = Self::pick_u32(
                stats,
                &[
                    "candidatesTokenCount",
                    "candidates_tokens",
                    "completion_tokens",
                    "completion",
                ],
            ) {
                let cached = Self::pick_u32(
                    stats,
                    &["cachedContentTokenCount", "cached_tokens", "cached"],
                );
                let reasoning = Self::pick_u32(stats, &["reasoning_tokens", "thoughts"]);
                return Some(
                    super::TokenUsage::new(prompt, completion)
                        .with_cached(cached)
                        .with_reasoning(reasoning),
                );
            }
        }

        None
    }

    fn merge_usage(values: impl IntoIterator<Item = super::TokenUsage>) -> super::TokenUsage {
        values
            .into_iter()
            .reduce(|acc, usage| {
                let cached = match (acc.cached_tokens, usage.cached_tokens) {
                    (Some(a), Some(b)) => Some(a.saturating_add(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                let reasoning = match (acc.reasoning_tokens, usage.reasoning_tokens) {
                    (Some(a), Some(b)) => Some(a.saturating_add(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                super::TokenUsage::new(
                    acc.prompt_tokens.saturating_add(usage.prompt_tokens),
                    acc.completion_tokens
                        .saturating_add(usage.completion_tokens),
                )
                .with_cached(cached)
                .with_reasoning(reasoning)
            })
            .unwrap_or_default()
    }

    fn response_from_json(value: &Value) -> Option<String> {
        if let Some(text) = value.get("response").and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }

        if let Some(text) = value.get("text").and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }

        if let Some(message) = value.get("message") {
            if let Some(text) = message
                .get("content")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                return Some(text.to_string());
            }
            if let Some(text) = Self::response_from_json(message) {
                return Some(text);
            }
        }

        if let Some(output) = value.get("output") {
            if let Some(text) = output.get("content").and_then(Value::as_str) {
                let text = text.trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            if let Some(text) = Self::response_from_json(output) {
                return Some(text);
            }
        }

        if let Some(result) = value.get("result") {
            if let Some(text) = result.get("content").and_then(Value::as_str) {
                let text = text.trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            if let Some(text) = Self::response_from_json(result) {
                return Some(text);
            }
        }

        if let Some(content) = value.get("content").and_then(Value::as_str) {
            let content = content.trim();
            if !content.is_empty() {
                return Some(content.to_string());
            }
        }

        if let Some(text) = value.get("assistant").and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }

        if let Some(array) = value.as_array() {
            let responses: Vec<String> = array
                .iter()
                .filter_map(Self::response_from_json)
                .filter(|s| !s.trim().is_empty())
                .collect();
            if !responses.is_empty() {
                return Some(responses.join("\n"));
            }
        }

        if let Some(object) = value.as_object() {
            for (key, child) in object {
                if Self::is_auxiliary_key(key) {
                    continue;
                }
                if let Some(text) = Self::response_from_json(child) {
                    return Some(text);
                }
            }
        }

        None
    }

    fn is_auxiliary_key(key: &str) -> bool {
        matches!(
            key,
            "usage"
                | "stats"
                | "event"
                | "type"
                | "model"
                | "status"
                | "command"
                | "id"
                | "session_id"
        )
    }

    fn extract_opencode_response_text(value: &Value) -> Option<String> {
        if let Some(event_type) = value.get("type").and_then(Value::as_str) {
            if event_type != "text" {
                return None;
            }
        } else if Self::response_from_json(value).is_some() {
            return Self::response_from_json(value);
        }

        if let Some(text) = value.get("text").and_then(Value::as_str) {
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }

        if let Some(part) = value.get("part") {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                let text = text.trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            if let Some(text) = part.get("content").and_then(Value::as_str) {
                let text = text.trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }

        None
    }

    fn extract_opencode_usage(value: &Value) -> Option<super::TokenUsage> {
        if let Some(event_type) = value.get("type").and_then(Value::as_str) {
            if event_type != "step_finish" {
                return None;
            }
        }

        let mut usages: Vec<super::TokenUsage> = Vec::new();

        let mut collect = |candidate: &Value| {
            if let Some(usage) = candidate
                .get("usage")
                .and_then(Self::parse_usage_value)
                .or_else(|| {
                    candidate
                        .get("stats")
                        .and_then(Self::parse_usage_from_stats)
                })
                .or_else(|| candidate.get("tokens").and_then(Self::parse_usage_value))
            {
                usages.push(usage);
            }
        };

        collect(value);

        if let Some(part) = value.get("part") {
            collect(part);
        }

        if usages.is_empty() {
            None
        } else {
            Some(Self::merge_usage(usages))
        }
    }

    fn parse_opencode_output(stdout: &str) -> Option<(String, Option<super::TokenUsage>)> {
        let mut response_parts: Vec<String> = Vec::new();
        let mut usages: Vec<super::TokenUsage> = Vec::new();

        if let Ok(value) = serde_json::from_str::<Value>(stdout) {
            if let Some(text) = Self::extract_opencode_response_text(&value) {
                response_parts.push(text);
            }
            if let Some(usage) = Self::extract_opencode_usage(&value) {
                usages.push(usage);
            }
            if response_parts.is_empty() {
                return None;
            }

            let usage = if usages.is_empty() {
                None
            } else {
                Some(Self::merge_usage(usages))
            };
            return Some((response_parts.join("\n"), usage));
        }

        let mut parsed_line = false;
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            parsed_line = true;
            if let Some(text) = Self::extract_opencode_response_text(&value) {
                response_parts.push(text);
            }
            if let Some(usage) = Self::extract_opencode_usage(&value) {
                usages.push(usage);
            }
        }

        if parsed_line {
            let response = response_parts.join("\n");
            if response.is_empty() {
                return None;
            }

            let usage = if usages.is_empty() {
                None
            } else {
                Some(Self::merge_usage(usages))
            };
            return Some((response, usage));
        }

        None
    }

    pub(crate) fn parse_backend_output(stdout: &str) -> (String, Option<super::TokenUsage>) {
        if let Some((response, usage)) = Self::parse_opencode_output(stdout) {
            if !response.is_empty() {
                return (response, usage);
            }
        }

        if let Some(env) = Self::parse_gemini_envelope(stdout) {
            let usage = Self::envelope_to_usage(env.stats);
            return (env.response, usage);
        }

        (stdout.to_string(), None)
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

    /// Build argv for the Gemini backend invocation.
    ///
    /// `opencode` is treated as the new execution path (run/json + strict argv shape).
    /// Legacy Gemini CLI invocation paths pass through their configured arguments unchanged,
    /// with the prompt appended as a trailing positional argument.
    fn build_argv(
        command: &str,
        base_args: &[String],
        model: Option<&str>,
        sandbox: Option<super::SandboxMode>,
        apply_edits: bool,
        prompt: &str,
    ) -> Vec<String> {
        let command_name = command.rsplit('/').next().unwrap_or(command);
        let uses_opencode = command_name == "opencode";

        let mut argv: Vec<String> = if base_args.is_empty() {
            if uses_opencode {
                vec![
                    "run".to_string(),
                    "--format".to_string(),
                    "json".to_string(),
                ]
            } else {
                Vec::new()
            }
        } else {
            base_args.to_vec()
        };

        if !uses_opencode {
            argv.push(prompt.to_string());
            return argv;
        }

        let effective = Self::resolve_effective_sandbox(sandbox, apply_edits);
        let agent = match effective {
            Some(super::SandboxMode::ReadOnly) => "plan",
            Some(super::SandboxMode::WorkspaceWrite) => "build",
            Some(super::SandboxMode::DangerFullAccess) => "build",
            None => "build",
        };

        argv.push("--agent".to_string());
        argv.push(agent.to_string());

        if matches!(effective, Some(super::SandboxMode::DangerFullAccess)) {
            argv.push("--dangerously-skip-permissions".to_string());
        }

        if let Some(raw_model) = model.filter(|m| !m.is_empty()) {
            argv.push("--model".to_string());
            argv.push(Self::normalize_model(raw_model));
        }

        argv.push("--".to_string());
        argv.push(prompt.to_string());

        argv
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

        let effective_model: Option<String> = model
            .filter(|m| !m.is_empty())
            .map(String::from)
            .or_else(|| self.default_model.clone());

        let argv = Self::build_argv(
            &self.command,
            &self.args,
            effective_model.as_deref(),
            ctx.sandbox,
            ctx.apply_edits,
            prompt,
        );

        let mut cmd = Command::new(&self.command);
        cmd.args(&argv)
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
            return Err(super::BackendError::ExecutionFailed {
                message: format!("Gemini failed: {}", stderr_str),
                exit_code: Some(exit_code),
            });
        }

        let (response_text, usage) = Self::parse_backend_output(&stdout);
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
                message: format!(
                    "Gemini backend command '{}' not found on PATH",
                    self.command
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SandboxMode;
    use super::GeminiBackend;
    use std::fs;

    fn default_args_with_flag() -> Vec<String> {
        vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ]
    }

    fn custom_args_without_flags() -> Vec<String> {
        vec!["--skip-trust".to_string()]
    }

    #[test]
    fn gemini_default_config_uses_opencode_command_and_run_json_args() {
        let cfg = crate::config::Config::default();
        let gemini_cfg = cfg
            .backends
            .get("gemini")
            .expect("default gemini backend exists");
        let backend = GeminiBackend::new(gemini_cfg).expect("backend constructs");
        assert_eq!(backend.command, "opencode");
        assert_eq!(backend.args, vec!["run", "--format", "json"]);
        assert_eq!(
            backend.default_model.as_deref(),
            Some("google/gemini-2.5-flash")
        );
        assert_eq!(gemini_cfg.skip_lines, 0);
    }

    #[test]
    fn gemini_custom_args_without_flag_preserved() {
        let mut cfg = crate::config::Config::default();
        let gemini_cfg = cfg
            .backends
            .get_mut("gemini")
            .expect("default gemini backend exists");
        gemini_cfg.args = custom_args_without_flags();
        let backend = GeminiBackend::new(gemini_cfg).expect("backend constructs");
        assert_eq!(backend.args, vec!["--skip-trust"]);
    }

    #[test]
    fn gemini_build_argv_includes_format_json() {
        let argv = GeminiBackend::build_argv("opencode", &[], None, None, false, "hello");
        let idx = argv
            .iter()
            .position(|a| a == "--format")
            .expect("format flag present");
        assert_eq!(argv[idx + 1], "json");
    }

    #[test]
    fn gemini_build_argv_passes_prompt_as_positional_after_dash() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            None,
            None,
            false,
            "hello",
        );
        let idx = argv.iter().position(|a| a == "--").expect("-- present");
        assert_eq!(argv[idx + 1], "hello");
        assert_eq!(idx, argv.len() - 2);
    }

    #[test]
    fn gemini_build_argv_none_sandbox_defaults_to_build_agent() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            None,
            None,
            false,
            "hello",
        );
        let idx = argv
            .iter()
            .position(|a| a == "--agent")
            .expect("--agent present");
        assert_eq!(argv[idx + 1], "build");
    }

    #[test]
    fn gemini_build_argv_read_only_maps_to_plan_agent() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            None,
            Some(SandboxMode::ReadOnly),
            false,
            "hello",
        );
        let idx = argv
            .iter()
            .position(|a| a == "--agent")
            .expect("--agent present");
        assert_eq!(argv[idx + 1], "plan");
    }

    #[test]
    fn gemini_build_argv_workspace_write_maps_to_build_agent() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            None,
            Some(SandboxMode::WorkspaceWrite),
            false,
            "hello",
        );
        let idx = argv
            .iter()
            .position(|a| a == "--agent")
            .expect("--agent present");
        assert_eq!(argv[idx + 1], "build");
    }

    #[test]
    fn gemini_build_argv_danger_adds_skip_permissions() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            None,
            Some(SandboxMode::DangerFullAccess),
            false,
            "hello",
        );
        let idx = argv
            .iter()
            .position(|a| a == "--agent")
            .expect("--agent present");
        assert_eq!(argv[idx + 1], "build");
        assert!(argv
            .iter()
            .any(|arg| arg == "--dangerously-skip-permissions"));
    }

    #[test]
    fn gemini_build_argv_apply_edits_true_without_sandbox_defaults_to_build() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            None,
            None,
            true,
            "hello",
        );
        let idx = argv
            .iter()
            .position(|a| a == "--agent")
            .expect("--agent present");
        assert_eq!(argv[idx + 1], "build");
    }

    #[test]
    fn gemini_build_argv_model_flag_prefixes_bare_model() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            Some("gemini-2.5-flash"),
            None,
            false,
            "hello",
        );
        let idx = argv
            .iter()
            .position(|a| a == "--model")
            .expect("--model present");
        assert_eq!(argv[idx + 1], "google/gemini-2.5-flash");
    }

    #[test]
    fn gemini_build_argv_preserves_prefixed_model() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            Some("google/gemini-2.5-flash"),
            None,
            false,
            "hello",
        );
        let idx = argv
            .iter()
            .position(|a| a == "--model")
            .expect("--model present");
        assert_eq!(argv[idx + 1], "google/gemini-2.5-flash");
    }

    #[test]
    fn gemini_build_argv_prompt_hostile_characters_preserved_as_single_arg() {
        let argv = GeminiBackend::build_argv(
            "opencode",
            &default_args_with_flag(),
            None,
            None,
            false,
            "it's fine; rm -rf /",
        );
        let idx = argv.iter().position(|a| a == "--").expect("-- present");
        assert_eq!(argv[idx + 1], "it's fine; rm -rf /");
    }

    #[test]
    fn gemini_build_argv_legacy_command_passes_prompt_tail_and_preserves_args() {
        let legacy_args = vec![
            "@google/gemini-cli".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
        ];
        let argv = GeminiBackend::build_argv(
            "npx",
            &legacy_args,
            Some("gemini-2.5-flash"),
            Some(SandboxMode::WorkspaceWrite),
            true,
            "hello",
        );

        assert_eq!(
            argv,
            vec![
                "@google/gemini-cli".to_string(),
                "--output-format".to_string(),
                "json".to_string(),
                "hello".to_string(),
            ]
        );
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
    fn gemini_parse_backend_output_extracts_opencode_response_text() {
        let json = fs::read_to_string("tests/fixtures/gemini/success-no-stats.json")
            .expect("fixture exists");
        let (response, usage) = GeminiBackend::parse_backend_output(&json);
        assert_eq!(response, "Hi! How can I help you today?");
        assert!(usage.is_none());
    }

    #[test]
    fn gemini_parse_backend_output_extracts_opencode_usage_when_present() {
        let json = fs::read_to_string("tests/fixtures/gemini/success-with-stats.json")
            .expect("fixture exists");
        let (response, usage) = GeminiBackend::parse_backend_output(&json);
        assert_eq!(response, "Hi! How can I help you today?");
        let usage = usage.expect("usage expected");
        assert_eq!(usage.prompt_tokens, 11251);
        assert_eq!(usage.completion_tokens, 10);
        assert_eq!(usage.total_tokens, 11261);
        assert_eq!(usage.cached_tokens, Some(7552));
        assert_eq!(usage.reasoning_tokens, Some(185));
    }

    #[test]
    fn gemini_parse_backend_output_preserves_legacy_gemini_envelope() {
        let json = r#"{"response":"ok.","stats":{"models":{"gemini-test":{"tokens":{"prompt":10,"candidates":5,"cached":0,"thoughts":0}}}}}"#;
        let (response, usage) = GeminiBackend::parse_backend_output(json);
        assert_eq!(response, "ok.");
        let usage = usage.expect("usage expected");
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
    }

    #[test]
    fn gemini_parse_backend_output_fallback_on_malformed_json() {
        let json = r#"{"response": "ok.", "stats": {"models"{"#;
        let (response, usage) = GeminiBackend::parse_backend_output(json);
        assert_eq!(response, json);
        assert!(usage.is_none());
    }

    #[test]
    fn gemini_parse_backend_output_handles_missing_usage_and_nested_message() {
        let nested = r#"{"message": {"role": "assistant", "content": "Hello"}}"#;
        let (response, usage) = GeminiBackend::parse_backend_output(nested);
        assert_eq!(response, "Hello");
        assert!(usage.is_none());
    }

    #[test]
    fn gemini_parse_backend_output_extracts_ndjson_text_and_tokens() {
        let stdout = "{\"type\":\"text\",\"text\":\"Hello from opencode\"}\n{\"type\":\"step_finish\",\"tokens\":{\"input\":5,\"output\":10,\"reasoning\":3}}";
        let (response, usage) = GeminiBackend::parse_backend_output(stdout);
        assert_eq!(response, "Hello from opencode");
        let usage = usage.expect("usage expected");
        assert_eq!(usage.prompt_tokens, 5);
        assert_eq!(usage.completion_tokens, 10);
        assert_eq!(usage.reasoning_tokens, Some(3));
    }

    #[test]
    fn gemini_parse_backend_output_extracts_nested_part_text_and_tokens() {
        let stdout = "{\"type\":\"text\",\"part\":{\"text\":\"Hello from nested part\",\"type\":\"text\"}}\n{\"type\":\"step_finish\",\"part\":{\"type\":\"step_finish\",\"tokens\":{\"input\":7,\"output\":4,\"reasoning\":2,\"cache\":{\"read\":5,\"write\":1}}}}";
        let (response, usage) = GeminiBackend::parse_backend_output(stdout);
        assert_eq!(response, "Hello from nested part");
        let usage = usage.expect("usage expected");
        assert_eq!(usage.prompt_tokens, 7);
        assert_eq!(usage.completion_tokens, 4);
        assert_eq!(usage.reasoning_tokens, Some(2));
        assert_eq!(usage.cached_tokens, Some(5));
    }

    #[test]
    fn gemini_parse_backend_output_ignores_non_text_opencode_events() {
        let stdout = "{\"type\":\"text\",\"part\":{\"text\":\"Hello\"}}\n{\"type\":\"tool_use\",\"part\":{\"state\":{\"output\":\"secret tool output\"}}}\n{\"type\":\"step_finish\",\"part\":{\"type\":\"step_finish\",\"tokens\":{\"input\":10,\"output\":1}}}";
        let (response, usage) = GeminiBackend::parse_backend_output(stdout);
        assert_eq!(response, "Hello");
        let usage = usage.expect("usage expected");
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 1);
    }
}
