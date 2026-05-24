#[cfg(feature = "bedrock")]
mod bedrock;
mod claude;
mod codex;
mod codex_event;
mod context;
mod gemini;
mod ollama;
mod retry;

#[cfg(feature = "bedrock")]
#[allow(unused_imports)]
pub use bedrock::BedrockBackend;
pub use claude::ClaudeBackend;
pub use codex::FLAG_MATRIX;
#[allow(unused_imports)]
pub use context::{HealthStatus, Message, ModelInfo, Role, SandboxMode, StepContext, StepOptions};
pub use retry::{RetryExecutor, RetryPolicy};

use crate::config::{BackendConfig, Config};
use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

/// Typed backend errors replacing opaque `anyhow::Error` from `Backend::query()`.
/// Each variant represents a distinct failure mode that callers can match on
/// for retry decisions, user-facing messages, and error classification.
#[derive(Debug, Clone, thiserror::Error)]
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
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BackendError::Timeout { .. }
                | BackendError::RateLimit { .. }
                | BackendError::Network { .. }
        )
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
    /// Tokens served from prompt cache (Anthropic `cache_read_input_tokens`,
    /// Codex `cached_input_tokens`). `None` when the backend does not report it.
    /// NOT included in `total_tokens`; surfaced separately so cache savings are
    /// visible to run summary / JSON output.
    ///
    /// **Note**: This value is reported directly by the upstream API and may
    /// exceed `prompt_tokens` in edge cases (e.g. server-side caching on a
    /// different message). It is stored as-reported; no validation is applied.
    pub cached_tokens: Option<u32>,
    /// Reasoning / thinking tokens billed in addition to completion
    /// (Codex `reasoning_output_tokens`, o-series). `None` when not reported.
    /// NOT included in `total_tokens`.
    pub reasoning_tokens: Option<u32>,
}

impl TokenUsage {
    /// Construct a `TokenUsage` from prompt and completion counts, computing `total_tokens`
    /// via `saturating_add` so that `u32::MAX + 1` clamps to `u32::MAX` instead of panicking.
    ///
    /// `cached_tokens` and `reasoning_tokens` default to `None`; use [`with_cached`](Self::with_cached)
    /// and [`with_reasoning`](Self::with_reasoning) to set them.
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
            cached_tokens: None,
            reasoning_tokens: None,
        }
    }

    /// Set `cached_tokens`. Consumes `self` for use in method-chaining
    /// construction patterns (e.g. `TokenUsage::new(p, c).with_cached(Some(40))`).
    pub fn with_cached(mut self, cached: Option<u32>) -> Self {
        self.cached_tokens = cached;
        self
    }

    /// Set `reasoning_tokens`. Consumes `self` for use in method-chaining
    /// construction patterns.
    pub fn with_reasoning(mut self, reasoning: Option<u32>) -> Self {
        self.reasoning_tokens = reasoning;
        self
    }

    pub fn saturating_add(&self, other: &Self) -> Self {
        Self {
            prompt_tokens: self.prompt_tokens.saturating_add(other.prompt_tokens),
            completion_tokens: self
                .completion_tokens
                .saturating_add(other.completion_tokens),
            total_tokens: self.total_tokens.saturating_add(other.total_tokens),
            cached_tokens: sum_opt(self.cached_tokens, other.cached_tokens),
            reasoning_tokens: sum_opt(self.reasoning_tokens, other.reasoning_tokens),
        }
    }
}

/// Saturating addition for `Option<u32>`: `None` + `None` = `None`,
/// `Some(x)` + `None` = `Some(x)`, `Some(x)` + `Some(y)` = `Some(x.saturating_add(y))`.
fn sum_opt(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x),
        (Some(x), Some(y)) => Some(x.saturating_add(y)),
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
    async fn query(&self, ctx: StepContext<'_>) -> std::result::Result<QueryOutput, BackendError>;
    fn is_available(&self) -> bool;
    /// Live async health probe. Default delegates to `is_available()`.
    /// Returns a placeholder `HealthStatus` so the trait signature is stable
    /// when FR-9/9a adds real fields.
    async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
        if self.is_available() {
            Ok(HealthStatus::new_available())
        } else {
            Err(BackendError::Unavailable {
                message: format!("Backend {} is not available", self.name()),
            })
        }
    }
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

/// Default timeout applied when no timeout is configured at any layer.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Near-infinite sentinel: map timeout=0 to this (existing convention for "no timeout").
pub const NO_TIMEOUT: Duration = Duration::from_secs(365 * 24 * 60 * 60);

/// Resolve the effective timeout for a step using the three-layer priority:
/// 1. Step-level timeout (highest priority)
/// 2. Backend-level timeout (medium priority)
/// 3. Global defaults timeout (lowest priority)
///    Falls back to `DEFAULT_TIMEOUT` (300s) if all three are `None`.
pub fn effective_timeout(
    step_timeout: Option<Duration>,
    backend_name: &str,
    config: &Config,
) -> Duration {
    let backend_timeout = config.backends.get(backend_name).and_then(|b| b.timeout);
    step_timeout
        .or(backend_timeout)
        .or(config.defaults.timeout)
        .map(|mut d| {
            if d.is_zero() {
                d = NO_TIMEOUT;
            }
            d
        })
        .unwrap_or(DEFAULT_TIMEOUT)
}

pub fn step_context_for_backend<'a>(
    prompt: &'a str,
    cwd: &'a Path,
    config: &'a Config,
    backend_name: &str,
) -> StepContext<'a> {
    let timeout = Some(effective_timeout(None, backend_name, config));
    let model = config
        .backends
        .get(backend_name)
        .and_then(|backend| backend.model.as_deref());

    StepContext {
        timeout,
        ..StepContext::from_prompt(prompt, cwd, model)
    }
}

pub fn create_backend(
    name: &str,
    config: &BackendConfig,
    retry_policy: RetryPolicy,
) -> Result<Arc<dyn Backend>> {
    // Check unified cache first
    {
        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        if let Some(entry) = lock.get(name) {
            return Ok(Arc::clone(&entry.backend));
        }
    }

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
            tokio::task::block_in_place(|| {
                rt.block_on(async {
                    anyhow::Ok(Arc::new(bedrock::BedrockBackend::new(&config).await?) as Arc<dyn Backend>)
                })
            })?
        }
        #[cfg(not(feature = "bedrock"))]
        "bedrock" => anyhow::bail!("Bedrock backend requires the 'bedrock' feature. Rebuild with: cargo build --features bedrock"),
        _ => anyhow::bail!("Unknown backend: {}", name),
    };

    let backend = if retry_policy.max_retries > 0 {
        Arc::new(RetryExecutor::new(inner, retry_policy)) as Arc<dyn Backend>
    } else {
        inner
    };

    // Write to unified cache; health stays None until warmup actually probes.
    // Distinguishing "not probed" (None) from "probed and unavailable" (Some(unavailable))
    // is what lets warmup_backends know to skip only entries that have already been probed.
    {
        let cache = get_backend_cache();
        let mut lock = cache.write().expect("backend cache lock poisoned");
        lock.insert(
            name.to_string(),
            CachedBackend {
                backend: Arc::clone(&backend),
                health: None,
            },
        );
    }

    Ok(backend)
}

pub fn create_claude_backend(config: &Config) -> Result<ClaudeBackend> {
    let backend_config = config
        .backends
        .get("claude")
        .ok_or_else(|| anyhow::anyhow!("Claude backend not configured"))?;
    ClaudeBackend::new(backend_config)
}

pub static BACKEND_CACHE: OnceLock<RwLock<HashMap<String, CachedBackend>>> = OnceLock::new();

pub fn get_backend_cache() -> &'static RwLock<HashMap<String, CachedBackend>> {
    BACKEND_CACHE.get_or_init(|| RwLock::new(HashMap::with_capacity(16)))
}

/// Combined cache entry linking a constructed backend instance with its health status.
/// Replaces separate CONSTRUCTED_BACKENDS and HEALTH_CACHE maps, ensuring consistency.
pub struct CachedBackend {
    pub backend: Arc<dyn Backend>,
    pub health: Option<HealthStatus>,
}

/// Minimal stub backend used by test helpers when the unified cache needs
/// a backend instance for mock entries (e.g., set_mock_health).
#[cfg(test)]
#[derive(Debug)]
pub struct StubBackend {
    pub(crate) name: String,
}

#[cfg(test)]
#[async_trait]
impl Backend for StubBackend {
    fn name(&self) -> &str {
        &self.name
    }
    async fn query(&self, _ctx: StepContext<'_>) -> std::result::Result<QueryOutput, BackendError> {
        unimplemented!("StubBackend does not support query")
    }
    fn is_available(&self) -> bool {
        false
    }
}

/// Helper to reset/clear all caches in tests
#[cfg(test)]
pub fn clear_health_cache() {
    if let Some(cache) = BACKEND_CACHE.get() {
        let mut lock = cache.write().expect("backend cache lock poisoned");
        lock.clear();
    }
}

/// Helper to insert a mock entry into the cache during tests
#[cfg(test)]
pub fn set_mock_health(backend_name: &str, status: HealthStatus) {
    let cache = get_backend_cache();
    let mut lock = cache.write().expect("backend cache lock poisoned");
    lock.entry(backend_name.to_string())
        .and_modify(|entry| entry.health = Some(status.clone()))
        .or_insert(CachedBackend {
            backend: Arc::new(StubBackend {
                name: backend_name.to_string(),
            }) as Arc<dyn Backend>,
            health: Some(status),
        });
}

pub struct Engine;

impl Engine {
    /// Warm up all enabled backends in parallel, populating the health cache.
    pub async fn warmup_backends(config: &Config) -> Result<()> {
        let mut futures = Vec::with_capacity(config.backends.len());

        for (name, backend_config) in &config.backends {
            if !backend_config.enabled {
                continue;
            }

            // Skip only if this backend has already been probed (health is Some).
            // Entries inserted by create_backend (e.g. via display_backends_status or
            // get_backends) have health = None and still need a real probe here.
            {
                let cache = get_backend_cache();
                let lock = cache.read().expect("backend cache lock poisoned");
                if let Some(entry) = lock.get(name.as_str()) {
                    if entry.health.is_some() {
                        continue;
                    }
                }
            } // lock dropped before cross-backend work

            let retry_policy = get_retry_policy(backend_config, &config.defaults);
            match create_backend(name, backend_config, retry_policy) {
                Ok(backend) => {
                    futures.push(async move {
                        let name = backend.name().to_string();
                        let res = backend.health_check().await;
                        (name, res)
                    });
                }
                Err(e) => {
                    eprintln!(
                        "{} Failed to construct backend {}: {}",
                        "warning:".yellow(),
                        name,
                        e
                    );
                }
            }
        }

        if futures.is_empty() {
            return Ok(());
        }

        let results = futures::future::join_all(futures).await;

        // Process results outside the write lock to minimize lock hold time
        // (eprintln! can block on I/O, so it runs before the lock)
        let mut updates: Vec<(String, HealthStatus)> = Vec::with_capacity(results.len());
        for (name, res) in results {
            match res {
                Ok(status) => {
                    updates.push((name, status));
                }
                Err(e) => {
                    eprintln!(
                        "{} Health check failed for backend {}: {}",
                        "warning:".yellow(),
                        name,
                        e
                    );
                    updates.push((name, HealthStatus::new_unavailable()));
                }
            }
        }

        // Update unified cache (backend was already inserted by create_backend)
        let cache = get_backend_cache();
        let mut lock = cache.write().expect("backend cache lock poisoned");
        for (name, status) in updates {
            if let Some(entry) = lock.get_mut(&name) {
                entry.health = Some(status);
            }
        }

        Ok(())
    }

    /// Check if a backend is available in the cache.
    /// Returns `false` immediately if the cache hasn't been initialized yet,
    /// avoiding unnecessary RwLock+HashMap allocation.
    pub fn is_backend_available(name: &str) -> bool {
        let Some(cache) = BACKEND_CACHE.get() else {
            return false;
        };
        let lock = cache.read().expect("backend cache lock poisoned");
        lock.get(name)
            .and_then(|c| c.health.as_ref())
            .map(|h| h.available)
            .unwrap_or(false)
    }
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
    let config = Arc::new(config.clone());
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
                     config: Arc<Config>,
                     pb: ProgressBar| async move {
        let backend_name = backend.name().to_string();
        pb.set_message(format!("Querying {}...", backend_name));

        let ctx = step_context_for_backend(&prompt, &cwd, &config, &backend_name);
        let timeout_duration = ctx
            .timeout
            .expect("step_context_for_backend always sets timeout");
        let timeout = timeout_duration.as_secs();

        let start = Instant::now();
        let result = tokio::time::timeout(timeout_duration, backend.query(ctx)).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        pb.inc(1);

        match result {
            Ok(Ok(query_output)) => QueryResult {
                backend: backend_name.clone(),
                output: query_output.stdout,
                success: true,
                elapsed_ms,
                error: None,
            },
            Ok(Err(e)) => QueryResult {
                backend: backend_name.clone(),
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
                    backend: backend_name,
                    output: format!("Error: {}", timeout_err),
                    success: false,
                    elapsed_ms,
                    error: Some(timeout_err),
                }
            }
        }
    };

    let results = if parallel {
        let futures: Vec<_> = backends
            .iter()
            .map(|backend| {
                query_one(
                    Arc::clone(backend),
                    Arc::clone(&prompt),
                    Arc::clone(&cwd),
                    Arc::clone(&config),
                    pb.clone(),
                )
            })
            .collect();
        join_all(futures).await
    } else {
        let mut results = Vec::new();
        for backend in backends {
            let result = query_one(
                Arc::clone(backend),
                Arc::clone(&prompt),
                Arc::clone(&cwd),
                Arc::clone(&config),
                pb.clone(),
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
pub static TEST_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
pub async fn acquire_test_lock() -> tokio::sync::MutexGuard<'static, ()> {
    TEST_MUTEX.lock().await
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── effective_timeout tests (FR-23) ──

    #[test]
    fn test_effective_timeout_step_overrides_all() {
        let config = Config::default();
        assert_eq!(
            effective_timeout(Some(Duration::from_secs(10)), "gemini", &config),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn test_effective_timeout_backend_overrides_global() {
        let mut config = Config::default();
        config.backends.get_mut("gemini").unwrap().timeout = Some(Duration::from_secs(60));
        assert_eq!(
            effective_timeout(None, "gemini", &config),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn test_effective_timeout_global_only() {
        let mut config = Config::default();
        config.defaults.timeout = Some(Duration::from_secs(30));
        assert_eq!(
            effective_timeout(None, "codex", &config),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn test_effective_timeout_fallback_default() {
        let config = Config::default();
        // None at every layer → DEFAULT_TIMEOUT (300s)
        assert_eq!(
            effective_timeout(None, "nonexistent-backend", &config),
            DEFAULT_TIMEOUT
        );
    }

    #[test]
    fn test_effective_timeout_zero_is_sentinel() {
        let mut config = Config::default();
        // Set global to 0 → maps to NO_TIMEOUT
        config.defaults.timeout = Some(Duration::from_secs(0));
        assert_eq!(effective_timeout(None, "codex", &config), NO_TIMEOUT);
    }

    #[test]
    fn test_effective_timeout_backend_absent_falls_through() {
        let mut config = Config::default();
        config.defaults.timeout = Some(Duration::from_secs(45));
        assert_eq!(
            effective_timeout(None, "unknown-backend", &config),
            Duration::from_secs(45)
        );
    }

    #[test]
    fn test_step_context_for_backend_uses_backend_model() {
        let mut config = Config::default();
        config
            .backends
            .get_mut("ollama")
            .expect("default ollama backend exists")
            .model = Some("custom-model".to_string());

        let cwd = Path::new("/tmp");
        let ctx = step_context_for_backend("hello", cwd, &config, "ollama");

        assert_eq!(ctx.prompt, "hello");
        assert_eq!(ctx.cwd, cwd);
        assert_eq!(ctx.model, Some("custom-model"));
    }

    #[test]
    fn test_step_context_for_backend_uses_backend_timeout() {
        let mut config = Config::default();
        config
            .backends
            .get_mut("ollama")
            .expect("default ollama backend exists")
            .timeout = Some(Duration::from_secs(42));

        let ctx = step_context_for_backend("hello", Path::new("/tmp"), &config, "ollama");

        assert_eq!(ctx.timeout, Some(Duration::from_secs(42)));
    }

    #[test]
    fn test_step_context_for_backend_falls_back_to_default_timeout() {
        let mut config = Config::default();
        config.defaults.timeout = Some(Duration::from_secs(17));
        config
            .backends
            .get_mut("ollama")
            .expect("default ollama backend exists")
            .timeout = None;

        let ctx = step_context_for_backend("hello", Path::new("/tmp"), &config, "ollama");

        assert_eq!(ctx.timeout, Some(Duration::from_secs(17)));
    }

    #[test]
    fn test_step_context_for_backend_preserves_phase1_defaults() {
        let config = Config::default();
        let ctx = step_context_for_backend("hello", Path::new("/tmp"), &config, "ollama");

        assert!(ctx.history.is_empty());
        assert!(ctx.sandbox.is_none());
        assert!(ctx.schema.is_none());
        assert!(ctx.options.is_none());
    }

    #[test]
    fn test_step_context_for_backend_preserves_zero_as_no_timeout() {
        let mut config = Config::default();
        config.defaults.timeout = Some(Duration::from_secs(0));
        config
            .backends
            .get_mut("ollama")
            .expect("default ollama backend exists")
            .timeout = None;

        let ctx = step_context_for_backend("hello", Path::new("/tmp"), &config, "ollama");

        assert_eq!(ctx.timeout, Some(Duration::from_secs(365 * 24 * 60 * 60)));
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordedContext {
        prompt: String,
        model: Option<String>,
        timeout: Option<Duration>,
    }

    struct RecordingBackend {
        observed: std::sync::Arc<std::sync::Mutex<Option<RecordedContext>>>,
    }

    #[async_trait]
    impl Backend for RecordingBackend {
        fn name(&self) -> &str {
            "ollama"
        }

        async fn query(
            &self,
            ctx: StepContext<'_>,
        ) -> std::result::Result<QueryOutput, BackendError> {
            *self.observed.lock().expect("recording mutex poisoned") = Some(RecordedContext {
                prompt: ctx.prompt.to_string(),
                model: ctx.model.map(str::to_string),
                timeout: ctx.timeout,
            });

            Ok(QueryOutput::from_text(
                "ok".to_string(),
                "ollama",
                Duration::ZERO,
            ))
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_run_query_with_config_passes_step_context_model_and_timeout() {
        let observed = std::sync::Arc::new(std::sync::Mutex::new(None));
        let backend: Arc<dyn Backend> = Arc::new(RecordingBackend {
            observed: Arc::clone(&observed),
        });
        let mut config = Config::default();
        config.defaults.parallel = false;
        let backend_config = config
            .backends
            .get_mut("ollama")
            .expect("default ollama backend exists");
        backend_config.model = Some("run-query-model".to_string());
        backend_config.timeout = Some(Duration::from_secs(13));

        let results = run_query_with_config(&[backend], "hello", Path::new("."), &config)
            .await
            .expect("run query succeeds");

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(results[0].output, "ok");
        assert_eq!(
            *observed.lock().expect("recording mutex poisoned"),
            Some(RecordedContext {
                prompt: "hello".to_string(),
                model: Some("run-query-model".to_string()),
                timeout: Some(Duration::from_secs(13)),
            })
        );
    }

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
        let output =
            QueryOutput::from_process("".to_string(), "".to_string(), 0, "test", Duration::ZERO);
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
    fn test_token_usage_saturating_add() {
        let a = TokenUsage::new(100, 200);
        let b = TokenUsage::new(50, 75);
        let sum = a.saturating_add(&b);
        assert_eq!(sum.prompt_tokens, 150);
        assert_eq!(sum.completion_tokens, 275);
        assert_eq!(sum.total_tokens, 425);

        let big = TokenUsage::new(u32::MAX, u32::MAX);
        let overflow = big.saturating_add(&TokenUsage::new(1, 1));
        assert_eq!(overflow.prompt_tokens, u32::MAX);
        assert_eq!(overflow.completion_tokens, u32::MAX);
        assert_eq!(overflow.total_tokens, u32::MAX);
    }

    #[test]
    fn test_token_usage_new_defaults_new_optionals_to_none() {
        let usage = TokenUsage::new(10, 20);
        assert_eq!(usage.cached_tokens, None);
        assert_eq!(usage.reasoning_tokens, None);
    }

    #[test]
    fn test_token_usage_default_is_all_zero_and_none() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.cached_tokens, None);
        assert_eq!(usage.reasoning_tokens, None);
    }

    #[test]
    fn test_token_usage_with_cached_sets_field() {
        let usage = TokenUsage::new(10, 20).with_cached(Some(7));
        assert_eq!(usage.cached_tokens, Some(7));
        assert_eq!(usage.prompt_tokens, 10);
    }

    #[test]
    fn test_token_usage_with_reasoning_sets_field() {
        let usage = TokenUsage::new(10, 20).with_reasoning(Some(13));
        assert_eq!(usage.reasoning_tokens, Some(13));
        assert_eq!(usage.completion_tokens, 20);
    }

    #[test]
    fn test_token_usage_with_cached_none_is_idempotent() {
        let usage = TokenUsage::new(10, 20)
            .with_cached(Some(7))
            .with_cached(None);
        assert_eq!(usage.cached_tokens, None);
    }

    #[test]
    fn test_token_usage_total_excludes_cached_and_reasoning() {
        let usage = TokenUsage::new(100, 50)
            .with_cached(Some(40))
            .with_reasoning(Some(20));
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_token_usage_saturating_add_folds_optionals() {
        let a = TokenUsage::new(10, 20).with_cached(Some(5));
        let b = TokenUsage::new(3, 4).with_cached(Some(7));
        let sum = a.saturating_add(&b);
        assert_eq!(sum.cached_tokens, Some(12));

        let sum_none_left = a.saturating_add(&TokenUsage::new(1, 2));
        assert_eq!(sum_none_left.cached_tokens, Some(5));

        let sum_none_right = TokenUsage::new(1, 2).saturating_add(&a);
        assert_eq!(sum_none_right.cached_tokens, Some(5));

        let sum_none_none = TokenUsage::new(1, 2).saturating_add(&TokenUsage::new(3, 4));
        assert_eq!(sum_none_none.cached_tokens, None);

        // reasoning_tokens follows same logic
        let ra = TokenUsage::new(10, 20).with_reasoning(Some(5));
        let rb = TokenUsage::new(3, 4).with_reasoning(Some(7));
        assert_eq!(ra.saturating_add(&rb).reasoning_tokens, Some(12));

        let rsum_none_left = ra.saturating_add(&TokenUsage::new(1, 2));
        assert_eq!(rsum_none_left.reasoning_tokens, Some(5));

        let rsum_none_right = TokenUsage::new(1, 2).saturating_add(&ra);
        assert_eq!(rsum_none_right.reasoning_tokens, Some(5));

        let rsum_none_none = TokenUsage::new(1, 2).saturating_add(&TokenUsage::new(3, 4));
        assert_eq!(rsum_none_none.reasoning_tokens, None);
    }

    #[test]
    fn test_token_usage_saturating_add_clamps_optional_overflow() {
        let a = TokenUsage::new(0, 0).with_cached(Some(u32::MAX));
        let sum = a.saturating_add(&TokenUsage::new(0, 0).with_cached(Some(1)));
        assert_eq!(sum.cached_tokens, Some(u32::MAX));

        let ra = TokenUsage::new(0, 0).with_reasoning(Some(u32::MAX));
        let rsum = ra.saturating_add(&TokenUsage::new(0, 0).with_reasoning(Some(1)));
        assert_eq!(rsum.reasoning_tokens, Some(u32::MAX));
    }

    #[test]
    fn test_token_usage_saturating_add_preserves_total_invariant() {
        let a = TokenUsage::new(10, 20);
        let b = TokenUsage::new(3, 4)
            .with_cached(Some(1))
            .with_reasoning(Some(2));
        let sum = a.saturating_add(&b);
        // total_tokens is prompt + completion only; cached/reasoning don't leak in
        assert_eq!(sum.prompt_tokens, 13);
        assert_eq!(sum.completion_tokens, 24);
        assert_eq!(sum.total_tokens, 37);
        assert_eq!(sum.cached_tokens, Some(1));
        assert_eq!(sum.reasoning_tokens, Some(2));
    }

    #[test]
    fn test_query_output_from_text_populates_backend_and_duration() {
        let output = QueryOutput::from_text("ok".to_string(), "claude", Duration::from_millis(100));
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
                ..Default::default()
            })
        );
    }

    #[test]
    fn test_query_output_with_usage_none() {
        let output =
            QueryOutput::from_text("ok".to_string(), "claude", Duration::ZERO).with_usage(None);
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

    struct HealthCheckBackend {
        available: bool,
    }

    #[async_trait]
    impl Backend for HealthCheckBackend {
        fn name(&self) -> &str {
            "health-check-mock"
        }
        async fn query(
            &self,
            _ctx: StepContext<'_>,
        ) -> std::result::Result<QueryOutput, BackendError> {
            Ok(QueryOutput::from_text(
                "ok".into(),
                "health-check-mock",
                Duration::from_secs(0),
            ))
        }
        fn is_available(&self) -> bool {
            self.available
        }
        // Deliberately NOT overriding health_check — using default impl
    }

    #[tokio::test]
    async fn test_health_check_default_returns_ok_when_available() {
        let backend = HealthCheckBackend { available: true };
        let result = backend.health_check().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_health_check_default_returns_err_when_unavailable() {
        let backend = HealthCheckBackend { available: false };
        let result = backend.health_check().await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            BackendError::Unavailable { .. }
        ));
    }

    #[tokio::test]
    async fn test_health_status_constructors() {
        let _guard = acquire_test_lock().await;
        let available = HealthStatus::new_available();
        assert!(available.available);
        assert!(available.version.is_none());
        assert!(available.unusable_flags.is_empty());
        assert!(available.models.is_empty());

        let unavailable = HealthStatus::new_unavailable();
        assert!(!unavailable.available);
        assert!(unavailable.version.is_none());
        assert!(unavailable.unusable_flags.is_empty());
        assert!(unavailable.models.is_empty());

        // Verify they round-trip through cache correctly
        set_mock_health("test-avail", HealthStatus::new_available());
        assert!(super::Engine::is_backend_available("test-avail"));

        set_mock_health("test-unavail", HealthStatus::new_unavailable());
        assert!(!super::Engine::is_backend_available("test-unavail"));
    }

    #[tokio::test]
    async fn test_is_available_returns_false_for_empty_cache() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();
        assert!(!super::Engine::is_backend_available("nonexistent"));
        assert!(!super::Engine::is_backend_available("ollama"));
        assert!(!super::Engine::is_backend_available(""));
    }

    #[tokio::test]
    async fn test_health_cache_basic_read_write() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();
        assert!(!super::Engine::is_backend_available("test-backend"));

        set_mock_health("test-backend", HealthStatus::new_available());
        assert!(super::Engine::is_backend_available("test-backend"));

        clear_health_cache();
        assert!(!super::Engine::is_backend_available("test-backend"));
    }

    #[tokio::test]
    async fn test_is_available_cache_only_no_syscalls() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        struct MockSyscallBackend {
            probe_counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        }

        #[async_trait]
        impl Backend for MockSyscallBackend {
            fn name(&self) -> &str {
                "mock-syscall"
            }
            async fn query(
                &self,
                _ctx: StepContext<'_>,
            ) -> std::result::Result<QueryOutput, BackendError> {
                unimplemented!()
            }
            fn is_available(&self) -> bool {
                super::Engine::is_backend_available(self.name())
            }
            async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> {
                self.probe_counter
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(HealthStatus::new_available())
            }
        }

        let probe_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let backend = MockSyscallBackend {
            probe_counter: probe_counter.clone(),
        };

        // Before warmup, it must return false, and NO probe should have been executed.
        assert!(!backend.is_available());
        assert_eq!(probe_counter.load(std::sync::atomic::Ordering::SeqCst), 0);

        // Set mock health directly. is_available should now be true, and still NO probe executed (no syscalls).
        set_mock_health("mock-syscall", HealthStatus::new_available());
        assert!(backend.is_available());
        assert_eq!(probe_counter.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_warmup_backends_parallel() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        // Enable ollama
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );

        super::Engine::warmup_backends(&config).await.unwrap();

        // Assert that ollama is now available in cache
        assert!(super::Engine::is_backend_available("ollama"));
    }

    #[tokio::test]
    async fn test_warmup_backends_idempotence() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );

        // Run warmup first time — ollama is probed and recorded as available.
        super::Engine::warmup_backends(&config).await.unwrap();
        assert!(super::Engine::is_backend_available("ollama"));

        // Overwrite with Some(unavailable) — simulating a probed-but-unhealthy entry.
        set_mock_health("ollama", HealthStatus::new_unavailable());
        assert!(!super::Engine::is_backend_available("ollama"));

        // Run warmup second time. Since ollama's health is Some(_) (already probed),
        // warmup skips it — status stays unavailable and is NOT re-probed.
        super::Engine::warmup_backends(&config).await.unwrap();
        assert!(!super::Engine::is_backend_available("ollama"));

        // Conversely, if we reset health back to None (unprobed), warmup MUST re-probe.
        // This guards against the CONSTRUCTED_BACKENDS-era bug where a pre-populated
        // entry from create_backend would cause warmup to skip and leave the backend
        // marked unavailable forever.
        {
            let cache = get_backend_cache();
            let mut lock = cache.write().expect("backend cache lock poisoned");
            lock.get_mut("ollama")
                .expect("ollama should be cached")
                .health = None;
        }
        assert!(!super::Engine::is_backend_available("ollama"));
        super::Engine::warmup_backends(&config).await.unwrap();
        assert!(
            super::Engine::is_backend_available("ollama"),
            "warmup must re-probe entries whose health is None"
        );
    }

    #[tokio::test]
    async fn test_warmup_backends_empty() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.clear();
        super::Engine::warmup_backends(&config).await.unwrap();

        // Assert that nothing was populated in the cache
        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        assert!(lock.is_empty());
    }

    #[tokio::test]
    async fn test_warmup_lifecycle_roundtrip() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );

        // Step 1: warmup → ollama should be available
        super::Engine::warmup_backends(&config).await.unwrap();
        assert!(super::Engine::is_backend_available("ollama"));

        // Step 2: clear health cache only → ollama should NOT be available
        // (constructed backends remain intact, but health status is gone)
        if let Some(cache) = BACKEND_CACHE.get() {
            let mut lock = cache.write().expect("backend cache lock poisoned");
            lock.clear();
        }
        assert!(!super::Engine::is_backend_available("ollama"));

        // Step 3: warmup again → ollama should become available again
        super::Engine::warmup_backends(&config).await.unwrap();
        assert!(super::Engine::is_backend_available("ollama"));

        // Step 4: get_backends should include ollama
        let backends = super::get_backends(&config, None).unwrap();
        let names: Vec<&str> = backends.iter().map(|b| b.name()).collect();
        assert!(
            names.contains(&"ollama"),
            "Expected ollama in get_backends result, got: {:?}",
            names
        );
    }

    #[tokio::test]
    async fn test_warmup_populates_unified_cache() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );

        // Before warmup, unified cache should not have ollama yet
        let pre_cache = get_backend_cache();
        assert!(!pre_cache
            .read()
            .expect("lock poisoned")
            .contains_key("ollama"));

        super::Engine::warmup_backends(&config).await.unwrap();

        // After warmup, ollama should be in BACKEND_CACHE
        let cache = get_backend_cache();
        let lock = cache.read().expect("lock poisoned");
        assert!(
            lock.contains_key("ollama"),
            "Expected ollama in BACKEND_CACHE after warmup"
        );

        // Verify the cached backend reports the same name
        if let Some(entry) = lock.get("ollama") {
            assert_eq!(entry.backend.name(), "ollama");
        }
    }

    #[tokio::test]
    async fn test_warmup_backends_mixed_enabled_disabled() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        // Start with empty backends to control what gets warmed up
        config.backends.clear();

        // Add ollama as enabled
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );
        // Add claude as disabled
        config.backends.insert(
            "claude".to_string(),
            crate::config::BackendConfig {
                enabled: false,
                command: Some("echo".to_string()),
                args: vec!["hello".to_string()],
                ..Default::default()
            },
        );

        super::Engine::warmup_backends(&config).await.unwrap();

        // ollama should be available (it was enabled and health-checked)
        assert!(
            super::Engine::is_backend_available("ollama"),
            "ollama should be available after warmup"
        );

        // claude should NOT be in the health cache (it was disabled)
        // Note: is_backend_available returns false for any backend not in the cache
        assert!(
            !super::Engine::is_backend_available("claude"),
            "claude (disabled) should not be available after warmup"
        );

        // Also verify cache only has exactly one entry (for ollama)
        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        assert_eq!(lock.len(), 1, "Expected exactly 1 cached health status");
        assert!(lock.contains_key("ollama"), "Cache should contain ollama");
        assert!(
            !lock.contains_key("claude"),
            "Cache should NOT contain claude"
        );
    }

    #[tokio::test]
    async fn test_warmup_backends_health_check_failure() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.clear();
        // Add a backend configured with a non-existent command so health_check fails
        config.backends.insert(
            "gemini".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                command: Some("nonexistent-health-check-binary".to_string()),
                args: vec!["--version".to_string()],
                ..Default::default()
            },
        );

        // Warmup should handle the health check failure gracefully
        super::Engine::warmup_backends(&config).await.unwrap();

        // Backend should be marked as unavailable in the cache
        assert!(
            !super::Engine::is_backend_available("gemini"),
            "gemini should be unavailable after failed health check"
        );

        // Verify the cache has the entry with available = false
        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        let status = lock.get("gemini").expect("gemini should be in cache");
        let health = status
            .health
            .as_ref()
            .expect("gemini should have been probed by warmup");
        assert!(
            !health.available,
            "gemini health status should be unavailable"
        );
    }

    #[tokio::test]
    async fn test_codex_health_check_cached() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut script = tempfile::NamedTempFile::with_suffix(".sh").unwrap();
        let path = script.path().to_path_buf();
        std::io::Write::write_all(&mut script, b"#!/bin/sh\necho 'codex-cli 0.118.0'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }

        let mut config = Config::default();
        config.backends.clear();
        config.backends.insert(
            "codex".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                command: Some(path.to_string_lossy().into_owned()),
                ..Default::default()
            },
        );

        super::Engine::warmup_backends(&config).await.unwrap();

        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        let status = lock.get("codex").expect("codex should be in cache");
        let health = status
            .health
            .as_ref()
            .expect("codex should have been probed by warmup");
        assert_eq!(health.version, Some("0.118.0".to_string()));
        assert_eq!(
            health.unusable_flags,
            vec![
                "--output-schema",
                "-o",
                "--ephemeral",
                "--ignore-user-config",
                "--ignore-rules"
            ]
        );
    }

    #[tokio::test]
    async fn test_warmup_unknown_backend_skipped() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.clear();
        // Add a real backend that will succeed
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );
        // Add an unknown backend that create_backend will reject
        config.backends.insert(
            "nonexistent-backend-name".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                command: Some("echo".to_string()),
                ..Default::default()
            },
        );

        // Warmup should handle the unknown backend gracefully
        // (print warning, skip it) and still warm up ollama
        super::Engine::warmup_backends(&config).await.unwrap();

        // ollama should be available
        assert!(super::Engine::is_backend_available("ollama"));

        // Unknown backend should not be in the cache
        assert!(!super::Engine::is_backend_available(
            "nonexistent-backend-name"
        ));

        // Verify the cache only has ollama
        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        assert_eq!(lock.len(), 1, "Only ollama should be cached");
        assert!(lock.contains_key("ollama"));
    }

    #[tokio::test]
    async fn test_clear_health_cache_idempotent() {
        let _guard = acquire_test_lock().await;
        // Clear when both caches are not yet initialized
        clear_health_cache();
        clear_health_cache();
        clear_health_cache();

        // After triple-clear, cache should be empty, is_backend_available returns false
        assert!(!super::Engine::is_backend_available("anything"));

        // Now populate and clear again
        set_mock_health("test", HealthStatus::new_available());
        assert!(super::Engine::is_backend_available("test"));

        clear_health_cache();
        assert!(!super::Engine::is_backend_available("test"));

        // Double-clear after population
        clear_health_cache();
        assert!(!super::Engine::is_backend_available("test"));
    }

    #[tokio::test]
    async fn test_get_backends_with_filter() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        // Setup: warmup multiple backends
        let mut config = Config::default();
        config.backends.clear();
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );
        config.backends.insert(
            "gemini".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                command: Some("echo".to_string()),
                args: vec!["hello".to_string()],
                ..Default::default()
            },
        );

        super::Engine::warmup_backends(&config).await.unwrap();

        // Filter for ollama only
        let backends = super::get_backends(&config, Some("ollama")).unwrap();
        assert_eq!(backends.len(), 1, "Expected 1 backend with ollama filter");
        assert_eq!(backends[0].name(), "ollama");

        // Filter for both backends
        let backends = super::get_backends(&config, Some("ollama,gemini")).unwrap();
        assert_eq!(
            backends.len(),
            2,
            "Expected 2 backends with ollama,gemini filter"
        );
        let names: Vec<&str> = backends.iter().map(|b| b.name()).collect();
        assert!(names.contains(&"ollama"));
        assert!(names.contains(&"gemini"));

        // No filter returns all available backends
        let backends = super::get_backends(&config, None).unwrap();
        assert!(!backends.is_empty());
    }

    #[tokio::test]
    async fn test_get_backends_no_available_bails() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.clear();
        // Add a backend that will fail health check
        config.backends.insert(
            "gemini".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                command: Some("nonexistent-binary-that-NOT-exists".to_string()),
                ..Default::default()
            },
        );

        // Warmup marks it unavailable
        super::Engine::warmup_backends(&config).await.unwrap();
        assert!(!super::Engine::is_backend_available("gemini"));

        // get_backends should bail since no backends are available
        let result = super::get_backends(&config, None);
        assert!(
            result.is_err(),
            "Expected get_backends to return error when no backends available"
        );
        // Verify the error message mentions "No backends available"
        match result {
            Err(e) => {
                let msg = format!("{}", e);
                assert!(
                    msg.contains("No backends available"),
                    "Expected 'No backends available' error, got: {}",
                    msg
                );
            }
            Ok(_) => unreachable!(),
        }
    }

    #[tokio::test]
    async fn test_unified_cache_populated_by_create_backend() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        // Call create_backend which should populate BACKEND_CACHE
        let mut config = Config::default();
        config.backends.clear();
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );
        let retry_policy = crate::backend::get_retry_policy(
            config.backends.get("ollama").unwrap(),
            &config.defaults,
        );
        let _backend = super::create_backend(
            "ollama",
            config.backends.get("ollama").unwrap(),
            retry_policy,
        )
        .unwrap();

        // BACKEND_CACHE should have the backend with no health probed yet (None);
        // create_backend deliberately leaves health unset so warmup can detect entries
        // that still need to be probed.
        let cache = get_backend_cache();
        let lock = cache.read().expect("lock poisoned");
        let entry = lock
            .get("ollama")
            .expect("ollama should be in BACKEND_CACHE");
        assert!(
            entry.health.is_none(),
            "create_backend should leave health unprobed (None) so warmup will probe it"
        );
        assert_eq!(entry.backend.name(), "ollama");

        // Verify health status round-trips through the unified cache via set_mock_health
        drop(lock);
        clear_health_cache();
        set_mock_health("ollama", HealthStatus::new_available());
        assert!(super::Engine::is_backend_available("ollama"));

        set_mock_health("ollama", HealthStatus::new_unavailable());
        assert!(!super::Engine::is_backend_available("ollama"));

        // Verify the cache only has one entry
        let lock = cache.read().expect("lock poisoned");
        assert_eq!(lock.len(), 1);
    }

    #[tokio::test]
    async fn test_warmup_default_config() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        // Use the full default config which has codex, gemini, claude, ollama
        let config = Config::default();

        // Warmup with the full default config
        super::Engine::warmup_backends(&config).await.unwrap();

        // At minimum, ollama should be available (it's always enabled and present)
        assert!(super::Engine::is_backend_available("ollama"));

        // get_backends should return at least 1 backend
        let backends = super::get_backends(&config, None).unwrap();
        assert!(
            !backends.is_empty(),
            "Expected at least one available backend"
        );

        // Verify the health cache contains entries for all enabled backends
        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        for (name, cfg) in &config.backends {
            if cfg.enabled {
                assert!(
                    lock.contains_key(name),
                    "Enabled backend '{}' should be in health cache after warmup",
                    name
                );
            }
        }
    }

    #[tokio::test]
    async fn test_warmup_mixed_precached_and_new_backends() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        // Pre-populate unified cache with gemini (already available)
        // so warmup skips re-checking it
        set_mock_health("gemini", HealthStatus::new_available());

        let mut config = Config::default();
        config.backends.clear();
        // ollama is NOT in unified cache → needs health check
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );
        // gemini is in unified cache already → skip by warmup
        config.backends.insert(
            "gemini".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                command: Some("echo".to_string()),
                args: vec!["hello".to_string()],
                ..Default::default()
            },
        );

        // Before warmup: gemini is available (mock), ollama is not
        assert!(super::Engine::is_backend_available("gemini"));
        assert!(!super::Engine::is_backend_available("ollama"));

        // Warmup should skip gemini (already cached) and health-check ollama
        super::Engine::warmup_backends(&config).await.unwrap();

        // After warmup: both should be available
        assert!(
            super::Engine::is_backend_available("ollama"),
            "ollama should be available after warmup health check"
        );
        assert!(
            super::Engine::is_backend_available("gemini"),
            "gemini should still be available (was pre-cached)"
        );

        // Verify warmup didn't re-check gemini
        let cache = get_backend_cache();
        let lock = cache.read().expect("locked");
        assert_eq!(lock.len(), 2, "Both backends should be in health cache");
    }

    #[tokio::test]
    async fn test_set_mock_health_overwrites_existing() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        // Set initial health and verify
        set_mock_health("test", HealthStatus::new_available());
        assert!(super::Engine::is_backend_available("test"));

        // Overwrite with unavailable and verify
        set_mock_health("test", HealthStatus::new_unavailable());
        assert!(!super::Engine::is_backend_available("test"));

        // Overwrite back to available and verify
        set_mock_health("test", HealthStatus::new_available());
        assert!(super::Engine::is_backend_available("test"));

        // Verify only one entry exists (overwrite, not duplicate)
        let cache = get_backend_cache();
        let lock = cache.read().expect("locked");
        assert_eq!(lock.len(), 1, "Expected exactly 1 entry after overwrites");
    }

    #[tokio::test]
    async fn test_warmup_batch_writes_all_results() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let mut config = Config::default();
        config.backends.clear();
        // Add a backend that will succeed health check (ollama)
        config.backends.insert(
            "ollama".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                ..Default::default()
            },
        );
        // Add a backend that will fail health check (nonexistent command)
        config.backends.insert(
            "gemini".to_string(),
            crate::config::BackendConfig {
                enabled: true,
                command: Some("nonexistent-health-binary".to_string()),
                args: vec!["check".to_string()],
                ..Default::default()
            },
        );

        super::Engine::warmup_backends(&config).await.unwrap();

        // Both should be in the cache
        let cache = get_backend_cache();
        let lock = cache.read().expect("locked");
        assert_eq!(lock.len(), 2, "Both backends should be cached after warmup");

        // ollama should be available
        assert!(
            lock.get("ollama")
                .and_then(|s| s.health.as_ref())
                .map(|h| h.available)
                .unwrap_or(false),
            "ollama should be available"
        );

        // gemini should be probed and unavailable (health check failed).
        // Some(unavailable) and None must be distinguished here — None would mean warmup
        // never wrote a result, which is the bug Option<HealthStatus> exists to prevent.
        let gemini_health = lock
            .get("gemini")
            .and_then(|s| s.health.as_ref())
            .expect("gemini should have been probed and recorded as unavailable");
        assert!(!gemini_health.available, "gemini should be unavailable");
    }

    #[test]
    fn test_backend_error_unavailable_format() {
        let err = BackendError::Unavailable {
            message: "test backend not found".to_string(),
        };
        let display = format!("{}", err);
        assert_eq!(display, "unavailable: test backend not found");

        // Verify it's NOT retryable
        assert!(!err.is_retryable());

        // Verify Timeout IS retryable
        let timeout = BackendError::Timeout {
            message: "timeout".to_string(),
            elapsed_ms: 5000,
        };
        assert!(timeout.is_retryable());
        let display = format!("{}", timeout);
        assert_eq!(display, "timeout: timeout");

        // Verify Network IS retryable
        let network = BackendError::Network {
            message: "connection refused".to_string(),
        };
        assert!(network.is_retryable());

        // Verify Config is NOT retryable
        let config_err = BackendError::Config {
            message: "bad config".to_string(),
        };
        assert!(!config_err.is_retryable());
    }

    #[tokio::test]
    async fn test_retry_wrapper_delegates_health_check() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        // Create a backend with retry and verify health_check still works
        let inner = Arc::new(
            crate::backend::ollama::OllamaBackend::new(&BackendConfig {
                enabled: true,
                ..Default::default()
            })
            .unwrap(),
        );
        let retry_policy = crate::backend::retry::RetryPolicy {
            max_retries: 3,
            base_delay: std::time::Duration::from_millis(10),
            max_delay: std::time::Duration::from_millis(100),
        };
        let wrapped = crate::backend::retry::RetryExecutor::new(inner.clone(), retry_policy);

        // RetryWrapper's health_check should delegate to inner
        let _status = wrapped.health_check().await.unwrap();

        // Verify is_available() also delegates (cache-only at this point)
        assert_eq!(wrapped.is_available(), inner.is_available());
    }
}
