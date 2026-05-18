# CLO-371 Design: Migrate `Backend::query` to `StepContext` + add async `health_check` + sweep Step call sites (FR-19a/19b/20a)

**Author:** CLO-371 orchestrator (pi)
**Date:** 2026-05-18
**Source PRD:** `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §FR-19a, §FR-19b, §FR-20a, §9 release plan step 3
**Discovery report:** Workflow YAML `phases.discovery` (complete)

---

## 1. Problem

`Backend::query(&self, prompt, cwd, model_override)` at `src/backend/mod.rs:235` cannot grow new per-step concerns (sandbox, schema, timeout, history, options) without another positional argument or a parallel method. v5 §FR-19a introduces `StepContext` as the carrying struct so future fields land additively.

This is a **breaking trait change**: every one of the five concrete backends (`Codex`, `Gemini`, `Ollama`, `Claude` dual-mode, `Bedrock`) plus the `RetryExecutor` decorator must migrate in lockstep. Every caller of `Backend::query` (14 call sites across the codebase) must also migrate.

Doing FR-19a (signature change), FR-19b (async `health_check`), and FR-20a (Step-side migration) in one coordinated PR is the v5 §9 release plan: the trait change is breaking for all 5 backends, so doing it once is cheaper than three sequential breakages.

### Out of scope (tracked separately)
- FR-20b — non-Step call sites (conductor, spawn, team, debate, `run_query_with_config`) → CLO-372
- FR-21 — per-step sandbox routing logic → CLO-374
- FR-22, FR-23 — schema enforcement, options plumbing → future CLOs
- FR-19c/d/e/f — history, tempfile prompts, subprocess I/O, `rendered_prompt` → future CLOs

---

## 2. Goals / Non-goals

### Goals
| # | Goal | Acceptance |
|---|---|---|
| G1 | `Backend` trait signature changed to `query(&self, ctx: StepContext<'_>)` | `cargo build --features bedrock` passes |
| G2 | `Backend` trait gains `async fn health_check(&self) -> Result<HealthStatus, BackendError>` with a default impl | `cargo test` passes; at least one test calls it |
| G3 | All 5 backends compile against new trait | Zero compiler errors |
| G4 | All 8 Step-aware call sites construct `StepContext` from the active `Step` | `! rg -n '\.query\([^c][^t][^x]' src/workflow.rs` returns nothing |
| G5 | No regression in Phase 1 behavior when no `StepContext` field is set | Existing snapshot / unit tests pass |
| G6 | `RetryExecutor` wrapper migrates transparently | Retry behavior unchanged |

### Non-goals
- N1: Non-Step callers (conductor, spawn, team, debate, `run_query_with_config`) → CLO-372 (FR-20b)
- N2: `HealthStatus` struct, `HealthCache`, `warmup_backends()` → future CLO (FR-9/9a/10)
- N3: `BackendCapabilities`, capability registry, per-step capability routing → future CLO (FR-16/17)
- N4: `Step` field additions (`sandbox`, `schema`, `history_window`, `options`, `include_directories`) → these are config-level and outlive this design; this PR only changes plumbing shape
- N5: Prompt delivery via tempfile/stdin → FR-19d, future CLO
- N6: `TokenUsage` extension with `cached_tokens`/`reasoning_tokens` → CLO-370 was the blocker; that is merged (FR-25a). `TokenUsage` already has the new fields.

---

## 3. Architecture

### 3.1 New file: `src/backend/context.rs`

```rust
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Carrying struct for all per-step concerns passed to `Backend::query`.
///
/// Lives in `src/backend/context.rs` per PRD open-question resolution.
///
/// ## Lifetime
/// All borrows are tied to the caller's stack frame; the backend must not
/// retain references past the `await` point. This is the same contract as the
/// old `(prompt, cwd, model)` tuple.
#[derive(Debug, Clone, Copy)]
pub struct StepContext<'a> {
    pub prompt: &'a str,
    /// Conversation history (FR-19c). Empty slice = single-turn.
    pub history: &'a [Message],
    /// Model override from the active Step or caller config.
    pub model: Option<&'a str>,
    /// CWD for subprocess-backed backends.
    pub cwd: &'a Path,
    /// Sandbox level (FR-21). None = backend default.
    pub sandbox: Option<SandboxMode>,
    /// JSON Schema for structured output (FR-22). None = text mode.
    pub schema: Option<&'a Value>,
    /// Per-step options bag (temperature, top_p, etc.) (FR-24).
    /// `None` when no options are set.
    pub options: Option<&'a StepOptions>,
    /// Per-step timeout. None = no override; backend uses its own default.
    pub timeout: Option<Duration>,
}

/// Minimal options bag for per-step config passthrough.
/// Replace with a typed struct when FR-24 lands.
pub type StepOptions = std::collections::HashMap<String, serde_json::Value>;

/// Placeholder for the full health status struct introduced in FR-9/9a.
/// Empty today so `Backend::health_check` return type is stable.
#[derive(Debug, Clone)]
pub struct HealthStatus;

/// Sandbox permission levels for subprocess backends.
/// Maps to Codex `-s` and Gemini `--approval-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

/// One turn in a conversation history.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}
```

### 3.2 Trait change in `src/backend/mod.rs`

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;

    async fn query(
        &self,
        ctx: StepContext<'_>,
    ) -> std::result::Result<QueryOutput, BackendError>;

    /// Sync cached probe — never spawns or blocks.
    fn is_available(&self) -> bool;

    /// Live async health probe. Default delegates to `is_available()`.
    /// Returns a placeholder `HealthStatus` so the trait signature is stable
    /// when FR-9/9a adds real fields.
    async fn health_check(&self) -> Result<HealthStatus, BackendError> {
        if self.is_available() {
            Ok(HealthStatus)
        } else {
            Err(BackendError::Unavailable {
                message: format!("Backend {} is not available", self.name()),
            })
        }
    }
}
```

### 3.3 Backend migration pattern (all 5 backends)

Each backend's `query` method changes signature only. The body extracts the same three fields it already used:

```rust
// OLD:
async fn query(&self, prompt: &str, cwd: &Path, model: Option<&str>) -> ...

// NEW:
async fn query(&self, ctx: StepContext<'_>) -> ... {
    let prompt = ctx.prompt;
    let cwd = ctx.cwd;
    let model = ctx.model;
    // Existing body unchanged
}
```

For **API backends** (Ollama, Claude `Api`, Bedrock): the body is literally identical after destructuring.

For **CLI backends** (Codex, Gemini, Claude `Cli`): same destructuring; later FR-19d will replace argv-based prompt delivery, but this PR only changes the trait boundary.

### 3.4 RetryExecutor migration

```rust
#[async_trait]
impl Backend for RetryExecutor {
    // ... name() unchanged

    async fn query(
        &self,
        ctx: StepContext<'_>,
    ) -> std::result::Result<QueryOutput, BackendError> {
        // Forward ctx by value (it's Copy)
        self.inner.query(ctx).await // with retry loop unchanged
    }

    fn is_available(&self) -> bool { self.inner.is_available() }

    async fn health_check(&self) -> Result<HealthStatus, BackendError> {
        self.inner.health_check().await
    }
}
```

### 3.5 Step call site migration in `src/workflow.rs`

There are **8 Step-aware call sites** that must construct `StepContext`:

| Line | Function / path | Step fields used |
|------|----------------|-----------------|
| ~767 | `run_llm_validation` | `validate.model`, `validate.timeout_ms` |
| ~778 | `run_llm_validation` (timeout branch) | same |
| ~1179 | `WorkflowEditRequester::request_edits` | `self.model_override`, `self.cwd` |
| ~1737 | loop iteration (`for_each` path) | `step.model`, `workflow.step_timeout(step)` |
| ~1943 | multi-backend consensus | `step.model` |
| ~2041 | synthesis backend | `step.model` (when synth matches step's primary backend) |
| ~2171 | single-backend query | `step.model`, `workflow.step_timeout(step)` |

Construction helper (added in `src/workflow.rs` near `StepResult`):

```rust
/// Build a `StepContext` from a `Step` and the current workflow state.
///
/// History is always empty here because FR-19c (multi-turn) is not in scope.
fn step_context<'a>(
    step: &Step,
    workflow: &Workflow,
    prompt: &'a str,
    cwd: &'a Path,
) -> backend::StepContext<'a> {
    let effective_timeout = workflow
        .step_timeout(step)
        .map(|ms| Duration::from_millis(ms));
    backend::StepContext {
        prompt,
        history: &[], // FR-19c
        model: step.model.as_deref(),
        cwd,
        sandbox: None,     // FR-21
        schema: None,      // FR-22
        options: None,     // FR-24
        timeout: effective_timeout,
    }
}
```

Call-site transformation (example from ~2171):

```rust
// OLD:
backend.query(&prompt, &cwd, model_override.as_deref())

// NEW:
let ctx = step_context(step, workflow, &prompt, &cwd);
backend.query(ctx)
```

The validation path (~767/778) does **not** have a `Step` directly, but it already extracts `model_override` from `validate_config.model`. It constructs `StepContext` inline with `model` set and `timeout` from `validate_config.timeout_ms`.

The edit-requester path (~1179) does not have a `Step` either — it has `self.model_override` and `self.cwd`. It constructs inline.

### 3.6 Bedrock feature gate

`BedrockBackend` is feature-gated behind `#[cfg(feature = "bedrock")]`. The migration must compile under both `cargo build` and `cargo build --features bedrock`. There is a risk of drift because the feature is not exercised on every CI run. The PR build matrix must include `--features bedrock`.

---

## 4. Public API surface

### 4.1 New exports from `src/backend/mod.rs`

```rust
pub mod context;
pub use context::{StepContext, Message, Role, SandboxMode, StepOptions};
```

### 4.2 Changed trait

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, ctx: StepContext<'_>) -> Result<QueryOutput, BackendError>;
    fn is_available(&self) -> bool;
    async fn health_check(&self) -> Result<HealthStatus, BackendError>;
}
```

### 4.3 `RetryExecutor` — unchanged public surface

`RetryExecutor::new` takes the same arguments. Callers of `create_backend` in `mod.rs` are unaffected because `RetryExecutor` wraps a `dyn Backend` and the wrapper change is internal.

### 4.4 Back-compat note

`StepResult` is **not** part of this PR's change surface. `StepResult` already gained `usage: Option<TokenUsage>` in the merged CLO-370 (FR-25a). The new `rendered_prompt` field is out of scope (FR-19f, future CLO).

---

## 5. Assumptions

| # | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | The conductor and other non-Step callers can tolerate `StepContext` being constructed inline from local variables. | High | Compiler will reject any mismatch; CLO-372 (FR-20b) refines them later. |
| A2 | `#[async_trait]` handles lifetime elision on `StepContext<'_>` the same way it handled `&str` + `&Path` + `Option<&str>`. | High | `async_trait` expands to `Box<dyn Future>`; the lifetime is captured by the opaque return type. Verified by compiling one backend before full migration. |
| A3 | All five backends live in the same repo, so a single PR can migrate them lockstep. | High | Verified by `ls src/backend/`. Bedrock is feature-gated but present. |
| A4 | `Step` struct does not need new fields for this PR. We only change the plumbing shape, not the config surface. | High | `Step` already has `model` and `timeout`. `sandbox`, `schema`, `options`, `history_window` are FR-21/22/24/19c scoped to future CLOs. |
| A5 | `cargo check --features bedrock` passes today against the current trait. | Medium | Run `cargo check --features bedrock` before opening the PR; if drifted, reconcile in this PR. `.pi/lessons/pr-review-failures.md` does not cover Bedrock drift, but PRD §11.8 flags it. |
| A6 | Empty `history`, `None` sandbox/schema, and empty `options` are safe defaults that produce Phase-1-equivalent behavior. | High | Each backend today ignores these concepts; passing empty/Nones is a no-op. |
| A7 | Downstream crate consumers (rs-wisper) do not implement `Backend`. They only call it via `Arc<dyn Backend>`. | Medium | If a downstream crate implements `Backend`, this is a breaking change for them. The PRD risk matrix lists this as "Likelihood: L (single repo)". Verified by grepping rs-wisper source for `impl Backend`. |
| A8 | Default `health_check` impl returning `Ok(HealthStatus)` when `is_available()` is `true` is sufficient for the one test we need in this PR. | High | Real health checks (FR-9..15) are a future CLO; this PR only adds the trait surface. |

---

## 6. Test plan

### 6.1 Unit tests

| Test | Location | What it checks |
|---|---|---|
| `test_step_context_default_is_phase1_equivalent` | `src/backend/context.rs` | `StepContext { prompt: "hi", ..default() }` produces same `prompt`/`model`/`cwd` extraction as old positional args |
| `test_retry_executor_forwards_step_context` | `src/backend/retry.rs` | `RetryExecutor.query(ctx)` forwards `ctx` to the inner backend; retry loop still works |
| `test_health_check_default_returns_ok_when_available` | `src/backend/mod.rs` (tests) | Mock backend with `is_available() -> true` gets `Ok(HealthStatus)` from default `health_check` |
| `test_health_check_default_returns_err_when_unavailable` | `src/backend/mod.rs` (tests) | Mock backend with `is_available() -> false` gets `Err(BackendError::Unavailable)` |
| `test_codex_backend_compiles_with_new_trait` | `src/backend/codex.rs` (tests) | `CodexBackend` `impl Backend` compiles; one call to `query(ctx)` |
| `test_gemini_backend_compiles_with_new_trait` | `src/backend/gemini.rs` (tests) | Same for `GeminiBackend` |
| `test_ollama_backend_compiles_with_new_trait` | `src/backend/ollama.rs` (tests) | Same for `OllamaBackend` |
| `test_claude_backend_compiles_with_new_trait` | `src/backend/claude.rs` (tests) | Same for both `Api` and `Cli` modes |
| `test_bedrock_backend_compiles_with_new_trait` | `src/backend/bedrock.rs` (tests) | Same for `BedrockBackend` (gated behind `#[cfg(feature = "bedrock")]`) |

### 6.2 Integration / regression tests

| Test | Command / script | What it checks |
|---|---|---|
| `cargo test` | CI | All unit + integration tests pass |
| `cargo clippy -- -D warnings` | CI | No new warnings |
| `cargo build --features bedrock` | CI | Bedrock backend compiles |
| `! rg -n '\.query\([^c][^t][^x]' src/workflow.rs` | CI grep gate | No Step-side legacy `query(prompt, cwd, model)` calls remain |
| `! rg -n 'step\..*query\(.*, None\)' src/workflow.rs` | CI grep gate | No Step-side calls pass `None` as a positional arg (FR-20a) |

### 6.3 Manual verification

- Run `cargo test` locally after trait change but before call-site migration → expect only compiler errors at call sites.
- Run `cargo test` after full migration → all green.
- Run `cargo test --features bedrock` → confirm Bedrock compiles.

---

## 7. Migration / rollout

### 7.1 PR scope

This is a **single, coordinated PR** containing:

1. `src/backend/context.rs` — new file (includes `HealthStatus` placeholder)
2. `src/backend/mod.rs` — trait change + re-exports
3. `src/backend/retry.rs` — wrapper forwarding
4. `src/backend/codex.rs` — signature migration
5. `src/backend/gemini.rs` — signature migration
6. `src/backend/ollama.rs` — signature migration
7. `src/backend/claude.rs` — signature migration (both `Api` and `Cli`)
8. `src/backend/bedrock.rs` — signature migration
9. `src/workflow.rs` — Step call site sweep (8 sites) + `step_context` helper
10. `tests/` — any `MockBackend` or test-only `impl Backend` structs updated to new signature

### 7.2 Build matrix

```yaml
# .github/workflows/ci.yml addition (if not present)
- name: Check with bedrock feature
  run: cargo check --features bedrock
```

### 7.3 Rollback

If the PR is reverted, the old `query(prompt, cwd, model)` signature is restored. Because `StepContext` is a new struct (not a renamed parameter), the revert is a clean deletion of `src/backend/context.rs` and reverting the signature in each file. The `RetryExecutor` and `BackendError` types are untouched.

### 7.4 Follow-on PRs (tracked in Linear)

- **CLO-372** (FR-20b): non-Step call sites — `run_query_with_config`, conductor, spawn, team, debate
- **CLO-374** (FR-21): per-step sandbox routing
- Future CLOs: FR-19c (history), FR-22 (schema), FR-24 (options), FR-9..15 (health checks + warmup)

---

## 8. Open questions

| # | Question | Owner | Resolution deadline | Suggested answer |
|---|---|---|---|---|
| Q1 | Should `StepContext` derive `Copy`? It contains only references and `Option` of small types. | MK | Before implement | **Yes** — simplifies forwarding through `RetryExecutor` and avoids lifetime juggling. |
| Q2 | ~~Should `health_check` return `Result<(), BackendError>` or `Result<HealthStatus, BackendError>`?~~ **RESOLVED.** Gemini review (§2) pointed out that accepting a second trait break defeats the batch-once rationale. | MK | Resolved | **Return `Result<HealthStatus, BackendError>` now** — introduce a placeholder `pub struct HealthStatus;` in `src/backend/context.rs`. When FR-9/9a lands, fields are added to the struct without touching the trait signature again. |
| Q3 | Do we need a `StepContext::builder()` or is direct struct literal construction enough for 8 call sites? | MK | Before implement | **Direct literal** — 8 sites is small; a builder is overkill until FR-21/22/24 add more fields. |
| Q4 | The PRD says `StepContext` should have `timeout: Duration`; the Linear issue says `timeout: Option<Duration>`. Which one? | MK | Before implement | **`Option<Duration>`** — `None` means "no per-step override, use backend default". Matches existing `Step.timeout` semantics (0 = no timeout, None = inherit from workflow). |

---

## Appendix: Call site inventory (pre-migration)

### Step-aware sites (FR-20a) — must migrate in this PR

```
src/workflow.rs:767   backend_instance.query(&prompt, cwd, model_override)
src/workflow.rs:778   backend_instance.query(&prompt, cwd, model_override)
src/workflow.rs:1179  self.backend.query(&fix_prompt, &self.cwd, self.model_override.as_deref())
src/workflow.rs:1737  backend.query(&iter_prompt, &cwd, model_override.as_deref())
src/workflow.rs:1943  backend.query(&prompt, &cwd, model_override.as_deref())
src/workflow.rs:2041  synth_backend.query(&synth_prompt, &cwd, None)
src/workflow.rs:2171  backend.query(&prompt, &cwd, model_override.as_deref())
```

### Non-Step sites (FR-20b) — out of scope, tracked in CLO-372

```
src/backend/mod.rs:381   backend.query(&prompt, &cwd, None)   // run_query_with_config
src/conductor.rs:186     // query_backend tool
src/spawn.rs:173,282     // plan_with_backend, AgentTask
src/team.rs:82,121       // BackendProfile
src/debate.rs:230        // debate round
```

These will construct `StepContext` inline with `model: None` (or from `config.defaults.model`) in the follow-on PR.

---

*End of design document.*
