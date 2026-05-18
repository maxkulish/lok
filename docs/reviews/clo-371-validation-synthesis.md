# Pre-PR validation: clo-371

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-18
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Verdict FAIL, 3 findings (2 MEDIUM, 1 LOW) |
| Gemini | OK | Verdict PASS_WITH_NOTES, 1 MEDIUM finding + 1 LOW tooling artifact |
| Claude fallback | SKIPPED | Both external reviewers succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- **Populate `StepContext.timeout` at all Step-aware call sites.** Both reviewers independently flagged this; the design's §3.5 explicitly specifies a `step_context(step, workflow, prompt, cwd)` helper that sets `timeout: workflow.step_timeout(step).map(Duration::from_millis)`, and `run_llm_validation` should set `timeout` from `validate_config.timeout_ms`. Current code uses `StepContext::from_prompt(...)` which hardcodes `timeout: None` at `src/backend/context.rs:36`. Outer `tokio::time::timeout` preserves runtime behavior, so there is no Phase-1 regression — but the FR-19a/FR-20a carrier contract is incomplete (the field is documented but never set). Affected sites: `src/workflow.rs:763`, `1746`, `1968`, `2077`, `2217`. Bounded fix: add the helper from the design, use it in step-aware sites, and inline-construct with `timeout` set in `run_llm_validation` and `WorkflowEditRequester`.
- **Re-export `Message`, `Role`, `SandboxMode`, `StepOptions` from `src/backend/mod.rs`.** Design §4.1 specifies `pub use context::{StepContext, Message, Role, SandboxMode, StepOptions};`. Current code only re-exports `HealthStatus, StepContext` at `src/backend/mod.rs:14`. Without the other re-exports, downstream callers cannot construct a non-default `StepContext` cleanly. One-line fix.

## Out of Scope / Deferred
- **Makefile `pi-init` target addition** (Codex LOW). Real but not part of CLO-371 design or plan. Recommend splitting into a separate PR, but not blocking — the change is additive and non-functional to the trait migration. If the orchestrator chooses to keep it in the PR, call it out in the PR description.

## False Positives / Tooling Artifacts
- **`test_retry_workflow` local failure** (Gemini LOW). Caused by macOS Seatbelt blocking `/tmp/lok_retry_test_counter` writes inside the agent sandbox. Not a regression; passes on unrestricted hosts/CI.

## Recommendation
PROCEED_WITH_FIXES. Apply two bounded fixes in one iteration: (1) introduce the `step_context` helper in `src/workflow.rs` and populate `timeout` from `workflow.step_timeout(step)` at the five step-aware sites; for the validation path inline-construct `StepContext` with `timeout` from `validate_config.timeout_ms`; (2) extend the re-export in `src/backend/mod.rs` to `pub use context::{HealthStatus, Message, Role, SandboxMode, StepContext, StepOptions};`. After those, the PR meets the design's FR-19a/FR-20a contract and §4.1 public-API surface. The Makefile `pi-init` change should be split out unless explicitly tied to this task.

## Re-validation
- Fix iteration count: 1
- Applied Must Fix #1: `src/workflow.rs` now has `step_context(step, workflow, prompt, cwd)` that sets `timeout: workflow.step_timeout(step).map(std::time::Duration::from_millis)`. Validation and edit-retry paths inline-construct `StepContext` with their timeout values; multi-backend and synthesis paths populate `timeout` from the effective step timeout.
- Applied Must Fix #2: `src/backend/mod.rs` now re-exports `HealthStatus`, `Message`, `Role`, `SandboxMode`, `StepContext`, and `StepOptions`.
- Verification after fix: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --features bedrock`, and both workflow grep gates passed.
