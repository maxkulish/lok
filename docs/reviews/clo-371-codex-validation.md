# Pre-PR validation: clo-371

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-18
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

**MEDIUM** - `StepContext.timeout` is never populated for Step-aware calls.
The design requires validation and Step call sites to carry effective timeout in `StepContext`, but all workflow sites use `StepContext::from_prompt(...)`, which hardcodes `timeout: None` in [src/backend/context.rs](/Users/mk/Code/orchestrator/lok/src/backend/context.rs:36). Examples: validation [src/workflow.rs](/Users/mk/Code/orchestrator/lok/src/workflow.rs:763), loop steps [src/workflow.rs](/Users/mk/Code/orchestrator/lok/src/workflow.rs:1746), multi-backend [src/workflow.rs](/Users/mk/Code/orchestrator/lok/src/workflow.rs:1968), synthesis [src/workflow.rs](/Users/mk/Code/orchestrator/lok/src/workflow.rs:2077), single-backend [src/workflow.rs](/Users/mk/Code/orchestrator/lok/src/workflow.rs:2217). Existing outer `tokio::timeout` behavior is preserved, but the FR-19a/FR-20a carrier contract is incomplete.

**MEDIUM** - Context types required by the public API are not re-exported.
The design specifies `pub mod context` and exports `StepContext, Message, Role, SandboxMode, StepOptions`; current code keeps `context` private and only re-exports `HealthStatus` and `StepContext` in [src/backend/mod.rs](/Users/mk/Code/orchestrator/lok/src/backend/mod.rs:5). This makes fields like `sandbox`, `history`, and `options` difficult or impossible for downstream callers to populate intentionally.

**LOW** - Unrelated Makefile changes are included in the CLO-371 branch.
`make pi-init` was added in [Makefile](/Users/mk/Code/orchestrator/lok/Makefile:72), but it is not part of the design or implementation plan. It should be split out unless intentionally tied to this task.

## Missing Items

- Design G4 / FR-20a is only partially implemented: Step-aware call sites migrate to `StepContext`, but they do not construct it from the active Step's effective timeout as specified.
- Public API surface from design §4.1 is incomplete: `Message`, `Role`, `SandboxMode`, and `StepOptions` are not exported.

## Recommendations

- Add a workflow helper matching the design, e.g. `step_context(step, workflow, prompt, cwd)`, setting `timeout: workflow.step_timeout(step).map(Duration::from_millis)`.
- For validation/edit retry paths without a `Step`, construct `StepContext` inline and set `timeout` from `validate_config.timeout_ms` or `self.timeout_duration`.
- Change exports to match the design: either `pub mod context;` plus explicit re-exports, or re-export all required context types from `backend/mod.rs`.
- Move the `Makefile`/`.pi` documentation changes to a separate PR unless they are required for CLO-371.
