# Pre-PR validation: clo-655

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-09-14
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Review completed with verdict FAIL: one HIGH finding (live gates AC13/AC14) and one LOW finding (truncation length) |
| Claude fallback | SKIPPED | The Codex review succeeded |

## Verdict
PASS

## Must Fix Before PR
- None.

## Out of Scope / Deferred
- **AC13 and AC14 live gates did not pass (Codex HIGH).** This is real, but it does not block the PR. I checked it against the spec.
  - **AC13:** Ollama never succeeded. Both success-path runs hit the Ollama cloud session usage limit, so the skipped-fallback path never ran live. That is a backend availability problem, not a defect in this change. The branch did test the skipped-fallback render offline: `SKIPPED` rendered, and only the ollama and synthesis files were written (`docs/status/clo-655-workflow.yaml:86-90`).
  - **AC14:** `claude_fallback` ran and all three files were written, so the original defect is fixed: the pipeline no longer dies before the fallback. The fallback itself timed out. That timeout comes from `timeout = "2m"` at `.lok/workflows/design-review.toml:77`, which this branch did not change. The spec forbids changing timeouts in this change (`specs/...:143`).
  - **The spec already covers both cases.** Its escalation clause (`specs/...:162`) says that when AC13 or AC14 fails for a reason outside template rendering, the output is recorded as a follow-up, the PR description states which live gates passed, and CLO-655 stays open. The spec calls both gates "required to close CLO-655" (`:113-114`), not required to open the PR. AC12 is the only live gate that blocks the PR, and it passed on three runs.
  - **Conditions for the PR phase:**
    - The PR description lists gate 1 as PASS, gate 2 as NOT VERIFIED LIVE (quota) and gate 3 as PARTIAL (fallback timeout).
    - The PR description has the "Behaviour change" heading.
    - The PR must not use a Linear closing keyword for CLO-655 (for example `Fixes CLO-655`), because that would close the ticket on merge. CLO-656 can be closed.
- **The 2m fallback timeout makes AC14 impossible to pass as the spec stands.** File a follow-up linked to CLO-655 that amends the timeout. For comparison, spec-review uses 5m and pre-pr-validation uses 10m. AC14 can be rerun after that change lands.
- **`write_reviews` accepts timeout/error output as a fallback review (Codex recommendation).** The new filter keeps the old semantics exactly: output that is not blank and has no `REVIEW_FAILED`. The spec forbids changing reviewer behaviour. This belongs with CLO-624 (bad invocation vs empty response) as a follow-up.
- **Rerun AC13 when Ollama quota is available,** using the spec's marker-file procedure and the worktree binary.

## False Positives / Tooling Artifacts
- **The 120-character limit gives up to 123 characters (Codex LOW).** `expression_context` (`src/workflow.rs:3240-3244`) cuts the quoted expression at 120 characters and then adds `...` to show the cut. The spec says "truncated to 120 characters" (`specs/...:142`), and the code follows a reasonable reading of it. The limit only affects a diagnostic message, so nothing depends on the exact length. This is optional polish, not a defect.
- **Codex could not run a fresh `cargo build`** because its environment is read-only. It did run the prebuilt unit suites, which passed. The branch record shows `make check`, 578 binary tests and `cargo +1.83.0 check` passing. I did not rerun them in this synthesis.

## Recommendation
PROCEED. Everything else in the static implementation matches the approved spec:
- Comment syntax is disabled engine-wide through named placeholder constants.
- `ParseError` now holds only `SyntaxError`.
- Error attribution is conservative, and `GENERIC_VAR_RE` is gone.
- `design-review.toml` has the `wc -c` form, the `claude_fallback` guards and the quoted heredoc.
- The `${#VAR}` end-to-end regression test exists.
- The docs section covers all four AC15 points.

The only open items are the two live gates. The spec's escalation clause already decides how to handle them, so the user does not need to make a new decision. In the PR phase, the orchestrator should:
1. State the live gate results in the PR description, with the "Behaviour change" heading.
2. Avoid any Linear closing keyword for CLO-655 and keep the ticket open.
3. File follow-ups for the 2m fallback timeout, for timeout output being accepted as a fallback review, and for rerunning AC13 and AC14.
