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
use std::collections::hash_map::Entry;
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
/// Instances are memoised in [`BACKEND_CACHE`], keyed by [`BackendKey`] - the
/// name together with the effective configuration and retry policy. Two callers
/// in one process asking for the same name with different configurations each
/// get an instance honouring their own.
///
/// Not every construction input is part of that identity. See [`BackendKey`] for
/// the ambient state it cannot capture.
///
/// # Errors
/// Returns an error when `name` is unknown or the configuration cannot build
/// that backend.
pub fn create_backend(
    name: &str,
    config: &BackendConfig,
    retry_policy: RetryPolicy,
) -> Result<Arc<dyn Backend>> {
    // Borrowed, not moved: `retry_policy` is read again below to decide on the
    // `RetryExecutor` wrapper, and `RetryPolicy` is not `Copy`.
    let key = BackendKey::new(name, config, &retry_policy);

    // Check unified cache first
    {
        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        if let Some(entry) = lock.get(&key) {
            return Ok(Arc::clone(&entry.backend));
        }
    }

    let inner: Arc<dyn Backend> = match name {
        "codex" => Arc::new(codex::CodexBackend::new(config)?.with_cache_key(key.clone())),
        "gemini" => Arc::new(gemini::GeminiBackend::new(config)?.with_cache_key(key.clone())),
        "claude" => Arc::new(claude::ClaudeBackend::new(config)?.with_cache_key(key.clone())),
        "ollama" => Arc::new(ollama::OllamaBackend::new(config)?.with_cache_key(key.clone())),
        #[cfg(feature = "bedrock")]
        "bedrock" => {
            // BedrockBackend::new is async, need runtime
            let rt = tokio::runtime::Handle::current();
            let config = config.clone();
            let bedrock_key = key.clone();
            tokio::task::block_in_place(|| {
                rt.block_on(async {
                    anyhow::Ok(Arc::new(
                        bedrock::BedrockBackend::new(&config)
                            .await?
                            .with_cache_key(bedrock_key),
                    ) as Arc<dyn Backend>)
                })
            })?
        }
        #[cfg(not(feature = "bedrock"))]
        "bedrock" => anyhow::bail!("Bedrock backend requires the 'bedrock' feature. Rebuild with: cargo build --features bedrock"),
        _ => anyhow::bail!("Unknown backend: {}", name),
    };

    let candidate = if retry_policy.max_retries > 0 {
        Arc::new(RetryExecutor::new(inner, retry_policy)) as Arc<dyn Backend>
    } else {
        inner
    };

    Ok(cache_or_keep_incumbent(key, candidate))
}

/// File `candidate` under `key`, unless something is already there.
///
/// Construction in [`create_backend`] happens outside the lock, so by the time
/// the write guard is taken another caller may have populated this key - and may
/// already have probed it. An unconditional `insert` would write `health: None`
/// over that completed probe, leaving the entry claiming unprobed:
/// `is_available` would report a healthy backend as down and warmup would
/// re-probe something it had just probed. Nothing would error.
///
/// So the loser of that race discards its candidate and takes the incumbent.
/// Equal keys imply an equal [`BackendConfig`] and [`RetryPolicy`], so the two
/// instances are equivalent and only the health beside them is at stake.
/// `set_mock_health` uses the same shape for the same reason.
///
/// A fresh insert leaves `health: None` until warmup probes. Distinguishing
/// "not probed" (`None`) from "probed and unavailable" (`Some(unavailable)`) is
/// what lets `warmup_backends` skip only entries that have already been probed.
///
/// Split out from [`create_backend`] so the guarantee is directly testable: the
/// interleaving that motivates it needs the cache read to miss while another
/// caller inserts, which no sequential test can produce - `create_backend`
/// returns on the read hit long before reaching this point.
fn cache_or_keep_incumbent(key: BackendKey, candidate: Arc<dyn Backend>) -> Arc<dyn Backend> {
    let cache = get_backend_cache();
    let mut lock = cache.write().expect("backend cache lock poisoned");
    match lock.entry(key) {
        Entry::Occupied(existing) => Arc::clone(&existing.get().backend),
        Entry::Vacant(slot) => {
            slot.insert(CachedBackend {
                backend: Arc::clone(&candidate),
                health: None,
                checked_at: None,
            });
            candidate
        }
    }
}

/// Identity a backend instance is cached under.
///
/// Two callers that ask for the same name with an equal [`BackendConfig`] and an
/// equal [`RetryPolicy`] share one instance; a difference in either yields a
/// distinct instance. The retry policy participates because it decides whether
/// the cached value is wrapped in a [`RetryExecutor`], and because it is derived
/// from `Config::defaults` rather than from `BackendConfig` - so two consumers
/// with byte-identical `BackendConfig` can still need different instances.
///
/// # What this identity does not capture
/// Two providers read process-ambient state while constructing, which no value
/// in this key can see:
///
/// - `ClaudeBackend` resolves `api_key_env` to a variable *name*, then reads
///   that variable and stores the resulting secret. The key holds the name, not
///   the value.
/// - `BedrockBackend` loads the ambient AWS profile, region and credential
///   chain.
///
/// So two consumers with identical configuration but different credentials in
/// the environment still share one instance, carrying whichever credentials were
/// live when it was first built. Hashing a resolved secret would put credential
/// material into a map key and into `Debug` output, which is worse than the
/// problem. A host that needs isolation should give each tenant a distinct
/// `api_key_env` name, which *is* part of this key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BackendKey {
    name: String,
    config: BackendConfig,
    retry: RetryPolicy,
}

impl BackendKey {
    /// Build the identity for `name` under `config` and `retry`.
    ///
    /// `retry` is borrowed rather than moved because every caller reads it again
    /// after building the key and [`RetryPolicy`] is not `Copy`.
    pub fn new(name: &str, config: &BackendConfig, retry: &RetryPolicy) -> Self {
        Self {
            name: name.to_string(),
            config: config.clone(),
            retry: retry.clone(),
        }
    }

    /// The backend name this key identifies.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The canonical key for `name` under a default configuration and retry
    /// policy.
    ///
    /// A test that exercises cache mechanics rather than configuration
    /// differences needs one stable key per name, and this provides it. It is
    /// also the shortest migration for a downstream `test-support` consumer
    /// whose calls previously passed a bare name.
    ///
    /// Do not use it to stand in for a real configuration: two names built this
    /// way are distinct, but so is any *real* configuration of the same name.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_test(name: &str) -> Self {
        Self::new(name, &BackendConfig::default(), &RetryPolicy::default())
    }
}

/// Process-global cache of constructed backends and their health, keyed by
/// [`BackendKey`].
///
/// The key carries the effective configuration and retry policy alongside the
/// name, so two consumers in one process asking for the same backend with
/// different configurations each get an instance honouring their own. See
/// [`BackendKey`] for the ambient construction inputs it cannot represent.
pub static BACKEND_CACHE: OnceLock<RwLock<HashMap<BackendKey, CachedBackend>>> = OnceLock::new();

/// Access [`BACKEND_CACHE`], initialising it on first use.
pub fn get_backend_cache() -> &'static RwLock<HashMap<BackendKey, CachedBackend>> {
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

/// Helper to insert a mock entry into the cache during tests.
///
/// Takes the [`BackendKey`] the entry is filed under, matching how the cache is
/// keyed. Build one with [`BackendKey::new`]; a bare name is no longer enough to
/// identify an entry.
#[cfg(any(test, feature = "test-support"))]
pub fn set_mock_health(key: &BackendKey, status: HealthStatus) {
    let cache = get_backend_cache();
    let mut lock = cache.write().expect("backend cache lock poisoned");
    let now = Some(Instant::now());
    let stub_name = key.name().to_string();
    lock.entry(key.clone())
        .and_modify(|entry| {
            entry.health = Some(status.clone());
            entry.checked_at = now;
        })
        .or_insert(CachedBackend {
            backend: Arc::new(StubBackend::new(stub_name)) as Arc<dyn Backend>,
            health: Some(status),
            checked_at: now,
        });
}
/// How long a health probe stays fresh when `LOK_HEALTH_TTL` is unset.
pub const DEFAULT_HEALTH_CACHE_TTL: Duration = Duration::from_secs(60 * 30);
/// Environment variable overriding [`DEFAULT_HEALTH_CACHE_TTL`]. Accepts a
/// humantime duration such as `"5m"`.
pub(crate) const HEALTH_TTL_ENV: &str = "LOK_HEALTH_TTL";

static HEALTH_CACHE_TTL: OnceLock<Duration> = OnceLock::new();
/// Latch ensuring the resolved TTL is announced at most once per process.
pub static HEALTH_TTL_LOGGED: OnceLock<()> = OnceLock::new();

/// Parse a `HEALTH_TTL_ENV` value into a TTL, plus a warning when the input
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
///
/// Takes the [`BackendKey`] the caller was constructed under, so it reports on
/// the same identity the cache is keyed by. A provider built outside
/// [`create_backend`] holds no key and is not in the cache, so its
/// `is_available` answers `false`.
pub(crate) fn is_backend_available(key: &BackendKey) -> bool {
    let Some(cache) = BACKEND_CACHE.get() else {
        return false;
    };
    let ttl = health_cache_ttl();
    let lock = cache.read().expect("backend cache lock poisoned");
    let Some(entry) = lock.get(key) else {
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

/// `log` target for the retry notice emitted by [`RetryExecutor`].
///
/// Carved out so a front end can render retries differently from ordinary
/// warnings. `lok` uses it to keep the `↻` glyph it has always shown, which
/// lets the library log facts while the binary owns presentation.
pub const RETRY_LOG_TARGET: &str = "lokomotiv::retry";

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

/// Write `body` to an executable temporary script, without this process ever
/// holding a write descriptor for it.
///
/// Linux refuses to `execve` a file that any process holds open for writing and
/// fails the spawn with `ETXTBSY` ("Text file busy"); macOS does not enforce it.
/// Closing the handle before `execve` is not sufficient on its own: the unit
/// suite runs these tests in parallel, and a `fork` from any other thread while
/// this process holds the script open copies that descriptor into the child,
/// where `O_CLOEXEC` does not close it until the child reaches its own `execve`.
/// A script executed inside that gap still finds a writer and fails.
///
/// So the file is materialised by a short-lived child instead: the body travels
/// down a pipe and only that child ever opens the script for writing. Once it
/// exits, no descriptor to the script exists anywhere, and no concurrent `fork`
/// in this process can have inherited one. A descriptor that never exists here
/// cannot be inherited from here.
///
/// The returned `TempPath` deletes the file when dropped, so callers keep it
/// alive for as long as the script must exist.
#[cfg(all(any(test, feature = "test-support"), unix))]
pub fn write_exec_script(body: &[u8]) -> tempfile::TempPath {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    let path = std::env::temp_dir().join(format!(
        "lok-test-{}-{}.sh",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));

    let mut writer = Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"cat > "$1" && chmod 755 "$1""#)
        .arg("write_exec_script")
        .arg(&path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn temp script writer");

    {
        let mut stdin = writer.stdin.take().expect("writer stdin");
        stdin.write_all(body).expect("write temp script");
    }

    let status = writer.wait().expect("temp script writer exits");
    assert!(status.success(), "temp script writer failed: {status:?}");

    tempfile::TempPath::from_path(path)
}

/// Non-unix fallback. `ETXTBSY` is a unix rule, so the straightforward
/// in-process write is correct where it does not apply.
#[cfg(all(any(test, feature = "test-support"), not(unix)))]
pub fn write_exec_script(body: &[u8]) -> tempfile::TempPath {
    use std::io::Write;

    let mut script = tempfile::NamedTempFile::with_suffix(".sh").expect("create temp script");
    script.write_all(body).expect("write temp script");
    script.flush().expect("flush temp script");
    script.into_temp_path()
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

    /// Pins the mechanism `write_exec_script` is built around, by performing the
    /// interleaving it avoids: hold the script open for writing, let a child
    /// inherit that descriptor and carry it through its own `execve`, then try to
    /// run the script. The `dup` in `pre_exec` is what makes this deterministic -
    /// it clears `O_CLOEXEC` on the copy, so the descriptor survives into the
    /// living `sleep` rather than vanishing at exec, which is the state a losing
    /// race lands in for a few microseconds.
    ///
    /// If this test ever fails, the premise behind the helper's shape has changed
    /// and the helper should be revisited rather than the test relaxed.
    #[test]
    #[cfg(target_os = "linux")]
    fn etxtbsy_is_reachable_when_a_child_inherits_the_write_descriptor() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut script = tempfile::NamedTempFile::with_suffix(".sh").expect("create temp script");
        script
            .write_all(b"#!/bin/sh\necho ready\n")
            .expect("write temp script");
        script.flush().expect("flush temp script");
        let raw = script.as_file().as_raw_fd();

        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "echo holding; sleep 30"])
            .stdout(Stdio::piped());
        // SAFETY: `dup` is async-signal-safe, which is the only requirement the
        // post-fork/pre-exec context imposes.
        unsafe {
            cmd.pre_exec(move || {
                if libc::dup(raw) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut holder = cmd.spawn().expect("spawn descriptor holder");

        // Reading the child's first line proves it is past `pre_exec` and `execve`,
        // so the duplicated write descriptor is live before we try to run anything.
        let mut line = String::new();
        BufReader::new(holder.stdout.take().expect("holder stdout"))
            .read_line(&mut line)
            .expect("holder announces itself");
        assert_eq!(line.trim(), "holding");

        let path = script.into_temp_path();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("mark temp script executable");

        let err = Command::new(path.as_os_str())
            .output()
            .expect_err("execve must fail while a child holds a write descriptor");

        let _ = holder.kill();
        let _ = holder.wait();

        assert_eq!(
            err.raw_os_error(),
            Some(libc::ETXTBSY),
            "expected ETXTBSY, got {err:?}"
        );
    }

    /// Guards the property the helper exists for: scripts stay executable while
    /// this process is forking on other threads. The old helper lost this race in
    /// CI (run 30223254125); the current one cannot, because the descriptor a
    /// child would need to inherit is never opened here.
    #[test]
    #[cfg(unix)]
    fn write_exec_script_survives_concurrent_forks() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let stop = Arc::new(AtomicBool::new(false));
        let forker = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::process::Command::new("/bin/sh")
                        .args(["-c", ":"])
                        .output();
                }
            })
        };

        for i in 0..50 {
            let path = write_exec_script(b"#!/bin/sh\necho ready\n");
            let output = std::process::Command::new(path.as_os_str())
                .output()
                .unwrap_or_else(|e| panic!("iteration {i} failed to spawn: {e:?}"));
            assert!(
                output.status.success(),
                "iteration {i} exited {:?}",
                output.status
            );
        }

        stop.store(true, Ordering::Relaxed);
        forker.join().expect("forker thread joins");
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

/// Tests for CLO-653: the cache must distinguish configurations, not just names.
#[cfg(test)]
mod cache_key_tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A chat reply that deliberately omits `model`.
    ///
    /// `OllamaBackend::chat` falls back to the *configured* model when the
    /// response leaves the field out (`ollama.rs:144-146`), which turns
    /// `QueryOutput::model` into a read-out of the configuration the instance
    /// actually holds. That is the observable this suite needs: the backend's
    /// own `base_url` and `model` fields are private with no accessor, so two
    /// distinct `Arc`s would show distinct allocation and prove nothing about
    /// which configuration each one honours.
    const CHAT_REPLY_WITHOUT_MODEL: &str = r#"{"message":{"role":"assistant","content":"ok"}}"#;

    /// Minimal HTTP server that records request bodies and answers each request
    /// with a fixed payload.
    ///
    /// Hand-rolled on `tokio::net::TcpListener` rather than pulling in a
    /// mock-server crate. `Cargo.toml` keeps dev-dependencies to this crate
    /// alone on purpose - the comment there records that a dev-dependency
    /// carrying default features silently re-enables `cli` during
    /// `cargo test --no-default-features` - and `tokio` is already a
    /// non-optional dependency, so this costs nothing and survives every
    /// feature combination CI builds.
    struct RecordingServer {
        url: String,
        bodies: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingServer {
        async fn start() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind ephemeral port");
            let addr = listener.local_addr().expect("local addr");
            let bodies = Arc::new(Mutex::new(Vec::new()));
            let recorded = Arc::clone(&bodies);

            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        return;
                    };
                    let recorded = Arc::clone(&recorded);
                    tokio::spawn(async move {
                        let mut buf = Vec::new();
                        let mut chunk = [0u8; 1024];

                        // Read until the header terminator.
                        let header_end = loop {
                            match stream.read(&mut chunk).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                            }
                            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                                break pos + 4;
                            }
                        };

                        let headers =
                            String::from_utf8_lossy(&buf[..header_end]).to_ascii_lowercase();
                        let content_length = headers
                            .lines()
                            .find_map(|l| l.trim_end().strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);

                        while buf.len() < header_end + content_length {
                            match stream.read(&mut chunk).await {
                                Ok(0) | Err(_) => break,
                                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                            }
                        }

                        recorded
                            .lock()
                            .expect("recorder lock poisoned")
                            .push(String::from_utf8_lossy(&buf[header_end..]).to_string());

                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                            CHAT_REPLY_WITHOUT_MODEL.len(),
                            CHAT_REPLY_WITHOUT_MODEL
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                        let _ = stream.flush().await;
                    });
                }
            });

            Self {
                url: format!("http://{}", addr),
                bodies,
            }
        }

        fn url(&self) -> String {
            self.url.clone()
        }

        fn bodies(&self) -> Vec<String> {
            self.bodies.lock().expect("recorder lock poisoned").clone()
        }
    }

    fn ollama_config(endpoint: String, model: &str) -> BackendConfig {
        BackendConfig {
            enabled: true,
            command: Some(endpoint),
            model: Some(model.to_string()),
            ..Default::default()
        }
    }

    fn no_retry() -> RetryPolicy {
        RetryPolicy {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Two consumers asking for `"ollama"` with different endpoints and models
    /// must each get an instance honouring their own configuration.
    ///
    /// Proof is the observed request traffic, not pointer identity. Against the
    /// name-keyed cache this fails with server B never contacted at all, because
    /// `create_backend` returns the first instance before it ever looks at the
    /// second caller's config.
    #[tokio::test]
    async fn two_configs_same_name_honour_their_own_endpoint_and_model() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let server_a = RecordingServer::start().await;
        let server_b = RecordingServer::start().await;

        let cfg_a = ollama_config(server_a.url(), "model-a");
        let cfg_b = ollama_config(server_b.url(), "model-b");

        let backend_a = create_backend("ollama", &cfg_a, no_retry()).expect("build first ollama");
        let backend_b = create_backend("ollama", &cfg_b, no_retry()).expect("build second ollama");

        let out_a = backend_a
            .query(StepContext::from_prompt("ping", Path::new("."), None))
            .await
            .expect("first query succeeds");
        let out_b = backend_b
            .query(StepContext::from_prompt("ping", Path::new("."), None))
            .await
            .expect("second query succeeds");

        let hits_a = server_a.bodies();
        let hits_b = server_b.bodies();

        assert_eq!(
            hits_b.len(),
            1,
            "second consumer's endpoint was never contacted: the cache handed back \
             the first instance, so both queries went to server A ({} requests there)",
            hits_a.len()
        );
        assert_eq!(
            hits_a.len(),
            1,
            "first consumer's endpoint got {} requests",
            hits_a.len()
        );

        assert!(
            hits_a[0].contains("model-a"),
            "server A received the wrong model: {}",
            hits_a[0]
        );
        assert!(
            hits_b[0].contains("model-b"),
            "server B received the wrong model: {}",
            hits_b[0]
        );

        assert_eq!(out_a.model.as_deref(), Some("model-a"));
        assert_eq!(out_b.model.as_deref(), Some("model-b"));

        clear_health_cache();
    }

    /// Same endpoint, different model. Isolates the model from the endpoint, so
    /// the test above cannot pass on URL routing alone.
    #[tokio::test]
    async fn same_endpoint_different_models_send_their_own_model() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let server = RecordingServer::start().await;

        let cfg_a = ollama_config(server.url(), "alpha");
        let cfg_b = ollama_config(server.url(), "beta");

        let a = create_backend("ollama", &cfg_a, no_retry()).expect("build alpha");
        let b = create_backend("ollama", &cfg_b, no_retry()).expect("build beta");

        a.query(StepContext::from_prompt("ping", Path::new("."), None))
            .await
            .expect("alpha query");
        b.query(StepContext::from_prompt("ping", Path::new("."), None))
            .await
            .expect("beta query");

        let bodies = server.bodies();
        assert_eq!(bodies.len(), 2, "expected one request per consumer");
        assert!(
            bodies.iter().any(|b| b.contains("alpha")),
            "no request carried the first model: {bodies:?}"
        );
        assert!(
            bodies.iter().any(|b| b.contains("beta")),
            "no request carried the second model: {bodies:?}"
        );

        clear_health_cache();
    }

    /// A provider built by hand is not in the cache, so it must not claim to be
    /// available - even when some other entry of the same name is healthy.
    #[tokio::test]
    async fn unkeyed_provider_reports_unavailable() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let config = ollama_config("http://127.0.0.1:1".to_string(), "whatever");
        set_mock_health(
            &BackendKey::new("ollama", &config, &no_retry()),
            HealthStatus::new_available(),
        );

        let hand_built = ollama::OllamaBackend::new(&config).expect("construct directly");
        assert!(
            !hand_built.is_available(),
            "an instance never registered in the cache must not report available"
        );

        clear_health_cache();
    }

    /// A construction that loses the race must not erase a completed probe.
    ///
    /// This calls [`cache_or_keep_incumbent`] directly rather than going through
    /// [`create_backend`] twice. The second `create_backend` would return on the
    /// cache *read* hit and never reach the insert, so a sequential test through
    /// the public path exercises nothing - an earlier version of this test did
    /// exactly that and passed against the unconditional `insert` it was meant to
    /// rule out. Driving the insert directly reproduces the state a losing racer
    /// arrives in: entry present, probe already landed, candidate in hand.
    #[tokio::test]
    async fn losing_racer_does_not_erase_a_probe() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let config = ollama_config("http://127.0.0.1:1".to_string(), "raced");
        let key = BackendKey::new("ollama", &config, &no_retry());

        // The winner's instance is in the cache and has since been probed.
        let winner = create_backend("ollama", &config, no_retry()).expect("winner builds");
        set_mock_health(&key, HealthStatus::new_available());

        // The loser finishes constructing and reaches the insert.
        let loser: Arc<dyn Backend> =
            Arc::new(ollama::OllamaBackend::new(&config).expect("loser builds"));
        let returned = cache_or_keep_incumbent(key.clone(), Arc::clone(&loser));

        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        let entry = lock.get(&key).expect("entry present");

        assert!(
            entry.health.is_some(),
            "the losing construction erased a completed health probe"
        );
        assert!(
            entry.health.as_ref().expect("health present").available,
            "health was replaced rather than preserved"
        );
        assert!(
            !Arc::ptr_eq(&returned, &loser),
            "the loser's candidate was installed over the incumbent"
        );
        assert!(
            Arc::ptr_eq(&returned, &winner),
            "the loser should receive the incumbent instance"
        );

        drop(lock);
        clear_health_cache();
    }

    /// The vacant path still installs the candidate and leaves it unprobed, so
    /// warmup can tell it apart from a probed-and-unavailable entry.
    #[tokio::test]
    async fn first_writer_installs_its_candidate_unprobed() {
        let _guard = acquire_test_lock().await;
        clear_health_cache();

        let config = ollama_config("http://127.0.0.1:1".to_string(), "fresh");
        let key = BackendKey::new("ollama", &config, &no_retry());

        let candidate: Arc<dyn Backend> =
            Arc::new(ollama::OllamaBackend::new(&config).expect("builds"));
        let returned = cache_or_keep_incumbent(key.clone(), Arc::clone(&candidate));

        assert!(
            Arc::ptr_eq(&returned, &candidate),
            "an uncontended insert should return the candidate it installed"
        );

        let cache = get_backend_cache();
        let lock = cache.read().expect("backend cache lock poisoned");
        let entry = lock.get(&key).expect("entry present");
        assert!(
            entry.health.is_none() && entry.checked_at.is_none(),
            "a fresh entry must read as unprobed so warmup knows to probe it"
        );

        drop(lock);
        clear_health_cache();
    }
}
