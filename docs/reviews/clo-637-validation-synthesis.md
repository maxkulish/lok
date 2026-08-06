# Pre-PR validation: clo-637

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-08-06
**Pipeline**: lok pre-pr-validation
---

All findings are now independently verified. Here is the binding synthesis.

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Full review of `git diff main...HEAD` at `c636ef9`, findings verified with local probes |
| Claude fallback | SKIPPED | Codex review succeeded |

## Verdict

PASS_WITH_NOTES

Codex reported FAIL, but its own findings don't meet the FAIL bar: there is no pivot or scope issue, and the implemented dual-shape poll matches the spec's design. The two real defects are a missing one-line guard (duplicated in two files) and a prose reconciliation the spec's decomposition item 3 already called for - both fit comfortably in one bounded fix iteration. The "unverified AC1-AC4" gap is plan item 4's deliverable, which the spec directs into the PR body; it is due at PR creation, not evidence the implementation is wrong.

## Must Fix Before PR

- **Guard the head SHA in both poll snippets** (`.claude/commands/pr/review.md:346`, `.pi/skills/pr-review-cycle.md:134`). Both validate the `since` bound but not the head; I reproduced the failure locally - with `$h=""`, jq's `contains($h)` is true, so the completion-comment path (Shape 2) accepts any fresh Qodo completion comment without proving the covered commit. This violates the spec's Must constraints (fail closed; every path pairs freshness with a covered-commit check). Fix: reject an invalid head before polling, ideally `^[0-9a-f]{40}$`, mirroring the existing `REQUESTED_AT` guard. Note Shape 1 (`commit_id==$h`) does not fail open; only Shape 2 does.
- **Reconcile the two contradictory prose passages in `.pi/skills/pr-review-cycle.md`.** Lines 35-38 still state the review-object-only rule ("Bot reviewers post a GitHub review... Poll for that observable review"), and lines 570-573 still recommend comparing "the comment's `updated_at`" - the exact signal the spec's Must-not list forbids and the same file's new lines 123-125 explicitly reject. Spec decomposition item 3 required updating adjacent prose; an executor following line 570 would reintroduce the original defect.
- **Trivial, bundle into the fix commit:** remove the blank line at EOF of `docs/reviews/clo-637-spec-review-ollama.md` (`git diff --check` confirms).
- **Verification evidence (due at the PR transition, which this verdict gates):** run the AC1-AC4 single-probe queries against PRs #71/#80 plus the negative probes for the new head guard (empty head, spoof-shaped login), shellcheck the changed blocks, and record commands + output + raw JSON snapshots in the PR body, per the spec's Verification method and decomposition item 4. Codex could not reach live GitHub; the orchestrator's environment can.

## Out of Scope / Deferred

- **Exact Qodo app-login matching.** The spec explicitly prescribes keeping `test("qodo")` (Evaluation edge case: "keep `test(\"qodo\")` matching `qodo-code-review[bot]`"), and the same loose match pre-exists on main (`review.md:350`, the skill's `BOT_RE` and `grep -qi qodo`). On a public repo, review objects are as easy to spoof as issue comments, so the new path does not materially widen the surface. Real hardening idea - defer to the CLO-623 extraction, where the filter becomes a tested script.
- **`DEPENDENCIES.md:45` stale wording and the open-task count.** Tracking-file accuracy, normally reconciled at task completion; cheap to fold into the fix commit but not a blocker for this PR's correctness.

## False Positives / Tooling Artifacts

- None. Every Codex finding reproduced. The only calibration issue is the overall FAIL verdict, which overweighted a spec-prescribed choice (the login filter) and environment-limited verification (no live GitHub access) as blocking.

## Recommendation

PROCEED_WITH_FIXES. One bounded iteration: (1) add the 40-hex head-SHA guard before both poll loops; (2) rewrite `pr-review-cycle.md` lines 35-38 and 570-573 to the two-shape rule and delete the `updated_at` recommendation; (3) drop the trailing blank line in the ollama review doc; then (4) at PR creation, execute the AC1-AC4 probes plus the new negative probes (empty head, failed head lookup) and paste commands, output, and raw JSON snapshots into the PR body as the spec requires. Optionally fold the two-line `DEPENDENCIES.md` correction into the same commit. No user decision is needed.
