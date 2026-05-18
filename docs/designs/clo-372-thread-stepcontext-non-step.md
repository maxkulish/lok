# CLO-372 Design: Thread `StepContext` through non-Step `Backend::query` call sites (FR-20b)

**Author:** CLO-372 orchestrator (pi)
**Date:** 2026-05-18
**Source PRD:** `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §FR-20b
**Discovery report:** `docs/discovery/clo-372.md`
**Chosen discovery approach:** Approach A — minimal per-site migration using local context builders

---

## 1. Problem

CLO-371 migrated `Backend::query` to accept `StepContext`, but several non-workflow/non-`Step` callers still construct bare contexts with `StepContext::from_prompt(..., None)`. Per discovery, the remaining gaps are in `src/conductor.rs`, `src/spawn.rs`, `src/team.rs`, `src/debate.rs`, and `src/backend/mod.rs::run_query_with_config`. This means model and timeout information already present in config is not consistently represented in the carrying struct. FR-20b finishes the migration by ensuring every non-`Step` caller populates the `StepContext` fields it can know today, especially `model` and `timeout`.

---

## 2. Goals / Non-goals

### Goals

| # | Goal | Acceptance |
|---|---|---|
| G1 | Non-`Step` callers no longer pass `None` for model when the selected backend config has a model | Tests or grep verify helper use at all remaining sites |
| G2 | Non-`Step` callers populate `StepContext.timeout` from per-backend or default timeout where that value is known | `run_query_with_config`, conductor, spawn, team, and debate helpers all set timeout |
| G3 | Behavior remains Phase-1 compatible for future fields not yet represented in config | `history: &[]`, `sandbox: None`, `schema: None`, `options: None` remain explicit defaults |
| G4 | The implementation stays localized and reviewable | No broad trait redesign or new dependency |
| G5 | Regression tests cover helper behavior and at least one call path | `cargo test` passes with targeted unit tests |

### Non-goals

- N1: Add new workflow `Step` fields for sandbox/schema/options/history. Those are separate FR-21/22/24/19c scopes.
- N2: Change the `Backend` trait or `StepContext` public shape. CLO-371 already completed that breaking change.
- N3: Add model selection to CLI flags or task config. This design only propagates values already present in backend config or direct API context.
- N4: Replace all local helpers with a central factory abstraction. Discovery rejected that for this small migration.
- N5: Change runtime timeout enforcement semantics. Existing outer `tokio::time::timeout` behavior remains; `StepContext.timeout` mirrors the same value for backend visibility.

---

## 3. Architecture

### 3.1 Context-source rule

Each non-`Step` caller constructs a `StepContext` from the closest owning configuration source:

| Module | Current gap | Context source |
|---|---|---|
| `src/backend/mod.rs::run_query_with_config` | Uses `from_prompt(..., None)` for every backend | `Config.backends[backend.name()].model`, `Config.backends[backend.name()].timeout`, `Config.defaults.timeout` |
| `src/conductor.rs::execute_tool` | Query tool drops selected backend config fields | `self.config.backends[backend_name]` plus `self.config.defaults` |
| `src/spawn.rs::plan_with_backend` | Planner fallback drops model/timeout | Selected `backend.name()` lookup in `self.config.backends` plus defaults |
| `src/spawn.rs::execute` | Agent execution drops model/timeout after backend delegation | Selected `backend.name()` lookup in cloned config plus defaults |
| `src/team.rs::execute` | Primary and second-opinion queries drop selected backend config fields | `Team` stores a cloned `Config`; each selected backend resolves against it |
| `src/debate.rs::get_responses` | Debate response rounds drop selected backend config fields | Existing `self.config` and selected `backend.name()` |

`Debate::get_initial_positions` already delegates to `backend::run_query`, so fixing `run_query_with_config` covers that path.

### 3.2 Shared helper location

Add a small public helper in `src/backend/mod.rs` near `run_query_with_config`:

```rust
pub fn step_context_for_backend<'a>(
    prompt: &'a str,
    cwd: &'a Path,
    config: &'a Config,
    backend_name: &str,
) -> StepContext<'a> {
    let backend_config = config.backends.get(backend_name);
    let timeout_secs = backend_config
        .and_then(|cfg| cfg.timeout)
        .unwrap_or(config.defaults.timeout);

    StepContext {
        model: backend_config.and_then(|cfg| cfg.model.as_deref()),
        timeout: Some(Duration::from_secs(effective_timeout_secs(timeout_secs))),
        ..StepContext::from_prompt(prompt, cwd, None)
    }
}
```

Add a private helper for the existing "0 means effectively no timeout" convention:

```rust
fn effective_timeout_secs(timeout_secs: u64) -> u64 {
    if timeout_secs == 0 {
        // Preserve the existing convention where 0 disables timeout by mapping
        // it to a near-infinite duration used by the outer timeout wrapper.
        365 * 24 * 60 * 60
    } else {
        timeout_secs
    }
}
```

The helper intentionally leaves future fields at safe defaults:

```rust
history: &[]
sandbox: None
schema: None
options: None
```

This implements the lesson from `.pi/lessons/clo-371-stepcontext-migration-lessons.md § L1`: migrating to a carrying struct is incomplete unless call sites populate fields promised by the current slice.

### 3.3 `run_query_with_config`

Replace the inline timeout closure and bare context construction with helper-based construction:

```rust
let timeout = backend_timeout_secs(config, backend.name());
let ctx = step_context_for_backend(&prompt, &cwd, config, backend.name());
let result = tokio::time::timeout(Duration::from_secs(timeout), backend.query(ctx)).await;
```

Because `query_one` is an async closure, it must receive an owned `Arc<Config>` or a cloned `Config` reference that outlives the future. Preferred implementation:

- clone `config` into `Arc<Config>` once in `run_query_with_config`;
- pass `Arc<Config>` into `query_one` alongside `Arc<str>` and `Arc<Path>`;
- use `config.as_ref()` inside the async block.

This preserves parallel execution and avoids borrowing `config` across spawned futures unsafely.

### 3.4 `conductor`

After looking up `backend_config`, construct context with the shared helper:

```rust
let ctx = backend::step_context_for_backend(prompt, cwd, &self.config, backend_name);
let result = backend.query(ctx).await?;
```

The conductor’s own Claude API model (`self.model`) is not propagated to tool backends; tool calls target the selected backend and must use that backend’s configured model.

### 3.5 `spawn`

`Spawn` already owns `Config`, so both remaining query sites can use the shared helper:

- `plan_with_backend`: lookup by `backend.name()`.
- `execute`: include `let config = self.config.clone();` in each async closure capture, then call `backend::step_context_for_backend(&prompt, &cwd, &config, backend.name())`.

This avoids adding model fields to `AgentTask`; agent planning/delegation still selects a backend, and the backend config remains the model source.

### 3.6 `team`

Change `Team` to retain cloned config:

```rust
pub struct Team {
    backends: Vec<Arc<dyn Backend>>,
    delegator: Delegator,
    cwd: std::path::PathBuf,
    config: Config,
}
```

Populate it in `Team::new(config, cwd)`. Both primary and second-opinion queries use the selected backend name and `self.config` with `step_context_for_backend`.

### 3.7 `debate`

`Debate` already stores `config: &'a Config`; replace the response-round bare context with:

```rust
let ctx = backend::step_context_for_backend(&prompt, &self.cwd, self.config, backend.name());
backend.query(ctx).await
```

No change is needed for initial positions beyond the `run_query_with_config` fix.

---

## 4. Public API surface

### New helper exported from `src/backend/mod.rs`

```rust
pub fn step_context_for_backend<'a>(
    prompt: &'a str,
    cwd: &'a Path,
    config: &'a Config,
    backend_name: &str,
) -> StepContext<'a>;
```

This helper is intentionally small and additive. It does not change the `Backend` trait or existing structs.

### Private helper in `src/backend/mod.rs`

```rust
fn backend_timeout_secs(config: &Config, backend_name: &str) -> u64;
fn effective_timeout_secs(timeout_secs: u64) -> u64;
```

`backend_timeout_secs` mirrors current behavior:

```rust
config.backends
    .get(backend_name)
    .and_then(|b| b.timeout)
    .unwrap_or(config.defaults.timeout)
```

`effective_timeout_secs` preserves current `timeout = 0` behavior by mapping zero to one year.

### Struct changes

Only `Team` gains one private field:

```rust
config: Config,
```

No external construction API changes are required because `Team::new(config, cwd)` already receives `Config`.

---

## 5. Assumptions

| # | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | Backend config is the correct source of model selection for non-`Step` calls because these paths have no active workflow `Step`. | high | Verified by `Config::BackendConfig.model` and absence of `model` fields in `AgentTask`, `Team`, and `Debate`; tests cover helper model resolution. |
| A2 | Mirroring the existing outer timeout into `StepContext.timeout` is behavior-preserving because runtime timeout enforcement remains unchanged. | high | Covered by `.pi/lessons/clo-371-stepcontext-migration-lessons.md § L1`; tests assert helper timeout equals existing backend/default timeout resolution. |
| A3 | Leaving `history`, `sandbox`, `schema`, and `options` empty/None is safe for this slice because no non-`Step` caller currently has those values. | high | Matches CLO-371 phase-equivalent defaults and is verified by existing `StepContext::from_prompt` tests plus new helper tests. |
| A4 | Cloning `Config` into `Team` and `Spawn` async closures is acceptable because config is small and already `Clone`. | medium | Verified by compile time and existing config clone usage in `Conductor`/`Spawn`; no performance-critical hot loop is introduced. |
| A5 | A public helper in `backend::mod` is sufficient and does not require re-export changes because it only uses already-public `Config`, `Path`, and `StepContext`. | high | Covered by `.pi/lessons/clo-371-stepcontext-migration-lessons.md § L2`; compiler validates public signature visibility. |
| A6 | `backend.name()` always matches the key used in `Config.backends` for configured backends. | medium | Verified by existing `create_backend`/`get_backends` construction path and by fallback behavior when config lookup returns `None`. |

---

## 6. Test plan

### Unit tests

Add tests in `src/backend/mod.rs`:

1. `test_step_context_for_backend_uses_backend_model`
   - Build default config with `backends["ollama"].model = Some("custom")`.
   - Assert `ctx.model == Some("custom")`.
2. `test_step_context_for_backend_uses_backend_timeout`
   - Set backend timeout to `42`.
   - Assert `ctx.timeout == Some(Duration::from_secs(42))`.
3. `test_step_context_for_backend_falls_back_to_default_timeout`
   - Clear backend timeout and set defaults timeout to `17`.
   - Assert `ctx.timeout == Some(Duration::from_secs(17))`.
4. `test_step_context_for_backend_preserves_phase1_defaults`
   - Assert empty history and None sandbox/schema/options.
5. `test_effective_timeout_secs_preserves_zero_as_no_timeout`
   - Assert zero maps to `365 * 24 * 60 * 60`, matching existing behavior.

### Module/call-site coverage

- Add a lightweight `Team::new` or `Team` construction test only if constructing backends can be done without requiring installed CLI tools. If not, rely on helper tests plus compile coverage.
- Use grep as a static guard:

```bash
rg -n "StepContext::from_prompt\([^\n]*None|from_prompt\([^\n]*,\s*None" src/conductor.rs src/spawn.rs src/team.rs src/debate.rs src/backend/mod.rs
```

Expected: no remaining direct bare non-step query contexts, except workflow-specific or test code that intentionally verifies defaults.

### Integration / acceptance commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

If bedrock dependencies are available:

```bash
cargo build --features bedrock
```

### Manual checks

- Run one backend query command with a configured model (where available) and inspect verbose/backend behavior if practical.
- Confirm Team and Spawn commands still choose backends the same way; only context population changes.

---

## 7. Migration / rollout

1. Add helper tests in `src/backend/mod.rs` and make them fail against current code if practical.
2. Add `backend_timeout_secs`, `effective_timeout_secs`, and `step_context_for_backend`, including an explanatory comment for the `timeout = 0` convention.
3. Update `run_query_with_config` to use helper for model and timeout propagation.
4. Update `conductor`, `spawn`, `team`, and `debate` query sites.
5. Add `config: Config` to `Team` and clone/capture config where async closures need owned access.
6. Run grep guard and acceptance commands.

The change is additive and local. No data migration, config migration, or release flag is required.

---

## 8. Open questions

- **Q1: Should `timeout = 0` produce `StepContext.timeout = None` instead of one year?**
  - Resolved for this PR: preserve current behavior by mirroring the effective outer timeout. Changing semantics belongs in a dedicated timeout policy follow-up.
- **Q2: Should Team/Spawn introduce task-level model overrides?**
  - Resolved out of scope: this PR only propagates existing backend config. Task-level overrides need product/design work.
- **Q3: Should helper be private instead of public?**
  - Resolved: make it public within `backend` API because multiple modules need it and helper tests live naturally beside backend utilities.
