# Design: CLO-384 — FR-23: per-step `timeout` layered override (step > backend > global)

## Problem

Workflow authors cannot express per-step or per-backend timeout policies. The codebase has the
plumbing (`StepContext.timeout`, `BackendError::Timeout`, `Step.timeout` field) but the resolution
logic is duplicated across two inconsistent paths — the workflow runner uses `step.timeout.or(workflow.timeout)`
while the standalone dispatch uses `backend_config.timeout.or(config.defaults.timeout)`. Neither path
implements the full step > backend > global chain. Additionally, all timeout values are raw integers
that conflate units (seconds vs milliseconds) and do not support human-readable strings like `"30s"`
or `"5m"`.

## Goals / Non-goals

**Goals:**
- Accept human-readable duration strings (`"30s"`, `"5m"`, `"1h"`) in all timeout config fields
- Retain backward compatibility with existing integer timeout values
- Implement unified three-layer resolution: `step.timeout.or(backend.timeout).or(global.timeout).unwrap_or(DEFAULT)`
- Consolidate resolution into a single function called from both the workflow runner and the standalone dispatcher
- Populate `StepContext.timeout` at every construction site (CLO-371 L1)
- Eliminate duplicate timeout calculations (CLO-371 L3)

**Non-goals:**
- Per-retry timeout overrides (retries already re-use the same step timeout)
- Soft-deadline budgeting across multi-step workflows (future PRD)
- Changing the runtime behavior of `tokio::time::timeout` wrapping (already correct)
- Adding timeout fields to `Workflow` struct beyond what exists — the scope is step > backend > global

## Architecture

### Module layout

```
src/config.rs          — duration deserializer helpers + Config/Defaults/BackendConfig type changes
src/backend/mod.rs     — unified `effective_timeout()` function + caller consolidation
src/backend/context.rs — no changes (StepContext.timeout already exists as Option<Duration>)
src/workflow.rs        — Step.timeout type change, remove Workflow::step_timeout(), update step_context()
Cargo.toml             — add humantime 2.1
```

### Data flow

```
Workflow TOML                      lok.toml
    step.timeout                       backends.codex.timeout
         |          defaults.timeout         |
         |               |                  |
         v               v                  v
    Step { timeout }   Defaults { timeout } BackendConfig { timeout }
         |               |                  |
         \_______________|__________________/
                          |
                          v
                effective_timeout(step_timeout, backend_name, config)
                          |
                          v
                StepContext { timeout: Some(Duration) }
                          |
                          v
                tokio::time::timeout(ctx.timeout, backend.query(ctx))
                          |
                          v
                BackendError::Timeout { message, elapsed_ms }
```

### Custom duration deserializer

Two deserializer variants handle the unit ambiguity:

| Context | Integer semantics | Placed in | Serde attr |
|---|---|---|---|
| `Defaults.timeout` | seconds | `src/config.rs` | `#[serde(default, deserialize_with = "deser_duration_seconds")]` |
| `BackendConfig.timeout` | seconds | `src/config.rs` | `#[serde(default, deserialize_with = "deser_duration_seconds")]` |
| `Step.timeout` | milliseconds (backward compat) | `src/workflow.rs` | `#[serde(default, deserialize_with = "deser_duration_millis")]` |
| `Workflow.timeout` | milliseconds (backward compat) | `src/workflow.rs` | `#[serde(default, deserialize_with = "deser_duration_millis")]` |

Both accept:
- **String form**: `"30s"`, `"5m"`, `"1h"`, `"500ms"` via `humantime::parse_duration`
- **Integer form**: raw `u64`, interpreted as seconds (config) or milliseconds (workflow)

Implementation approach: a free fn `fn deser_duration_seconds<'de, D>(d: D) -> Result<Option<Duration>, D::Error>` and a corresponding `deser_duration_millis`. Each tries `humantime::parse_duration` if the TOML value is a string, otherwise deserializes as `Option<u64>` and converts to `Duration`. Defined in `src/config.rs` and re-exported where needed.

### Unified resolution function

```rust
// src/backend/mod.rs

/// Default timeout when nothing is configured: 300 seconds.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Sentinel for "effectively no timeout" (0 in config maps here).
const NO_TIMEOUT: Duration = Duration::from_secs(365 * 24 * 60 * 60);

/// Resolve the effective timeout for a step running against a specific backend.
///
/// Chain: `step_timeout > backend_config.timeout > defaults.timeout > DEFAULT_TIMEOUT`
///
/// `step_timeout` is `step.timeout` from the workflow TOML (or `None` for
/// standalone queries outside a workflow).
pub fn effective_timeout(
    step_timeout: Option<Duration>,
    backend_name: &str,
    config: &Config,
) -> Duration {
    let raw = step_timeout
        .or_else(|| config.backends.get(backend_name).and_then(|b| b.timeout))
        .or(config.defaults.timeout)
        .unwrap_or(DEFAULT_TIMEOUT);

    // Preserve existing convention: 0 disables timeout.
    if raw == Duration::ZERO {
        NO_TIMEOUT
    } else {
        raw
    }
}
```

### Call site changes

**Workflow runner** (`src/workflow.rs` step_context):

```rust
// BEFORE
fn step_context(step, workflow, prompt, cwd) -> StepContext {
    StepContext {
        timeout: workflow.step_timeout(step).map(Duration::from_millis),
        ..
    }
}

// AFTER
fn step_context(step, config, backend_name, prompt, cwd) -> StepContext {
    StepContext {
        timeout: Some(effective_timeout(step.timeout, backend_name, config)),
        ..
    }
}
```

`Workflow::step_timeout()` is removed. The resolution moves entirely into `effective_timeout()`. The `step_context` function now takes `config: &Config` and `backend_name: &str` instead of `workflow: &Workflow`. `Workflow.timeout` is removed from the chain per the issue scope (step > backend > global). If workflow-level timeout is desired later, it fits naturally as an optional fourth layer before global.

**Caller update checklist (`src/workflow.rs`):**

| Line | Current call | New call |
|---|---|---|
| 2255 | `step_context(step, workflow, &prompt, &cwd)` | `step_context(step, config, &backend_name, &prompt, &cwd)` |
| 1780 | `step_context(step, workflow, &iter_prompt, &cwd)` | `step_context(step, config, &backend_name, &iter_prompt, &cwd)` |
| 2111 | Manual `StepContext { timeout: step_timeout.map(…) }` | Use `step_context()` or inline `effective_timeout()` |

Both caller closures already have `config` (as `Arc<Config>` or `&Config`) and `backend_name` (as `String` or `&str`) in scope — no new Arc-wrapping needed.

**Multi-backend note:** In the multi-backend loop (line ~2240), each iteration must pass its specific `backend_name` to `step_context`, enabling per-backend timeout resolution. Previously all backends used the same step/workflow timeout; now each backend's `BackendConfig.timeout` is independently consulted.

**Standalone dispatch** (`src/backend/mod.rs` step_context_for_backend):

```rust
// BEFORE
fn step_context_for_backend(prompt, cwd, config, backend_name) -> StepContext {
    let timeout_secs = config.backends.get(backend_name)
        .and_then(|b| b.timeout)
        .unwrap_or(config.defaults.timeout);
    StepContext {
        timeout: Some(Duration::from_secs(effective_timeout_secs(timeout_secs))),
        ..
    }
}

// AFTER
fn step_context_for_backend(prompt, cwd, config, backend_name) -> StepContext {
    StepContext {
        timeout: Some(effective_timeout(None, backend_name, config)),
        ..
    }
}
```

`effective_timeout_secs()` is removed (absorbed by `effective_timeout()`).

## Public API surface

### New function

```rust
// src/backend/mod.rs — exported via pub use in src/backend/mod.rs

pub fn effective_timeout(
    step_timeout: Option<Duration>,
    backend_name: &str,
    config: &Config,
) -> Duration;
```

Existing callers (`step_context`, `step_context_for_backend`, test infrastructure) switch to this.

### Changed type fields

**`src/config.rs`:**

```rust
// BEFORE
pub struct Defaults {
    pub timeout: u64,   // seconds, default 300
}

pub struct BackendConfig {
    pub timeout: Option<u64>,  // seconds
}

// AFTER
pub struct Defaults {
    #[serde(default, deserialize_with = "deser_duration_seconds")]
    pub timeout: Option<Duration>,
}

pub struct BackendConfig {
    #[serde(default, deserialize_with = "deser_duration_seconds")]
    pub timeout: Option<Duration>,
}
```

**`src/workflow.rs`:**

```rust
// BEFORE
pub struct Step {
    pub timeout: Option<u64>,  // milliseconds
}
pub struct Workflow {
    pub timeout: Option<u64>,  // milliseconds
}

// AFTER
pub struct Step {
    #[serde(default, deserialize_with = "deser_duration_millis")]
    pub timeout: Option<Duration>,
}
pub struct Workflow {
    #[serde(default, deserialize_with = "deser_duration_millis")]
    pub timeout: Option<Duration>,
}
```

**`Workflow::step_timeout()` is removed** and replaced by `effective_timeout()`.

### New dependency

```toml
# Cargo.toml
humantime = "2.1"
```

(`humantime-serde` is not required — we use `humantime::parse_duration` directly within the custom deserializer, keeping the dep footprint minimal.)

## Assumptions

1. **Config `timeout` integers are seconds; workflow `timeout` integers are milliseconds** — this is the existing convention. Our dual-format deserializer preserves it. `confidence: high`, verification: unit test on deserializer round-trip.

2. **No workflow TOML file uses `timeout = "..."` string form today** — the field is `u64` now, so any TOML with a string would already fail to parse. No existing user depends on string parsing. `confidence: high`, verification: grep for `timeout = "` in `.lok/workflows/` and `examples/`.

3. **StepContext.timeout is always `Some` after construction** — the existing `.expect("step_context_for_backend always sets timeout")` in `run_query_with_config` line 470 proves this invariant. We maintain it. `confidence: high`, verification: existing test at `test_run_query_with_config_passes_step_context_model_and_timeout`.

4. **The `0` sentinel (effectively no timeout) is preserved** — `effective_timeout()` maps `Duration::ZERO` to the `NO_TIMEOUT` sentinel, matching the current `effective_timeout_secs()` behavior. `confidence: high`, verification: unit test with `timeout = 0`.

5. **`Workflow.timeout` is out of scope for the resolution chain** — the issue specifies step > backend > global, not step > workflow > backend > global. `Workflow.timeout` is removed from the resolution but the field remains on the struct for backward compat. `confidence: medium`, verification: confirm during implementation that `Workflow.timeout` is either unused or serves only as a TOML convenience that maps into the step chain separately.

6. **BackendConfig::default() divergence won't happen** — CLO-382 L3 taught us to keep `Config::default()` and backend constructors in sync. The timeout change only touches the config struct; no backend constructor changes are needed. `confidence: high`, verification: `Config::default().backends.get("codex").unwrap().timeout` assertion in existing tests.

## Test plan

### Unit tests

| Test name | File | What it proves |
|---|---|---|
| `test_deser_duration_seconds_string` | `src/config.rs` | `"30s"` → `Some(Duration::from_secs(30))` |
| `test_deser_duration_seconds_int` | `src/config.rs` | `30` → `Some(Duration::from_secs(30))` |
| `test_deser_duration_seconds_none` | `src/config.rs` | missing field → `None` |
| `test_deser_duration_millis_string` | `src/workflow.rs` or `src/config.rs` | `"30s"` → `Some(Duration::from_secs(30))` |
| `test_deser_duration_millis_int` | `src/workflow.rs` or `src/config.rs` | `30000` → `Some(Duration::from_secs(30))` |
| `test_deser_duration_invalid_string` | `src/config.rs` | `"not_a_duration"` → deserialization error |
| `test_effective_timeout_step_overrides_all` | `src/backend/mod.rs` | step=10s, backend=60s, global=30s → 10s |
| `test_effective_timeout_backend_overrides_global` | `src/backend/mod.rs` | step=None, backend=60s, global=30s → 60s |
| `test_effective_timeout_global_only` | `src/backend/mod.rs` | step=None, backend=None, global=30s → 30s |
| `test_effective_timeout_fallback_default` | `src/backend/mod.rs` | all None → `DEFAULT_TIMEOUT` (300s) |
| `test_effective_timeout_zero_is_sentinel` | `src/backend/mod.rs` | timeout=0 → `NO_TIMEOUT` |
| `test_step_context_populates_timeout` | `src/workflow.rs` | `step_context(step, config, ...).timeout` is `Some` |
| `test_run_query_populates_timeout` (existing, update) | `src/backend/mod.rs` | RecordingBackend sees timeout matching config |
| `test_multibackend_timeout_per_backend` | `src/workflow.rs` | multi-backend step: each backend query uses its config |

### Integration tests

| Test | What it proves |
|---|---|
| Mock backend that sleeps past timeout → `BackendError::Timeout` | Timeout fires and produces correct error variant |
| `BackendError::Timeout` appears in `StepResult.failure` with `kind: Timeout` | End-to-end error propagation |
| Workflow TOML with `timeout = "5s"` parses and step runs with 5-second cap | Full TOML → execution path |
| `lok.toml` with `backends.test.timeout = "10s"` is parsed correctly | Config-level string parsing |

### Manual verification

- Run `cargo test` on the full suite
- Run `cargo fmt --check && cargo clippy -- -D warnings`
- Manually test with a workflow that has `step.timeout = "5s"` against a slow backend

## Migration / rollout

**No feature flags required.** The change is purely additive for the string form; backward compatible for the integer form.

**Config backward compatibility:**
- `lok.toml` with `defaults.timeout = 300` → unchanged behavior (integer parsed as seconds)
- `lok.toml` with `defaults.timeout = "5m"` → new string form, parsed as 300s
- `lok.toml` missing `defaults.timeout` → was `u64(300)`, now `None` → falls through to `DEFAULT_TIMEOUT` (300s). Behavior identical.
- Gemini's `Config::default()` entry: `timeout: Some(600)` → `timeout: Some(Duration::from_secs(600))` (only non-None backend default; easy to miss during implementation)

**Workflow backward compatibility:**
- Workflow TOML with `step.timeout = 30000` → unchanged behavior (integer parsed as milliseconds = 30s)
- Workflow TOML with `step.timeout = "30s"` → new string form, parsed as 30s
- Workflow TOML missing `step.timeout` → was `None`, still `None` → falls through to backend/global

**Removed symbols:**
- `NO_TIMEOUT_SECS: u64` → replaced by `NO_TIMEOUT: Duration`
- `effective_timeout_secs()` → absorbed by `effective_timeout()`
- `fn default_timeout() -> u64 { 300 }` in `src/config.rs` → no longer called from `Defaults::default()`; remove if no remaining callers
- Grep for all three names during implementation to ensure no stale references.

**`Config::default()` update:** `Defaults { timeout: None }` instead of `Defaults { timeout: 300 }`. All callers that previously read `config.defaults.timeout` directly (not through `effective_timeout`) must be audited. Known call sites: `step_context_for_backend` (refactored), test fixtures (update expected values).

## Open questions

1. **Should `Workflow.timeout` enter the resolution chain?** The issue specifies step > backend > global. `Workflow.timeout` (per-workflow default) is semantically a layer between step and backend ("all steps in this workflow default to X unless overridden by backend or step"). Currently it acts as that but only in the workflow path, not the dispatcher path. **Decision:** Omit from this CL; file a follow-up issue if needed. Removing it from the chain is a behavior change for workflows that relied on `Workflow.timeout` alone — those will now fall through to backend/global defaults. Flagged for review.

2. **Should validation timeout use the same resolution?** `ValidateConfig.timeout_ms` is a standalone `Option<u64>` (ms) that bypasses `StepContext` entirely. It has its own `tokio::time::timeout` wrapper in `src/workflow.rs:797`. Unifying it with `effective_timeout()` would require a `ValidateConfig.timeout` field of type `Option<Duration>`. **Decision:** Out of scope for this CL; file a separate issue if desired.

3. **Is `humantime::parse_duration` the right parser?** It supports `"30s"`, `"5m"`, `"1h"`, `"500ms"`, but not compound forms like `"1h 30m"`. The PRD only mentions simple forms. **Decision:** Sufficient for now. Compound forms can be addressed later without breaking the deserializer signature.

4. **Should `StepContext.timeout` stay `Option<Duration>` or become `Duration`?** Currently it's `Option<Duration>` to distinguish "not yet populated" from "set to zero". Since all construction sites will now set it to `Some(...)`, the option is a sentinel for construction bugs. CLO-371 L1 says all call sites must populate it. **Decision:** Keep as `Option<Duration>` for now; the `.expect()` in `run_query_with_config` guards against missed sites. A follow-up PR can make it `Duration` once all call sites are proven to populate it.
