/// Provider-agnostic configuration accepted by every backend.
pub mod config;

#[cfg(feature = "bedrock")]
/// AWS Bedrock backend. Requires the `bedrock` feature.
pub mod bedrock;
/// Anthropic Claude backend, in API or CLI mode.
pub mod claude;
/// OpenAI Codex CLI backend.
pub mod codex;
mod codex_event;
/// Per-call inputs and health/capability reporting types.
pub mod context;
/// Google Gemini backend, via the `opencode` CLI.
pub mod gemini;
/// Local Ollama backend, over its HTTP API.
pub mod ollama;
mod retry;

pub use config::{BackendConfig, RetryDefaults};

#[cfg(feature = "bedrock")]
#[allow(unused_imports)]
pub use bedrock::BedrockBackend;
pub use claude::ClaudeBackend;
pub use codex::CodexBackend;
#[allow(unused_imports)]
pub use context::{HealthStatus, Message, ModelInfo, Role, SandboxMode, StepContext, StepOptions};
pub use gemini::GeminiBackend;
pub use ollama::OllamaBackend;
pub use retry::{RetryExecutor, RetryPolicy};

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

/// Typed backend errors replacing opaque `anyhow::Error` from `Backend::query()`.
/// Each variant represents a distinct failure mode that callers can match on
/// for retry decisions, user-facing messages, and error classification.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BackendError {
    /// The backend did not answer within its effective timeout.
    #[error("timeout: {message}")]
    Timeout {
        /// What timed out.
        message: String,
        /// How long the call ran before giving up.
        elapsed_ms: u64,
    },

    /// The provider refused the request for rate-limit reasons.
    #[error("rate limited: {message}")]
    RateLimit {
        /// What the provider reported.
        message: String,
        /// Server-supplied backoff, honoured in preference to the retry policy
        /// when present.
        retry_after_ms: Option<u64>,
    },

    /// Credentials were missing, malformed or rejected.
    #[error("auth: {message}")]
    Auth {
        /// What failed to authenticate.
        message: String,
    },

    /// The provider could not be reached.
    #[error("network: {message}")]
    Network {
        /// The underlying transport failure.
        message: String,
    },

    /// The backend answered, but its response could not be decoded.
    #[error("parse: {message}")]
    Parse {
        /// What could not be parsed.
        message: String,
    },

    /// A subprocess-backed backend ran and failed.
    #[error("execution failed: {message}")]
    ExecutionFailed {
        /// What the subprocess reported.
        message: String,
        /// Process exit code, when one was produced.
        exit_code: Option<i32>,
    },

    /// The backend is not usable at all, e.g. its binary is missing.
    #[error("unavailable: {message}")]
    Unavailable {
        /// Why the backend is unavailable.
        message: String,
    },

    /// The supplied [`BackendConfig`] cannot drive this backend.
    #[error("config: {message}")]
    Config {
        /// What is wrong with the configuration.
        message: String,
    },
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
    /// Tokens in the request.
    pub prompt_tokens: u32,
    /// Tokens in the response.
    pub completion_tokens: u32,
    /// `prompt_tokens + completion_tokens`, saturating.
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

    /// Sum two usage records field by field, saturating rather than
    /// overflowing. `None` and `Some` counts combine as `None + x = x`.
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
    /// The backend's answer.
    pub stdout: String,
    /// Diagnostics from a subprocess-backed backend, when it produced any.
    pub stderr: Option<String>,
    /// Process exit code for subprocess-backed backends.
    pub exit_code: Option<i32>,
    /// Model that actually served the request, as reported by the backend.
    pub model: Option<String>,
    /// Wall-clock time for the call.
    pub duration: Duration,
    /// Token counts, when the backend reports them.
    pub usage: Option<TokenUsage>,
    /// Decoded JSON when the call requested structured output via
    /// [`StepContext::schema`].
    pub structured: Option<serde_json::Value>,
    /// Name of the backend that produced this output.
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

/// A provider that can answer a query.
///
/// Implementors are `Send + Sync` and are normally handled as
/// `Arc<dyn Backend>`, which is what [`create_backend`] returns.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Stable identifier for this backend, e.g. `"claude"` or `"ollama"`.
    fn name(&self) -> &str;
    /// Run one query.
    ///
    /// # Errors
    /// Returns a [`BackendError`] describing the failure mode. Check
    /// [`BackendError::is_retryable`] to decide whether retrying is sensible,
    /// or wrap the backend in a [`RetryExecutor`] to handle that for you.
    async fn query(&self, ctx: StepContext<'_>) -> std::result::Result<QueryOutput, BackendError>;
    /// Cheap, synchronous availability check answered from cache. Performs no
    /// I/O; call [`Backend::health_check`] to probe for real.
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

/// Construct a backend by name, wrapped in a [`RetryExecutor`].
///
/// Recognised names are `"codex"`, `"gemini"`, `"claude"`, `"ollama"`, and
/// `"bedrock"` under the `bedrock` feature.
///
/// # Caching
/// Instances are memoised in [`BACKEND_CACHE`], a process-global keyed by name
/// alone. Two callers in one process asking for the same name with different
/// configurations therefore share the first instance built. See the
/// `BACKEND_CACHE` documentation for why this is not yet keyed by config.
///
/// # Errors
/// Returns an error when `name` is unknown or the configuration cannot build
/// that backend.
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
                checked_at: None,
            },
        );
    }

    Ok(backend)
}

/// Process-global cache of constructed backends and their health, keyed by
/// backend name.
///
/// # Known constraint
/// The key is the backend name alone, so two consumers in one process using
/// the same name with different configurations silently share one instance.
///
/// Keying by configuration hash, or moving the cache out of the library, would
/// both break [`is_backend_available`], which looks entries up by name from
/// inside each provider's `is_available` implementation and has no
/// configuration to hash. Fixing this properly means reworking `is_available`;
/// tracked as a follow-up rather than done here.
pub static BACKEND_CACHE: OnceLock<RwLock<HashMap<String, CachedBackend>>> = OnceLock::new();

/// Access [`BACKEND_CACHE`], initialising it on first use.
pub fn get_backend_cache() -> &'static RwLock<HashMap<String, CachedBackend>> {
    BACKEND_CACHE.get_or_init(|| RwLock::new(HashMap::with_capacity(16)))
}

/// Combined cache entry linking a constructed backend instance with its health status.
/// Replaces separate CONSTRUCTED_BACKENDS and HEALTH_CACHE maps, ensuring consistency.
pub struct CachedBackend {
    /// The constructed backend instance.
    pub backend: Arc<dyn Backend>,
    /// Result of the last health probe, if one has run.
    pub health: Option<HealthStatus>,
    /// When that probe ran, for TTL comparison.
    pub checked_at: Option<Instant>,
}

/// Minimal stub backend used by test helpers when the unified cache needs
/// a backend instance for mock entries (e.g., set_mock_health).
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct StubBackend {
    /// Name this stub reports from [`Backend::name`].
    pub name: String,
}

#[cfg(any(test, feature = "test-support"))]
impl StubBackend {
    /// Build a stub reporting `name`. Querying it panics; it exists only to
    /// occupy the backend slot of a cache entry.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[cfg(any(test, feature = "test-support"))]
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
#[cfg(any(test, feature = "test-support"))]
pub fn clear_health_cache() {
    if let Some(cache) = BACKEND_CACHE.get() {
        let mut lock = cache.write().expect("backend cache lock poisoned");
        lock.clear();
    }
}

/// Helper to insert a mock entry into the cache during tests
#[cfg(any(test, feature = "test-support"))]
pub fn set_mock_health(backend_name: &str, status: HealthStatus) {
    let cache = get_backend_cache();
    let mut lock = cache.write().expect("backend cache lock poisoned");
    let now = Some(Instant::now());
    lock.entry(backend_name.to_string())
        .and_modify(|entry| {
            entry.health = Some(status.clone());
            entry.checked_at = now;
        })
        .or_insert(CachedBackend {
            backend: Arc::new(StubBackend::new(backend_name.to_string())) as Arc<dyn Backend>,
            health: Some(status),
            checked_at: now,
        });
}
/// How long a health probe stays fresh when `LOK_HEALTH_TTL` is unset.
pub const DEFAULT_HEALTH_CACHE_TTL: Duration = Duration::from_secs(60 * 30);
/// Environment variable overriding [`DEFAULT_HEALTH_CACHE_TTL`]. Accepts a
/// humantime duration such as `"5m"`.
pub const HEALTH_TTL_ENV: &str = "LOK_HEALTH_TTL";

static HEALTH_CACHE_TTL: OnceLock<Duration> = OnceLock::new();
/// Latch ensuring the resolved TTL is announced at most once per process.
pub static HEALTH_TTL_LOGGED: OnceLock<()> = OnceLock::new();

/// Parse a [`HEALTH_TTL_ENV`] value into a TTL, plus a warning when the input
/// was present but unusable and the default was substituted.
pub fn parse_health_cache_ttl(val: Option<&str>) -> (Duration, Option<String>) {
    match val {
        Some(v) if !v.trim().is_empty() => {
            let trimmed = v.trim();
            match humantime::parse_duration(trimmed) {
                Ok(d) => (d, None),
                Err(e) => {
                    let warn = format!(
                        "Invalid {} '{}': {}; using default TTL ({})",
                        HEALTH_TTL_ENV,
                        trimmed,
                        e,
                        humantime::format_duration(DEFAULT_HEALTH_CACHE_TTL),
                    );
                    (DEFAULT_HEALTH_CACHE_TTL, Some(warn))
                }
            }
        }
        _ => (DEFAULT_HEALTH_CACHE_TTL, None),
    }
}

pub(crate) fn resolve_health_cache_ttl() -> Duration {
    let val = std::env::var_os(HEALTH_TTL_ENV);
    let val_str = val.as_ref().map(|os| os.to_string_lossy());
    let (ttl, warn) = parse_health_cache_ttl(val_str.as_deref());
    if let Some(w) = warn {
        log::warn!("{}", w);
    }
    ttl
}

/// The process-wide health-cache TTL, resolved once from the environment.
pub fn health_cache_ttl() -> Duration {
    *HEALTH_CACHE_TTL.get_or_init(resolve_health_cache_ttl)
}

/// Whether `entry`'s health probe is still within `ttl`.
pub fn is_cache_entry_fresh(entry: &CachedBackend, ttl: Duration) -> bool {
    entry
        .checked_at
        .map(|t| t.elapsed() <= ttl)
        .unwrap_or(false)
}

/// Library-side cache availability check used by provider `is_available` impls.
pub fn is_backend_available(name: &str) -> bool {
    let Some(cache) = BACKEND_CACHE.get() else {
        return false;
    };
    let ttl = health_cache_ttl();
    let lock = cache.read().expect("backend cache lock poisoned");
    let Some(entry) = lock.get(name) else {
        return false;
    };
    if !is_cache_entry_fresh(entry, ttl) {
        return false;
    }
    entry.health.as_ref().map(|h| h.available).unwrap_or(false)
}

/// Resolve the effective timeout for a query using the three-layer priority:
/// 1. Step-level timeout (highest priority)
/// 2. Backend-level timeout (medium priority)
/// 3. Global timeout (lowest priority)
///
///    Falls back to `DEFAULT_TIMEOUT` (300s) if all three are `None`.
///    A zero duration is mapped to `NO_TIMEOUT` to preserve the existing "no timeout" sentinel.
pub fn resolve_timeout(
    step_timeout: Option<Duration>,
    backend_timeout: Option<Duration>,
    global_timeout: Option<Duration>,
) -> Duration {
    step_timeout
        .or(backend_timeout)
        .or(global_timeout)
        .map(|mut d| {
            if d.is_zero() {
                d = NO_TIMEOUT;
            }
            d
        })
        .unwrap_or(DEFAULT_TIMEOUT)
}

/// Default timeout applied when no timeout is configured at any layer.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Near-infinite sentinel: map timeout=0 to this (existing convention for "no timeout").
pub const NO_TIMEOUT: Duration = Duration::from_secs(365 * 24 * 60 * 60);

#[cfg(any(test, feature = "test-support"))]
static TEST_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Opaque guard returned by [`acquire_test_lock`].
///
/// The wrapped lock type is deliberately hidden. Returning a
/// `tokio::sync::MutexGuard` directly, or exposing the static it borrows from,
/// would pin a downstream test suite to this crate's Tokio version. Holding
/// this value holds the lock; dropping it releases the lock.
#[cfg(any(test, feature = "test-support"))]
pub struct TestLockGuard(
    /// Held for its `Drop`, never read.
    #[allow(dead_code)]
    tokio::sync::MutexGuard<'static, ()>,
);

/// Serialize tests that touch process-global state, such as [`BACKEND_CACHE`]
/// or the `LOK_HEALTH_TTL` environment variable.
///
/// Hold the returned guard for the duration of the test:
///
/// ```ignore
/// let _guard = acquire_test_lock().await;
/// ```
#[cfg(any(test, feature = "test-support"))]
pub async fn acquire_test_lock() -> TestLockGuard {
    TestLockGuard(TEST_MUTEX.lock().await)
}

/// Write `body` to an executable temporary script and close the write handle.
///
/// Linux refuses to `execve` a file that any process still holds open for
/// writing and fails the spawn with `ETXTBSY` ("Text file busy"); macOS does
/// not enforce that, so a still-open `NamedTempFile` only breaks on Linux. The
/// returned `TempPath` deletes the file when dropped, so callers keep it alive
/// for as long as the script must exist.
#[cfg(any(test, feature = "test-support"))]
pub fn write_exec_script(body: &[u8]) -> tempfile::TempPath {
    use std::io::Write;

    let mut script = tempfile::NamedTempFile::with_suffix(".sh").expect("create temp script");
    script.write_all(body).expect("write temp script");
    script.flush().expect("flush temp script");
    let path = script.into_temp_path();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("mark temp script executable");
    }

    path
}

#[cfg(test)]
mod library_tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn write_exec_script_yields_a_spawnable_script() {
        let path = write_exec_script(b"#!/bin/sh\necho ready\n");

        let output = std::process::Command::new(path.as_os_str())
            .output()
            .expect("temp script spawns");

        assert!(output.status.success(), "status was {:?}", output.status);
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ready");
    }

    #[test]
    fn test_resolve_timeout_step_wins() {
        assert_eq!(
            resolve_timeout(
                Some(Duration::from_secs(10)),
                Some(Duration::from_secs(20)),
                Some(Duration::from_secs(30)),
            ),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn test_resolve_timeout_backend_beats_global() {
        assert_eq!(
            resolve_timeout(
                None,
                Some(Duration::from_secs(20)),
                Some(Duration::from_secs(30)),
            ),
            Duration::from_secs(20)
        );
    }

    #[test]
    fn test_resolve_timeout_falls_back_to_default() {
        assert_eq!(resolve_timeout(None, None, None), DEFAULT_TIMEOUT);
    }

    #[test]
    fn test_resolve_timeout_zero_is_no_timeout_sentinel() {
        assert_eq!(
            resolve_timeout(None, Some(Duration::from_secs(0)), None),
            NO_TIMEOUT
        );
    }

    #[test]
    fn test_retry_policy_from_backend_config_uses_overrides() {
        let cfg = config::BackendConfig {
            max_retries: Some(5),
            retry_delay_ms: Some(2500),
            ..Default::default()
        };
        let policy = RetryPolicy::from_backend_config(
            &cfg,
            RetryDefaults {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.base_delay, Duration::from_millis(2500));
    }

    #[test]
    fn test_retry_policy_from_backend_config_falls_back_to_defaults() {
        let cfg = config::BackendConfig::default();
        let policy = RetryPolicy::from_backend_config(
            &cfg,
            RetryDefaults {
                max_retries: 3,
                retry_delay_ms: 2000,
            },
        );
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.base_delay, Duration::from_millis(2000));
    }
}
