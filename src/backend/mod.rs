#[cfg(feature = "bedrock")]
mod bedrock;
mod claude;
mod codex;
mod gemini;
mod ollama;
mod retry;

#[cfg(feature = "bedrock")]
#[allow(unused_imports)]
pub use bedrock::BedrockBackend;
pub use claude::ClaudeBackend;
pub use retry::{RetryExecutor, RetryPolicy};

use crate::config::{BackendConfig, Config};
use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Typed backend errors replacing opaque `anyhow::Error` from `Backend::query()`.
/// Each variant represents a distinct failure mode that callers can match on
/// for retry decisions, user-facing messages, and error classification.
#[derive(Debug, Clone, thiserror::Error)]
#[allow(dead_code)]
pub enum BackendError {
    #[error("timeout: {message}")]
    Timeout { message: String, elapsed_ms: u64 },

    #[error("rate limited: {message}")]
    RateLimit {
        message: String,
        retry_after_ms: Option<u64>,
    },

    #[error("auth: {message}")]
    Auth { message: String },

    #[error("network: {message}")]
    Network { message: String },

    #[error("parse: {message}")]
    Parse { message: String },

    #[error("execution failed: {message}")]
    ExecutionFailed {
        message: String,
        exit_code: Option<i32>,
    },

    #[error("unavailable: {message}")]
    Unavailable { message: String },

    #[error("config: {message}")]
    Config { message: String },
}

impl BackendError {
    /// Returns true if this error is transient and the operation should be retried.
    /// Only `Timeout`, `RateLimit`, and `Network` are retryable.
    #[allow(dead_code)]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BackendError::Timeout { .. }
                | BackendError::RateLimit { .. }
                | BackendError::Network { .. }
        )
    }
}

// TODO: remove once all backends return typed errors directly
impl From<anyhow::Error> for BackendError {
    fn from(err: anyhow::Error) -> Self {
        use crate::utils::classify_backend_error;
        use crate::utils::BackendErrorKind;

        let msg = err.to_string();
        match classify_backend_error(&msg) {
            BackendErrorKind::RateLimited => BackendError::RateLimit {
                message: msg,
                retry_after_ms: None,
            },
            BackendErrorKind::CapacityExhausted => BackendError::Unavailable { message: msg },
            BackendErrorKind::AuthError => BackendError::Auth { message: msg },
            BackendErrorKind::NetworkError => BackendError::Network { message: msg },
            BackendErrorKind::NotInstalled => BackendError::Unavailable { message: msg },
            BackendErrorKind::Unknown => BackendError::ExecutionFailed {
                message: msg,
                exit_code: None,
            },
        }
    }
}

/// Token usage metadata reported by LLM backends, used for cost tracking and observability.
///
/// Counts are `u32` (max ~4 billion), which is sufficient for any realistic LLM context.
/// `total_tokens` is computed via saturating addition to avoid overflow panics on pathological inputs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl TokenUsage {
    /// Construct a `TokenUsage` from prompt and completion counts, computing `total_tokens`
    /// via `saturating_add` so that `u32::MAX + 1` clamps to `u32::MAX` instead of panicking.
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
        }
    }
}

/// Structured output from a backend query.
///
/// Carries the raw text channels (`stdout`, `stderr`, `exit_code`) plus metadata about
/// which backend produced the output, how long it took, which model responded, and
/// optional token usage / parsed JSON.
///
/// ## Duration semantics
///
/// `duration` is the backend's internal wall-clock measurement from the start of `query()`
/// to its return. It is distinct from `QueryResult.elapsed_ms`, which is measured by
/// `run_query_with_config` around the entire task spawn (including tokio task overhead
/// and progress-bar updates). The two may differ by a few milliseconds; both are valid
/// views of "how long the query took".
///
/// When a `RetryExecutor` wraps a backend, the returned `duration` reflects the final
/// successful attempt only, NOT the cumulative retry time. Callers wanting total retry
/// time should measure externally.
///
/// `structured` is NOT auto-populated by constructors. Callers that need parsed JSON
/// should invoke `workflow::extract_json_from_text(&output.stdout)` and pass the result
/// through `with_structured()`. This avoids silent failures on markdown-fenced JSON
/// (the common CLI case) and keeps extraction logic in one place.
// New fields (duration, structured, backend) are populated but not yet consumed
// by workflow.rs / template/context.rs - that migration is scoped as a follow-up.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct QueryOutput {
    pub stdout: String,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub model: Option<String>,
    pub duration: Duration,
    pub usage: Option<TokenUsage>,
    pub structured: Option<serde_json::Value>,
    pub backend: String,
}

impl QueryOutput {
    /// Create output for API backends (no process I/O).
    ///
    /// `backend` and `duration` are required to enforce the always-populated invariant;
    /// there is intentionally no `Default` impl for `QueryOutput`.
    pub fn from_text(text: String, backend: impl Into<String>, duration: Duration) -> Self {
        Self {
            stdout: text,
            stderr: None,
            exit_code: None,
            model: None,
            duration,
            usage: None,
            structured: None,
            backend: backend.into(),
        }
    }

    /// Create output for CLI backends with full process data.
    ///
    /// `backend` and `duration` are required to enforce the always-populated invariant.
    pub fn from_process(
        stdout: String,
        stderr: String,
        exit_code: i32,
        backend: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            stdout,
            stderr: Some(stderr).filter(|s| !s.is_empty()),
            exit_code: Some(exit_code),
            model: None,
            duration,
            usage: None,
            structured: None,
            backend: backend.into(),
        }
    }

    /// Builder setter for `model`. Accepts `Option<...>` so that chaining with API
    /// response fields (already `Option<String>`) compiles without `if let` guards.
    pub fn with_model(mut self, model: Option<impl Into<String>>) -> Self {
        self.model = model.map(Into::into);
        self
    }

    /// Builder setter for `usage`. Accepts `Option<TokenUsage>` to match the optional
    /// nature of token reporting (not all backends / responses include usage data).
    pub fn with_usage(mut self, usage: Option<TokenUsage>) -> Self {
        self.usage = usage;
        self
    }

    /// Builder setter for `structured`. Callers populate this explicitly after running
    /// their preferred JSON extraction (typically `workflow::extract_json_from_text`).
    #[allow(dead_code)]
    pub fn with_structured(mut self, structured: Option<serde_json::Value>) -> Self {
        self.structured = structured;
        self
    }
}

#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(
        &self,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
    ) -> std::result::Result<QueryOutput, BackendError>;
    fn is_available(&self) -> bool;
}

pub struct QueryResult {
    pub backend: String,
    pub output: String,
    pub success: bool,
    pub elapsed_ms: u64,
    pub error: Option<BackendError>,
}

pub fn get_retry_policy(config: &BackendConfig, defaults: &crate::config::Defaults) -> RetryPolicy {
    RetryPolicy {
        max_retries: config.max_retries.unwrap_or(defaults.max_retries),
        base_delay: Duration::from_millis(config.retry_delay_ms.unwrap_or(defaults.retry_delay_ms)),
        max_delay: Duration::from_secs(30),
    }
}

pub fn create_backend(
    name: &str,
    config: &BackendConfig,
    retry_policy: RetryPolicy,
) -> Result<Arc<dyn Backend>> {
    let inner: Arc<dyn Backend> = match name {
        "codex" => Arc::new(codex::CodexBackend::new(config)?),
        "gemini" => Arc::new(gemini::GeminiBackend::new(config)?),
        "claude" => Arc::new(claude::ClaudeBackend::new(config)?),
        "ollama" => Arc::new(ollama::OllamaBackend::new(config)?),
        #[cfg(feature = "bedrock")]
        "bedrock" => {
            // BedrockBackend::new is async, need runtime
            let rt = tokio::runtime::Handle::current();
            let config = config.clone();
            rt.block_on(async {
                anyhow::Ok(Arc::new(bedrock::BedrockBackend::new(&config).await?) as Arc<dyn Backend>)
            })?
        }
        #[cfg(not(feature = "bedrock"))]
        "bedrock" => anyhow::bail!("Bedrock backend requires the 'bedrock' feature. Rebuild with: cargo build --features bedrock"),
        _ => anyhow::bail!("Unknown backend: {}", name),
    };

    if retry_policy.max_retries > 0 {
        Ok(Arc::new(RetryExecutor::new(inner, retry_policy)))
    } else {
        Ok(inner)
    }
}

pub fn create_claude_backend(config: &Config) -> Result<ClaudeBackend> {
    let backend_config = config
        .backends
        .get("claude")
        .ok_or_else(|| anyhow::anyhow!("Claude backend not configured"))?;
    ClaudeBackend::new(backend_config)
}

pub fn get_backends(config: &Config, filter: Option<&str>) -> Result<Vec<Arc<dyn Backend>>> {
    let mut backends = Vec::new();

    let filter_names: Option<Vec<&str>> = filter.map(|f| f.split(',').collect());

    for (name, backend_config) in &config.backends {
        if !backend_config.enabled {
            continue;
        }

        if let Some(ref names) = filter_names {
            if !names.contains(&name.as_str()) {
                continue;
            }
        }

        let retry_policy = get_retry_policy(backend_config, &config.defaults);
        match create_backend(name, backend_config, retry_policy) {
            Ok(backend) => {
                if backend.is_available() {
                    backends.push(backend);
                } else {
                    eprintln!("{} Backend {} is not available", "warning:".yellow(), name);
                }
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to create backend {}: {}",
                    "warning:".yellow(),
                    name,
                    e
                );
            }
        }
    }

    if backends.is_empty() {
        anyhow::bail!("No backends available");
    }

    Ok(backends)
}

pub async fn run_query(
    backends: &[Arc<dyn Backend>],
    prompt: &str,
    cwd: &Path,
    config: &Config,
) -> Result<Vec<QueryResult>> {
    run_query_with_config(backends, prompt, cwd, config).await
}

pub async fn run_query_with_config(
    backends: &[Arc<dyn Backend>],
    prompt: &str,
    cwd: &Path,
    config: &Config,
) -> Result<Vec<QueryResult>> {
    let cwd = crate::utils::canonicalize_async(cwd).await;
    let prompt: Arc<str> = Arc::from(prompt);
    let cwd: Arc<Path> = Arc::from(cwd.as_path());
    let default_timeout = config.defaults.timeout;
    let parallel = config.defaults.parallel;

    let pb = ProgressBar::new(backends.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("hardcoded progress bar template should be valid")
            .progress_chars("#>-"),
    );

    let query_one = |backend: Arc<dyn Backend>,
                     prompt: Arc<str>,
                     cwd: Arc<Path>,
                     pb: ProgressBar,
                     timeout: u64| async move {
        pb.set_message(format!("Querying {}...", backend.name()));

        let start = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(timeout),
            backend.query(&prompt, &cwd, None),
        )
        .await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        pb.inc(1);

        match result {
            Ok(Ok(query_output)) => QueryResult {
                backend: backend.name().to_string(),
                output: query_output.stdout,
                success: true,
                elapsed_ms,
                error: None,
            },
            Ok(Err(e)) => QueryResult {
                backend: backend.name().to_string(),
                output: format!("Error: {}", e),
                success: false,
                elapsed_ms,
                error: Some(e),
            },
            Err(_) => {
                let timeout_err = BackendError::Timeout {
                    message: format!("Timeout ({}s)", timeout),
                    elapsed_ms,
                };
                QueryResult {
                    backend: backend.name().to_string(),
                    output: format!("Error: {}", timeout_err),
                    success: false,
                    elapsed_ms,
                    error: Some(timeout_err),
                }
            }
        }
    };

    // Helper to get timeout for a backend (0 means no timeout)
    let get_timeout = |backend_name: &str| -> u64 {
        let timeout = config
            .backends
            .get(backend_name)
            .and_then(|b| b.timeout)
            .unwrap_or(default_timeout);
        if timeout == 0 {
            365 * 24 * 60 * 60 // 1 year = effectively no timeout
        } else {
            timeout
        }
    };

    let results = if parallel {
        let futures: Vec<_> = backends
            .iter()
            .map(|backend| {
                let timeout = get_timeout(backend.name());
                query_one(
                    Arc::clone(backend),
                    Arc::clone(&prompt),
                    Arc::clone(&cwd),
                    pb.clone(),
                    timeout,
                )
            })
            .collect();
        join_all(futures).await
    } else {
        let mut results = Vec::new();
        for backend in backends {
            let timeout = get_timeout(backend.name());
            let result = query_one(
                Arc::clone(backend),
                Arc::clone(&prompt),
                Arc::clone(&cwd),
                pb.clone(),
                timeout,
            )
            .await;
            results.push(result);
        }
        results
    };

    pb.finish_and_clear();

    Ok(results)
}

/// Print verbose debug info before running a query
pub fn print_verbose_header(prompt: &str, backends: &[Arc<dyn Backend>], cwd: &Path) {
    println!("{}", "=== VERBOSE MODE ===".cyan().bold());
    println!();
    println!("{} {}", "Working directory:".dimmed(), cwd.display());
    println!(
        "{} {}",
        "Backends:".dimmed(),
        backends
            .iter()
            .map(|b| b.name())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!();
    println!("{}", "Prompt:".dimmed());
    println!("{}", "-".repeat(50).dimmed());
    println!("{}", prompt);
    println!("{}", "-".repeat(50).dimmed());
    println!();
}

/// Print verbose timing info after results
pub fn print_verbose_timing(results: &[QueryResult]) {
    println!();
    println!("{}", "=== TIMING ===".cyan().bold());
    for result in results {
        let status = if result.success {
            "OK".green()
        } else {
            "FAIL".red()
        };
        let time = format_duration(result.elapsed_ms);
        let chars = result.output.len();
        println!(
            "  {} {} ({}, {} chars)",
            result.backend.bold(),
            status,
            time,
            chars
        );
    }
    println!();
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60000.0)
    }
}

pub fn list_backends(config: &Config) -> Result<()> {
    println!("{}", "Available backends:".bold());
    println!();

    for (name, backend_config) in &config.backends {
        let status = if backend_config.enabled {
            "enabled".green()
        } else {
            "disabled".red()
        };

        let retry_policy = get_retry_policy(backend_config, &config.defaults);
        let available = match create_backend(name, backend_config, retry_policy) {
            Ok(b) if b.is_available() => "available".green(),
            _ => "not available".yellow(),
        };

        println!("  {} - {} ({})", name.bold(), status, available);

        if let Some(ref cmd) = backend_config.command {
            println!("    command: {} {}", cmd, backend_config.args.join(" "));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_output_from_text() {
        let output = QueryOutput::from_text("hello world".to_string(), "test", Duration::ZERO);
        assert_eq!(output.stdout, "hello world");
        assert!(output.stderr.is_none());
        assert!(output.exit_code.is_none());
        assert_eq!(output.backend, "test");
        assert_eq!(output.duration, Duration::ZERO);
        assert!(output.model.is_none());
        assert!(output.usage.is_none());
        assert!(output.structured.is_none());
    }

    #[test]
    fn test_query_output_from_process_with_stderr() {
        let output = QueryOutput::from_process(
            "stdout content".to_string(),
            "stderr content".to_string(),
            0,
            "test",
            Duration::ZERO,
        );
        assert_eq!(output.stdout, "stdout content");
        assert_eq!(output.stderr, Some("stderr content".to_string()));
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.backend, "test");
    }

    #[test]
    fn test_query_output_from_process_empty_stderr_normalized() {
        let output = QueryOutput::from_process(
            "stdout".to_string(),
            "".to_string(),
            0,
            "test",
            Duration::ZERO,
        );
        assert_eq!(output.stdout, "stdout");
        assert!(output.stderr.is_none());
        assert_eq!(output.exit_code, Some(0));
    }

    #[test]
    fn test_query_output_from_process_empty_stdout() {
        let output = QueryOutput::from_process(
            "".to_string(),
            "".to_string(),
            0,
            "test",
            Duration::ZERO,
        );
        assert_eq!(output.stdout, "");
        assert!(output.stderr.is_none());
        assert_eq!(output.exit_code, Some(0));
    }

    #[test]
    fn test_token_usage_new_computes_total() {
        let usage = TokenUsage::new(10, 20);
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.total_tokens, 30);
    }

    #[test]
    fn test_token_usage_new_saturates_on_overflow() {
        let usage = TokenUsage::new(u32::MAX, 1);
        assert_eq!(usage.prompt_tokens, u32::MAX);
        assert_eq!(usage.completion_tokens, 1);
        assert_eq!(usage.total_tokens, u32::MAX);
    }

    #[test]
    fn test_token_usage_default_zero() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_query_output_from_text_populates_backend_and_duration() {
        let output =
            QueryOutput::from_text("ok".to_string(), "claude", Duration::from_millis(100));
        assert_eq!(output.backend, "claude");
        assert_eq!(output.duration, Duration::from_millis(100));
        assert!(output.structured.is_none());
    }

    #[test]
    fn test_query_output_from_process_populates_backend_and_duration() {
        let output = QueryOutput::from_process(
            "stdout".to_string(),
            "".to_string(),
            0,
            "gemini",
            Duration::from_millis(250),
        );
        assert_eq!(output.backend, "gemini");
        assert_eq!(output.duration, Duration::from_millis(250));
        assert!(output.structured.is_none());
    }

    #[test]
    fn test_query_output_with_model_some() {
        let output = QueryOutput::from_text("ok".to_string(), "claude", Duration::ZERO)
            .with_model(Some("sonnet"));
        assert_eq!(output.model, Some("sonnet".to_string()));
    }

    #[test]
    fn test_query_output_with_model_none() {
        let output = QueryOutput::from_text("ok".to_string(), "claude", Duration::ZERO)
            .with_model(None::<String>);
        assert!(output.model.is_none());
    }

    #[test]
    fn test_query_output_with_usage_some() {
        let output = QueryOutput::from_text("ok".to_string(), "claude", Duration::ZERO)
            .with_usage(Some(TokenUsage::new(5, 10)));
        assert_eq!(
            output.usage,
            Some(TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 10,
                total_tokens: 15,
            })
        );
    }

    #[test]
    fn test_query_output_with_usage_none() {
        let output = QueryOutput::from_text("ok".to_string(), "claude", Duration::ZERO)
            .with_usage(None);
        assert!(output.usage.is_none());
    }

    #[test]
    fn test_query_output_with_structured_some() {
        let value = serde_json::json!({"a": 1});
        let output = QueryOutput::from_text("ok".to_string(), "claude", Duration::ZERO)
            .with_structured(Some(value.clone()));
        assert_eq!(output.structured, Some(value));
    }

    #[test]
    fn test_query_output_with_structured_none() {
        let output = QueryOutput::from_text("ok".to_string(), "claude", Duration::ZERO)
            .with_structured(None);
        assert!(output.structured.is_none());
    }

    #[test]
    fn test_backend_error_retryable() {
        assert!(BackendError::Timeout {
            message: "timed out".into(),
            elapsed_ms: 5000
        }
        .is_retryable());
        assert!(BackendError::RateLimit {
            message: "429".into(),
            retry_after_ms: None
        }
        .is_retryable());
        assert!(BackendError::Network {
            message: "refused".into()
        }
        .is_retryable());
    }

    #[test]
    fn test_backend_error_not_retryable() {
        assert!(!BackendError::Auth {
            message: "bad key".into()
        }
        .is_retryable());
        assert!(!BackendError::Parse {
            message: "invalid json".into()
        }
        .is_retryable());
        assert!(!BackendError::ExecutionFailed {
            message: "failed".into(),
            exit_code: Some(1)
        }
        .is_retryable());
        assert!(!BackendError::Unavailable {
            message: "gone".into()
        }
        .is_retryable());
        assert!(!BackendError::Config {
            message: "bad config".into()
        }
        .is_retryable());
    }

    #[test]
    fn test_backend_error_display() {
        let err = BackendError::Timeout {
            message: "request took too long".into(),
            elapsed_ms: 30000,
        };
        assert_eq!(err.to_string(), "timeout: request took too long");

        let err = BackendError::RateLimit {
            message: "429 Too Many Requests".into(),
            retry_after_ms: Some(5000),
        };
        assert_eq!(err.to_string(), "rate limited: 429 Too Many Requests");

        let err = BackendError::ExecutionFailed {
            message: "process exited".into(),
            exit_code: Some(1),
        };
        assert_eq!(err.to_string(), "execution failed: process exited");
    }

    #[test]
    fn test_backend_error_from_anyhow() {
        let anyhow_err = anyhow::anyhow!("Error 429: Too Many Requests");
        let backend_err = BackendError::from(anyhow_err);
        assert!(matches!(backend_err, BackendError::RateLimit { .. }));

        let anyhow_err = anyhow::anyhow!("ECONNREFUSED: Connection refused");
        let backend_err = BackendError::from(anyhow_err);
        assert!(matches!(backend_err, BackendError::Network { .. }));

        let anyhow_err = anyhow::anyhow!("Something unknown happened");
        let backend_err = BackendError::from(anyhow_err);
        assert!(matches!(backend_err, BackendError::ExecutionFailed { .. }));
    }
}
