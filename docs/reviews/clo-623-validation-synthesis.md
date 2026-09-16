# Pre-PR validation: clo-623

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-09-16
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Review finished with verdict FAIL: 6 findings plus missing items. Its local checks (ShellCheck, Actionlint, schema parity, shell suite, cargo) passed. |
| Claude fallback | SKIPPED | Not needed because the Codex review succeeded. |

I checked every Codex claim against the branch myself. The full shell suite passed (63 matched, 0 failed), ShellCheck and the inline-gate guard exited 0, and `shell-gates` appears in both `needs` and the assertion loop of `ci-gate` (`.github/workflows/ci.yml:161,172`). I then reproduced the reported failure cases with scratch fixtures and the fake `gh`.

## Verdict
PASS_WITH_NOTES

Codex's FAIL is downgraded. The defects it found are real, but each is local to one function or test, and together they fit in one fix pass. The change follows the design and needs no change of direction.

## Must Fix Before PR

1. **Unchecked `jq` failures produce a passing result (HIGH, reproduced).** `gh_capture` checks `gh`'s exit status, but the `jq` step that turns the response into the gate result is not checked.
   - **Billing-blocked PR read as "no bots":** I gave `probe-bots` one issue comment with `"user": null` (allowed by GitHub's REST schema) next to a Qodo comment carrying the billing-blocked marker. The null user makes `test($re)` raise an error, the login list stays empty, and it printed `none` with exit 0. The correct result is exit 4. A non-JSON comments body also gives `none` with exit 0.
   - **Broken response read as "no new comments":** I gave `new-comments` a truncated inline-comments body, and it exited 0 as if the PR were clean.
   - **Where it happens:** `pr-review-cycle.sh` lines 499-500, 503, 507-508, 515-517 (`probe-bots`) and 667 and 682 (`new-comments`). In `poll_for_pass` (375, 391) the same error only makes a tick silently miss a pass, so the wait ends in exit 1 instead of exit 3.
   - **Fix:** add one checked-`jq` helper that exits 3 on a non-zero status and use it at all of these lines. Write the login filters as `(.user.login // "")`. Add fixtures for a null user and a malformed body for `probe-bots`, `new-comments` and the wait legs.

2. **Several billing markers skip the billing gate (MEDIUM, reproduced).** `pr-review-cycle.sh:528` tests `[ "$blocked" = "1" ]`, so two comments with the marker exit 0. Change it to `-gt 0` and add a fixture with a duplicate marker.

3. **Named tests from the design are missing, and the fake `gh` has a bug.**
   - **Second-tick pass:** no test ever finds a pass after the first poll tick, so the retry loop is untested. Add `test_wait_rereview_detects_pass_on_second_tick` with `.1.json`/`.2.json` fixtures. The head-moved test can use the same sequencing, so it really spans two ticks.
   - **Other missing tests:**
     - `test_request_rereview_fails_closed_when_post_errors`
     - `test_wait_rereview_fails_closed_on_non_40_hex_head`
     - `test_wait_rereview_rejects_previous_run_pass_on_same_sha`
     - `test_new_comments_fails_closed_when_user_and_pr_lookups_error`
   - **Fake `gh` sequence bug:** `fake-gh/gh:111-122` does not do what its header says. From the (count+2)th call on, it serves `.1.json` again instead of repeating the last fixture.
   - **Filter false-green:** a test filter that matches no test exits 0. This makes the plan's filtered acceptance commands pass on a typo. Make zero matches exit non-zero.

4. **Bot identity is still checked in more than one place (design goal).** A literal `grep -qi qodo` at line 514 and the `QODO_LOGIN` constant at lines 22 and 582 bypass `reviewer_re`/`qodo_re`. Send both through that single check so CLO-650 stays one change. Keeping `reviewer_re` and `qodo_re` as two adjacent functions is fine.

## Out of Scope / Deferred
- **`unresolved-threads` cuts `latest_body` to 120 characters (line 732).** This was copied as-is from the old skill (`main:.pi/skills/pr-review-cycle.md:682`), so it is not a regression. It does contradict the design's `<BODY>` contract. Follow-up: either document the cut in the design and `usage` text, or return the full body.

## False Positives / Tooling Artifacts
- **"Head movement during polling is not implemented."** The design does not ask for it. `poll_for_pass` takes the head as an argument and the design's data flow never fetches `pulls` again. The old `wait_for_bot_review` did not either, and adding it would change gate behavior, which the design rules out. The implemented test does check the stated outcome: exit 1 when the pass is on a head the gate is not tracking. The `pulls/71.json` fixture in that scenario is never read. Item 3 covers turning the test into a real two-tick one.
- **"ST10 is incomplete."** The plan (lines 107 and 109-116) places the dogfood run and the failing-CI proof in the PR phase. CI cannot run this branch until a PR exists, so this cannot block moving to the PR. It still has to be recorded in the PR body before merge.
- **Uncommitted `docs/status/clo-623-workflow.yaml`.** This is the orchestrator's own state file being updated during validation, as expected.

## Recommendation
PROCEED_WITH_FIXES. In one pass:
1. Add a checked-`jq` helper and use it on every `jq` call that feeds a gate result in `probe-bots`, `new-comments` and `poll_for_pass`, with login filters that tolerate a null user.
2. Change the billing check to `-gt 0` and add a duplicate-marker fixture.
3. Add the five missing tests (second-tick pass, POST error, non-40-hex head in `wait-rereview`, earlier pass on the same SHA, `new-comments` lookup errors), with the fixtures from items 1 and 2.
4. Fix the fake `gh`'s sequence exhaustion and make a zero-match test filter fail.
5. Send the literal `qodo` match and `QODO_LOGIN` through the single identity check.

After that, run the pre-merge gate again and open the PR. ST10's dogfood run and the failing-then-green CI proof then happen in the PR phase.
