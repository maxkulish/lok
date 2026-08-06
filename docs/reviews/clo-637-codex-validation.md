# Pre-PR validation: clo-637

**Reviewer**: Codex (gpt-5.6-sol)
**Validated**: 2026-08-06
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

Reviewed `main...HEAD` through `d2e30a6`. The checkout also has an uncommitted workflow-status update. Core AC behavior passes local fixtures, but two medium findings block approval.

## Findings

- **MEDIUM — Bot identity can be spoofed.** Both gates trust any login containing `qodo` (`.claude/commands/pr/review.md:355`, `.pi/skills/pr-review-cycle.md:150`). A fixture using `not-qodo-reviewer` successfully satisfied the completion gate. GitHub permits identities with repository read access to comment and review, so substring matching is not app authentication. This is a design-level security weakness because the spec explicitly preserves `test("qodo")`. [GitHub review model](https://docs.github.com/en/pull-requests/concepts/giving-reviews)

- **MEDIUM — Workflow state dispatches the wrong phase.** The state pointer says `current_phase: implement` while the PR phase is already in progress and review feedback has been addressed (`docs/status/clo-637-workflow.yaml:9`). Task orchestration dispatches from this field, so resuming CLO-637 can incorrectly return to implementation. The implementation commit list also omits `d2e30a6`.

- **LOW — The new server-side `since` optimization is stricter than the specified gate.** The client accepts `created_at >= REQUESTED_AT`, but GitHub documents `since` as returning comments updated *after* the supplied time (`.claude/commands/pr/review.md:364`, `.pi/skills/pr-review-cycle.md:157`). A completion comment created in the same second as the request can therefore be removed server-side before jq sees it, despite satisfying the client condition. [GitHub issue-comments API](https://docs.github.com/en/rest/issues/comments?apiVersion=2022-11-28)

- **LOW — Tracking prose still contradicts the spec.** `docs/DEPENDENCIES.md:45` says Qodo "submits no new review," while the spec establishes that it submits review objects when new findings exist. Its "ten open tasks" header also appears inconsistent with nine listed backlog tasks plus CLO-637 and newly filed CLO-649.

## Missing Items

- No functional AC is visibly absent: local fixtures passed AC1–AC4, AC5/AC6 static comparison, AC7 guards, `bash -n`, ShellCheck, and `git diff --check`.
- The required live AC1–AC4 commands and raw API snapshots in PR #84 remain **UNVERIFIED** here because GitHub API access failed. The branch records that they passed but does not contain the evidence itself.
- The successful re-review result for current head `d2e30a6` is not yet recorded at `HEAD`; the uncommitted workflow update says polling is still underway.

## Recommendations

1. Anchor bot identity to the verified Qodo/Copilot app logins, preferably also requiring `.user.type == "Bot"`, in both review and comment paths.
2. Set `workflow.current_phase: pr`, complete the implementation phase, and record `d2e30a6` plus the final re-review result.
3. Remove the `?since=` prefilter or query from at least one second before `REQUESTED_AT`; keep jq's `created_at` condition authoritative.
4. Correct `DEPENDENCIES.md` wording and task count.
5. Confirm PR #84 contains the required commands, outputs, and raw JSON snapshots before merge.
