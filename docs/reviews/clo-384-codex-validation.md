# Pre-PR validation: clo-384

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-20
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

**HIGH** - Multi-backend workflow timeout resolution still includes `Workflow.timeout`, violating the specified `step > backend > global` chain.  
At [src/workflow.rs:1689](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:1689), `step_timeout` is computed as `step.timeout.or(workflow.timeout)`, then passed to `effective_timeout()` at [src/workflow.rs:2007](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:2007). That means a workflow-level timeout overrides per-backend timeout in multi-backend runs, while the single-backend path does not. This directly contradicts the design’s decision to remove `Workflow.timeout` from the runtime chain.

**HIGH** - Edit-fix LLM re-query still uses the old duplicated timeout calculation instead of `effective_timeout()`.  
`timeout_duration` is derived from `step.timeout.or(workflow.timeout)` with a hardcoded 120s fallback at [src/workflow.rs:1689](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:1689) and [src/workflow.rs:1697](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:1697). That value is passed into `WorkflowEditRequester` at [src/workflow.rs:2369](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:2369), which sets `StepContext.timeout` and wraps `backend.query()` with it at [src/workflow.rs:1222](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:1222) and [src/workflow.rs:1230](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:1230). Backend/global timeouts are ignored on this backend query path.

**MEDIUM** - Negative timeout integers deserialize into huge positive durations.  
Both duration visitors cast signed integers directly to `u64`: [src/config.rs:165](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/config.rs:165) and [src/config.rs:197](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/config.rs:197). `timeout = -1` becomes `Duration::from_secs(u64::MAX)` or `Duration::from_millis(u64::MAX)` instead of a validation error, effectively bypassing intended timeout validation.

**LOW** - Unrelated generated artifact appears committed.  
`autoresearch.jsonl` contains a discarded “Wrong tool” record and is unrelated to CLO-384 implementation.

## Missing Items

- `test_step_context_populates_timeout` from ST4 is not implemented.
- `test_multibackend_timeout_per_backend` from ST4 is not implemented.
- ST5 integration tests are missing: sleepy backend timeout, `StepResult.failure.kind == Timeout`, workflow TOML string timeout execution, and config-level string timeout execution.
- Full pre-merge gate was not verified here. I ran `cargo fmt --check` successfully, but did not run `cargo clippy` or `cargo test` in this read-only sandbox.

## Recommendations

- In multi-backend execution, pass only `step.timeout` into `effective_timeout()`, or route each backend through the shared `step_context()` helper.
- For `WorkflowEditRequester`, carry the already resolved effective timeout for the active backend instead of the old `timeout_duration`.
- Reject negative integers in both duration deserializers with `de::Error::invalid_value`.
- Add the missing ST4/ST5 tests before merging.
- Remove `autoresearch.jsonl` from the branch.
