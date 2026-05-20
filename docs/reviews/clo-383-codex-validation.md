# Pre-PR validation: clo-383

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-20
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

HIGH: Consensus/multi-backend LLM steps drop both `apply_edits` and `sandbox` before querying each backend.
In [src/workflow.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/workflow.rs:1999), the spawned per-backend context is built with `StepContext::from_prompt(...)` plus only `timeout`, so `apply_edits` stays `false` and `sandbox` stays `None`. A step with `backends = [...]`, `apply_edits = true`, and no explicit sandbox will still run Codex as `read-only` and Gemini without `auto_edit`. This directly contradicts the design assumption that consensus fan-out should get the same defaulting rule on every subprocess backend.

MEDIUM: The design's required workflow/integration coverage is missing.
The design calls for `step_context_threads_apply_edits` and an integration-style workflow test for `apply_edits = true` observing `workspace-write` / explicit `read-only`. The branch only adds backend helper tests in [src/backend/codex.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/backend/codex.rs:291) and [src/backend/gemini.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/backend/gemini.rs:463). This is why the consensus path regression above is not covered.

LOW: `git diff --check main...HEAD` fails on trailing whitespace in docs.
Failures are in `docs/reviews/clo-383-design-gemini.md` lines 3-4 and `docs/reviews/clo-383-design-synthesis.md` lines 3-4.

LOW: Warning emission does not exactly match the design wording.
The design says emit the `apply_edits + read-only` warning via `println!`; implementation uses `eprintln!` in [src/backend/codex.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/backend/codex.rs:47) and [src/backend/gemini.rs](/Users/mk/Code/orchestrator/lok--feat-clo-383-sandbox/src/backend/gemini.rs:131). This may be acceptable as backend diagnostics, but it is a spec mismatch.

## Missing Items

- Workflow/unit test proving `step_context()` threads `apply_edits`.
- Integration-style workflow test proving `apply_edits = true` defaults Codex/Gemini sandbox behavior through the actual workflow path.
- Coverage for multi-backend consensus steps with `apply_edits = true`.

## Recommendations

- In the consensus branch, capture `step.apply_edits` and `step.sandbox` before spawning, then include both fields in the per-backend `StepContext`.
- Add tests for single-backend and multi-backend workflow paths, not only backend argv builders.
- Fix trailing whitespace and run `git diff --check main...HEAD`.
- I could not independently run `cargo test` / `clippy` in this read-only environment; the status file claims they passed.
