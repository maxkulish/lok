use crate::config::BackendConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use std::time::Instant;
use tokio::process::Command;
use tokio::time::timeout;

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
    ) -> std::result::Result<super::QueryOutput, BackendError> {
        let start = Instant::now();

        let (command, default_model) = match &self.mode {
            ClaudeMode::Cli { command, model } => (command, model),
            ClaudeMode::Api { .. } => {
                return Err(BackendError::Config {
                    message: "CLI mode required for this operation".to_string(),
                });
            }
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

        let output = cmd.output().await.map_err(|e| BackendError::Unavailable {
            message: format!("Failed to execute claude command: {}", e),
        })?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(BackendError::ExecutionFailed {
                message: format!("Claude CLI failed: {}", stderr_str),
                exit_code: Some(exit_code),
            });
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
        ctx: super::StepContext<'_>,
    ) -> std::result::Result<super::QueryOutput, BackendError> {
        match &self.mode {
            ClaudeMode::Api { .. } => {
                self.query_api("You are a helpful assistant.", ctx.prompt, ctx.model)
                    .await
            }
            ClaudeMode::Cli { .. } => self.query_cli(ctx.prompt, ctx.cwd, ctx.model).await,
        }
    }

    fn is_available(&self) -> bool {
        super::Engine::is_backend_available(self.name())
    }

    async fn health_check(&self) -> std::result::Result<super::HealthStatus, super::BackendError> {
        match &self.mode {
            ClaudeMode::Api { .. } => self.probe_api().await,
            ClaudeMode::Cli { .. } => self.probe_cli().await,
        }
    }
}

/// Per-version cache for `claude --help` output so we don't re-parse on every warmup.
/// The lock is held only for a short HashMap lookup/insert — never across an `.await`.
/// The Option<String> value distinguishes: key missing = never attempted,
/// Some(Some(text)) = cached help output, Some(None) = attempted but failed.
static HELP_CACHE: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Compiled once — avoids re-compiling on every `parse_semver_line` call.
static SEMVER_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\d+\.\d+\.\d+").expect("valid semver regex"));

/// Parse a semver string (X.Y.Z) from text using regex.
fn parse_semver_line(text: &str) -> Option<String> {
    SEMVER_RE.find(text).map(|m| m.as_str().to_string())
}

/// Get `claude --help` output, memoized per version string.
/// If version is None, the result is not cached (re-runs on next warmup).
async fn get_help_output(path: &Path, version: Option<&str>) -> Option<String> {
    if let Some(v) = version {
        // SAFETY: std::sync::Mutex is safe here because the lock is held for
        // a very short, non-async operation (HashMap lookup). Do NOT hold this
        // lock across an `.await` boundary.
        match HELP_CACHE.lock().unwrap().get(v) {
            // Cached success
            Some(Some(text)) => return Some(text.clone()),
            // Cached failure (previously attempted and failed)
            Some(None) => return None,
            // Not yet attempted — continue
            None => {}
        }
    }
    let output = timeout(
        Duration::from_secs(2),
        Command::new(path).arg("--help").kill_on_drop(true).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        if let Some(v) = version {
            HELP_CACHE.lock().unwrap().insert(v.to_string(), None);
        }
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if let Some(v) = version {
        HELP_CACHE
            .lock()
            .unwrap()
            .insert(v.to_string(), Some(text.clone()));
    }
    Some(text)
}

impl ClaudeBackend {
    /// Offline API probe — checks ANTHROPIC_API_KEY and model non-empty.
    async fn probe_api(&self) -> std::result::Result<super::HealthStatus, super::BackendError> {
        match &self.mode {
            ClaudeMode::Api { api_key, model, .. } => {
                if api_key.expose_secret().trim().is_empty() {
                    return Ok(super::HealthStatus {
                        available: false,
                        mode: Some("api".into()),
                        diagnostic: Some("ANTHROPIC_API_KEY not set or empty".into()),
                        ..super::HealthStatus::new_unavailable()
                    });
                }
                if model.trim().is_empty() {
                    return Ok(super::HealthStatus {
                        available: false,
                        mode: Some("api".into()),
                        diagnostic: Some("model config is empty".into()),
                        ..super::HealthStatus::new_unavailable()
                    });
                }
                Ok(super::HealthStatus {
                    available: true,
                    mode: Some("api".into()),
                    ..super::HealthStatus::new_available()
                })
            }
            _ => Err(BackendError::Config {
                message: "not API mode".to_string(),
            }),
        }
    }

    /// CLI probe — runs `claude --version` (2s) and `claude --help` (2s).
    async fn probe_cli(&self) -> std::result::Result<super::HealthStatus, super::BackendError> {
        match &self.mode {
            ClaudeMode::Cli { command, .. } => {
                // 1. Check binary exists
                let path = match which::which(command) {
                    Ok(p) => p,
                    Err(_) => {
                        return Ok(super::HealthStatus {
                            available: false,
                            mode: Some("cli".into()),
                            diagnostic: Some(format!(
                                "claude CLI command '{}' not found on PATH",
                                command
                            )),
                            ..super::HealthStatus::new_unavailable()
                        });
                    }
                };

                // 2. Run claude --version with 2s budget
                let version_output = timeout(
                    Duration::from_secs(2),
                    Command::new(&path)
                        .arg("--version")
                        .kill_on_drop(true)
                        .output(),
                )
                .await;

                let version = match version_output {
                    Ok(Ok(output)) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        parse_semver_line(&stdout)
                    }
                    Ok(Ok(_)) => None,
                    Ok(Err(e)) => {
                        eprintln!("claude --version IO error: {:?}", e);
                        None
                    }
                    Err(_elapsed) => {
                        eprintln!("claude --version timed out after 2s");
                        None
                    }
                };

                // 3. Check --help for --output-format json (cached per version)
                let help_cache_entry = get_help_output(&path, version.as_deref()).await;
                let supports_json = help_cache_entry
                    .as_deref()
                    .map(|help| help.contains("--output-format") && help.contains("json"))
                    .unwrap_or(false);

                // 4. Build unusable_flags if json not supported
                let mut unusable_flags = Vec::new();
                if !supports_json {
                    eprintln!("claude CLI --output-format json not advertised in --help");
                    unusable_flags.push("--output-format json".into());
                }

                Ok(super::HealthStatus {
                    available: true,
                    version,
                    mode: Some("cli".into()),
                    unusable_flags,
                    ..super::HealthStatus::new_available()
                })
            }
            _ => Err(BackendError::Config {
                message: "not CLI mode".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::LazyLock;
    use tokio::sync::Mutex as TokioMutex;

    /// Global lock for tests that modify global state (PATH env var).
    /// Prevents concurrent execution of CLI mock tests.
    static PATH_LOCK: LazyLock<TokioMutex<()>> = LazyLock::new(|| TokioMutex::new(()));

    /// Helper: creates a mock `claude` binary at a temp directory and returns
    /// (TempDir, PathBuf to the binary, the original PATH for restoration).
    /// The mock binary outputs `version_text` for `--version` and `help_text` for `--help`.
    fn setup_mock_claude(
        version_text: &str,
        help_text: &str,
    ) -> (
        tempfile::TempDir,
        std::path::PathBuf,
        Option<std::ffi::OsString>,
    ) {
        let dir = tempfile::tempdir().expect("tempdir");
        let bin_path = dir.path().join("claude");

        #[cfg(unix)]
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&bin_path).expect("create mock binary");
            // Write help text to a file alongside the binary
            let help_path = dir.path().join("help.txt");
            std::fs::write(&help_path, help_text).expect("write help file");
            let help_path_str = help_path.to_string_lossy().to_string();
            writeln!(
                f,
                "#!/bin/sh\ncase \"$1\" in\n  --version) echo '{}' ;;\n  --help)    /bin/cat {help};;\n  *)\texit 1 ;;
esac\n",
                version_text,
                help = help_path_str
            )
            .expect("write mock script");
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }

        // Prepend the mock dir to PATH rather than replacing it. PATH is
        // process-global, so a replace would leak into other parallel tests
        // and strip away /bin, /usr/bin, etc. — breaking any sibling test
        // that shells out to `sleep`, `echo`, `cat`, etc. Prepending keeps
        // the mock `claude` winning the lookup while leaving system dirs
        // reachable. Use split_paths/join_paths so the separator and any
        // non-UTF8 entries in PATH are preserved correctly across platforms.
        let orig_path_os = std::env::var_os("PATH");
        let new_path = std::env::join_paths(
            std::iter::once(dir.path().to_path_buf())
                .chain(orig_path_os.as_ref().into_iter().flat_map(std::env::split_paths)),
        )
        .expect("failed to join PATH");
        std::env::set_var("PATH", new_path);

        (dir, bin_path, orig_path_os)
    }

    /// Helper: restores the original PATH after a mock test.
    fn restore_path(orig_path: Option<std::ffi::OsString>) {
        match orig_path {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
    }

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

    // --- API probe tests ---

    #[tokio::test]
    async fn test_probe_api_valid_key_and_model() {
        let backend = ClaudeBackend {
            mode: ClaudeMode::Api {
                api_key: SecretString::from("sk-ant-test123"),
                model: "claude-sonnet-4-20250514".into(),
                client: reqwest::Client::new(),
            },
        };
        let result = backend.probe_api().await.expect("should not error");
        assert!(result.available);
        assert_eq!(result.mode, Some("api".into()));
        assert!(result.diagnostic.is_none());
        assert!(result.version.is_none());
    }

    #[tokio::test]
    async fn test_probe_api_empty_key() {
        let backend = ClaudeBackend {
            mode: ClaudeMode::Api {
                api_key: SecretString::from(""),
                model: "claude-sonnet-4-20250514".into(),
                client: reqwest::Client::new(),
            },
        };
        let result = backend.probe_api().await.expect("should not error");
        assert!(!result.available);
        assert_eq!(result.mode, Some("api".into()));
        assert_eq!(
            result.diagnostic.as_deref(),
            Some("ANTHROPIC_API_KEY not set or empty")
        );
    }

    #[tokio::test]
    async fn test_probe_api_empty_model() {
        let backend = ClaudeBackend {
            mode: ClaudeMode::Api {
                api_key: SecretString::from("sk-ant-test123"),
                model: "".into(),
                client: reqwest::Client::new(),
            },
        };
        let result = backend.probe_api().await.expect("should not error");
        assert!(!result.available);
        assert_eq!(result.mode, Some("api".into()));
        assert_eq!(result.diagnostic.as_deref(), Some("model config is empty"));
    }

    // --- CLI probe tests ---

    #[tokio::test]
    async fn test_probe_cli_present_with_json_support() {
        let _lock = PATH_LOCK.lock().await;
        let (_dir, _path, orig_path) = setup_mock_claude(
            "Claude CLI 0.42.0",
            "Usage: claude [OPTIONS]
--output-format <FORMAT>  [json|text]
",
        );

        let backend = ClaudeBackend {
            mode: ClaudeMode::Cli {
                command: "claude".into(),
                model: None,
            },
        };
        let result = backend.probe_cli().await.expect("should not error");

        restore_path(orig_path);

        assert!(result.available);
        assert_eq!(result.mode, Some("cli".into()));
        assert_eq!(result.version.as_deref(), Some("0.42.0"));
        assert!(result.unusable_flags.is_empty());
    }

    #[tokio::test]
    async fn test_probe_cli_present_without_json_support() {
        let _lock = PATH_LOCK.lock().await;
        let (_dir, _path, orig_path) = setup_mock_claude(
            "Claude CLI 1.2.3",
            "Usage: claude [OPTIONS]
  --model <MODEL>  Model name
",
        );

        let backend = ClaudeBackend {
            mode: ClaudeMode::Cli {
                command: "claude".into(),
                model: None,
            },
        };
        let result = backend.probe_cli().await.expect("should not error");

        restore_path(orig_path);

        assert!(result.available);
        assert_eq!(result.mode, Some("cli".into()));
        assert_eq!(result.version.as_deref(), Some("1.2.3"));
        assert_eq!(result.unusable_flags, vec!["--output-format json"]);
    }

    #[tokio::test]
    async fn test_probe_cli_not_found() {
        let backend = ClaudeBackend {
            mode: ClaudeMode::Cli {
                command: "claude-nonexistent-xyz".into(),
                model: None,
            },
        };
        let result = backend.probe_cli().await.expect("should not error");
        assert!(!result.available);
        assert_eq!(result.mode, Some("cli".into()));
        assert!(result.diagnostic.is_some());
        assert!(result
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("not found on PATH"));
    }

    #[test]
    fn test_parse_semver_line_standard() {
        assert_eq!(
            parse_semver_line("Claude CLI 0.42.0"),
            Some("0.42.0".into())
        );
    }

    #[test]
    fn test_parse_semver_line_with_prefix() {
        assert_eq!(
            parse_semver_line("claude version 1.2.3 (build 456)"),
            Some("1.2.3".into())
        );
    }

    #[test]
    fn test_parse_semver_line_no_version() {
        assert_eq!(parse_semver_line("Claude CLI (unknown version)"), None);
    }

    #[test]
    fn test_parse_semver_line_empty() {
        assert_eq!(parse_semver_line(""), None);
    }
}
