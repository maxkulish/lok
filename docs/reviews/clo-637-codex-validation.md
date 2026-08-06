# Pre-PR validation: clo-637

**Reviewer**: Codex (gpt-5.6-sol)
**Validated**: 2026-08-06
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

Reviewed commit `c636ef9` via `git diff main...HEAD`. The spec also serves as the implementation plan. One uncommitted workflow-status change was excluded from that diff.

## Findings

- **HIGH — The new comment path can fail open when the head lookup fails.** [review.md](/Users/mk/Code/orchestrator/lok--fix-clo-637-qodo-review/.claude/commands/pr/review.md:346) validates `REQUESTED_AT` but not `NEW_HEAD`; [pr-review-cycle.md](/Users/mk/Code/orchestrator/lok--fix-clo-637-qodo-review/.pi/skills/pr-review-cycle.md:134) similarly validates only `since`. In jq, `contains("")` is true, so an empty head accepts any fresh matching completion comment without proving the reviewed commit. A local probe confirmed this. This violates the fail-closed and covered-commit constraints.

- **MEDIUM — The new completion signal can be spoofed by a login merely containing `qodo`.** Both [review.md](/Users/mk/Code/orchestrator/lok--fix-clo-637-qodo-review/.claude/commands/pr/review.md:361) and [pr-review-cycle.md](/Users/mk/Code/orchestrator/lok--fix-clo-637-qodo-review/.pi/skills/pr-review-cycle.md:149) use `test("qodo")`. A synthetic `qodo-spoof` author passed the filter. Because issue comments are easier to create than bot review objects, the new path should verify the exact app identity.

- **MEDIUM — The Pi workflow still contradicts the two-shape rule.** Its introductory rules still require a review object at [pr-review-cycle.md](/Users/mk/Code/orchestrator/lok--fix-clo-637-qodo-review/.pi/skills/pr-review-cycle.md:35), while the later guidance explicitly recommends the forbidden persistent-comment `updated_at` signal at [pr-review-cycle.md](/Users/mk/Code/orchestrator/lok--fix-clo-637-qodo-review/.pi/skills/pr-review-cycle.md:570). This leaves the executable procedure internally inconsistent and plan item 3 incomplete.

- **LOW — Project tracking retains inaccurate task text.** [DEPENDENCIES.md](/Users/mk/Code/orchestrator/lok--fix-clo-637-qodo-review/docs/DEPENDENCIES.md:45) still says the poll "can never fire" and Qodo submits no new review, although the spec establishes that review objects appear when findings exist. Its header also says nine open tasks, while the tracking files show nine backlog tasks plus active CLO-637.

- **LOW — Diff hygiene is not clean.** `git diff --check main...HEAD` reports an extra blank line at EOF in `docs/reviews/clo-637-spec-review-ollama.md`.

## Missing Items

- The fail-closed contract lacks validation that the covered head is a nonempty full SHA.
- The Pi workflow's prose reconciliation is incomplete.
- AC1–AC4's required raw API snapshots and PR-body command output are not present in the committed branch. The nominal filters passed synthetic probes, but live GitHub verification was unavailable from this environment, so those criteria remain **UNVERIFIED**.
- AC7 is implemented for an empty `REQUESTED_AT`; AC6's executable conditions otherwise match apart from the existing multi-bot review-object filter.

## Recommendations

1. Reject invalid heads before polling in both snippets, ideally requiring `^[0-9a-f]{40}$`.
2. Match the exact observed Qodo app login, or use a tightly anchored allowlist verified against captured API JSON.
3. Replace every remaining "current-head review"/`updated_at` instruction in the Pi skill with the dual-shape `created_at + full SHA` rule.
4. Correct `DEPENDENCIES.md`, remove the trailing blank line, and commit the workflow-status update only after validation evidence is recorded.
5. Add negative probes for empty head, failed head API lookup, and spoofed Qodo-like usernames; include their output and the raw PR #71/#80 snapshots in the PR description.
