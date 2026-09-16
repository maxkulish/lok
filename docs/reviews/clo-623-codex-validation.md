# Pre-PR validation: clo-623

**Reviewer**: Codex (gpt-5.6-sol)
**Validated**: 2026-09-16
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- **HIGH — malformed REST JSON can still fail open.** Several `jq` invocations ignore their exit status. In `probe-bots`, malformed comments, pull lists, or review bodies can leave the login file empty and produce `none` with exit 0. In `new-comments`, a failed final `jq` leaves an empty output file and reports the PR clean. This directly violates the design's malformed-response exit-3 contract and the main acceptance criterion. See [pr-review-cycle.sh:499](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:499), [pr-review-cycle.sh:667](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:667), and [pr-review-cycle.sh:682](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:682).

- **MEDIUM — multiple billing-block markers bypass the gate.** `blocked` is a count, but the code exits 4 only when it equals exactly `1`. Two matching Qodo comments produce `2` and exit 0. The required behavior is "marker present," so this must test `> 0`. See [pr-review-cycle.sh:515](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:515) and [pr-review-cycle.sh:528](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:528).

- **MEDIUM — mid-poll head movement is not implemented or genuinely tested.** `wait-rereview` reads the head once and `poll_for_pass` never re-fetches it. The named test supplies an explicit old head; consequently the moved-head fixture's PR response is never read. It only proves that a review on a different SHA is rejected, not that movement during polling is detected. See [pr-review-cycle.sh:622](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:622) and [pr-review-cycle.test.sh:422](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/tests/pr-review-cycle.test.sh:422).

- **MEDIUM — reviewer identity is not centralized as designed.** Identity logic is spread across `QODO_LOGIN`, `reviewer_re`, `qodo_re`, a literal `grep -qi qodo`, and the `--bots` membership check. CLO-650 therefore cannot change one function as the design requires. See [pr-review-cycle.sh:22](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:22), [pr-review-cycle.sh:207](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:207), and [pr-review-cycle.sh:514](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:514).

- **MEDIUM — `unresolved-threads` truncates a field promised as complete.** The public contract specifies `latest_body:"<BODY>"`, but output is limited to 120 characters. This can remove the approval, rationale, or requested action an agent needs to categorize a thread. See [pr-review-cycle.sh:727](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/pr-review-cycle.sh:727).

- **LOW — the harness can report false-green acceptance.** A filter matching zero tests exits 0; I confirmed `DOES_NOT_MATCH_ANY_TEST` reports "0 matched, 0 failed." Also, after a numbered fixture sequence is exhausted, the fake does not reliably repeat the final fixture as documented. See [pr-review-cycle.test.sh:752](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/tests/pr-review-cycle.test.sh:752) and [fake-gh/gh:111](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/.pi/scripts/tests/fake-gh/gh:111).

## Missing Items

- Required malformed REST-response tests for `probe-bots`, `wait-review`, and `new-comments`.
- The planned `request-rereview` POST-error test.
- Genuine sequenced-poll coverage: stale pass on the same SHA, pass on the second tick, and actual PR-head movement during polling.
- ST10 is incomplete: there is currently no PR for this branch, so the required end-to-end dogfood record and failing-then-green `shell-gates`/`CI Gate` run evidence do not exist. See [implementation plan:109](/Users/mk/Code/orchestrator/lok--feat-clo-623-pr-review-cycle-tested/docs/plans/clo-623-pr-review-cycle-tested.md:109).
- The worktree has an uncommitted workflow-status update, so that completion evidence is not part of `main...HEAD`.

## Recommendations

- Add checked `jq_capture`/`jq_write` helpers and validate REST response shapes and required fields before any clean/absence verdict.
- Change the billing test to numeric `> 0` and add a duplicate-marker fixture.
- Re-fetch and validate the current PR head before accepting each pass, with a sequenced fixture that proves movement is observed.
- Consolidate all bot identity decisions behind one function.
- Return the complete thread body or change the documented contract explicitly.
- Make zero matched tests an error and fix sequence exhaustion to repeat the highest available fixture.
- After fixes, complete ST10 on an actual PR and record both negative and restored CI runs.

Local verification otherwise passed: ShellCheck, Actionlint, schema parity, all 63 shell tests, `cargo fmt`, clippy, and the full Rust test suite. No hardcoded secrets or direct shell-injection path was found.
