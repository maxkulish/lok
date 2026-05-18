# Plan: CLO-371 Migrate `Backend::query` to `StepContext` + add async `health_check` + sweep Step call sites (FR-19a/19b/20a)

## Context
- **Design:** `docs/designs/clo-371-migrate-backendquery-to-stepcontext.md`
- **Linear:** https://linear.app/cloud-ai/issue/clo-371/
- **Type:** Development (breaking trait change across 5 backends + call sites)
- **Scope:** FR-19a (`StepContext` carrying struct), FR-19b (`async health_check`), FR-20a (Step call site sweep)

---

## Sub-tasks

### ST1 Scaffold `StepContext` + `HealthStatus` types
**Files:** `src/backend/context.rs` (new)
**What:** Create the new carrying struct with all fields, plus `HealthStatus` placeholder, `SandboxMode`, `Message`/`Role`, and `StepOptions` alias. Add `#[cfg(test)]` module with `test_step_context_default_is_phase1_equivalent`.
**Acceptance:** `cargo test test_step_context_default_is_phase1_equivalent` passes.
**Estimate:** S

### ST2 Add `health_check` to `Backend` trait
**Files:** `src/backend/mod.rs`
**What:** Add `async fn health_check(&self) -> Result<HealthStatus, BackendError>` with a default impl that delegates to `is_available()`. Re-export new types from `context.rs`. Update `RetryExecutor` to forward `health_check`.
**Acceptance:** `cargo test test_health_check_default_returns_ok_when_available test_health_check_default_returns_err_when_unavailable` passes.
**Estimate:** S

### ST3 Lockstep trait migration — `query` signature + all impls + call sites
**Files:** `src/backend/mod.rs`, `src/backend/codex.rs`, `src/backend/gemini.rs`, `src/backend/ollama.rs`, `src/backend/claude.rs`, `src/backend/bedrock.rs`, `src/backend/retry.rs`, `src/workflow.rs`, `tests/` mocks
**What:**
1. Change `Backend::query` signature from `(prompt, cwd, model)` to `(ctx: StepContext<'_>)`.
2. Update all 5 concrete backends: destructure `ctx` into local `prompt`/`cwd`/`model` (body otherwise unchanged).
3. Update `RetryExecutor` to forward `ctx` (transparent).
4. Add `step_context()` helper in `src/workflow.rs`.
5. Sweep all 8 Step-aware call sites to construct `StepContext` inline or via helper.
6. Update any `MockBackend` or test-only `impl Backend` in `tests/`.
**Acceptance:** `cargo build --features bedrock` compiles with zero errors.
**Estimate:** L

### ST4 Integration verification
**Files:** repo-wide
**What:**
1. Run `cargo clippy --all-targets -- -D warnings`.
2. Run `cargo test` (all targets).
3. Run `cargo build --features bedrock`.
4. Grep gate: `! rg -n '\.query\([^c][^t][^x]' src/workflow.rs` returns nothing.
5. Grep gate: `! rg -n 'step\..*query\(.*, None\)' src/workflow.rs` returns nothing.
**Acceptance:** All 5 gates pass.
**Estimate:** S

---

## Pre-merge gate
```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && cargo build --features bedrock
```

---

## Risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | `bedrock` feature compilation drift (not exercised on every CI run) | Added `cargo build --features bedrock` to pre-merge gate and ST3 acceptance |
| R2 | Lifetime inference friction with `StepContext<'_>` in `#[async_trait]` | Destructure `ctx` into local `prompt`/`cwd`/`model` immediately in each backend; do not pass the borrow across await boundaries |
| R3 | Mock backends in `tests/` missed during lockstep migration | Explicitly listed in ST3 scope; grep for `impl Backend` in `tests/` before declaring ST3 complete |
| R4 | `RetryExecutor` retry loop accidentally drops `ctx` on retry | `StepContext` is `Copy`; forward by value each iteration |
| R5 | Non-Step callers (conductor, spawn, team, debate, `run_query_with_config`) break compile | Out of scope (CLO-372), but compiler will flag them. Keep them compiling by constructing `StepContext` inline with `model: None` as a temporary shim if needed, then revert in CLO-372. |

---

## Follow-on work (not in this plan)

- **CLO-372** (FR-20b): Migrate non-Step call sites (conductor, spawn, team, debate, `run_query_with_config`)
- **CLO-374** (FR-21): Per-step sandbox routing logic
- Future CLOs: FR-19c (history), FR-22 (schema), FR-24 (options), FR-9..15 (real health checks)

---

*Plan generated from design doc §3 Architecture, §6 Test Plan, §7 Migration.*
