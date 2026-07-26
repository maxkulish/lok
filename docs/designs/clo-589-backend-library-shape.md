# Design: CLO-589 - Record the crate-shape ADR for extracting the backend abstraction as a library

## Problem

The backend abstraction in `src/backend/*` is the one part of lok that other codebases actually want to reuse - the `Backend` trait, the typed `BackendError` taxonomy, `TokenUsage` accounting, and the per-call `StepContext` - yet `Cargo.toml` declares only `[[bin]]` targets (`lok` and `lokomotiv`, both pointing at `src/main.rs`) and no `[lib]`, so every module is reachable only through the binary crate root. Discovery scored the interface itself at 8/10 for coherence but found the package boundary entirely undocumented: nothing in the repository records whether the reusable surface should become a `[lib]` target of `lokomotiv` or a separate `lok-backend` crate, what stays binary-owned, or why `Config` cannot cross the line (it pulls in `crate::cache::CacheConfig` and `crate::role` types at `src/config.rs:12-27`). The people affected are the follow-on extraction ticket CLO-590 and the downstream consumers waiting on it, including the documented `remem-ai` fork path: without a frozen boundary they either guess at the split, take a dependency on CLI orchestration internals such as `run_query` and `Engine`, or re-litigate the same package-shape argument in review. This matters now because CLO-589 blocks CLO-590, and the cost of choosing wrong rises the moment code starts moving.

## Goals / Non-goals

**Goals**

- Land an accepted ADR at `docs/adrs/clo-589-backend-library-shape.md` recording the chosen shape: expose the backend abstraction as a `[lib]` target inside the existing `lokomotiv` package (one crate, two products), with the rejected `lok-backend` workspace-crate alternative and its rationale stated explicitly.
- Record the boundary item by item: every currently public item in `src/backend/mod.rs` and `src/backend/context.rs` is assigned to either the library surface or binary-owned orchestration, with the source path it lives in today.
- Record the `Config`-cycle rule as non-negotiable: `Config` stays binary-owned; only the leaf config types (`BackendConfig`, `Defaults`) and the duration serde helpers they depend on are library-eligible.
- Record the versioning and publication decision (shared `lokomotiv` version, no separate crates.io package at this stage) and the `bedrock` feature-exposure decision (`feature = "bedrock"` stays optional and gates `BedrockBackend`).
- Register the ADR in the index at `docs/adrs/README.md` so the directory has a discoverable entry point from its first record.
- Keep the repository green: `cargo fmt --check && cargo clippy -- -D warnings && cargo test` passes unchanged, because no Rust source is touched.
- Give CLO-590 a citable contract, so its diff can be reviewed against a written boundary rather than against reviewer intuition.

**Non-goals**

- No `src/lib.rs`, no `[lib]` section in `Cargo.toml`, and no module moves in this ticket. Approach A from discovery is decision-only.
- No new crate, no Cargo workspace, no second release cadence.
- No crates.io publish and no change to the date-based version scheme (`20260603.0.0`).
- No change to the `Backend` trait, `StepContext`, `QueryOutput`, `QueryResult`, `TokenUsage`, or `BackendError` - signatures are recorded, not edited.
- No refactor of `src/config.rs`, `src/cache.rs`, or `src/role/`.
- No new dependencies. The change is markdown only.
- No decision on the internal file layout the extraction will use (which file the leaf config types land in) - see Open questions.

## Architecture

The deliverable is documentation, so the "modules" here are two markdown artifacts plus the boundary they freeze over existing Rust modules. Nothing under `src/` changes.

**Artifacts**

| Path | Role | State |
| --- | --- | --- |
| `docs/adrs/clo-589-backend-library-shape.md` | The ADR itself, MADR style: context, decision, boundary allocation, `Config`-cycle rule, versioning/publication, bedrock exposure, rejected alternative | Drafted on branch `feat/clo-589-record`, untracked; this design finalizes and commits it |
| `docs/adrs/README.md` | ADR directory index: one table row per ADR (ID, title, status, date) plus a link list | Drafted on the same branch; grows by one row per future ADR |
| `docs/designs/clo-589-backend-library-shape.md` | This design document | New |

**The boundary the ADR freezes**

```
                       lokomotiv package (single crate, two products)
  ┌──────────────────────────────────────────────────────────────────────────┐
  │  LIBRARY SIDE (reusable; target of CLO-590)                              │
  │                                                                          │
  │   src/backend/context.rs   StepContext<'a>, StepOptions, Message, Role,  │
  │                            SandboxMode, HealthStatus, ModelInfo          │
  │   src/backend/mod.rs       Backend, BackendError, TokenUsage,            │
  │                            QueryOutput, QueryResult, DEFAULT_TIMEOUT,    │
  │                            NO_TIMEOUT, get_retry_policy                  │
  │   src/backend/retry.rs     RetryExecutor, RetryPolicy                    │
  │   src/backend/claude.rs    ClaudeBackend                                 │
  │   src/backend/codex.rs     CodexBackend, FLAG_MATRIX                     │
  │   src/backend/codex_event.rs  Codex stream-event parsing                 │
  │   src/backend/gemini.rs    GeminiBackend                                 │
  │   src/backend/ollama.rs    OllamaBackend                                 │
  │   src/backend/bedrock.rs   BedrockBackend      [cfg(feature = "bedrock")]│
  │   src/config.rs (leaves)   BackendConfig, Defaults,                      │
  │                            deser_duration_seconds/_millis,               │
  │                            serialize_duration_seconds/_millis            │
  └───────────────────────────────▲──────────────────────────────────────────┘
                                  │ constructors take &BackendConfig only
  ┌───────────────────────────────┴──────────────────────────────────────────┐
  │  BINARY SIDE (CLI orchestration; stays behind main.rs)                   │
  │                                                                          │
  │   src/config.rs            Config (owns conductor, cache, tasks,         │
  │                            roles, teams -> the cycle source)             │
  │   src/backend/mod.rs       create_backend, create_claude_backend,        │
  │                            get_backends, run_query,                      │
  │                            run_query_with_config, list_backends,         │
  │                            Engine, print_verbose_header,                 │
  │                            print_verbose_timing, BACKEND_CACHE,          │
  │                            get_backend_cache, CachedBackend,             │
  │                            get_cached_health                             │
  │   src/main.rs              CLI entry; declares all 19 modules today      │
  │   src/workflow.rs, src/conductor.rs, src/apply_verify/, src/tasks/, ...  │
  └──────────────────────────────────────────────────────────────────────────┘
```

**Why the line falls there.** Every backend implementation already constructs from a `&BackendConfig` leaf, never from the `Config` root: `ClaudeBackend::new` (`src/backend/claude.rs:59`), `CodexBackend::new` (`src/backend/codex.rs:112`), `GeminiBackend::new` (`src/backend/gemini.rs:31`), `OllamaBackend::new` (`src/backend/ollama.rs:41`), and `BedrockBackend::new` (`src/backend/bedrock.rs:90`). A scan of the backend implementation files finds references to `crate::config::Config` only inside `#[cfg(test)]` modules (`src/backend/codex.rs:480`, `src/backend/gemini.rs:891`, `src/backend/gemini.rs:908`) and no references to `crate::cache`, `crate::role`, `crate::workflow`, or `crate::output` at all. The single non-backend crate reference in the module is `crate::utils::canonicalize_async` at `src/backend/mod.rs:708`, and it sits inside `run_query_with_config`, which the ADR places on the binary side. The library-eligible set is therefore already coupling-clean; the coupling lives entirely in the orchestration wrappers, which is exactly what the ADR keeps binary-owned.

**Data flow after the boundary is applied (CLO-590 and later).**

```
  library consumer                          lok / lokomotiv binary
  ───────────────                           ──────────────────────
  BackendConfig { .. }                      Config (TOML, src/config.rs)
        │                                         │
        ▼                                         ▼ config.backends[name]
  CodexBackend::new(&cfg)?             get_backends(&config, filter)
        │                                         │ -> create_backend(name, &BackendConfig, RetryPolicy)
        ▼                                         ▼
  RetryExecutor::new(Arc<dyn Backend>, RetryPolicy)   (shared library type)
        │                                         │
        ▼                                         ▼
  Backend::query(StepContext<'_>)          run_query_with_config(..) -> Vec<QueryResult>
        │                                         │
        ▼                                         ▼
  Result<QueryOutput, BackendError>        progress bars, verbose printing, health cache
```

The consumer path stops at `Backend::query`. Everything below the second column - the process-global `BACKEND_CACHE` (`src/backend/mod.rs:407`), the `indicatif` progress bar, the `colored` stderr warnings emitted by `get_backends` and `create_backend` - stays binary-side, which is what keeps the library surface free of terminal I/O and global state.

## Public API surface

CLO-589 changes no Rust signature. The signatures below are recorded as the frozen contract, quoted from the current source so that CLO-590 can be diffed against them.

Library-side trait and types, `src/backend/mod.rs` and `src/backend/context.rs`:

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, ctx: StepContext<'_>) -> std::result::Result<QueryOutput, BackendError>;
    fn is_available(&self) -> bool;
    /// Live async health probe. Default delegates to `is_available()`.
    async fn health_check(&self) -> std::result::Result<HealthStatus, BackendError> { /* default impl */ }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum BackendError {
    Timeout { message: String, elapsed_ms: u64 },
    RateLimit { message: String, retry_after_ms: Option<u64> },
    Auth { message: String },
    Network { message: String },
    Parse { message: String },
    ExecutionFailed { message: String, exit_code: Option<i32> },
    Unavailable { message: String },
    Config { message: String },
}

impl BackendError {
    pub fn is_retryable(&self) -> bool;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cached_tokens: Option<u32>,
    pub reasoning_tokens: Option<u32>,
}

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

pub struct QueryResult {
    pub backend: String,
    pub output: String,
    pub success: bool,
    pub elapsed_ms: u64,
    pub error: Option<BackendError>,
}

#[derive(Debug, Clone, Copy)]
pub struct StepContext<'a> {
    pub prompt: &'a str,
    pub history: &'a [Message],
    pub model: Option<&'a str>,
    pub cwd: &'a Path,
    pub sandbox: Option<SandboxMode>,
    pub apply_edits: bool,
    pub schema: Option<&'a Value>,
    pub options: Option<&'a StepOptions>,
    pub timeout: Option<Duration>,
}

impl<'a> StepContext<'a> {
    pub fn from_prompt(prompt: &'a str, cwd: &'a Path, model: Option<&'a str>) -> Self;
}
```

Library-side constructors, one per backend module, all taking the leaf config only:

```rust
impl ClaudeBackend  { pub fn new(config: &BackendConfig) -> Result<Self>; }        // src/backend/claude.rs:59
impl CodexBackend   { pub fn new(config: &BackendConfig) -> Result<Self>; }        // src/backend/codex.rs:112
impl GeminiBackend  { pub fn new(config: &BackendConfig) -> Result<Self>; }        // src/backend/gemini.rs:31
impl OllamaBackend  { pub fn new(config: &BackendConfig) -> Result<Self>; }        // src/backend/ollama.rs:41
#[cfg(feature = "bedrock")]
impl BedrockBackend { pub async fn new(config: &BackendConfig) -> Result<Self>; }  // src/backend/bedrock.rs:90

impl RetryExecutor  { pub fn new(inner: Arc<dyn Backend>, policy: RetryPolicy) -> Self; } // src/backend/retry.rs:57

pub fn get_retry_policy(config: &BackendConfig, defaults: &crate::config::Defaults) -> RetryPolicy;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
pub const NO_TIMEOUT: Duration = Duration::from_secs(365 * 24 * 60 * 60);
```

Binary-side items the ADR excludes from the library surface, quoted so the exclusion is unambiguous:

```rust
pub fn create_backend(name: &str, config: &BackendConfig, retry_policy: RetryPolicy) -> Result<Arc<dyn Backend>>;
pub fn create_claude_backend(config: &Config) -> Result<ClaudeBackend>;
pub fn get_backends(config: &Config, filter: Option<&str>) -> Result<Vec<Arc<dyn Backend>>>;
pub async fn run_query(backends: &[Arc<dyn Backend>], prompt: &str, cwd: &Path, config: &Config) -> Result<Vec<QueryResult>>;
pub async fn run_query_with_config(backends: &[Arc<dyn Backend>], prompt: &str, cwd: &Path, config: &Config) -> Result<Vec<QueryResult>>;
pub fn list_backends(config: &Config) -> Result<()>;
pub fn print_verbose_header(prompt: &str, backends: &[Arc<dyn Backend>], cwd: &Path);
pub fn print_verbose_timing(results: &[QueryResult]);
pub struct Engine;
impl Engine { pub async fn warmup_backends(config: &Config) -> Result<()>; }
```

Two helpers sit on the seam: `effective_timeout(step_timeout: Option<Duration>, backend_name: &str, config: &Config) -> Duration` and `step_context_for_backend<'a>(prompt: &'a str, cwd: &'a Path, config: &'a Config, backend_name: &str) -> StepContext<'a>` (`src/backend/mod.rs:303` and `:321`). Both are semantically backend-runtime concerns but take the binary-owned `Config`; the ADR does not assign them. See Open questions.

**Before / after.** Identical. No public item is added, removed, renamed, or re-typed by CLO-589, and `Cargo.toml` keeps its two `[[bin]]` targets with no `[lib]` section. The target re-export block below is what the ADR commits CLO-590 to build; it is recorded here as the contract, not written to `src/` in this ticket:

```rust
// Target shape recorded by the ADR. Lands in src/lib.rs under CLO-590; NOT created by CLO-589.
pub mod backend;
pub use backend::{
    Backend, BackendError, QueryOutput, QueryResult, TokenUsage,
    HealthStatus, Message, ModelInfo, Role, SandboxMode, StepContext, StepOptions,
    RetryExecutor, RetryPolicy,
    ClaudeBackend, CodexBackend, GeminiBackend, OllamaBackend,
};
#[cfg(feature = "bedrock")]
pub use backend::BedrockBackend;
```

## Assumptions

- The ADR and index drafts currently untracked on `feat/clo-589-record` (`docs/adrs/clo-589-backend-library-shape.md`, `docs/adrs/README.md`) are the intended deliverables of this ticket and are to be finalized rather than rewritten. Confidence: high. Verification: `git status --porcelain docs/adrs/` shows both as untracked additions on this branch, and the workflow record places the task in the design phase with `approach_chosen: A - decision-only ADR boundary freeze`.
- The library-eligible set in `src/backend/*` has no compile-time dependency on binary-only modules today, so the recorded boundary is achievable without a preparatory refactor. Confidence: high. Verification: `rg -n 'crate::(cache|role|workflow|output|conductor|tasks|team|spawn|template)' src/backend/` returns nothing; the only non-`config`/`backend` crate reference is `crate::utils::canonicalize_async` at `src/backend/mod.rs:708`, inside a binary-side function.
- `BackendConfig` and `Defaults` can be exposed to library consumers without dragging in `cache`/`role`/`conductor` types. Confidence: high. Verification: both structs contain only primitives, `Option<Duration>`, `Vec<String>`, and `Option<String>` (`src/config.rs:31-50` and `:340-364`); `Config` is the only struct in that file referencing `crate::cache::CacheConfig` and `crate::role::*` (`src/config.rs:12-27`).
- Keeping the ADR decision-only leaves the pre-merge gate unaffected, since no Rust source or manifest is touched. Confidence: high. Verification: `git diff --stat` on the final branch touches `docs/` only; `cargo fmt --check && cargo clippy -- -D warnings && cargo test` runs green.
- CLO-590 is the sole consumer of this decision inside the repository, so no other in-flight ticket needs the boundary restated. Confidence: medium. Verification: the workflow record lists `blocks: [CLO-590]` and an empty `blocked_by`; re-check open Lok tickets before merge in case another extraction ticket has since been filed.
- Adding a `[lib]` target later is mechanically feasible for this package even though both `[[bin]]` targets currently share `path = "src/main.rs"` and `src/main.rs` declares all 19 modules. Confidence: medium. Verification: CLO-590 must move the `mod` declarations into a new `src/lib.rs`, leave `src/main.rs` as a thin binary entry, and confirm `cargo build --bins --lib` plus `cargo build --features bedrock` still succeed; if that proves invasive, the ADR's shape decision needs an explicit amendment rather than a silent workaround.

## Test plan

The change is markdown only, so the Rust suite is a regression guard rather than a proof of the change. The substance is verified by document-level checks and by the per-backend boundary audit that the ADR asserts.

**Existing suite (regression guard, must stay green)**

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test` - full existing suite, including `tests/integration.rs`, `tests/codex_fixtures.rs`, `tests/codex_parse_output.rs`, `tests/gemini_fixtures.rs`. Expectation: identical pass/fail set to `main`, because no source file is modified.

**Optional guard test (recommended, additive, no new dependencies)**

- `tests/adr_index.rs::adr_index_lists_every_adr_file` - reads `docs/adrs/` with `std::fs`, asserts every `*.md` file other than `README.md` appears as a link in `docs/adrs/README.md`, and asserts the reverse (no dangling index links). This is the one mechanical failure mode of an ADR directory that reviewers reliably miss. It uses only `std`, so it adds no dependency and costs one file.
- If that test is judged out of scope for a decision-only ticket, the same check runs manually as step 3 below.

**Per-backend boundary audit matrix**

Each row is asserted by the ADR and checkable today with one command. This is the matrix that CLO-590 re-runs after the code moves, where the expected results must be unchanged.

| Backend | Source path | Constructor input | References `Config` outside `#[cfg(test)]` | Feature gate | Boundary | Audit command |
| --- | --- | --- | --- | --- | --- | --- |
| claude | `src/backend/claude.rs` | `&BackendConfig` | no | none | library | `rg -n 'config::Config' src/backend/claude.rs` -> no hits |
| codex | `src/backend/codex.rs` | `&BackendConfig` | no (only `:480`, in `mod tests`) | none | library | `rg -n 'config::Config' src/backend/codex.rs` -> test module only |
| gemini | `src/backend/gemini.rs` | `&BackendConfig` | no (only `:891`, `:908`, in `mod tests`) | none | library | `rg -n 'config::Config' src/backend/gemini.rs` -> test module only |
| ollama | `src/backend/ollama.rs` | `&BackendConfig` | no | none | library | `rg -n 'config::Config' src/backend/ollama.rs` -> no hits |
| bedrock | `src/backend/bedrock.rs` | `&BackendConfig` (async) | no | `feature = "bedrock"` | library, feature-gated | `cargo check --features bedrock` and `rg -n 'cfg\(feature = "bedrock"\)' src/backend/mod.rs` |
| retry wrapper | `src/backend/retry.rs` | `Arc<dyn Backend>` + `RetryPolicy` | no | none | library | `rg -n 'crate::config' src/backend/retry.rs` -> no hits |
| step context | `src/backend/context.rs` | n/a (data types) | no | none | library | `rg -n 'crate::' src/backend/context.rs` -> no hits |
| orchestration wrappers | `src/backend/mod.rs` | `&Config` | yes, by design | none | binary | `rg -n 'config: &Config' src/backend/mod.rs` -> only the items listed as binary-side |

**Manual verification steps**

1. Read `docs/adrs/clo-589-backend-library-shape.md` against the PRD acceptance criteria in `docs/prds/clo-589-backend-library-shape.md`: chosen shape present, rejected alternative present with rationale, boundary recorded item by item, `Config` coupling explained, versioning/publishing decision explicit, bedrock exposure explicit. Six checks, all must be satisfied by text in the ADR.
2. Cross-check every symbol named in the ADR's boundary allocation against the source. Each library-side and binary-side name must resolve: `rg -n 'pub (fn|struct|enum|trait|const|static|use)' src/backend/mod.rs` and compare to the two ADR lists. A name in the ADR that does not exist in the source is a defect in the ADR.
3. Confirm `docs/adrs/README.md` contains a row and a link for CLO-589 and that the link resolves (`ls docs/adrs/clo-589-backend-library-shape.md`).
4. Confirm the diff is documentation-only: `git diff --stat main...HEAD` touches nothing under `src/`, `Cargo.toml`, or `Cargo.lock`.
5. Run the pre-merge gate once on the final branch state and record that the result matches `main`.

## Migration / rollout

**This change is purely additive and documentation-only.** No Rust source, no manifest, no configuration, and no CLI behavior changes. There is nothing to migrate, no backward-compatibility surface to preserve, and no feature flag to introduce - `bedrock` remains exactly as it is today, an optional feature in `Cargo.toml` with `#[cfg(feature = "bedrock")]` gating `BedrockBackend`. Rollback is `git revert` of a markdown-only commit with zero runtime effect.

Rollout order:

1. Commit `docs/adrs/clo-589-backend-library-shape.md` and `docs/adrs/README.md` on `feat/clo-589-record`, together with this design document.
2. Run the pre-merge gate; confirm it matches `main` (no source touched).
3. Open the PR against `main`. Review is a document review against the PRD acceptance criteria, not a code review.
4. On merge, the ADR status stays `Accepted` and becomes the reference CLO-590 cites in its design.
5. CLO-590 performs the actual extraction (`src/lib.rs`, `[lib]` in `Cargo.toml`, module ownership move out of `src/main.rs`) and is reviewed against the boundary table in this ADR. Any deviation CLO-590 needs is handled as an ADR amendment (a follow-up ADR superseding or amending this one), not as an undocumented divergence.
6. Publishing to crates.io as a library remains out of scope until a ticket explicitly requests it; the ADR records that consumers use the existing `lokomotiv` package version in the meantime.

## Open questions

- **Where do `BackendConfig`, `Defaults`, and the duration serde helpers physically live after extraction?** The ADR assigns them to the library side but does not name a file. Leaving them in `src/config.rs` keeps the diff small and preserves every existing `crate::config::BackendConfig` path, but it means a library consumer imports from a module whose other half (`Config`, `ConductorConfig`, `TaskConfig`) is binary-owned - a confusing surface to document. Moving them to something like `src/backend/config.rs` with re-exports from `src/config.rs` gives a clean library module at the cost of touching every import site and the `deny_unknown_fields` serde derives. Unresolved; CLO-590 must pick one and record it.
- **Which side owns `effective_timeout` and `step_context_for_backend`?** Both live in `src/backend/mod.rs` (`:303`, `:321`), both are backend-runtime concerns a consumer would plausibly want, and both take `&Config`. Keeping them binary-side is consistent with the `Config`-cycle rule but leaves consumers to reimplement three-layer timeout resolution. Re-typing them to take `&BackendConfig` plus `&Defaults` would make them library-eligible but is a signature change, which CLO-589 explicitly does not make. Unresolved.
- **Does the library surface expose a name-to-backend factory?** `create_backend` is the only place mapping `"codex"`/`"gemini"`/`"claude"`/`"ollama"`/`"bedrock"` to a concrete type, and the ADR puts it on the binary side because it also writes to the process-global `BACKEND_CACHE` and prints warnings. Consumers are therefore expected to name the concrete type themselves. If that proves too rigid, the alternative is a pure factory in the library (no cache, no printing) with the caching wrapper staying binary-side - more surface, but it removes the one piece of dispatch logic consumers would otherwise duplicate. Unresolved.
- **Is the process-global `BACKEND_CACHE` (`src/backend/mod.rs:407`) acceptable to leave reachable from a library build?** It is binary-side under the ADR, but it is a `OnceLock<RwLock<...>>` in the same module as the library types; a library consumer linking the crate gets the static in their address space whether or not they call into it. Whether that needs to be moved out of `src/backend/mod.rs` or gated is not addressed. Unresolved.
- **Do external consumers need a test double?** `StubBackend`, `clear_health_cache`, `set_mock_health`, and `acquire_test_lock` are all `#[cfg(test)]` today, so they vanish for any downstream build. Consumers writing tests against the `Backend` trait would need to write their own stub, or lok would need a `testing` feature exposing one. The ADR is silent; deferring costs downstream duplication, adding it widens the surface CLO-590 must maintain.
- **Does the date-based version scheme (`20260603.0.0`) carry a usable compatibility signal for library consumers?** The ADR decides library and binary share the `lokomotiv` version, which is correct for a single package, but a calendar version communicates no semver guarantee about the `Backend` trait. Consumers pinning the crate need to know whether a breaking trait change can ship in a routine version bump. Unresolved; may warrant its own ADR rather than an amendment to this one.
