# Pre-PR validation: clo-383

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-20
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Structured findings produced (1 HIGH, 1 MEDIUM, 2 LOW) |
| Gemini | REVIEW_FAILED | Empty model output due to untrusted-directory restriction; no usable findings |
| Claude fallback | SKIPPED | Codex reviewer succeeded; fallback gate not tripped |

## Verdict
PASS_WITH_NOTES

The HIGH consensus-path finding is real and in scope per the design's stated assumption (`apply_edits` must default identically across every subprocess backend in a multi-backend step), but it's a bounded fix: thread `step.apply_edits` and `step.sandbox` into the per-backend `StepContext` spawned in `src/workflow.rs:1999` (and ideally route via `step_context()`). The two missing tests called out by the design's test plan are also small additions. None of this requires a pivot or a redesign.

## Must Fix Before PR
- **Consensus fan-out drops `apply_edits` and `sandbox`** — `src/workflow.rs:1999` builds the per-backend `StepContext` with only `timeout` set, so a multi-backend step with `apply_edits = true` and no explicit `sandbox` still launches Codex as `read-only` and Gemini without `--approval-mode`. This contradicts the design's high-confidence assumption (`docs/designs/clo-383-apply-edits-sandbox-default.md:240`) that consensus steps get the same FR-22 defaulting on every subprocess backend, and renders the open question at lines 313-314 moot in the wrong direction. Fix: capture `step.apply_edits` and `step.sandbox` before the `tokio::spawn` and set both fields on the spawned `StepContext` (or refactor the path to call `step_context()`).
- **Add the workflow-level `step_context_threads_apply_edits` test** — the design's test plan (line 283) names this test explicitly; the current branch only covers backend argv builders. Without it, the consensus regression above would still pass CI.
- **Add a workflow-driven coverage test for `apply_edits = true`** — design lines 286-291 require a test that observes `workspace-write` / explicit `read-only` flowing through the actual workflow path. The argv-builder unit tests don't exercise the field-threading regression.
- **Trailing whitespace blocks `git diff --check main...HEAD`** — `docs/reviews/clo-383-design-gemini.md:3-4` and `docs/reviews/clo-383-design-synthesis.md:3-4`. Trivial cleanup but the pre-merge gate command in the design's manual-verification section assumes a clean run.

## Out of Scope / Deferred
- Threading `apply_edits` / `sandbox` through the validate-step path (`src/workflow.rs:790`), retry/fix path (`src/workflow.rs:1216`), and consensus-synthesis path (`src/workflow.rs:2112`). These also build `StepContext` from scratch, but they query a *different* prompt for a different purpose (validation, error-fix re-query, synthesis) and the design only commits to the per-step query. Worth a follow-on issue if operators report drift, but not a CLO-383 blocker.
- Promoting the duplicated resolution `match` into `StepContext::effective_sandbox()` (design open question line 313). Deferred by design; do not bundle.
- Parse-time validation of `apply_edits = true` against non-edit-capable backends (Claude / Ollama / Bedrock) — design line 314 explicitly leaves this to a later issue.

## False Positives / Tooling Artifacts
- **`println!` vs `eprintln!` for the warning** (Codex LOW). The design's Assumptions block (line 243) explicitly accepts either: it lists `println!` first but says "Verification path: grep `println!` / `eprintln!`" — both are pre-existing diagnostic patterns in the project. Picking `eprintln!` for a warning is the more conventional choice and is not a spec violation worth blocking on.
- **Gemini REVIEW_FAILED**. Tooling-side: Gemini's untrusted-directory restriction returned empty output. No actionable signal; treat as missing-reviewer rather than dissent.

## Recommendation
PROCEED_WITH_FIXES. Bounded one-iteration cleanup before opening the PR:
1. In `src/workflow.rs:1999`, set `sandbox: step.sandbox` and `apply_edits: step.apply_edits` on the spawned `StepContext` (or call `step_context(step, workflow, &prompt, &cwd)` and clone what's needed across the `tokio::spawn` boundary).
2. Add `step_context_threads_apply_edits` to the `workflow.rs` test module per design line 283.
3. Add a workflow-driven coverage test for the multi-backend consensus case with `apply_edits = true` (one Codex backend, one Gemini backend, no `sandbox`) — asserts both effective-sandbox resolutions fire end-to-end.
4. Strip trailing whitespace from `docs/reviews/clo-383-design-gemini.md` and `docs/reviews/clo-383-design-synthesis.md` so `git diff --check` is clean.

Re-run the pre-merge gate (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`) before flipping to PR.
