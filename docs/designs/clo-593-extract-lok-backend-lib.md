# Design: CLO-593 - Extract lok's Backend abstraction into a consumable library target

**Status:** Draft
**Linear:** [CLO-593](https://linear.app/cloud-ai/issue/CLO-593/extract-loks-backend-abstraction-into-a-consumable-library-target)
**Branch:** `feat/clo-593-extract`
**PRD:** `docs/prds/clo-593-extract-lok-backend-lib.md`
**Discovery:** `docs/discovery/clo-593.md`
**Blocks:** [CLO-594](https://linear.app/cloud-ai/issue/CLO-594/lock-the-gcm-library-boundary-the-syncasync-seam-and-the-config-shape-adr)
**Code under change:** the `lokomotiv` package at `~/Code/orchestrator/lok`, `main @ 796154d`. This design document is tracked in the lok repo alongside the code it describes. gcm is one of the two downstream consumers, but no gcm source changes are in scope here.

---

## Problem

Discovery found lok's backend layer to be the healthiest part of an otherwise binary-only package: roughly 7,350 lines across `src/backend/` implementing `claude`, `codex`, `gemini`, `ollama`, and an optional `bedrock` provider behind a single `Backend` trait, carrying 210 of its own unit tests, and reaching outside itself only through `crate::config::{BackendConfig, Config, Defaults}` and one `crate::utils::canonicalize_async` call. None of that is reachable by anyone else, because `Cargo.toml` declares two `[[bin]]` targets and no `src/lib.rs`, so the entire module is private to the binary crate. gcm already carries its own parallel provider abstraction in `src/provider/`, and a `remem` fork is about to need the same thing; without a library target each of the three projects maintains a separate implementation of the same retry, timeout, health-probe, and token-accounting semantics, and provider behaviour drifts per project. This matters now because CLO-594 is queued to lock the gcm library boundary against whatever CLO-593 publishes: the shape chosen here is the shape gcm and remem have to build against, and reworking it after two consumers exist costs far more than getting the boundary right once.

## Goals / Non-goals

### Goals

- **G1:** Add a `[lib]` target to the existing `lokomotiv` package (discovery Approach A) that exposes the `Backend` trait, the concrete backends, and the error/output/context types as a stable public API.
- **G2:** Move `BackendConfig`, the duration serde helpers it needs, and the retry/timeout defaults into the library, so a consumer can configure a backend without lok's orchestration `Config`.
- **G3:** Push every `Config`-shaped orchestration helper (`Engine`, `get_backends`, `run_query*`, `list_backends`, `effective_timeout`, `step_context_for_backend`, `get_retry_policy`, the `print_verbose_*` reporters) out of the library and into a binary-side `src/engine.rs`, leaving the library free of `Config`, `indicatif`, and `futures`.
- **G4:** Classify and resolve all 5 distinct `crate::config` / `crate::utils` coupling points identified in discovery (PRD S1, S4).
- **G5:** Keep both binaries (`lok`, `lokomotiv`) building and behaving identically, with the discovery test baseline (1,356 tests across 6 suites) green after each move, not only at the end.
- **G6:** Prove the boundary with an integration test under `tests/` that imports `lokomotiv::` exactly as an external crate would, including an Ollama query.
- **G7:** Record the crate-shape decision (`[lib]` in-package vs workspace split) and the `async_trait` contract as an ADR.

### Non-goals

- **Splitting into a Cargo workspace with a standalone `lok-backend` crate** (discovery Approach B). Rejected in discovery; the design keeps the split viable later by making the library module tree self-contained.
- **Vendoring the backend module into gcm or remem**, or writing a third implementation in remem. Both explicitly rejected in the Linear issue.
- **Replacing `async_trait` with native async traits.** The current shape is preserved verbatim (PRD out-of-scope).
- **Changing provider behaviour**: no new models, flags, prompts, CLI invocations, retry maths, or health-probe semantics. Every diff in this ticket is a move, a re-export, or a signature narrowing.
- **Wiring gcm or remem onto the new library.** That is CLO-594 and its remem sibling; this ticket only publishes the surface.
- **Trimming lok's dependency set** for library consumers. Approach A knowingly ships the full `Cargo.toml` dependency list to consumers; see Open questions.

## Architecture

### Target layout

```
lokomotiv/
├─ Cargo.toml                 [lib] added; [[bin]] lok + lokomotiv unchanged
│                             [features] test-support added
│
├─ src/lib.rs                 NEW  library root: pub mod backend; curated re-exports
│  └─ src/backend/            MOVED under the library root (files stay in place)
│     ├─ mod.rs               Backend trait, BackendError, QueryOutput, TokenUsage,
│     │                       create_backend, health cache, timeout constants
│     ├─ config.rs            NEW  BackendConfig + duration serde + RetryDefaults
│     ├─ context.rs           StepContext, HealthStatus, ModelInfo, SandboxMode,
│     │                       Message, Role, StepOptions   (unchanged)
│     ├─ retry.rs             RetryPolicy, RetryExecutor   (unchanged)
│     ├─ claude.rs codex.rs codex_event.rs gemini.rs ollama.rs bedrock.rs
│     │                       provider impls; `use crate::config::BackendConfig`
│     │                       becomes `use super::config::BackendConfig`
│
├─ src/main.rs                `mod backend;` removed; `mod engine;` added
├─ src/engine.rs              NEW  binary-side orchestration: Engine, get_backends,
│                             run_query{,_with_config}, list_backends,
│                             effective_timeout, step_context_for_backend,
│                             get_retry_policy, create_claude_backend,
│                             print_verbose_header, print_verbose_timing,
│                             plus `pub use lokomotiv::backend::*;`
├─ src/config.rs              Config/Defaults stay; BackendConfig becomes a
│                             re-export of lokomotiv::backend::config::BackendConfig
├─ src/utils.rs               canonicalize_async stays (its only backend caller moved
│                             into the binary)
│
└─ tests/backend_public_api.rs  NEW  external-consumer integration test
```

### Data flow after the split

```
   lok / lokomotiv binary                     lokomotiv library
   ────────────────────────                   ─────────────────
   config.rs                                  backend/config.rs
     Config { defaults, backends, ... }  ───▶   BackendConfig { command, args,
     Defaults { timeout, max_retries }   ─┐     model, api_key_env, timeout,
                                          │     max_retries, retry_delay_ms, ... }
   engine.rs                              │     RetryDefaults { max_retries,
     get_retry_policy(cfg, defaults) ─────┴──▶    retry_delay_ms }
     effective_timeout(step, name, cfg) ─────▶  resolve_timeout(step, backend, global)
     step_context_for_backend(...)      ─────▶  StepContext<'a>
     get_backends(cfg, filter)          ─────▶  create_backend(name, cfg, policy)
                                                   │
     run_query_with_config(...)                    ├─▶ ClaudeBackend / CodexBackend
       canonicalize_async(cwd)                     │   GeminiBackend / OllamaBackend
       ProgressBar (indicatif)                     │   BedrockBackend (feature)
       join_all (futures)                          └─▶ RetryExecutor<Arc<dyn Backend>>
         └── backend.query(ctx) ─────────────────▶ Backend::query(StepContext) -> QueryOutput
     QueryResult { backend, output, success, elapsed_ms, error }
                                                  BACKEND_CACHE: OnceLock<RwLock<
                                                    HashMap<String, CachedBackend>>>
```

The binary keeps every type that mentions `Config`; the library keeps every type that a consumer needs to run a query. `QueryResult` is the one boundary judgement call: it is produced only by `run_query_with_config`, which moves to the binary, so `QueryResult` moves with it. Nothing in the library returns it.

### S1 classification: every production `crate::` reference in `src/backend/`

| # | Reference | Sites | Classification | Resolution |
|---|-----------|-------|----------------|------------|
| C1 | `crate::config::BackendConfig` | `claude.rs:*`, `codex.rs:*`, `gemini.rs:1`, `ollama.rs:4`, `bedrock.rs:*`, `mod.rs:20` | **Move into library** | New `src/backend/config.rs` owns `BackendConfig` plus `deser_duration_seconds` / `serialize_duration_seconds` (lifted from `src/config.rs:111,128`). Imports become `use super::config::BackendConfig`. The binary's `src/config.rs` re-exports it so `Config.backends: HashMap<String, BackendConfig>` and its `deny_unknown_fields` derive are untouched. |
| C2 | `crate::config::Defaults` in `get_retry_policy` (`mod.rs:284`) | 1 | **Caller-populated struct** | Library gains `RetryDefaults { max_retries, retry_delay_ms }` and `RetryPolicy::from_backend_config(&BackendConfig, RetryDefaults)`. `get_retry_policy(config, defaults)` moves to `src/engine.rs` and becomes a two-line adapter that builds `RetryDefaults` from lok's `Defaults`. |
| C3 | `crate::config::Config` in `effective_timeout`, `step_context_for_backend`, `get_backends`, `run_query`, `run_query_with_config`, `list_backends`, `create_claude_backend`, `Engine::warmup_backends` (`mod.rs:303-321, 399, 520-893`) | 8 fns | **Stays in the binary** | All eight move verbatim to `src/engine.rs`. The library keeps the pure core they call: `resolve_timeout(step, backend, global) -> Duration`, `DEFAULT_TIMEOUT`, `NO_TIMEOUT`, `create_backend`, `StepContext::from_prompt`. |
| C4 | `crate::utils::canonicalize_async` (`mod.rs:708`) | 1 | **Stays in the binary** | Its only caller is `run_query_with_config`, which moves to `src/engine.rs`, where `crate::utils` resolves natively. No duplication and no `tokio::fs` helper added to the library. This satisfies PRD S4 by removing the reference rather than relocating the helper; flagged in Migration so review does not read it as a skipped requirement. |
| C5 | `colored::Colorize` in `retry.rs` retry-warning `eprintln!` | 1 file | **Stays in the library (for now)** | Behaviour-preserving. The `colored` writes in `mod.rs:504-877` all sit inside functions moving to `src/engine.rs`, so after the move the library's only terminal output is the retry warning. See Open questions Q4. |

Moving C3 removes `indicatif` (progress bar in `run_query_with_config`) and `futures::future::join_all` (`Engine::warmup_backends`) from the library's import set entirely. Verified by inspection: those two crates appear nowhere in `src/backend/` except `mod.rs`.

### Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| **D1** | `[lib]` target inside the `lokomotiv` package, not a workspace split. | Discovery `approach_chosen`. One package, one version, one crates.io entry; existing CI, `shell.nix`, and publishing keep working. `[package.metadata.docs.rs]` already exists and starts producing real API docs the moment a lib target lands. |
| **D2** | Library root is `src/lib.rs` declaring `pub mod backend;`; the binary drops `mod backend;` and gains `mod engine;`. Files under `src/backend/` do not move on disk. | Keeps the diff readable as "module reparented" rather than "7,350 lines relocated", and keeps `git blame` intact on the provider implementations. |
| **D3** | Binary call sites are rewritten from `crate::backend::` to `crate::engine::`, and `src/engine.rs` re-exports the library types so existing `use crate::backend::{self, Backend, QueryResult}` forms survive as `use crate::engine::{self, Backend, QueryResult}`. | 16 files declare `use crate::backend...` and 44 path references exist, 33 of them in `src/workflow.rs`. That is a mechanical, reviewable rename. The alternative (`pub(crate) use engine as backend;` alias at the binary root, zero churn) is kept in reserve if the rename makes the diff unreviewable. |
| **D4** | `Cargo.toml` gets `[lib] name = "lokomotiv", path = "src/lib.rs"` alongside the existing `[[bin]] name = "lokomotiv"`. | Cargo permits a lib and a bin sharing the package name. `cargo run` is already ambiguous today (two bins, no `default-run`), so nothing regresses. |
| **D5** | The `#[cfg(test)]` helpers `StubBackend`, `clear_health_cache`, `set_mock_health`, `TEST_MUTEX`, `acquire_test_lock` become `#[cfg(any(test, feature = "test-support"))]`, and the package takes a dev-dependency on itself with that feature enabled. `StubBackend.name` changes from `pub(crate)` to `pub` plus a `StubBackend::new` constructor. | These five items are consumed by `src/workflow.rs` test code (lines 5586-7072) which stays in the binary. Once the library is a separate compilation unit, the binary's `#[cfg(test)]` build can no longer see the library's `#[cfg(test)]` items, so roughly 6 workflow tests break unless the helpers are reachable through a feature. |
| **D6** | `BACKEND_CACHE` (`OnceLock<RwLock<HashMap<String, CachedBackend>>>`) and the health TTL stay process-global in the library, unchanged. | Behaviour preservation. `src/workflow.rs:104,222,7066` reads the cache through the already-`pub` `get_backend_cache()`, so the production path needs no change. The design consequence (one shared cache per process across all consumers) is recorded in Open questions Q2 rather than silently redesigned inside an extraction ticket. |
| **D7** | `pub use` for the concrete backends is widened: `CodexBackend`, `GeminiBackend`, and `OllamaBackend` become public alongside the already-exported `ClaudeBackend` and (feature-gated) `BedrockBackend`. | PRD FR-2 requires the concrete constructors. Today `mod codex/gemini/ollama` are private and only `ClaudeBackend`, `BedrockBackend`, and `FLAG_MATRIX` are re-exported, so an external consumer could not build an Ollama backend directly. |

## Public API surface

### `src/lib.rs` (new)

```rust
//! Multi-backend LLM abstraction extracted from the `lok` orchestrator.
//!
//! The entry point is the [`Backend`] trait; construct a concrete backend with
//! [`backend::create_backend`] or a provider constructor, then call
//! [`Backend::query`] with a [`StepContext`].

pub mod backend;

pub use backend::{
    create_backend, Backend, BackendError, HealthStatus, Message, ModelInfo, QueryOutput,
    RetryDefaults, RetryExecutor, RetryPolicy, Role, SandboxMode, StepContext, StepOptions,
    TokenUsage, DEFAULT_TIMEOUT, NO_TIMEOUT,
};
pub use backend::config::BackendConfig;
```

### `src/backend/mod.rs` - the trait, preserved verbatim

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, ctx: StepContext<'_>) -> std::result::Result<QueryOutput, BackendError>;
    fn is_available(&self) -> bool;
    /// Live async health probe. Default delegates to `is_available()`.
    async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> { /* unchanged */ }
}
```

`BackendError` (8 variants plus `is_retryable`), `TokenUsage` (`new`, `with_cached`, `with_reasoning`, `saturating_add`), `QueryOutput` (`from_text`, `from_process`, `with_model`, `with_usage`, `with_structured`), `StepContext<'a>` (`Copy`, `from_prompt`), `HealthStatus`, `ModelInfo`, `SandboxMode`, `Message`, `Role`, `StepOptions`, `RetryPolicy`, and `RetryExecutor` all keep their current definitions and derives. The extraction changes their module path, not their shape.

### Provider constructors (`src/backend/{claude,codex,gemini,ollama,bedrock}.rs`)

```rust
impl ClaudeBackend { pub fn new(config: &BackendConfig) -> anyhow::Result<Self>; }
impl CodexBackend  { pub fn new(config: &BackendConfig) -> anyhow::Result<Self>; }
impl GeminiBackend { pub fn new(config: &BackendConfig) -> anyhow::Result<Self>; }
impl OllamaBackend { pub fn new(config: &BackendConfig) -> anyhow::Result<Self>; }
#[cfg(feature = "bedrock")]
impl BedrockBackend { pub async fn new(config: &BackendConfig) -> anyhow::Result<Self>; }

pub fn create_backend(
    name: &str,
    config: &BackendConfig,
    retry_policy: RetryPolicy,
) -> anyhow::Result<Arc<dyn Backend>>;
```

Signatures are unchanged; only their visibility (D7) and the origin of `BackendConfig` change.

### `src/backend/config.rs` (new module, moved types)

```rust
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub skip_lines: usize,
    pub api_key_env: Option<String>,
    pub model: Option<String>,
    #[serde(
        default,
        deserialize_with = "deser_duration_seconds",
        serialize_with = "serialize_duration_seconds"
    )]
    pub timeout: Option<Duration>,
    pub max_retries: Option<usize>,
    pub retry_delay_ms: Option<u64>,
}

pub fn deser_duration_seconds<'de, D: de::Deserializer<'de>>(d: D)
    -> Result<Option<Duration>, D::Error>;
pub fn serialize_duration_seconds<S: ser::Serializer>(
    v: &Option<Duration>, s: S) -> Result<S::Ok, S::Error>;

/// Fallbacks a caller supplies when a `BackendConfig` leaves retry fields unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryDefaults {
    pub max_retries: usize,
    pub retry_delay_ms: u64,
}

impl Default for RetryDefaults {
    fn default() -> Self { Self { max_retries: 0, retry_delay_ms: 1000 } }
}
```

`Default for RetryDefaults` mirrors lok's current `default_retries()` (0) and `default_retry_delay_ms()` in `src/config.rs`, so a consumer that supplies nothing gets today's lok behaviour.

### Signature changes (before / after)

**Retry policy construction.** `crate::config::Defaults` is replaced by a caller-populated struct; the lok-specific adapter moves to the binary.

```rust
// BEFORE - src/backend/mod.rs:284
pub fn get_retry_policy(config: &BackendConfig, defaults: &crate::config::Defaults) -> RetryPolicy;

// AFTER - src/backend/retry.rs (library)
impl RetryPolicy {
    pub fn from_backend_config(config: &BackendConfig, defaults: RetryDefaults) -> Self;
}

// AFTER - src/engine.rs (binary), same call sites, same result
pub fn get_retry_policy(config: &BackendConfig, defaults: &crate::config::Defaults) -> RetryPolicy {
    RetryPolicy::from_backend_config(
        config,
        RetryDefaults { max_retries: defaults.max_retries, retry_delay_ms: defaults.retry_delay_ms },
    )
}
```

**Timeout resolution.** The three-layer priority becomes a pure library function; the `Config` lookup stays in the binary.

```rust
// BEFORE - src/backend/mod.rs:303
pub fn effective_timeout(
    step_timeout: Option<Duration>,
    backend_name: &str,
    config: &Config,
) -> Duration;

// AFTER - src/backend/mod.rs (library): no Config, no name lookup
pub fn resolve_timeout(
    step_timeout: Option<Duration>,
    backend_timeout: Option<Duration>,
    global_timeout: Option<Duration>,
) -> Duration;

// AFTER - src/engine.rs (binary): unchanged signature, delegates
pub fn effective_timeout(step: Option<Duration>, backend_name: &str, config: &Config) -> Duration {
    resolve_timeout(
        step,
        config.backends.get(backend_name).and_then(|b| b.timeout),
        config.defaults.timeout,
    )
}
```

`DEFAULT_TIMEOUT` (300s) and `NO_TIMEOUT` (the 365-day zero sentinel) stay in the library and keep their current semantics.

**Moved without signature change** to `src/engine.rs`: `Engine::warmup_backends`, `Engine::is_backend_available`, `get_cached_health`, `get_backends`, `run_query`, `run_query_with_config`, `list_backends`, `create_claude_backend`, `step_context_for_backend`, `print_verbose_header`, `print_verbose_timing`, and the `QueryResult` struct.

**Test-support surface** (D5, gated behind `feature = "test-support"` or `cfg(test)`):

```rust
pub struct StubBackend { pub name: String }
impl StubBackend { pub fn new(name: impl Into<String>) -> Self; }
pub fn clear_health_cache();
pub fn set_mock_health(backend_name: &str, status: HealthStatus);
pub async fn acquire_test_lock() -> tokio::sync::MutexGuard<'static, ()>;
```

## Assumptions

- **A1 (high):** Cargo accepts `[lib] name = "lokomotiv"` in a package that already declares `[[bin]] name = "lokomotiv"`. Verify with `cargo build --all-targets` as the very first commit, before any code moves.
- **A2 (high):** No doc comment in `src/backend/` contains a fenced code example, so enabling the lib target adds an empty doctest suite rather than a batch of new failures. Verified by grep at design time; re-verify with `cargo test --doc` after the `[lib]` commit.
- **A3 (medium):** The discovery baseline of 1,356 tests across 6 suites reproduces locally under `nix-shell`. Verify by capturing the pre-change `cargo test` summary on `main @ 796154d` and comparing after every step; if the local number differs, the captured number becomes the baseline and the delta is noted in the PR.
- **A4 (medium):** A dev-dependency of the package on itself with `features = ["test-support"]` unifies to a single library build, so `BACKEND_CACHE` stays one static and the mock-health tests in `src/workflow.rs` still observe what they set. Verify by running the `workflow.rs` health-cache tests immediately after the D5 commit; the fallback is making the five helpers unconditionally `pub`.
- **A5 (medium):** `src/backend/` on lok `main` does not change under us mid-extraction. Verify with `git log --oneline -5 src/backend` before starting and rebase before the PR.
- **A6 (medium):** `BedrockBackend::new` being `async` and reached through `tokio::task::block_in_place` inside `create_backend` keeps working unchanged from a library context, since the requirement (a multi-thread runtime) is a property of the caller, not the crate. Verify with `cargo test --features bedrock`.
- **A7 (medium):** gcm and remem will consume `lokomotiv` as a path or git dependency first, and crates.io publishing is not on this ticket's critical path. Verify against CLO-594 before the PR is opened.
- **A8 (high):** `BackendConfig`'s `deny_unknown_fields` and the duration serde helpers behave identically after moving modules, since neither depends on its declaring module. Verify with the existing `src/config.rs` serde tests (36 tests in that file today).
- **A9 (medium, from review):** The global `BACKEND_CACHE` and the `test-support` helpers are acceptable for this extraction, but they will be addressed before CLO-594 locks the boundary: either a `BackendRegistry`/config-hash cache key or documented global semantics, and an opaque `TestLockGuard` instead of leaking `tokio::sync::MutexGuard`. Verification: design follow-up or implement during rollout.
- **A10 (medium, from review):** `retry.rs` direct stderr output is acceptable for behaviour-preserving extraction, but should be replaced with a logging facade or documented as a known library limitation before downstream consumers depend on it. Verification: apply during rollout or file follow-up.

## Test plan

### Unit tests (library side, `src/backend/`)

All 210 existing unit tests in `src/backend/*.rs` must keep passing unmodified except for import paths. New and relocated tests:

| Test function | Location | Asserts |
|---|---|---|
| `test_resolve_timeout_step_wins` | `src/backend/mod.rs` | step timeout beats backend and global |
| `test_resolve_timeout_backend_beats_global` | `src/backend/mod.rs` | backend timeout beats global |
| `test_resolve_timeout_falls_back_to_default` | `src/backend/mod.rs` | all-`None` yields `DEFAULT_TIMEOUT` |
| `test_resolve_timeout_zero_is_no_timeout_sentinel` | `src/backend/mod.rs` | zero maps to `NO_TIMEOUT` |
| `test_retry_policy_from_backend_config_uses_overrides` | `src/backend/retry.rs` | per-backend `max_retries` / `retry_delay_ms` win |
| `test_retry_policy_from_backend_config_falls_back_to_defaults` | `src/backend/retry.rs` | unset fields take `RetryDefaults` |
| `test_retry_defaults_match_lok_config_defaults` | `src/backend/retry.rs` | `RetryDefaults::default()` equals lok's `default_retries` / `default_retry_delay_ms` |
| `test_backend_config_deserializes_without_orchestration_config` | `src/backend/config.rs` | a bare TOML table parses into `BackendConfig` with no lok `Config` in scope (PRD FR-4) |
| `test_backend_config_rejects_unknown_fields` | `src/backend/config.rs` | `deny_unknown_fields` survives the move |
| `test_backend_config_timeout_human_and_integer_forms` | `src/backend/config.rs` | `"30s"` and `30` both parse |

The 6 existing `test_effective_timeout_*` and 5 `test_step_context_for_backend_*` tests move to `src/engine.rs` with the functions they cover, since they construct a lok `Config`.

### Integration test (`tests/backend_public_api.rs`, new)

This is the first test in the repo that imports the crate; today all four files under `tests/` are process-level or fixture-parsing. It must reference only `lokomotiv::` paths, never `super::` or private modules (PRD FR-5).

| Test function | Gate | Asserts |
|---|---|---|
| `public_api_exposes_backend_trait_objects` | always | `create_backend("ollama", &cfg, RetryPolicy::default())` returns `Arc<dyn Backend>` and `name()` round-trips |
| `backend_config_builds_from_toml_string` | always | `toml::from_str::<BackendConfig>(...)` works with no lok `Config` in scope |
| `step_context_from_prompt_is_constructible` | always | `StepContext::from_prompt` plus `Message`/`Role`/`SandboxMode` are all reachable from outside the crate |
| `error_and_output_types_are_public` | always | `BackendError::is_retryable`, `QueryOutput::from_text`, `TokenUsage::new` reachable |
| `ollama_query_round_trip` | `#[ignore]`, opt-in | builds `OllamaBackend` via the public API against a live local Ollama and asserts non-empty `QueryOutput.stdout` and a populated `backend` field (PRD AC line 3) |

`ollama_query_round_trip` is `#[ignore]` because CI has no Ollama daemon; it runs via `cargo test -- --ignored ollama_query_round_trip` and is listed in the manual verification steps below so the acceptance criterion is demonstrably exercised, not just written.

### Per-backend matrix

| Backend | Construct via public API | Existing unit tests | `health_check` path | Live query |
|---|---|---|---|---|
| `claude` | `ClaudeBackend::new(&BackendConfig)` | 12 in `claude.rs` | api + cli modes, `HealthStatus.mode` | manual, needs `ANTHROPIC_API_KEY` |
| `codex` | `CodexBackend::new(&BackendConfig)` | 44 in `codex.rs` + 29 in `codex_event.rs` | CLI probe, `FLAG_MATRIX` | manual, needs `codex` on PATH |
| `gemini` | `GeminiBackend::new(&BackendConfig)` | 37 in `gemini.rs` | CLI version probe (configurable timeout) | manual, needs `gemini` on PATH |
| `ollama` | `OllamaBackend::new(&BackendConfig)` | in `ollama.rs` | HTTP probe + `models` list | **automated, `#[ignore]`** |
| `bedrock` | `BedrockBackend::new(&BackendConfig).await` | in `bedrock.rs` | AWS SDK | `cargo build --features bedrock` only |
| `RetryExecutor` | `RetryExecutor::new(Arc<dyn Backend>, RetryPolicy)` | in `retry.rs` | delegates to inner | covered by existing unit tests |

### Manual verification

1. `nix-shell --run "cargo build --all-targets"` - both bins plus the lib (PRD FR-1).
2. `nix-shell --run "cargo build --all-targets --features bedrock"` - the optional provider still compiles from the library.
3. `nix-shell --run "cargo test"` after every step in the rollout order below; compare the summary line to the A3 baseline.
4. `nix-shell --run "cargo test --doc"` - confirms the new doctest suite is green (expected: empty until `lib.rs` gains an example).
5. `nix-shell --run "cargo run --bin lok -- doctor"` and one real `lok` run against a repo - proves the binaries behave identically through `src/engine.rs`.
6. With `ollama serve` running: `nix-shell --run "cargo test -- --ignored ollama_query_round_trip"`.
7. `nix-shell --run "cargo doc --no-deps --open"` - the library renders on docs.rs terms for the first time.
8. External-consumer smoke check: in a scratch crate, `lokomotiv = { path = "<LOK_PATH>" }`, then build a file that constructs an Ollama backend and calls `query`. This is the strongest evidence that no `pub(crate)` leak remains.

Pre-merge gate for the lok changes: `nix-shell --run "cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test"`. gcm's own gate (`cargo fmt --check && cargo clippy -- -D warnings && cargo test`) applies to the gcm repo, which this ticket does not touch beyond this document.

## Migration / rollout

**For external consumers this change is purely additive.** No public API exists today, so nothing can break; gcm and remem gain a surface they did not have.

**For lok itself it is an internal reshuffle with no behaviour change.** The binaries keep their names, flags, config file format, and output. `lok.toml` / `~/.config/lok/config.toml` parse identically because `BackendConfig` moves but its serde attributes do not change, and `Config` keeps `deny_unknown_fields` over the same field set.

Rollout order, one commit per step, with `cargo test` green before moving on (PRD FR-6):

1. **Add the `[lib]` target.** `Cargo.toml` gains `[lib]`; `src/lib.rs` is created empty apart from a doc comment. Nothing moves yet. Confirms A1 and A2 cheaply.
2. **Extract `BackendConfig`** into `src/backend/config.rs` together with the two duration serde helpers and `RetryDefaults`; `src/config.rs` re-exports. Still compiled only into the binary at this point.
3. **Split the orchestration helpers** out of `src/backend/mod.rs` into `src/engine.rs` (C3, C4), still inside the binary crate. This is the largest single diff and the one that removes `indicatif`, `futures`, `Config`, and `crate::utils` from `src/backend/`.
4. **Reparent `backend` under the library.** `src/lib.rs` declares `pub mod backend;`; `src/main.rs` drops `mod backend;`; `src/engine.rs` re-exports `lokomotiv::backend::*`; the 16 binary files are rewritten from `crate::backend::` to `crate::engine::` (D3). Compilation errors at this step are the definitive list of remaining leaks.
5. **Restore the test helpers** behind `feature = "test-support"` plus the self dev-dependency (D5), fixing the `src/workflow.rs` tests broken by step 4.
6. **Widen the provider exports** (D7) and curate the `src/lib.rs` re-export list.
7. **Add `tests/backend_public_api.rs`** and run the live Ollama case manually.
8. **Write the ADR** covering the `[lib]`-vs-workspace decision, the `async_trait` contract, and the future workspace-split trigger (see Q1 for the destination path).

### Review feedback applied

- Added A9 and A10 capturing the global-cache and retry-logging concerns from the Gemini review.
- Step 8 ADR destination: create `lok/docs/adrs/` and mirror gcm's `docs/adrs/NNN-*.md` convention.
- Q7 resolution: keep `test-support` default-off and, if exposed, wrap `acquire_test_lock` in an opaque `TestLockGuard` before the public API is locked by CLO-594.
- Added clippy gate after each rollout step (synthesis action item #7).

Feature flags: one new flag, `test-support` (default off, no effect on release builds). The existing `bedrock` flag is untouched. No version bump semantics are implied; if the package is published from this branch, the first publish simply carries a library where there was none.

Rollback: steps 1 through 3 are independently revertible and leave a working binary. From step 4 onward, revert is the whole series, since the module reparenting is what everything after it depends on.

## Open questions

**Q1. Where does the ADR live?** PRD S8 says `architecture/`, but that directory in lok currently holds only generated dashboards (`architecture.html`, `dependencies.html`, `risks.html`, `security.html`) and no markdown. `docs/` has no `adrs/` subdirectory either, while gcm keeps ADRs at `docs/adrs/NNN-*.md`. Options: create `lok/docs/adrs/` mirroring gcm's convention (consistent across the two repos, but a new directory in lok), or drop a markdown file into `architecture/` next to generated HTML (literal PRD compliance, mixed content). Needs an owner call before step 8.

**Q2. Does `StepContext` keep `apply_edits` and `sandbox`, and does it keep its name?** PRD S6 flags both as orchestration concepts that may not belong in a library a memory tool consumes. Keeping them preserves behaviour and costs nothing today, but bakes lok's file-edit and sandbox model into the boundary CLO-594 is about to lock. Narrowing them (for example splitting a `QueryContext` core from an `EditPolicy` extension) is far cheaper now than after two consumers exist, but it is a redesign inside an extraction ticket and puts the 1,356-test baseline at risk. The `Copy` derive and the `'a` lifetime constrain how much can be added later without a breaking change. **This design assumes "keep as-is" so the extraction stays behaviour-preserving; if the answer is "narrow it", it should land as a follow-up before CLO-594 rather than inside this ticket.**

**Q3. Process-global backend and health cache.** `BACKEND_CACHE` is a `OnceLock<RwLock<HashMap<..>>>` in what is now library code, so every consumer in a process shares one cache keyed only by backend name, and `create_backend` returns a cached instance regardless of the `BackendConfig` passed on a later call. That is fine for a single-threaded CLI and questionable for a library. Options: leave it (D6, behaviour-preserving), key the cache by config hash, or introduce an instance-scoped `BackendRegistry` and keep the global as a deprecated convenience. Deciding this changes the public API, so it wants an answer before CLO-594 locks the boundary.

**Q4. Should library code write to stderr?** After the C3 move, `retry.rs` is the only library file that prints (a `colored` retry warning). Consumers may want that on a log target or a callback instead of raw stderr. Alternatives: keep it (zero risk, surprising in a library), add an optional callback on `RetryPolicy`, or drop the print and let the returned `BackendError` carry the information. No decision needed for the extraction to land.

**Q5. Dependency weight for consumers.** Approach A means gcm and remem compile lok's full dependency set, including `clap`, `indicatif`, `colored`, `minijinja`, `chrono`, `dirs`, and `humantime`, none of which the backend code needs once C3 lands. The discovery record accepts this and keeps the workspace split (Approach B) as a later move. Open: what concrete trigger promotes the split (a measured cold-build delta, a consumer complaint, a crates.io publish), and should that trigger be written into the ADR now? Reopening the crate-shape decision itself would contradict the approved discovery approach and would need to be raised with the owner, not decided here.

**Q6. Which optional surfaces do the consumers actually need?** `codex_event` (the Codex JSON event parser, 29 tests), `FLAG_MATRIX`, and the `bedrock` provider are currently internal or feature-gated. Exporting them widens the frozen surface; withholding them may force a second extraction ticket if remem needs Codex event parsing. Should be answered from CLO-594's gcm requirements plus whatever the remem fork has stated.

**Q7. Is `test-support` a published feature or a private one?** D5 needs the helpers reachable from the binary's tests. If the feature is published, external consumers can mock health status in their own suites (useful for gcm's tests), but `acquire_test_lock` leaks `tokio::sync::MutexGuard<'static, ()>` into the public API. If it stays undocumented, the same code ships anyway but nobody is expected to depend on it.
