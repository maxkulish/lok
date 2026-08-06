# Spec: Recognise Qodo's comment-update re-review shape in the PR review gates

**Created**: 2026-08-06
**Task**: [CLO-637](https://linear.app/cloud-ai/issue/CLO-637)
**Estimated scope**: M (2 files, 4 sub-tasks)

## 1. Problem Statement

`/pr:review` Step 9.5 (`.claude/commands/pr/review.md:343-355`) gates merge on a Qodo re-review by polling `GET /pulls/{n}/reviews` for a review object with `commit_id == NEW_HEAD`, a `qodo` login, and `submitted_at >= REQUESTED_AT`. The same file states at line 357 that Qodo "edits its existing review comment in place rather than posting a new one" - the poll assumes the opposite of what the document itself says.

Verification against this repo's GitHub data resolved the contradiction and sharpened it:

- **Qodo submits a review object only when the pass carries new inline findings.** PR #71 had 6 passes, each with 1-3 inline findings; each produced a review object on the new head. PR #80's re-review (requested 13:52:13Z on 2026-08-03) found nothing new and produced **no** review object.
- **Every completed re-review pass - both shapes - is announced by a new issue comment** from the Qodo bot: `[Code review](<link>) by qodo was updated up to the latest commit <full-sha-url>`. Observed on all 6 re-review passes across PRs #71 and #80 (each within seconds of pass completion), and on nothing else. It is append-only (`created_at`, never edited) and names the covered commit.
- **The persistent "Code Review by Qodo" comment's `updated_at` is NOT a landing signal.** It bumps during a pass (intermediate busy/`Bugs (0)` states, per review.md:357) and outside passes entirely: on PR #80 it was edited at 14:03:15Z, 10 seconds after merge, with no `/agentic_review` posted and no completion comment - Qodo refreshing embedded permalinks. Gating on `updated_at >= REQUESTED_AT` alone would report a re-validation that never happened (fails open). The Linear issue proposed this signal; this spec deviates deliberately, on this evidence.

Consequence of the current gate: it can only pass when Qodo found **new** problems. A clean re-review - the success case, the state the whole fix cycle drives toward - times out at the full 600s on every run (observed on PR #80), charging ~10 minutes to every task and teaching the operator to distrust the gate.

The same defect is duplicated in `.pi/skills/pr-review-cycle.md`: `wait_for_bot_review` (lines 119-134) polls only the reviews endpoint and is used both by step 1b's post-request wait (line 157) and step 8's re-validation gate (lines 500-507). CLO-623 (extract these snippets into one tested script) has **not** landed, so the fix goes inline in both files with identical logic; fixing only one recreates the drift that turned CLO-633's Defect 2 into two sites. The Linear issue scopes to `/pr:review` only - the second file is a deliberate scope addition with this rationale.

## 2. Acceptance Criteria

- [ ] **AC1 - clean-pass shape detected**: the new Step 9.5 poll, run with PR #80's recorded values (`NEW_HEAD=48eb001e438bfd43d9b7f820b7835770c281b815`, `REQUESTED_AT=2026-08-03T13:52:13Z`), detects the re-review via the completion comment (created 13:54:56Z) on the first probe - i.e. within one poll interval of landing, not at timeout.
- [ ] **AC2 - review-object shape still detected**: the same poll, run with PR #71's values (`NEW_HEAD=1d0f9843fb832462725bc4ebfaa6382b43ac365b`, `REQUESTED_AT=2026-08-02T12:47:21Z`), detects the pass via the review object path (submitted 12:50:36Z).
- [ ] **AC3 - fails closed on absence**: with `REQUESTED_AT` set *after* the last Qodo activity on the PR (e.g. `2026-08-04T00:00:00Z` on PR #80), neither path fires and the loop exits 1 at deadline with the no-re-review message.
- [ ] **AC4 - a cosmetic edit is not a pass**: PR #80's post-merge edit (updated_at 14:03:15Z, no completion comment, no review object) does not satisfy the gate: with `NEW_HEAD=090332286712dcfcf03be89a00c8fb76346fec9d` and `REQUESTED_AT=2026-08-03T13:56:00Z` the poll exits 1 at deadline.
- [ ] **AC5 - single description of Qodo's behaviour**: `review.md` states the two-shape delivery rule (review object iff new inline findings; completion comment + in-place edit always) in "How Qodo posts", and Step 9.5 and the workflow-cycle box reference that one rule. No section still says or implies "poll for a review object" as the sole shape. The `rg` probe in Evaluation row 5 is advisory only; the manual read of the three sections is the authoritative check.
- [ ] **AC6 - no drift between the two files**: `pr-review-cycle.md`'s wait function accepts both shapes with the same conditions as Step 9.5's poll (same endpoints, same jq conditions, allowing for the files' existing variable-name conventions).
- [ ] **AC7 - empty `REQUESTED_AT` fails immediately**: if the `/agentic_review` POST fails (empty/missing `created_at`), the gate aborts with exit 1 before polling, rather than polling with an empty lower bound that matches everything.

**Verification method**: these are markdown-embedded snippets with no test harness (CLO-623 not landed). Verify by (a) syntax-checking every changed snippet (`bash -n` + `shellcheck` on the extracted block), and (b) executing the new poll bodies once per AC against the live GitHub API for PRs #71/#80 with the pinned values above (single probe, no sleep loop needed: run the two detection queries and assert output non-empty/empty). Record command + output in the PR description, and snapshot the raw probe JSON (the relevant reviews/comments records) into the PR evidence so AC1-AC4 stay verifiable after live PR data drifts.

## 3. Constraints

**Must**:
- Fail closed: timeout exits 1; an unparseable/changed Qodo comment format degrades to timeout, never to false success.
- Keep `REQUESTED_AT` sourced from the POST response (`--jq .created_at`), never local `date` (clock-domain rule, review.md:339).
- Every detection path pairs a freshness bound with a covered-commit check: review objects via `submitted_at >= REQUESTED_AT` + `commit_id == NEW_HEAD`; completion comments via `created_at >= REQUESTED_AT` + body contains the full `NEW_HEAD` SHA + body matches `was updated up to the latest commit` + qodo login.
- Keep the existing snippet idiom: `gh api --paginate --slurp | jq` with ISO-8601 string comparison, 600s deadline, existing sleep intervals (20s command / 10s skill).
- Both files carry the same detection logic; the skill keeps its function form (`wait_for_bot_review` may gain a companion or extra leg, but both call sites - 1b and step 8 - must get dual-shape behaviour).
- Keep line 357's warning about intermediate states: gate landing and reading findings remain separate concerns.

**Must-not**:
- Gate on the persistent comment's `updated_at` as a success signal (see Problem Statement; document it as corroborating evidence only).
- Enable `handle_push_trigger` (review.md:371 rationale stands).
- Introduce new script files or move snippets out of markdown (that is CLO-623's job).
- Encode the legacy `/review` command anywhere.
- Match on SHA alone or timestamp alone in any path.

**Prefer**:
- Minimal diff: leave surrounding prose intact where it is already correct.
- The completion-comment jq filter tolerant of markdown variations (match the phrase and the SHA substring, not the full body layout).
- Timeout messages that tell the operator what to check by hand: both shapes, both endpoints.

**Escalate when**:
- Live-API verification contradicts the recorded shapes (e.g. completion comment wording changed since 2026-08-03): stop, re-dump `/config`, re-verify before writing.
- The fix appears to require changing Qodo configuration rather than the poll.

## 4. Decomposition

1. **Rewrite Step 9.5's poll and rationale** - replace the review-object-only loop (review.md:341-355) with the dual-shape poll (Path A: review object; Path B: completion comment), add the `REQUESTED_AT`-empty guard (AC7), and a short paragraph stating the two-shape rule and why `updated_at` is explicitly not the signal - files: `.claude/commands/pr/review.md`
2. **Reconcile every other description in the same file** - "How Qodo posts" (lines 639-648: add the per-pass shape rule), the workflow-cycle box step 8 (line 710: "Poll for a review whose commit_id == current head SHA" -> dual-shape wording), line 714, and the Step 12 summary template line 785 ("Re-review observed on head ...: [yes/no]" -> shape-neutral wording) - files: `.claude/commands/pr/review.md`
3. **Port the same gate to the pi skill** - extend `wait_for_bot_review` (lines 119-134) with the completion-comment leg (qodo-only) so 1b (line 157) and step 8 (lines 500-507) accept both shapes; update the adjacent prose (lines 112-114, 145-166, 519-524) to the same two-shape rule. Known limit (F5): the completion comment is evidenced for *re-review* passes only; a clean **initial** pass at 1b may produce neither signal, and the existing explicit `/agentic_review` re-request remains the recovery path - no worse than today, note it in the prose rather than inventing an unevidenced third detector - files: `.pi/skills/pr-review-cycle.md`
4. **Execute-and-record verification** - run each AC's probe against PRs #71/#80, shellcheck all changed snippets, paste results into the PR body - files: none (evidence only)

**Dependency order**: 1 -> 2 (wording follows the final snippet); 3 depends on 1 (same logic, second file); 4 last. 1 and 3 could land in one commit; 2 must not precede 1.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | AC1 clean shape | Path B query prints `2026-08-03T13:54:56Z` | Extract Path B jq from new Step 9.5; run against `repos/maxkulish/lok/issues/80/comments` with `h=48eb001e438bfd43d9b7f820b7835770c281b815`, `since=2026-08-03T13:52:13Z` |
| 2 | AC2 review-object shape | Path A query prints `2026-08-02T12:50:36Z` | Run Path A jq against `repos/maxkulish/lok/pulls/71/reviews` with `h=1d0f9843fb832462725bc4ebfaa6382b43ac365b`, `since=2026-08-02T12:47:21Z` |
| 3 | AC3 fail closed | Both queries empty; loop body reaches deadline branch, exit 1 | Same queries, `since=2026-08-04T00:00:00Z`, both PRs |
| 4 | AC4 cosmetic edit rejected | Both queries empty for `h=090332286712dcfcf03be89a00c8fb76346fec9d`, `since=2026-08-03T13:56:00Z` | Run both queries against PR #80 |
| 5 | AC5 one description | `rg -c "poll for a review" .claude/commands/pr/review.md` finds no review-object-only phrasing; manual read of the three sections agrees | `rg` + read |
| 6 | AC6 no drift | Diff of the two files' detection conditions shows only naming differences | Extract both snippets, normalise variable names, diff |
| 7 | AC7 empty request guard | Snippet aborts (exit 1, message) when `REQUESTED_AT=""` | `bash -n` the block, then dry-run the guard branch with an empty var |
| 8 | Syntax | `shellcheck` clean (or only pre-existing directives) on every changed block | Extract fenced blocks to temp files, `shellcheck` |

**Edge cases to verify**:
- Re-run of Step 9.5 without an intervening push: old completion comment predates the new `REQUESTED_AT` -> no false pass (covered by AC3 shape).
- Completion comment names an older head because a push raced the request: `contains($NEW_HEAD)` fails -> keeps waiting -> timeout (fail closed).
- `/agentic_review` posted twice: `tail -1`/`last` semantics still pick a valid landing; both events are >= their own request bound.
- Bot login variants: keep `test("qodo")` matching `qodo-code-review[bot]`.
- Copilot (or any bot that submits real review objects): Path A unchanged, still detected (Linear AC3).
