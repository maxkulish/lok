# Pre-PR validation: clo-383

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-20
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

- MEDIUM: Retry/fix LLM calls do not preserve the step's `apply_edits` or `sandbox` intent.
  [src/workflow.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/workflow.rs:1216) builds retry `StepContext` with `from_prompt(...)`, so `apply_edits` defaults back to `false` and `sandbox` to `None`. This requester is created from an `apply_edits` step at [src/workflow.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/workflow.rs:2364). If a verified edit step needs a fix retry, Codex will fall back to `-s read-only` and Gemini will omit `--approval-mode`, which is inconsistent with the FR-22 per-step intent.

- LOW: The explicit read-only warning is emitted but not covered by tests, and uses `eprintln!` while the goal says `println!`.
  Implemented at [src/backend/codex.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/backend/codex.rs:47) and [src/backend/gemini.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/backend/gemini.rs:131). Existing read-only tests only assert the sandbox/approval flag, not the warning.

## Missing Items

- The design asks for an integration/workflow-level test that drives `apply_edits = true` and observes the argv/shell command shape. I found only backend unit tests and a `step_context` unit test; no new `tests/apply_edits_sandbox.rs` or equivalent integration coverage.

## Recommendations

- Thread `sandbox` and `apply_edits` through `WorkflowEditRequester`, then set them in its retry `StepContext`.
- Add coverage for verified edit retries so the regression cannot recur.
- Add warning assertions for `apply_edits=true + read-only`, or refactor the resolver to return a warning/result that is easier to test.
- Add the design-requested workflow-level coverage, even if it remains a lightweight unit-style test around parsed workflow state and backend command construction.

Verification: `cargo fmt --check` passed. `cargo test` could not run in this environment because Cargo could not open `target/debug/.cargo-lock` under the read-only sandbox.
