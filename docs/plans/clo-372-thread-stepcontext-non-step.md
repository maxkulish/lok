# Plan: CLO-372 Thread `StepContext` through non-Step `Backend::query` call sites

## Context
- Design: `docs/designs/clo-372-thread-stepcontext-non-step.md`
- Discovery: `docs/discovery/clo-372.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-372/thread-stepcontext-through-non-step-backendquery-call-sites-fr-20b
- Source PRD: `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §FR-20b

## Sub-tasks

### ST1 Add backend context helper and focused helper tests
**Files:** `src/backend/mod.rs`, `src/backend/context.rs`, `src/config.rs`

**Work:**
- Add private `backend_timeout_secs(config, backend_name)` in `src/backend/mod.rs`.
- Add private `effective_timeout_secs(timeout_secs)` in `src/backend/mod.rs`, including the design-review-requested comment explaining why `timeout = 0` maps to a near-infinite one-year timeout.
- Add public `backend::step_context_for_backend(prompt, cwd, config, backend_name)`.
- Populate `StepContext.model` from `Config.backends[backend_name].model` when present.
- Populate `StepContext.timeout` from backend timeout or `Config.defaults.timeout`.
- Preserve Phase-1 defaults for `history`, `sandbox`, `schema`, and `options`.
- Add helper unit tests for model resolution, backend timeout resolution, default timeout fallback, Phase-1 defaults, and zero-timeout preservation.

**Acceptance:** `cargo test step_context_for_backend --lib`

**Estimate:** M

### ST2 Migrate `run_query_with_config` to helper-based context construction
**Files:** `src/backend/mod.rs`

**Work:**
- Replace the current local timeout closure and `StepContext::from_prompt(..., None)` call in `run_query_with_config`.
- Use `backend_timeout_secs` for the outer `tokio::time::timeout` duration.
- Use `step_context_for_backend` for the `Backend::query` call.
- Adjust async closure ownership with `Arc<Config>` or an equivalent owned config value so parallel backend queries do not borrow invalid data.
- Add a focused recording-backend test if practical to assert `run_query_with_config` passes model and timeout into the backend context.

**Acceptance:** `cargo test run_query_with_config --lib`

**Estimate:** M

### ST3 Migrate conductor and spawn non-Step query call sites
**Files:** `src/conductor.rs`, `src/spawn.rs`, `src/backend/mod.rs`

**Work:**
- In `Conductor::execute_tool`, construct the query context with `backend::step_context_for_backend(prompt, cwd, &self.config, backend_name)`.
- In `Spawn::plan_with_backend`, construct the planner fallback context with the shared helper and selected backend name.
- In `Spawn::execute`, clone/capture config into async agent execution closures and construct contexts with the shared helper.
- Do not add task-level model fields; backend config remains the model source.

**Acceptance:** `bash -lc 'cargo check --all-targets && ! rg -n "StepContext::from_prompt\([^\n]*None|from_prompt\([^\n]*,\s*None" src/conductor.rs src/spawn.rs'`

**Estimate:** M

### ST4 Migrate team and debate non-Step query call sites
**Files:** `src/team.rs`, `src/debate.rs`, `src/backend/mod.rs`

**Work:**
- Add a private `config: Config` field to `Team` and populate it in `Team::new(config, cwd)`.
- In `Team::execute`, use `backend::step_context_for_backend` for the primary and second-opinion backend queries.
- In `Debate::get_responses`, use `backend::step_context_for_backend(&prompt, &self.cwd, self.config, backend.name())`.
- Leave `Debate::get_initial_positions` unchanged except for the indirect `run_query_with_config` fix from ST2.

**Acceptance:** `bash -lc 'cargo check --all-targets && ! rg -n "StepContext::from_prompt\([^\n]*None|from_prompt\([^\n]*,\s*None" src/team.rs src/debate.rs'`

**Estimate:** M

### ST5 Run static guard and full pre-merge validation
**Files:** `src/backend/mod.rs`, `src/conductor.rs`, `src/spawn.rs`, `src/team.rs`, `src/debate.rs`

**Work:**
- Run the repository-wide non-Step static guard against the five affected modules.
- Confirm remaining `StepContext::from_prompt(..., None)` uses, if any, are only tests or intentionally default workflow-context constructors outside this FR-20b scope.
- Run formatting, clippy, and test gates.
- Optionally run `cargo build --features bedrock` if local bedrock dependencies are available.

**Acceptance:** `bash -lc '! rg -n "StepContext::from_prompt\([^\n]*None|from_prompt\([^\n]*,\s*None" src/conductor.rs src/spawn.rs src/team.rs src/debate.rs src/backend/mod.rs && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test'`

**Estimate:** S

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (fmt + clippy + test)

## Risks
- `run_query_with_config` uses parallel async execution, so borrowing `Config` directly across futures may produce lifetime errors; use owned `Arc<Config>` or cloned config as designed.
- `Team` currently does not retain config, so adding `config: Config` must preserve the existing public `Team::new(config, cwd)` construction API.
- Static grep can flag intentional tests or unrelated constructors; implementation should distinguish production non-Step query contexts from tests/default assertions.
- `timeout = 0` semantics must remain behavior-preserving: map to the current effective one-year timeout rather than `None`.
