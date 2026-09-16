# Design: CLO-623 - Make pr-review-cycle shell snippets executable and tested

## Problem

Every lok task that reaches the `pr` phase has an agent run merge gates (`bot_review_wait_completed`, `bot_rereview_verified`) whose logic exists only as bash blocks inside two markdown files, `.pi/skills/pr-review-cycle.md` and `.claude/commands/pr/review.md` (17 blocks each). Nothing executes, lints or tests those blocks, and discovery (`docs/discovery/clo-623.md`, baseline 4/10) found the logic duplicated and already drifting: `/pr:review` Step 9.5 re-implements the skill's step 1b poll with a 20s interval and a `qodo`-only review filter, while the skill uses 10s and both reviewer logins. Several gates fail open. An unset `INSTALLED_BOTS` used to read as "no bots installed", `jq max` over an empty array yields the string `null` and hides every new comment, and a failed `gh api` call in the step 1a probe produces empty output that the pipeline reports as "no bots installed" with exit status 0. PR #71 showed the cost: seven chained review findings, two of which (`gh api --jq --arg` being invalid, and the `null` bound) would have been caught by running the snippet once. The matter is urgent now because Phase 14 of `docs/ROADMAP.md` has grown to nine defects with this task as the structural fix, and CLO-650 (exact bot identity) is blocked until the gate logic lives in one executable place.

## Goals / Non-goals

### Goals

- Add one POSIX shell script, `.pi/scripts/pr-review-cycle.sh`, with six subcommands: the five named in the PRD - `probe-bots`, `wait-review`, `request-rereview`, `wait-rereview`, `new-comments` - plus `unresolved-threads` (decision 2). They port the skill's steps 1a, 1b, 2, 7 and 8 and `/pr:review` Step 9.5.
- Rewrite both markdown files so that they call the script. No executable bash remains inline beyond single-line script calls and illustrative one-liners.
- Add hermetic tests that run the script against a fake `gh` on `PATH` serving recorded JSON fixtures. The tests must prove that every gate fails closed on missing, stale, empty or errored input, including the two PR #71 defects.
- Run `shellcheck` over the script and its test files in a new CI job, wired into both the `needs` list and the assertion loop of `ci-gate`.
- Keep the reviewer-identity match in exactly one function, so that CLO-650 lands as a single change.

### Non-goals

- Changing gate semantics: the 600s timeouts, the two delivery shapes, billing-block handling, and the rule that timestamps come from GitHub's clock stay as the prose specifies.
- Exact reviewer-bot identity matching. That is CLO-650.
- Reworking `REVIEW_FAILED` handling on empty reviewer output in `.lok/workflows/*.toml`. That is the follow-up noted on the ticket.
- Moving the gates into the `lok` binary (discovery Approach B) or building a markdown snippet-extraction harness (Approach C).
- Porting the thin `gh api` wrappers in skill steps 3, 6 and 7 (fetch, commit, reply, resolve) and `/pr:review` Steps 2, 3 and 9. They stay as prose examples.
- Restoring `CI Gate` as a required status check on `main`.
- Any change to Rust code, `Cargo.toml` or the crate's dependency set.

## Architecture

### File layout

| Path | Status | Purpose |
|---|---|---|
| `.pi/scripts/pr-review-cycle.sh` | new | Subcommand dispatcher and all gate logic |
| `.pi/scripts/tests/pr-review-cycle.test.sh` | new | Test runner: named `test_*` functions, non-zero exit on any failure |
| `.pi/scripts/tests/fake-gh/gh` | new | Fake `gh` placed first on `PATH` by the runner |
| `.pi/scripts/tests/fixtures/pr-review-cycle/<scenario>/` | new | Recorded, trimmed `gh api` responses per scenario |
| `.pi/scripts/tests/check-inline-gates.sh` | new | Acceptance-criterion guard: exits non-zero when the two rewritten markdown files regain a banned gate shape (decision 5) |
| `.pi/scripts/tests/inline-gate-allowlist.txt` | new | One ERE per line exempting a legitimate illustrative one-liner, each with a trailing `#` rationale |
| `.github/workflows/ci.yml` | changed | New shell-gates job; `ci-gate` `needs` and assertion loop updated |
| `.pi/skills/pr-review-cycle.md` | changed | Steps 1a, 1b, 2, 7, 8 become prose plus script calls |
| `.claude/commands/pr/review.md` | changed | Step 9.5 billing check, request and poll become script calls |
| `.claude/commands/task/phases/pr.md` | changed | Step 4.3 pointer names the script instead of the `wait_for_bot_review` helper |

`.pi/scripts/check-schema-parity.mjs` is the only existing first-party script in the repository. It sets the placement precedent. There are no first-party `.sh` files and no shell test harness yet.

### Mapping from the current snippets to subcommands

| Current location | Behavior | Replaced by |
|---|---|---|
| Skill 1a; `/pr:review` 9.5 billing check | Installed-bot probe over the last 10 PRs plus this PR's comments; `<!-- qodo:billing-blocked -->` marker | `probe-bots` |
| Skill 1b, first wait | Poll for a pass on the current head since the PR's `created_at` | `wait-review` |
| Skill 1b fallback; skill 8; `/pr:review` 9.5 | POST `/agentic_review` and keep the `created_at` that GitHub assigns | `request-rereview` |
| Skill 1b second wait; skill 8; `/pr:review` 9.5 | Poll for a pass on the (post-push) head since the request | `wait-rereview` |
| Skill 2 | Fail closed on unset `INSTALLED_BOTS` / `BOT_REVIEW_SEEN` | Removed. Shell variables no longer carry gate state: a non-zero exit from a wait subcommand is the failure, and an empty `--bots` exits 2 |
| Skill 7 `REPLY_PUSH_TS`; skill 8 comment re-check | Since-bound with the `null` guard; inline comments created after it | `new-comments` |
| Skill 8 GraphQL unresolved threads | Unresolved-thread gate that loops back to step 4; the 20-line GraphQL query appears verbatim in steps 3, 7 and 8 | `unresolved-threads` (sixth subcommand, added in the soft gate) |
| `phases/pr.md` §5.0 | Pre-merge re-fetch gate with `yq ... \| tail -1` (the pipeline shape `docs/lessons/clo-625-l6` warns about) | Unchanged in this design; follow-up issue. See decision 2 below |

Decisions that depend on the user stay in prose next to the calls. Examples are "proceed without a bot review while Qodo is billing-blocked" and "reviews are back, request a pass". A script cannot make those decisions, and keeping the "is Qodo installed" branch in the same table gives the agent one decision point per step.

### Data flow

```
agent shell (zsh or bash)           .pi/scripts/pr-review-cycle.sh             GitHub REST (via gh)
-------------------------           ------------------------------             --------------------
skill 1a, /pr:review 9.5   ------>  probe-bots        -- gh api -->  issues/<PR>/comments
                                                                     pulls?state=all&per_page=10
                                                                     pulls/<n>/reviews (per prior PR)
skill 1b                   ------>  wait-review       -- gh api -->  pulls/<PR>
                                        |                            pulls/<PR>/reviews
                                        +-- poll_for_pass ---------> issues/<PR>/comments?since=<ts>
skill 1b, 8, /pr:review 9.5 ----->  request-rereview  -- gh api -X POST --> issues/<PR>/comments
skill 1b, 8, /pr:review 9.5 ----->  wait-rereview     -- poll_for_pass (as above)
skill 7-8                  ------>  new-comments      -- gh api -->  user, pulls/<PR>/comments, pulls/<PR>

<------ stdout: the value the markdown records (bots, timestamps, head SHA, comment lines)
<------ exit status: the gate verdict (table in Public API surface)
<------ stderr: "pr-review-cycle: <subcommand>: ..." diagnostics
```

Each subcommand is a separate process and reads no shell variables from the caller. State passes only through arguments and stdout. This removes the "run all blocks in one shell" requirement that steps 2 and 8 currently guard against.

### Script structure (`.pi/scripts/pr-review-cycle.sh`)

Shebang `#!/bin/sh`, per the discovery choice of POSIX shell.

**Shell rules.** Strict POSIX: no `local`, no `[[ ]]`, no arrays, no `pipefail`, no GNU-only flags. The script sets **neither `set -e` nor `pipefail`**: several subcommands intentionally exit non-zero as a gate verdict, and every command that matters has its status checked explicitly (`docs/lessons/clo-625-l6` - a gate that relies on an ambient shell setting is one paste away from losing it). `set -u` is used, with `:-` defaults on every optional environment variable. `shellcheck --shell=sh` in CI is the mechanical enforcement.

Internal functions:

- `main` - dispatches on the first argument to `cmd_probe_bots`, `cmd_wait_review`, `cmd_request_rereview`, `cmd_wait_rereview`, `cmd_new_comments`; an unknown subcommand exits 2.
- `require_repo`, `require_pr`, `require_sha40`, `require_iso8601z`, `require_bots` - validate inputs before any API call. `require_iso8601z` accepts only `YYYY-MM-DDTHH:MM:SSZ`, the shape the prose relies on for chronological string comparison in jq.
- `gh_capture <var> <gh api args...>` - runs `gh api`, stores stdout in the named variable, and exits 3 when `gh` exits non-zero, exceeds the per-call deadline, or returns an empty body. No `gh` call is ever piped straight into `jq` (`docs/lessons/clo-625-l6`: a pipeline's status is its last command's).
- `gh_call_deadline` wrapping - every `gh` invocation runs under a per-request deadline (default 30s, see `PR_REVIEW_CYCLE_CALL_DEADLINE` below), implemented POSIX-portably as background process + `kill -0` poll + `kill`, because `gh api` has no request-timeout flag and macOS ships no `timeout` binary. A request that exceeds the deadline is treated exactly like a non-zero `gh` exit: the subcommand fails closed with exit 3. Rationale: a stuck TCP connection must not block a merge gate forever; 30s is generous for the calls used (the largest is one page of 100 comments).
- `bot_login_filter` - the single jq fragment matching reviewer logins (currently `qodo-code-review|copilot-pull-request-reviewer`, with a `qodo` match for the completion-comment and billing-marker legs). CLO-650 changes only this function.
- `pr_lookup` - reads `head.sha` and `created_at` from `pulls/<PR>` and validates both.
- `poll_for_pass <head_sha> <since> <timeout>` - ports `wait_for_bot_review`: a review object on `<head_sha>` by a reviewer bot with `submitted_at >= <since>`, or a `qodo` issue comment with `created_at >= <since>` whose body matches `was updated up to the latest commit` and contains `<head_sha>`. It never gates on the persistent comment's `updated_at`. The `?since=` query stays a server-side prefilter only; the jq `created_at` comparison is the gate.
- `graphql_query_review_threads` - the single copy of the `reviewThreads` GraphQL query (eliminates the three verbatim copies in the skill). GraphQL responses are fail-closed: non-zero `gh` exit, a non-empty `errors` array, missing or null `data`, or a thread node missing a required field each produce exit 3, never an empty result that reads as "no unresolved threads".

Rules carried from the prose and the lessons:

1. No `gh api --jq`. All filtering happens in `jq`, with `--arg` passed to `jq` itself (the PR #71 defect).
2. Every jq aggregate ends in `// empty`. An empty result is never used as a comparison bound (the PR #71 `null` defect).
3. The script takes gate timestamps only from GitHub responses. `date -u +%s` is used only for local deadline arithmetic. All GitHub endpoints the script consumes return whole-second `YYYY-MM-DDTHH:MM:SSZ` timestamps, which is the one shape `require_iso8601z` accepts; chronological string comparison in jq is valid within that shape.
4. Absence is explicit. `probe-bots` prints `none`, never an empty line, so an unset caller variable (`--bots ""`) cannot be read as "no bots".
5. Every expansion is quoted, and nothing relies on word splitting (`docs/lessons/clo-625-l4`).
6. All JSON output (`new-comments`, `unresolved-threads`) is produced by `jq -c` over parsed API data - never by shell string interpolation, because comment and thread bodies are reviewer-controlled text that must survive JSON encoding untouched.
7. Boundary comparisons are explicit and pinned by tests. Review passes use an inclusive bound (`submitted_at >= <since>` / `created_at >= <since>`), exactly as the skill does, so a pass and the request in the same second still count. `new-comments` uses a strict bound (`created_at > <since>`) so the author's own last reply is never reported as new. Each boundary has a test that pins it.

### Test harness

```
pr-review-cycle.test.sh
  for each test_* function:
    FAKE_GH_SCENARIO=fixtures/pr-review-cycle/<scenario>
    FAKE_GH_STATE=<mktemp -d>
    PATH=.pi/scripts/tests/fake-gh:$PATH
    PR_REVIEW_CYCLE_POLL_INTERVAL=0
      -> sh .pi/scripts/pr-review-cycle.sh <subcommand> ...
           -> gh (fake) -> <scenario>/<method>__<path>.json | .<n>.json | .exit
    assert exit status, stdout, and FAKE_GH_STATE/calls.log
```

The fake `gh` does three things:

- It accepts only the argv shapes the script uses: `api <path>`, `api graphql`, `--paginate`, `--slurp`, `-X POST`, `-f body=<text>`, `-f query=<text>`, `-f owner=<owner>`, `-f repo=<repo>`, `-F pr=<n>`. Any other flag, including `--jq` and `--arg`, makes it exit 1 with an unknown-flag message, so the PR #71 invocation class fails a test instead of passing silently.
- It resolves `<METHOD> <path without query string>` to a fixture file, and the one `graphql` endpoint to `<scenario>/graphql__reviewThreads.json`. Sequenced files (`.1.json`, `.2.json`) serve successive poll ticks through a call counter in `FAKE_GH_STATE`, an `.exit` file forces a non-zero exit, and a `.hang` file makes the fake sleep for 60s - long enough to trip `PR_REVIEW_CYCLE_CALL_DEADLINE` in the hung-request test.
- It appends every invocation to `FAKE_GH_STATE/calls.log`, so tests can assert that no POST happened, or that no API call preceded a validation failure.

Fixtures are recorded from PRs that exhibited each behavior: #71 (review objects with findings), #80 (clean-pass completion comment), #106 (billing-blocked) and #89 (not blocked). They are trimmed to the fields the jq filters read and stored in the `--paginate --slurp` array-of-pages shape.

## Public API surface

No Rust trait, struct or function signature changes. The public surface of this change is the script's command-line contract, which both markdown consumers call.

### Before

```sh
# .pi/skills/pr-review-cycle.md step 1b: a shell function defined in the agent's shell,
# relying on REPO, PR and BOT_RE being set in that same shell
wait_for_bot_review <head_sha> <since_iso8601> [timeout_seconds]

# .claude/commands/pr/review.md Step 9.5: a separate inline copy of the same loop
# (sleep 20, reviews leg filtered on "qodo" only)
```

### After

```text
.pi/scripts/pr-review-cycle.sh probe-bots         --repo <OWNER/REPO> --pr <N>
.pi/scripts/pr-review-cycle.sh wait-review        --repo <OWNER/REPO> --pr <N> [--timeout <SECONDS>]
.pi/scripts/pr-review-cycle.sh request-rereview   --repo <OWNER/REPO> --pr <N> --bots <LIST|none>
.pi/scripts/pr-review-cycle.sh wait-rereview      --repo <OWNER/REPO> --pr <N> --since <ISO8601Z> [--head <SHA40>] [--timeout <SECONDS>]
.pi/scripts/pr-review-cycle.sh new-comments       --repo <OWNER/REPO> --pr <N> [--since <ISO8601Z>]
.pi/scripts/pr-review-cycle.sh unresolved-threads --repo <OWNER/REPO> --pr <N>
```

| Subcommand | stdout on exit 0 | Notes |
|---|---|---|
| `probe-bots` | `<login>[,<login>...]` or `none` | Also printed on exit 4 |
| `wait-review` | `<detected_at>` | Head and since-bound come from `pulls/<PR>` (`head.sha`, `created_at`) |
| `request-rereview` | `<created_at>` of the posted `/agentic_review` comment | `--bots` without `qodo-code-review` (including `none`) exits 2 and posts nothing |
| `wait-rereview` | `<head_sha> <detected_at>` | `--head` defaults to the `pulls/<PR>` head at call time, which is the post-push head in step 8 |
| `new-comments` | One JSON object per line: `{"id":<ID>,"user":"<LOGIN>","body":"<BODY>"}` (printed on exit 1) | Without `--since`, the bound is the authenticated user's latest inline comment, falling back to the PR's `created_at`; stderr names the bound used |
| `unresolved-threads` | One JSON object per line of every **unresolved** review thread: `{"id":"<NODE_ID>","path":"<PATH>","line":<LINE>,"is_outdated":<BOOL>,"latest_author":"<LOGIN>","latest_body":"<BODY>"}` (printed on exit 1); prints nothing on exit 0 | Runs the GraphQL `reviewThreads` query today duplicated in skill steps 3, 7 and 8; exit 1 means "loop back to step 4" |

`--timeout` defaults to `600`.

| Exit | Meaning |
|---|---|
| 0 | Condition met: probe succeeded and Qodo is not billing-blocked, pass observed, request posted, or no new comments |
| 1 | Negative gate verdict: no pass before the deadline, or new comments found |
| 2 | Invalid or inapplicable input (missing or empty argument, non-40-hex head, malformed timestamp, `--bots` without Qodo). Nothing was called or posted |
| 3 | Upstream failure: `gh` exited non-zero, returned an empty body or malformed JSON, or omitted a required field |
| 4 | `probe-bots` only: `qodo-code-review` is billing-blocked on this PR |

Only exit 0 may be recorded as a passed gate.

Environment variables (test seams, not for operators):

```text
PR_REVIEW_CYCLE_POLL_INTERVAL   seconds between poll ticks; default 10 (the skill's value - see decision 1)
PR_REVIEW_CYCLE_CALL_DEADLINE   per-request cap in seconds for any single gh call; default 30
```

The poll loop matches reviewer logins (`BOT_RE`) on the reviews leg and `qodo` on the completion-comment leg, exactly as the skill's `wait_for_bot_review` does. `/pr:review` Step 9.5's deviations (20s interval, qodo-only reviews leg) are drift and are retired.

Call sites in the rewritten markdown take this form:

```sh
INSTALLED_BOTS=$(.pi/scripts/pr-review-cycle.sh probe-bots --repo "$REPO" --pr "$PR"); RC=$?
REQUESTED_AT=$(.pi/scripts/pr-review-cycle.sh request-rereview --repo "$REPO" --pr "$PR" --bots "$INSTALLED_BOTS"); RC=$?
REREVIEW=$(.pi/scripts/pr-review-cycle.sh wait-rereview --repo "$REPO" --pr "$PR" --since "$REQUESTED_AT"); RC=$?
```

The status is captured with `; RC=$?` directly after the assignment and never through a pipe. In both bash and zsh, an assignment from a command substitution takes that substitution's exit status.

## Assumptions

- `CI Gate` is not a required status check today: on 2026-09-16, ruleset 20153405 on `main` returned only `deletion` and `non_fast_forward` rules, so `docs/lessons/clo-625-l2` is stale on this point. The new job will appear in `CI Gate` but will not block a merge by itself. Confidence: high. Verification: `gh api repos/maxkulish/lok/rulesets/20153405`.
- `shellcheck` and `jq` are preinstalled on the `ubuntu-latest` runner image. Confidence: medium (`shellcheck`), high (`jq`). Verification: the first CI run prints `shellcheck --version` and `jq --version`; fall back to `apt-get install` if either is missing.
- `zsh` is not preinstalled on `ubuntu-latest`, and the job can install it with `apt-get` for the call-site tests. Confidence: medium. Verification: first CI run.
- `/bin/sh` differs between hosts (dash on Ubuntu, a bash-compatible `sh` on macOS). Code that passes `shellcheck --shell=sh` and the test suite on both behaves the same for the constructs used. Confidence: medium. Verification: run the test suite locally on macOS and in CI on Ubuntu.
- Every agent host has a `gh` that supports `gh api --paginate --slurp`, because the current snippets already depend on it (local `gh` is 2.101.0). Confidence: high. Verification: `gh api --help`.
- Qodo's observable signals still match the prose: a review object only for passes with findings, the `was updated up to the latest commit <sha>` completion comment for re-review passes, and the `<!-- qodo:billing-blocked -->` marker. Confidence: medium, because the evidence is PRs #71, #80, #100 and #106 rather than vendor documentation. Verification: record fixtures from those PRs during implementation and compare them to the prose.
- Live data on PRs #71, #80, #89 and #106 is still retrievable for fixture recording. Qodo edits comments in place, so a fixture may show the final state rather than the state at pass time. Confidence: medium. Verification: fetch during implementation, and hand-trim to the shapes the prose documents where the live data has moved.
- Agents invoke the script from the repository or worktree root, so the relative path `.pi/scripts/pr-review-cycle.sh` resolves. Confidence: medium. Verification: the skill's prerequisites section states it, and the dogfood run on this task's own PR exercises it.
- The script reads no repository files at runtime; all inputs are arguments. Confidence: high (discovery). Verification: the tests run with an empty `FAKE_GH_STATE` working area.
- Discovery's objection to Approach B (a stale `lok` build runs stale gates) does not apply here, because the script ships in the same checkout as the markdown that calls it. Confidence: high.

## Test plan

No Rust unit tests or `tests/*.rs` integration tests are added, because no Rust code changes (the per-backend matrix does not apply). Following the discovery choice, the tests live in `.pi/scripts/tests/pr-review-cycle.test.sh` as named functions; Open question 3 covers the alternative of moving them into `tests/`.

### `probe-bots`

- `test_probe_bots_reports_qodo_from_current_pr_comments` - a bot with no history on prior PRs is still detected from this PR's comments (the PR #71 first-review case).
- `test_probe_bots_reports_bot_from_prior_pr_reviews`
- `test_probe_bots_prints_none_when_no_bot_activity`
- `test_probe_bots_fails_closed_when_comments_lookup_errors` - `gh` exits non-zero, so the probe exits 3 and never prints `none`.
- `test_probe_bots_fails_closed_when_prior_pr_review_lookup_errors`
- `test_probe_bots_exits_4_when_qodo_billing_blocked`
- `test_probe_bots_rejects_missing_pr_before_any_api_call` - exit 2, empty `calls.log`.

### `wait-review` and the shared poll

- `test_wait_review_accepts_review_object_on_head`
- `test_wait_review_accepts_qodo_completion_comment_naming_head` - the PR #80 clean-pass shape.
- `test_wait_review_rejects_review_on_older_commit` - a stale pass on a previous head times out with exit 1.
- `test_wait_review_rejects_completion_comment_naming_other_sha`
- `test_wait_review_rejects_pass_before_since`
- `test_wait_review_ignores_persistent_comment_updated_at` - a bumped `updated_at` with no completion comment is not a pass.
- `test_wait_review_gates_on_created_at_not_server_since` - the fake ignores `?since=`, and old comments are still filtered out.
- `test_wait_review_accepts_pass_exactly_at_since` - pins the inclusive `>=` boundary: a pass stamped exactly at `--since`/`created_at` still counts.
- `test_wait_review_times_out_closed` - `--timeout 0`, one tick, exit 1.
- `test_wait_review_fails_closed_when_pulls_lookup_errors` - exit 3.
- `test_wait_review_fails_closed_on_malformed_head_from_lookup`

### `request-rereview`

- `test_request_rereview_prints_github_created_at`
- `test_request_rereview_fails_closed_when_post_errors` - exit 3.
- `test_request_rereview_fails_closed_when_response_lacks_created_at` - exit 3, with no fallback to local `date`.
- `test_request_rereview_rejects_empty_bots_without_posting` - the `${INSTALLED_BOTS+x}` guard, ported; exit 2 with no POST in `calls.log`.
- `test_request_rereview_rejects_bots_without_qodo_without_posting` - `--bots none` and `--bots copilot-pull-request-reviewer`.

### `wait-rereview`

- `test_wait_rereview_fails_closed_on_empty_since` - a failed POST must not widen the window; exit 2.
- `test_wait_rereview_fails_closed_on_non_40_hex_head` - an empty head would make `contains()` vacuously true; exit 2.
- `test_wait_rereview_rejects_previous_run_pass_on_same_sha` - a pass on the right SHA but before `--since` returns exit 1.
- `test_wait_rereview_detects_pass_on_second_tick` - a sequenced fixture with `PR_REVIEW_CYCLE_POLL_INTERVAL=0`.
- `test_wait_rereview_prints_head_and_timestamp`
- `test_wait_rereview_defaults_head_to_current_pulls_head`
- `test_wait_rereview_fails_closed_when_head_changes_mid_poll` - the fake serves a different head on a later tick; the poll must not accept a pass for the head it is no longer tracking, and exits 1 (no pass on the tracked head) rather than 0.

### `unresolved-threads`

- `test_unresolved_threads_clean_pr_exits_0` - all threads resolved; empty stdout.
- `test_unresolved_threads_lists_open_thread_and_exits_1` - one unresolved thread produces its JSON line.
- `test_unresolved_threads_fails_closed_when_graphql_errors` - exit 3.
- `test_unresolved_threads_paginates_multiple_threads` - several unresolved threads spread across two GraphQL pages all appear as JSON lines, in a deterministic order (exit 1).

### `new-comments`

- `test_new_comments_null_max_falls_back_to_pr_created_at` - no replies from the author; the bound is the PR `created_at`, never `null` (the PR #71 defect).
- `test_new_comments_reports_comment_after_last_reply` - exit 1, JSON line on stdout.
- `test_new_comments_clean_exits_0`
- `test_new_comments_fails_closed_when_user_and_pr_lookups_error` - exit 3; never compares against an empty bound.
- `test_new_comments_rejects_malformed_since` - exit 2.
- `test_new_comments_excludes_comment_exactly_at_bound` - pins the strict `>` boundary: a comment stamped exactly at the bound is the author's own reply and is not reported as new (exit 0).

### Cross-cutting

- `test_fake_gh_rejects_jq_and_arg_flags` - pins the fake's flag allowlist, which every other test relies on to catch the `gh api --jq --arg` class.
- `test_gh_call_hung_request_fails_closed_after_deadline` - the `.hang` fixture makes the fake sleep 60s; with `PR_REVIEW_CYCLE_CALL_DEADLINE=1` the call fails closed with exit 3 and the gate returns promptly instead of hanging.
- `test_inline_gate_guard_fails_on_each_banned_shape` - the guard's self-test: each banned ERE in `check-inline-gates.sh` is written into a scratch copy of one markdown file and the guard must exit non-zero; the guard must exit 0 on the allowlist example (decision 5).
- `test_unknown_subcommand_exits_2`
- `test_call_site_status_capture_under_zsh` and `test_call_site_status_capture_under_bash` - run the documented `VAR=$(...); RC=$?` lines through `zsh -c` and `bash -c` against failing scenarios, and assert that `RC` is non-zero (`docs/lessons/clo-625-l4`, `clo-625-l6`). The zsh leg skips with a note when `zsh` is absent.

### Reviewer-identity drift pin

The `qodo`/`BOT_RE` matchers today are substring matches; CLO-650 will tighten them. Tests for the completion-comment and billing legs assert current behavior and carry a comment naming CLO-650 as the owner of any change.

### CI

The new job runs on every PR with no `paths:` filter (`docs/lessons/clo-625-l2`), on both `ubuntu-latest` and `macos-latest` (agents run on both; it catches BSD/GNU `date`/`grep`/`sed` differences). It runs `shellcheck` on `.pi/scripts/pr-review-cycle.sh`, `.pi/scripts/tests/pr-review-cycle.test.sh`, `.pi/scripts/tests/fake-gh/gh`, then `sh .pi/scripts/tests/pr-review-cycle.test.sh`, then the acceptance-criterion guard (decision 5). `shellcheck` and `jq` are preinstalled on both runner images; the Ubuntu leg `apt-get install -y zsh` for the call-site tests, and the tests skip that leg gracefully if installation were to fail. The job's name is added to `ci-gate`'s `needs`, and its result is added as an entry in the assertion loop. The loop checks only the jobs it lists, so adding the job to `needs` alone would not fail `CI Gate`.

### Manual verification

1. Locally on macOS: run `shellcheck` over the three files, then `sh .pi/scripts/tests/pr-review-cycle.test.sh`.
2. Read-only live probes against `maxkulish/lok`:
   - `probe-bots --pr 106` should exit 4.
   - `probe-bots --pr 89` should exit 0 with output other than `none`.
   - `wait-rereview --pr 80 --head <COVERED_SHA> --since <TS_BEFORE_CLEAN_PASS> --timeout 0` should exit 0 and print that SHA.
   - `new-comments --pr 71` should report the bound it used.
3. Dogfood: this task's own PR goes through skill steps 1-9 using the rewritten markdown and the script. Record the call outputs and exit statuses in the PR body.
4. Negative CI proof: push a temporary commit that introduces a `shellcheck` warning (for example, an unquoted expansion). Confirm the new job and `CI Gate` both fail, then revert.
5. Acceptance criterion 1: `rg -n 'wait_for_bot_review|--slurp' .pi/skills/pr-review-cycle.md .claude/commands/pr/review.md` returns no matches.

## Migration / rollout

Nothing changes for the crate: there is no Rust code, `Cargo.toml`, dependency or `lok` CLI change, and no feature flag. For the orchestration procedure, the change is a behavior-preserving port. The only intended differences are the unification of the two drifted copies (Open question 1) and the fail-open paths closed by the tests: a probe that errors, an empty `--bots`, and a request whose response lacks `created_at`.

Rollout is a single PR, in this order:

1. Add the script, the fake `gh`, the fixtures and the test runner. The tests pass locally on macOS.
2. Add the CI job and wire it into `ci-gate`'s `needs` and assertion loop. The job passes, and the negative proof fails as expected.
3. Rewrite `.pi/skills/pr-review-cycle.md` steps 1a, 1b, 2, 7 and 8, and `.claude/commands/pr/review.md` Step 9.5, as prose plus script calls. Update the helper pointer in `.claude/commands/task/phases/pr.md` Step 4.3. Keep the incident rationale and the `.pi/lessons/pr-review-failures.md` citations in prose.
4. Dogfood the rewritten procedure on this PR's own `pr` phase.

In-flight tasks in sibling worktrees keep the old markdown until they rebase. The script reads no caller shell variables, so a session that switches from old snippets to script calls mid-phase cannot pick up stale shell state. The workflow YAML fields (`bot_review_wait_completed`, `bot_rereview_head_sha`, `bot_rereview_at`) and `PHASE_CONFIG` in `.pi/extensions/orchestrate/index.ts` are unchanged, so `node .pi/scripts/check-schema-parity.mjs` stays green. Rollback is a revert of the PR.

`CI Gate` is not a required status check today (see Assumptions), so GitHub will not block a merge on a failing shell-gates job until a required check is restored. That decision is outside this task. CLO-650 follows on top of this change and edits `bot_login_filter` plus its tests.

## Resolved decisions (soft gate, 2026-09-16)

The five draft open questions were resolved before the AI review ran:

1. **Ported semantics come from the skill, not from `/pr:review` Step 9.5.** Poll interval 10s, both reviewer logins on the reviews leg, `qodo` on the completion-comment leg. The skill is the spec of record (discovery: "the prose encodes real, verified behavior"); the command file's 20s/qodo-only values are drift, and unifying on one implementation is the point of the task. This changes `/pr:review`'s wait rhythm and accepts a Copilot review Step 9.5 would have ignored - both intended.
2. **A sixth subcommand `unresolved-threads` is in scope; the `phases/pr.md` §5.0 pre-merge gate is not.** Skill step 8's GraphQL block is gate logic inline in one of the two files acceptance criterion 1 names, cannot compress to an illustrative one-liner, and is copy-pasted three times across those files, so it moves into the script. §5.0 lives in a third file outside the criterion, reads workflow YAML via `yq` (a different input domain than `gh`), and replacing it is tracked as follow-up.
3. **Dedicated CI job on both OSes** (see Test plan § CI), not a Rust integration test: the crate's `cargo test` (including the 20x `etxtbsy-stress` loop) must not depend on `.pi/` tooling or `jq`.
4. **CLO-650 stays separate.** The tests here pin the current substring matching with comments naming CLO-650; folding exact identity matching in would smuggle a semantic change into a port.
5. **Acceptance criterion 1 is enforced in CI by a guard that is itself tested.** The shell-gates job ends with `sh .pi/scripts/tests/check-inline-gates.sh`. The script takes optional file paths and defaults to exactly two files - `.pi/skills/pr-review-cycle.md` and `.claude/commands/pr/review.md` - so the self-test can point it at a scratch file. It runs `grep -nE` over the files and exits non-zero when a line matches a banned shape. The ban list is an explicit ERE alternation, one pattern per line in the script:

   | Banned shape | ERE |
   |---|---|
   | removed helper name | `wait_for_bot_review` |
   | slurp pipeline | `gh api[^|]*--slurp` |
   | paginated REST call | `gh api[^|]*--paginate` |
   | `gh api --jq` | `gh api[^|]*--jq` |
   | `gh api --arg` | `gh api[^|]*--arg` |
   | raw jq output built in a gate | `\| *jq -r` |
   | inline poll deadline | `DEADLINE=` |
   | inline poll sleep | `sleep 1([^0-9]|$)` |
   | inline review-submission field | `submitted_at` |
   | inline re-review POST | `-f body='/agentic_review'` |

   A line that legitimately needs one of these shapes is exempted by an explicit entry in `.pi/scripts/tests/inline-gate-allowlist.txt` - one ERE per line, each with a trailing `#` explaining why. The guard prints every matching line before failing and prints `ok: no inline gate shapes` on success. It runs as its own shell-gates job step (not inside `pr-review-cycle.test.sh`) so a guard failure and a test failure stay distinguishable in the CI log. `test_inline_gate_guard_fails_on_each_banned_shape` (see Test plan) re-runs the guard against a scratch file per banned ERE, so a regex that stops matching fails a test rather than silently disarming the guard.

The remaining genuinely open tradeoffs, carried to the plan phase: none blocking.

## Open questions

None blocking the plan phase; see "Resolved decisions" above. Follow-ups tracked:

- Pre-merge re-fetch gate in `.pi/orchestrator/phases/pr.md` §5.0 (reads workflow YAML via `yq`; needs its own subcommand and is outside acceptance criterion 1's file pair).
- `.lok/workflows/*.toml` reviewer legs conflate bad invocation with empty model output on `REVIEW_FAILED` (per the Linear issue body).
