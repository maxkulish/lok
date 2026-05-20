# Plan: CLO-384 — FR-23: per-step `timeout` layered override (step > backend > global)

## Context

- **Design:** `docs/designs/clo-384-per-step-timeout-layered-override.md`
- **Discovery:** `docs/discovery/clo-384.md`
- **Linear:** https://linear.app/cloud-ai/issue/CLO-384/fr-23-per-step-timeout-layered-override-step-backend-global
- **Branch:** `feat/clo-384-per-step`
- **Approach:** Add `humantime` crate for string-duration parsing, write dual-format serde deserializers, consolidate two inconsistent timeout resolution paths into a single `effective_timeout()` function.

## Sub-tasks

### ST1 — Add `humantime` dependency + duration deserializer helpers

**Files:** `Cargo.toml`, `src/config.rs`

**Changes:**
1. Add `humantime = "2.1"` to `Cargo.toml`.
2. In `src/config.rs`, add `use std::time::Duration;` and `use serde::de;` imports.
3. Write two free deserializer functions:
   - `deser_duration_seconds<'de, D>(d: D) -> Result<Option<Duration>, D::Error>` — tries `humantime::parse_duration` on string, falls back to deserializing `Option<u64>` (seconds).
   - `deser_duration_millis<'de, D>(d: D) -> Result<Option<Duration>, D::Error>` — same string parsing, integer branch treats raw u64 as milliseconds.
4. Add unit tests:
   - `test_deser_duration_seconds_string` — `"30s"` → `Some(Duration::from_secs(30))`
   - `test_deser_duration_seconds_int` — `30` → `Some(Duration::from_secs(30))`
   - `test_deser_duration_seconds_none` — missing field → `None`
   - `test_deser_duration_millis_string` — `"30s"` → `Some(Duration::from_secs(30))`
   - `test_deser_duration_millis_int` — `30000` → `Some(Duration::from_secs(30))`
   - `test_deser_duration_invalid_string` — `"not_a_duration"` → deserialization error

**Acceptance:** `cargo test test_deser_duration_seconds_string test_deser_duration_seconds_int test_deser_duration_seconds_none test_deser_duration_millis_string test_deser_duration_millis_int test_deser_duration_invalid_string`

**Estimate:** S

---

### ST2 — Change config type fields (`Defaults`, `BackendConfig`)

**Files:** `src/config.rs`

**Changes:**
1. `Defaults.timeout`: change type from `u64` to `Option<Duration>`. Add `#[serde(default, deserialize_with = "deser_duration_seconds")]`.
2. `BackendConfig.timeout`: change type from `Option<u64>` to `Option<Duration>`. Add `#[serde(default, deserialize_with = "deser_duration_seconds")]`.
3. Update `impl Default for Defaults` — replace `timeout: default_timeout()` with `timeout: None`.
4. Update `Config::default()` — Gemini's `timeout: Some(600)` becomes `timeout: Some(Duration::from_secs(600))`. All other backends remain `timeout: None`.
5. Remove `fn default_timeout() -> u64 { 300 }` (no longer referenced).

**Risks:**
- `BackendConfig` uses `#[serde(deny_unknown_fields)]`, but no new fields are being added (existing `timeout` field just changes type). Safe.
- `Config::default().backends.get("gemini").unwrap().timeout` assertions in existing tests must be updated to `Some(Duration::from_secs(600))` instead of `Some(600)`.

**Acceptance:** `cargo build` (no type errors) && `cargo test` (existing tests that reference `config.defaults.timeout` or `BackendConfig.timeout` pass)

**Estimate:** S

---

### ST3 — Add `effective_timeout()` function + consolidate standalone dispatch

**Files:** `src/backend/mod.rs`, `src/config.rs` (re-export deserializers if needed)

**Changes:**
1. At the module level of `src/backend/mod.rs`, add:
   - `pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);`
   - `const NO_TIMEOUT: Duration = Duration::from_secs(365 * 24 * 60 * 60);`
2. Add `pub fn effective_timeout(step_timeout: Option<Duration>, backend_name: &str, config: &Config) -> Duration`.
3. Remove `const NO_TIMEOUT_SECS: u64` (line 317) and `fn effective_timeout_secs(u64) -> u64` (line 319).
4. Update `step_context_for_backend()` (line 329) to call `effective_timeout(None, backend_name, config)` instead of the current three-line calculation.
5. Add unit tests in `src/backend/mod.rs`:
   - `test_effective_timeout_step_overrides_all` — step=10s, backend=60s, global=30s → 10s
   - `test_effective_timeout_backend_overrides_global` — step=None, backend=60s, global=30s → 60s
   - `test_effective_timeout_global_only` — step=None, backend=None, global=30s → 30s
   - `test_effective_timeout_fallback_default` — all None → `DEFAULT_TIMEOUT` (300s)
   - `test_effective_timeout_zero_is_sentinel` — timeout=0 → `NO_TIMEOUT`
6. Update existing `test_run_query_with_config_passes_step_context_model_and_timeout`:
   - `BackendConfig { timeout: Some(13) }` → `BackendConfig { timeout: Some(Duration::from_secs(13)) }`
   - Expected `RecordedContext.timeout` stays `Some(Duration::from_secs(13))` (no behavioral change).

**Acceptance:** `cargo test test_effective_timeout_step_overrides_all test_effective_timeout_backend_overrides_global test_effective_timeout_global_only test_effective_timeout_fallback_default test_effective_timeout_zero_is_sentinel test_run_query_populates_timeout`

**Estimate:** M

---

### ST4 — Update workflow runner call sites

**Files:** `src/workflow.rs`, `src/backend/mod.rs` (re-export `effective_timeout`)

**Changes:**
1. `Step.timeout`: change type from `Option<u64>` to `Option<Duration>`. Add `#[serde(default, deserialize_with = "deser_duration_millis")]`.
2. `Workflow.timeout`: change type from `Option<u64>` to `Option<Duration>`. Add `#[serde(default, deserialize_with = "deser_duration_millis")]`.
3. Remove `Workflow::step_timeout()` (line ~165). All callers switch to `effective_timeout()`.
4. Update `step_context()` (line ~173):
   - New signature: `fn step_context<'a>(step, config: &Config, backend_name: &str, prompt, cwd) → StepContext`
   - Remove `workflow: &Workflow` parameter
   - Replace `timeout: workflow.step_timeout(step).map(Duration::from_millis)` with `timeout: Some(effective_timeout(step.timeout, backend_name, config))`
5. Update `timeout_duration` computation (line ~1690):
   - Instead of computing from `step_timeout` separately, read from `ctx.timeout` after `step_context()` returns:
     - At each call site where `tokio::time::timeout(timeout_duration, ...)` wraps a backend query, replace `timeout_duration` with `ctx.timeout.expect("step_context always sets timeout")`.
6. Update caller site at line 2255 (main query path):
   - Pass `&config` and `&backend_name` to `step_context(step, ...)`
   - Use `ctx.timeout` for the `tokio::time::timeout` wrapper
7. Update caller site at line 1780 (for_each iteration):
   - Same pattern — pass `config` and `backend_name` to `step_context`
   - Use `ctx.timeout` for the wrapper
8. Update synthesis path at line 2111:
   - Replace manual `StepContext { timeout: step_timeout.map(Duration::from_millis), ... }` with either:
     - `step_context(step, config, synth_backend_name, &synth_prompt, &cwd)` (if step is in scope) or
     - Inline `effective_timeout(None, synth_backend_name, config)` for the timeout field
   - Replace `timeout_duration` usage with `ctx.timeout`
9. Add tests:
   - `test_step_context_populates_timeout` — after building step_context, assert `.timeout` is `Some`
   - `test_multibackend_timeout_per_backend` — multi-backend step yields different timeouts per backend
10. Grep for `default_timeout` — if no remaining callers outside ST2 changes, remove `fn default_timeout()`.

**Acceptance:** `cargo test test_step_context_populates_timeout test_multibackend_timeout_per_backend` && `cargo build`

**Estimate:** L

---

### ST5 — Integration tests + cleanup + pre-merge gate

**Files:** `src/workflow.rs`, `tests/` (if integration tests exist), or inline in `src/backend/mod.rs` tests module

**Changes:**
1. Integration test: mock backend that sleeps past timeout → `BackendError::Timeout`
   - Create a `SleepyBackend` that sleeps for `Duration::from_secs(10)` on query
   - Run with `timeout = "1s"` via config
   - Assert `BackendError::Timeout` is returned
2. Integration test: `BackendError::Timeout` appears in `StepResult.failure` with `kind: Timeout`
   - Create a workflow TOML with a step that has a very short timeout
   - Execute step, assert failure kind
3. Grep for stale references: `NO_TIMEOUT_SECS`, `effective_timeout_secs`, `default_timeout`, `step_timeout(`
   - Clean up any remaining references
4. Run full pre-merge gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
   - Fix any clippy warnings or test failures

**Acceptance:** pre-merge gate passes with zero warnings and zero test failures

**Estimate:** M

---

## Pre-merge gate

```
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

No additional gates required.

## Risks

1. **`BackendConfig` uses `#[serde(deny_unknown_fields)]`** — changing `timeout` from `Option<u64>` to `Option<Duration>` does not add or remove fields, so it's safe. But the custom deserializer must be wired correctly to avoid deserialization errors on existing configs. Mitigation: ST1 tests verify round-trip parsing before ST2 applies them.

2. **`step_context` signature change is widespread** — it's called from at least 3 sites in `src/workflow.rs` (~1780, ~2111, ~2255), some inside closures with `Arc<Config>` and some with `&Config`. The `Arc<Config>` sites may need `.as_ref()` or deref. Mitigation: ST4 caller checklist enumerates every site with before/after signatures.

3. **`timeout_duration` variable appears in ~10 `tokio::time::timeout` call sites** — switching from a precomputed `timeout_duration` to reading `ctx.timeout` after `step_context()` may miss some sites if the grep is incomplete. Mitigation: after ST4, grep for `timeout_duration` and verify the only remaining uses are those that aren't part of the step timeout chain (e.g. `WorkflowEditRequester`, format/verify steps).

## Dependency graph

```
ST1 ──→ ST2 ──→ ST3 ──→ ST4 ──→ ST5
  ↑                   ↗
  └── deserializer   └── effective_timeout() used by ST4
```

Each ST depends on the previous. ST5 is a cleanup pass that must come last to catch all stale references.
