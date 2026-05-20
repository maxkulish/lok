# Design Review: CLO-384 — Gemini (manual)

Review performed manually (lok design-review workflow broken — CLO-382 L2 `depends_on` bug).

## Verdict: approve_with_changes

The design is sound. The architecture (unified resolution function + dual-format deserializer) correctly addresses the discovery gap. No fundamental flaws. 7 findings below — 5 additive, 2 refinement.

---

## Finding 1: Missing caller enumeration for `step_context` signature change [Additive]

The design changes `step_context(step, workflow, prompt, cwd)` → `step_context(step, config, backend_name, prompt, cwd)`. Two caller sites in `src/workflow.rs` must be updated:

- **Line 2255** (main backend query path): `let ctx = step_context(step, workflow, &prompt, &cwd);`
- **Line 1780** (for_each iteration): `let ctx = step_context(step, workflow, &iter_prompt, &cwd);`

Both are inside `async move` closures that already have `config` in scope via `config: Arc<Config>` or similar — the design should note this explicitly so the implementor doesn't add new Arc-wrapping.

**Suggestion:** Add a "Caller update checklist" to the Architecture section listing the exact line numbers and the variable names that the new `config` and `backend_name` args map to.

## Finding 2: Synthesis path bypasses `step_context` [Additive]

At `src/workflow.rs:2111`, the consensus synthesis step constructs StepContext manually:

```rust
let ctx = backend::StepContext {
    timeout: step_timeout.map(std::time::Duration::from_millis),
    ..backend::StepContext::from_prompt(&synth_prompt, &cwd, None)
};
```

This bypasses `effective_timeout()` and the layered resolution. It uses the pre-resolved `step_timeout` variable (which comes from `workflow.step_timeout(step)` — the old single-layer resolution). After the change, this should use `effective_timeout()` or the `step_context()` builder.

**Suggestion:** Either call `step_context(step, config, "synth_backend", &synth_prompt, &cwd)` or inline `effective_timeout(step.timeout, synth_backend_name, config)` at this site.

## Finding 3: Config::default() Gemini timeout needs explicit Duration construction [Additive]

`Config::default()` sets `timeout: Some(600)` for Gemini (currently `u64` seconds). After the type change to `Option<Duration>`, this becomes:

```rust
timeout: Some(Duration::from_secs(600)),
```

The design notes that `Defaults` changes from `u64` to `Option<Duration>` but doesn't call out the manual `Config::default()` impl that constructs `BackendConfig` literals directly. The Gemini entry is the only one with a non-None timeout — it's easy to miss during implementation and produces a type error.

**Suggestion:** Add a bullet to the Migration section: "Gemini's `Config::default()` entry: `timeout: Some(600)` → `timeout: Some(Duration::from_secs(600))`."

## Finding 4: Multi-backend step timeout differentiation [Additive]

The test plan lists `test_multibackend_timeout_per_backend` but the design doesn't explicitly show how the workflow runner handles multi-backend steps. In the current code at line 2240-2260, there's a loop over backends that calls `step_context(step, workflow, &prompt, &cwd)` with the same arguments for each backend. After the change, each iteration must pass its own `backend_name` so `effective_timeout()` resolves the correct `BackendConfig.timeout`.

This is correct architecturally (the `backend_name` parameter enables it) but should be noted as a behavioral change: previously, multi-backend steps all used the same step/workflow timeout; now they can have per-backend timeouts.

**Suggestion:** Add a note under Architecture > Call site changes: "In multi-backend loops, each iteration passes its specific `backend_name`, enabling per-backend timeout resolution."

## Finding 5: `WorkflowEditRequester.timeout_duration` source [Additive]

The `WorkflowEditRequester` struct (line 1083) carries `timeout_duration: std::time::Duration` used for fix/retry queries. This is populated at construction (line 1097) from a value passed by the workflow runner. After the change, verify that this value is also resolved through `effective_timeout()` — or document why it uses a separate resolution path.

This is out of scope for the main timeout chain (it's the edit-verify-fix loop, not step queries) but the design should acknowledge it so the implementor checks.

**Suggestion:** Add a brief note: "EditRequester path unchanged — carries its own timeout from fix_retry loop. Not part of the step>backend>global chain."

## Finding 6: NO_TIMEOUT_SECS → NO_TIMEOUT rename not documented [Refinement]

The existing code defines `NO_TIMEOUT_SECS: u64` (line 317, `src/backend/mod.rs`). The design replaces it with `NO_TIMEOUT: Duration`. This is a rename + type change that should be noted in the Migration section so grepping for the old constant catches all references.

**Suggestion:** Add to Migration: "`NO_TIMEOUT_SECS` (u64) → `NO_TIMEOUT` (Duration). `effective_timeout_secs()` is removed. Grep for both names during implementation to ensure no stale references."

## Finding 7: `DEFAULT_TIMEOUT` vs `default_timeout()` constant source [Refinement]

The design introduces `DEFAULT_TIMEOUT: Duration = Duration::from_secs(300)` as a new constant. The existing code uses `fn default_timeout() -> u64 { 300 }` in `src/config.rs`. The design should note that `Defaults::default()` no longer calls `default_timeout()` (since `timeout` becomes `None`), but the `default_timeout()` helper function may still be referenced elsewhere.

**Suggestion:** Add to Migration: "`fn default_timeout()` in `src/config.rs` is no longer called from `Defaults::default()`. Grep for remaining callers; if none, remove the function. The authoritative default moves to `DEFAULT_TIMEOUT` in `src/backend/mod.rs`."
