# Plan: CLO-593 Extract lok's Backend abstraction into a consumable library target

## Context

- **Design:** `docs/designs/clo-593-extract-lok-backend-lib.md`
- **Discovery:** `docs/discovery/clo-593.md`
- **PRD:** `docs/prds/clo-593-extract-lok-backend-lib.md`
- **Linear:** [CLO-593](https://linear.app/cloud-ai/issue/CLO-593/extract-loks-backend-abstraction-into-a-consumable-library-target)
- **Target code repo:** `~/Code/orchestrator/lok`, `main @ 796154d`
- **Baseline:** 1,356 tests across 6 suites pass on `main` with `cargo test --quiet --no-fail-fast`

This plan decomposes the design into 11 ordered, mechanically testable sub-tasks. Each sub-task has a single acceptance command and leaves the repo in a green state.

---

## Sub-tasks

### ST1 Bootstrap the `[lib]` target without moving code
**Files:** `Cargo.toml`, `src/lib.rs`  
**Action:** Add `[lib] name = "lokomotiv", path = "src/lib.rs"` to `Cargo.toml`. Create `src/lib.rs` with a minimal doc comment only (no `pub mod backend` yet, so existing `mod backend;` in the binary remains the source of truth).  
**Acceptance:** `cargo build --all-targets` passes.  
**Estimate:** S

### ST2 Capture the pre-change test baseline
**Files:** (none)  
**Action:** Run the full test suite on `main @ 796154d` and record the summary line (test count, suite count). This becomes the immutable reference for all later sub-tasks.  
**Acceptance:** `cargo test --quiet --no-fail-fast` prints the same baseline count as discovery (1,356 tests across 6 suites) or the locally adjusted baseline is documented in the PR.  
**Estimate:** S

### ST3 Extract `BackendConfig` and retry defaults into the library module tree
**Files:** `src/backend/config.rs` (new), `src/config.rs`, `src/backend/{claude,codex,gemini,ollama,bedrock}.rs`, `src/backend/mod.rs`  
**Action:**
1. Create `src/backend/config.rs` containing `BackendConfig`, `RetryDefaults`, and the two duration serde helpers (`deser_duration_seconds`, `serialize_duration_seconds`) lifted from `src/config.rs`.
2. Change provider imports from `crate::config::BackendConfig` to `super::config::BackendConfig`.
3. Make `src/config.rs` re-export `BackendConfig` from `crate::backend::config` so existing call sites keep compiling.
4. Keep `src/lib.rs` still minimal; the binary still owns `mod backend;` at this point.
**Acceptance:** `cargo test` passes and `cargo test src/config.rs` serde tests still pass.  
**Estimate:** M

### ST4 Split orchestration helpers into a binary-side `src/engine.rs`
**Files:** `src/backend/mod.rs`, `src/engine.rs` (new), `src/config.rs` (minor re-exports)  
**Action:**
1. Move `Engine`, `get_backends`, `run_query`, `run_query_with_config`, `list_backends`, `effective_timeout`, `step_context_for_backend`, `get_retry_policy`, `create_claude_backend`, and the `print_verbose_*` helpers from `src/backend/mod.rs` to `src/engine.rs`.
2. Replace `effective_timeout` in the library with a pure `resolve_timeout(step, backend, global)` function; keep `DEFAULT_TIMEOUT` and `NO_TIMEOUT` in the library.
3. Replace `get_retry_policy` with `RetryPolicy::from_backend_config(config, RetryDefaults)` in the library; keep a lok-specific adapter in `src/engine.rs`.
4. Remove `indicatif`, `futures`, and `crate::utils::canonicalize_async` from `src/backend/` imports; confirm they now live only in `src/engine.rs`.
**Acceptance:** `cargo test` passes and `cargo clippy --all-targets -- -D warnings` is clean.  
**Estimate:** L

### ST5 Reparent `backend` under the library root
**Files:** `src/lib.rs`, `src/main.rs`, `src/engine.rs`, all binary `src/*.rs` files that reference `crate::backend::`  
**Action:**
1. `src/lib.rs` declares `pub mod backend;` plus curated `pub use` re-exports.
2. `src/main.rs` removes `mod backend;` and adds `mod engine;`.
3. `src/engine.rs` re-exports `lokomotiv::backend::*` so it can be used as `crate::engine::*` by other binary modules.
4. Rewrite the 16 binary files from `crate::backend::` to `crate::engine::` (mechanical rename).
**Acceptance:** `cargo build --all-targets` passes.  
**Estimate:** L

### ST6 Restore test helpers behind `feature = "test-support"`
**Files:** `Cargo.toml`, `src/backend/mod.rs` (test helpers)  
**Action:**
1. Add `[features] test-support = []` to `Cargo.toml`.
2. Add a dev-dependency on the package itself with `features = ["test-support"]`.
3. Convert `StubBackend`, `clear_health_cache`, `set_mock_health`, `TEST_MUTEX`, and `acquire_test_lock` from `#[cfg(test)]` to `#[cfg(any(test, feature = "test-support"))]`.
4. Make `StubBackend.name` public and add a `StubBackend::new` constructor.
**Acceptance:** `cargo test` passes, especially the `src/workflow.rs` health-cache tests.  
**Estimate:** M

### ST7 Widen concrete provider exports and curate `src/lib.rs`
**Files:** `src/lib.rs`, `src/backend/mod.rs`, `src/backend/{codex,gemini,ollama}.rs`  
**Action:**
1. Make `CodexBackend`, `GeminiBackend`, and `OllamaBackend` public in their modules and re-export them from `src/backend/mod.rs`.
2. Add `CodexEvent` parser / `FLAG_MATRIX` exports if required by CLO-594 consumer needs (defer if uncertain; document in PR).
3. Finalize `src/lib.rs` re-export list: `Backend`, `BackendError`, `BackendConfig`, `QueryOutput`, `TokenUsage`, `StepContext`, `Message`, `Role`, `SandboxMode`, `StepOptions`, `HealthStatus`, `ModelInfo`, `RetryPolicy`, `RetryExecutor`, `RetryDefaults`, `create_backend`, `DEFAULT_TIMEOUT`, `NO_TIMEOUT`, plus provider constructors.
**Acceptance:** `cargo build --all-targets` and `cargo test` pass.  
**Estimate:** S

### ST8 Add external-consumer integration test
**Files:** `tests/backend_public_api.rs`  
**Action:**
1. Create `tests/backend_public_api.rs` that imports only `lokomotiv::` paths.
2. Tests: construct `BackendConfig` from a TOML string, build an `Arc<dyn Backend>` via `create_backend`, call `StepContext::from_prompt`, verify public types (`BackendError`, `QueryOutput`, `TokenUsage`) are reachable.
3. Add an `#[ignored]` `ollama_query_round_trip` test for the live Ollama acceptance criterion.
**Acceptance:** `cargo test --test backend_public_api` passes (ignored test excluded).  
**Estimate:** M

### ST9 Run manual verification and live Ollama acceptance test
**Files:** (none)  
**Action:**
1. `nix-shell --run "cargo build --all-targets --features bedrock"`.
2. `nix-shell --run "cargo run --bin lok -- doctor"`.
3. With local `ollama serve` running: `nix-shell --run "cargo test -- --ignored ollama_query_round_trip"`.
4. External-consumer smoke check: create a scratch crate depending on `lokomotiv` by path and build a file that constructs an Ollama backend.
**Acceptance:** All four manual checks pass; results recorded in the PR description or a comment.  
**Estimate:** M

### ST10 Write the ADR
**Files:** `lok/docs/adrs/001-lib-target-vs-workspace.md`  
**Action:**
1. Create `lok/docs/adrs/` directory.
2. Write ADR covering: (a) decision to add `[lib]` inside `lokomotiv` rather than workspace split, (b) `async_trait` contract, (c) `StepContext` / `BackendConfig` boundary, (d) global cache semantics and follow-up trigger, (e) trigger criteria for future workspace split (e.g., measured downstream cold-build regression > 15%).
**Acceptance:** `cargo doc --no-deps` renders the library; ADR file is present and reviewed in the PR.  
**Estimate:** S

### ST11 Final pre-merge gate and documentation
**Files:** PR description  
**Action:**
1. Run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
2. Update the PR description with: rollout steps, baseline comparison, manual verification results, and a link to the ADR.
3. Confirm schema parity of `docs/status/clo-593-workflow.yaml` with `.claude/commands/task/orchestrate.md`.
**Acceptance:** Pre-merge gate is green and workflow YAML validates.  
**Estimate:** S

---

## Pre-merge gate

```bash
nix-shell --run "cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test"
```

This is the only gate for this ticket. No gcm source changes are in scope; gcm consumes the extracted library under CLO-594.

---

## Risks

| Risk | Mitigation |
|---|---|
| `BACKEND_CACHE` as process-global library state may surprise downstream consumers that instantiate the same backend name with different configs | Document in ADR; add follow-up before CLO-594 to either key by config hash or introduce `BackendRegistry` |
| `retry.rs` writes directly to stderr via `colored`, which is unusual for a library | Evaluate `tracing` / `log` replacement during ST4 or file follow-up before CLO-594 |
| `test-support` feature leaks `tokio::sync::MutexGuard` if left public | Wrap in opaque `TestLockGuard` during ST6 before CLO-594 locks the boundary |
| Binary-side rename (`crate::backend::` → `crate::engine::`) touches 16 files and may conflict with in-flight lok PRs | Rebase before ST5; keep diff mechanical and reviewable per file |
| Dependency bloat from Approach A may hurt downstream build times | ADR records the workspace-split trigger threshold; measure before deciding to split |
| Bedrock `async` constructor via `tokio::task::block_in_place` assumes multi-thread runtime | Verify with `cargo test --features bedrock` in ST9; document runtime requirement in ADR |
| MSRV / dependency alignment with gcm and remem not yet verified | Check downstream `Cargo.toml` constraints before opening PR; add to ST11 acceptance |
