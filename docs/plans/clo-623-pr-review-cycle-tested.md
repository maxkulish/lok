# Plan: CLO-623 Make pr-review-cycle shell snippets executable and tested

## Context
- **Design:** docs/designs/clo-623-pr-review-cycle-tested.md
- **Discovery:** docs/discovery/clo-623.md
- **Linear:** https://linear.app/cloud-ai/issue/CLO-623/make-pr-review-cycle-shell-snippets-executable-and-tested
- **Branch:** `feat/clo-623-pr-review-cycle-tested`
- **Reviews:** docs/reviews/clo-623-review-ollama.md, docs/reviews/clo-623-review-synthesis.md (verdict `approve_with_changes`, 9 findings all applied)

One new POSIX shell script (`.pi/scripts/pr-review-cycle.sh`) becomes the single
executable home for the merge-gate logic currently living as 17 untested bash
blocks in `.pi/skills/pr-review-cycle.md` and `.claude/commands/pr/review.md`. A
hermetic fake-`gh` test harness proves every gate fails closed, a `shellcheck` CI
job enforces it, and a self-tested guard keeps the inline shapes from coming back.

The test runner is invoked as `sh .pi/scripts/tests/pr-review-cycle.test.sh [pattern]`,
where `pattern` is an optional `grep -E` filter over test-function names (ERE, so
alternation is `a|b`). Every sub-task's acceptance command uses that filter, so
each sub-task is independently verifiable without running the whole suite.

## Sub-tasks

### ST1 Test harness, fake `gh`, and script skeleton
**Files:** `.pi/scripts/tests/pr-review-cycle.test.sh`, `.pi/scripts/tests/fake-gh/gh`, `.pi/scripts/tests/fixtures/pr-review-cycle/`, `.pi/scripts/pr-review-cycle.sh`
**Acceptance:** `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_unknown_subcommand|test_fake_gh|test_call_site'` exits 0
**Estimate:** M

- **Script skeleton:** `#!/bin/sh`, no `set -e`, no `pipefail`, `set -u` with `:-` defaults. `main` dispatches on `$1`; unknown subcommand exits 2. The five validators (`require_repo`, `require_pr`, `require_sha40`, `require_iso8601z`, `require_bots`) run before any API call; `require_iso8601z` accepts only `YYYY-MM-DDTHH:MM:SSZ`. Optional-argument parsing with the `--timeout`/`--since`/`--head`/`--bots` shapes from the design's Public API surface.
- **Test runner:** iterate `test_*` functions in name order, apply the optional `grep -E` filter, set `FAKE_GH_SCENARIO`, `FAKE_GH_STATE=$(mktemp -d)`, prepend `.pi/scripts/tests/fake-gh` to `PATH`, set `PR_REVIEW_CYCLE_POLL_INTERVAL=0`, run the test body in a subshell, print `ok`/`FAIL`, and exit non-zero if any test failed. `PATH` and `FAKE_GH_*` must be in scope during the test body, not only during setup.
- **Fake `gh`:** accept only `api <path>`, `api graphql`, `--paginate`, `--slurp`, `-X POST`, `-f body=`, `-f query=`, `-f owner=`, `-f repo=`, `-F pr=`; any other flag (notably `--jq`, `--arg`) exits 1 with an unknown-flag message. Resolve `<METHOD> <path without query string>` to a fixture file; `.1.json`/`.2.json` sequences via a counter in `FAKE_GH_STATE`; `.exit` forces non-zero; `.hang` sleeps 60s. Append every invocation to `FAKE_GH_STATE/calls.log`.
- **Tests:** `test_unknown_subcommand_exits_2`, `test_fake_gh_rejects_jq_and_arg_flags`, `test_call_site_status_capture_under_zsh`, `test_call_site_status_capture_under_bash` (the zsh leg skips with a note when `zsh` is absent). The call-site tests exercise the documented `VAR=$(...); RC=$?` shape against the unknown-subcommand failure, per `docs/lessons/clo-625-l4` and `clo-625-l6`.

### ST2 Per-call deadline and `probe-bots`
**Files:** `.pi/scripts/pr-review-cycle.sh`, `.pi/scripts/tests/fixtures/pr-review-cycle/`, `.pi/scripts/tests/pr-review-cycle.test.sh`
**Acceptance:** `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_probe_bots|test_gh_call_hung'` exits 0
**Estimate:** M

- `gh_capture <var> <gh api args...>`: run `gh api`, store stdout in the named variable, exit 3 when `gh` exits non-zero, returns an empty body, or exceeds the deadline. Never pipe `gh` into `jq`.
- Deadline wrapper: POSIX-portable background process + `kill -0` poll + `kill`, because `gh api` has no request-timeout flag and macOS ships no `timeout` binary. Default 30s via `PR_REVIEW_CYCLE_CALL_DEADLINE`. An over-deadline request fails closed with exit 3.
- `probe-bots`: installed-bot probe over this PR's comments plus the last 10 PRs' reviews; `<!-- qodo:billing-blocked -->` marker → exit 4; prints `none` (never an empty line) when there is no bot activity.
- Fixtures seeded from the design's recorded PRs: #106 (billing-blocked), #89 (not blocked), #71 (bot first seen in this PR's comments).
- **Tests:** the seven `test_probe_bots_*` cases in the design's Test plan, plus `test_gh_call_hung_request_fails_closed_after_deadline` (`.hang` fixture with `PR_REVIEW_CYCLE_CALL_DEADLINE=1`).

### ST3 `pr_lookup`, `poll_for_pass`, and `wait-review`
**Files:** `.pi/scripts/pr-review-cycle.sh`, `.pi/scripts/tests/fixtures/pr-review-cycle/`, `.pi/scripts/tests/pr-review-cycle.test.sh`
**Acceptance:** `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_wait_review'` exits 0
**Estimate:** M

- `pr_lookup`: read and validate `head.sha` (40 hex) and `created_at` (`require_iso8601z`) from `pulls/<PR>`.
- `poll_for_pass <head_sha> <since> <timeout>`: port `wait_for_bot_review` — a review object on `<head_sha>` by a reviewer bot with `submitted_at >= <since>`, **or** a `qodo` issue comment with `created_at >= <since>` whose body matches `was updated up to the latest commit` and contains `<head_sha>`. Never gate on the persistent comment's `updated_at`. `?since=` stays a server-side prefilter; the jq `created_at` comparison is the gate.
- `bot_login_filter`: the single jq fragment for reviewer logins (`qodo-code-review|copilot-pull-request-reviewer`, plus the `qodo` match for the completion-comment and billing legs). CLO-650 changes only this.
- **Tests:** the eleven `test_wait_review_*` cases, including `test_wait_review_gates_on_created_at_not_server_since` and the new inclusive-boundary `test_wait_review_accepts_pass_exactly_at_since`.

### ST4 `request-rereview` and `wait-rereview`
**Files:** `.pi/scripts/pr-review-cycle.sh`, `.pi/scripts/tests/fixtures/pr-review-cycle/`, `.pi/scripts/tests/pr-review-cycle.test.sh`
**Acceptance:** `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_request_rereview|test_wait_rereview'` exits 0
**Estimate:** M

- `request-rereview`: POST `/agentic_review` with `-f body=`, print the `created_at` GitHub assigns. `--bots` that is empty or lacks `qodo-code-review` exits 2 and posts nothing (the ported `${INSTALLED_BOTS+x}` guard). A response without `created_at` exits 3 with **no** fallback to local `date`.
- `wait-rereview`: `--head` defaults to the `pulls/<PR>` head at call time; `--since` is required and must be valid `YYYY-MM-DDTHH:MM:SSZ` (exit 2 otherwise, so a failed POST cannot widen the window); non-40-hex head exits 2 (an empty head would make `contains()` vacuously true). Prints `<head_sha> <detected_at>`.
- **Tests:** the five `test_request_rereview_*` and seven `test_wait_rereview_*` cases, including `test_wait_rereview_fails_closed_when_head_changes_mid_poll`.

### ST5 `new-comments`
**Files:** `.pi/scripts/pr-review-cycle.sh`, `.pi/scripts/tests/fixtures/pr-review-cycle/`, `.pi/scripts/tests/pr-review-cycle.test.sh`
**Acceptance:** `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_new_comments'` exits 0
**Estimate:** M

- Resolve the bound from the authenticated user's latest inline comment (`max // empty`), falling back to the PR's `created_at` — never the literal `null` (the PR #71 defect). Print the bound used to stderr.
- Report inline comments with `created_at > <bound>` as one `jq -c` JSON object per line: `{"id":<ID>,"user":"<LOGIN>","body":"<BODY>"}`. Exit 1 when any exist, 0 when clean, 3 on upstream failure, 2 on malformed `--since`.
- **Tests:** the six `test_new_comments_*` cases, including `test_new_comments_null_max_falls_back_to_pr_created_at` and the strict-boundary `test_new_comments_excludes_comment_exactly_at_bound`.

### ST6 `unresolved-threads` and `graphql_query_review_threads`
**Files:** `.pi/scripts/pr-review-cycle.sh`, `.pi/scripts/tests/fixtures/pr-review-cycle/`, `.pi/scripts/tests/pr-review-cycle.test.sh`
**Acceptance:** `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_unresolved_threads'` exits 0
**Estimate:** M

- `graphql_query_review_threads`: the single copy of the `reviewThreads` query that today appears verbatim three times in the skill.
- Fail closed on every malformed GraphQL response: non-zero `gh` exit, a non-empty `errors` array, missing or null `data`, or a thread node missing a required field → exit 3, never an empty result that reads as "no unresolved threads".
- Output every **unresolved** thread as one `jq -c` JSON object per line: `{"id":"<NODE_ID>","path":"<PATH>","line":<LINE>,"is_outdated":<BOOL>,"latest_author":"<LOGIN>","latest_body":"<BODY>"}`. Exit 1 when any exist (loop back to step 4), 0 with empty stdout when clean.
- The fake resolves the single `graphql` endpoint to `<scenario>/graphql__reviewThreads.json`, and a second page exercises pagination.
- **Tests:** the four `test_unresolved_threads_*` cases, including `test_unresolved_threads_fails_closed_when_graphql_errors` and `test_unresolved_threads_paginates_multiple_threads`.

### ST7 Rewrite `.pi/skills/pr-review-cycle.md`
**Files:** `.pi/skills/pr-review-cycle.md`
**Acceptance:** `rg -n 'wait_for_bot_review|--slurp|DEADLINE=|submitted_at' .pi/skills/pr-review-cycle.md` returns no matches, and `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_call_site'` still exits 0
**Estimate:** L

Steps 1a, 1b, 2, 7 and 8 become prose plus single-line script calls of the documented shape `INSTALLED_BOTS=$(...); RC=$?`. The helper definition `wait_for_bot_review`, the inline `poll_for_pass` loop, the `DEADLINE=`/`sleep 1` poll scaffolding, the `REPLY_PUSH_TS` jq pipeline (→ `new-comments`), and the three verbatim GraphQL blocks (→ `unresolved-threads`) are deleted. Keep the incident rationale, the `.pi/lessons/pr-review-failures.md` citations, the delivery-shape table, the billing-block decision branch, the "timestamps come from GitHub's clock" rule, and the one-reply-per-thread rule in prose. Add a prerequisites note that the script is invoked from the repository or worktree root.

### ST8 Rewrite `.claude/commands/pr/review.md` and the `phases/pr.md` pointer
**Files:** `.claude/commands/pr/review.md`, `.claude/commands/task/phases/pr.md`
**Acceptance:** `rg -n 'wait_for_bot_review|--slurp|--paginate --jq' .claude/commands/pr/review.md .claude/commands/task/phases/pr.md` returns no matches
**Estimate:** M

Step 9.5's inline billing check, request and poll become script calls (`probe-bots`, `request-rereview`, `wait-rereview`). Step 9.5's drifted values (20s interval, `qodo`-only reviews leg) are retired in favour of the skill's (10s, both reviewer logins) per decision 1; call that out in a one-line comment so the behavior change is visible. In `phases/pr.md` Step 4.3, the `wait_for_bot_review` pointer names the script instead. The `yq ...| tail -1` pre-merge gate in §5.0 is **not** touched (follow-up issue).

### ST9 CI `shell-gates` job, acceptance guard, and `ci-gate` wiring
**Files:** `.github/workflows/ci.yml`, `.pi/scripts/tests/check-inline-gates.sh`, `.pi/scripts/tests/inline-gate-allowlist.txt`
**Acceptance:** `sh .pi/scripts/tests/check-inline-gates.sh` exits 0, `sh .pi/scripts/tests/pr-review-cycle.test.sh 'test_inline_gate_guard'` exits 0, and a scratch file containing `wait_for_bot_review` makes the guard exit non-zero
**Estimate:** M

- `check-inline-gates.sh`: takes optional file paths, defaults to the two markdown files, runs `grep -nE` over them, prints every matching line, exits non-zero on a match and prints `ok: no inline gate shapes` when clean. The ban list is the ten EREs in the design's decision 5 table, one per line.
- `inline-gate-allowlist.txt`: one ERE per line with a trailing `#` rationale, for legitimate illustrative one-liners.
- `test_inline_gate_guard_fails_on_each_banned_shape`: for every banned ERE, write it into a scratch file and assert the guard fails; assert the allowlist example passes. A regex that stops matching must fail a test.
- CI job: no `paths:` filter (`docs/lessons/clo-625-l2`); runs on `ubuntu-latest` and `macos-latest`; prints `shellcheck --version`/`jq --version`; runs `shellcheck --shell=sh` over the script, the test runner and the fake `gh`; runs the test runner; runs the guard. The Ubuntu leg `apt-get install -y zsh`; the zsh test leg skips gracefully if that fails. Add the job to `ci-gate`'s `needs` **and** to the assertion loop (the loop checks only listed jobs, so `needs` alone would not fail `CI Gate`).
- Negative proof, part 1 (**local, done at ST9**): a scratch copy of `pr-review-cycle.sh` with `probe_unused_variable=1` appended makes `shellcheck --shell=sh` exit 1 (SC2034), a scratch copy of the skill with `DEADLINE=...` appended makes the guard exit 1 naming the line, and the `ci-gate` assertion loop exits 1 given a `shell-gates:failure` pair.
- Negative proof, part 2 (**deferred to ST10**, by sequencing). Confirming the job *and* `CI Gate` go red needs a failing CI run, and CI does not run this branch until the PR exists - `push` is filtered to `main` and `pull_request` needs the PR. Opening one earlier would violate the implement phase's Step 4.6. ST10 records the failing run URL per `docs/lessons/clo-638-msrv-gate-lessons.md` L3, which forbids substituting a local command for that evidence: push a temporary commit with a `shellcheck` warning, capture both the `shell-gates` and `CI Gate` run URLs, then revert.

### ST10 Dogfood and acceptance verification
**Files:** none (verification only; PR body records results)
**Acceptance:** this task's own `pr` phase completes using the rewritten markdown and the script, with call outputs and exit statuses recorded in the PR body; `rg -n 'wait_for_bot_review|--slurp' .pi/skills/pr-review-cycle.md .claude/commands/pr/review.md` returns no matches
**Estimate:** S

Run the read-only live probes from the design's Manual verification §2 (`probe-bots --pr 106` → 4; `probe-bots --pr 89` → 0 with non-`none`; `wait-rereview --pr 80 --head <COVERED_SHA> --since <TS> --timeout 0` → 0; `new-comments --pr 71` reports its bound), then dogfood steps 1–9 of the rewritten skill on this PR. Confirm `node .pi/scripts/check-schema-parity.mjs` stays green.

Also complete ST9's deferred negative proof (part 2): push a temporary commit carrying a `shellcheck` warning, record the `shell-gates` and `CI Gate` failing run URLs, confirm `CI Gate` is red *because of* `shell-gates` and not some other leg, then revert the commit and confirm both go green.

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- `shellcheck --shell=sh .pi/scripts/pr-review-cycle.sh .pi/scripts/tests/pr-review-cycle.test.sh .pi/scripts/tests/check-inline-gates.sh .pi/scripts/tests/fake-gh/gh`
- `sh .pi/scripts/tests/pr-review-cycle.test.sh`
- `sh .pi/scripts/tests/check-inline-gates.sh`

The Rust line is the documented repository gate and still runs (no Rust code changes, so it is a no-op risk-wise). The three shell lines are the task-specific superset; all four are also enforced by CI.

## Risks
- **Fixture staleness.** Qodo edits comments in place, so a fixture recorded today may show a post-edit state rather than the state at pass time. Mitigation: hand-trim to the shapes the prose documents, and keep the fixture fields to exactly what the jq filters read.
- **`shellcheck` absent on the local macOS host.** Present here (0.11.0, Homebrew), but a contributor without it cannot run the gate locally. Mitigation: CI is the enforcement point; the local command is documented, not required.
- **`zsh` install on `ubuntu-latest`.** Unverified (medium confidence). Mitigation: the runner skips the zsh leg with a note rather than failing, and the job installs zsh first so the skip should never fire on Linux. Re-validated at Step 2.5.
- **`shellcheck` present on both CI images.** Assumed (medium confidence). Mitigation: the macOS leg installs it via brew when absent, so a missing linter fails loudly instead of silently no-op'ing the step that is the gate.
- **Markdown rewrite is the largest diff.** 17 bash blocks across two files; a missed inline block is caught by ST9's guard, and a semantic drift is caught by ST10's dogfood run.
- **Lesson `clo-625-l6` (pipeline status laundering).** The guard explicitly bans `| jq -r` and `DEADLINE=`/`sleep 1` inline poll scaffolding; the script checks every status explicitly and never pipes `gh` into `jq`.
- **Lesson `clo-625-l4` (zsh does not word-split).** Every expansion is quoted; the call-site tests run under both `zsh` and `bash`.
- **Lesson `clo-625-l2`.** No `paths:` filter on the new job. On the assumption that `CI Gate` is not currently a required check (re-verified live 2026-09-16: ruleset 20153405 lists only `deletion` and `non_fast_forward`), the job will not block a merge by itself; restoring a required check is out of scope.
- **Lesson `clo-632-l1` (possibly stale).** Nothing here reads `.lok/lok.toml` or any repository config at runtime — the script's inputs are arguments only. No trust-boundary change.
- **Scope creep into CLO-650.** `bot_login_filter` is the single identity seam; tests pin today's substring behavior with a comment naming CLO-650 as the owner of any tightening.
