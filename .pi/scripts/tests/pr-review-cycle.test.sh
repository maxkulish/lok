#!/bin/sh
# Test suite for .pi/scripts/pr-review-cycle.sh
#
#   sh .pi/scripts/tests/pr-review-cycle.test.sh ['grep -E pattern']
#
# Runs every test_* function defined below whose name matches the optional
# egrep pattern - ERE, so write `a|b`, not `a\|b`. Exits non-zero if any test
# fails. Each test runs in its own subshell with a fresh scratch directory and
# the fake `gh` first on PATH, so no test touches the network or a real repo.

set -u

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)

SCRIPT="$ROOT/.pi/scripts/pr-review-cycle.sh"
FAKE_GH_DIR="$HERE/fake-gh"
FAKE_GH_BIN="$FAKE_GH_DIR/gh"
FIXTURES="$HERE/fixtures/pr-review-cycle"

PATTERN=${1:-.}

WORK=$(mktemp -d) || exit 1
trap 'rm -rf "$WORK"' EXIT INT TERM

# --- helpers -------------------------------------------------------------

fail_test() {
  printf '    %s\n' "$*" >&2
  return 1
}

skip_test() {
  printf '    SKIP: %s\n' "$*"
  exit 0
}

# Point the fake `gh` at a scenario directory. Created on demand so an
# assertion-only scenario needs no fixtures.
use_fixture() {
  FAKE_GH_SCENARIO="$FIXTURES/$1"
  mkdir -p "$FAKE_GH_SCENARIO"
  export FAKE_GH_SCENARIO
}

# Run the script under test, capturing stdout/stderr into OUT/ERR and the
# verdict into RC. Always returns 0 so the caller's assertions drive the test.
run_script() {
  sh "$SCRIPT" "$@" >"$FAKE_GH_STATE/stdout" 2>"$FAKE_GH_STATE/stderr"
  RC=$?
  OUT=$(cat "$FAKE_GH_STATE/stdout")
  ERR=$(cat "$FAKE_GH_STATE/stderr")
  return 0
}

# Invoke the fake `gh` directly, for assertions about the fake itself.
fake_gh() {
  "$FAKE_GH_BIN" "$@"
}

calls() {
  cat "$FAKE_GH_STATE/calls.log" 2>/dev/null
}

assert_rc() {
  [ "$RC" -eq "$1" ] || fail_test "expected exit $1, got $RC (stderr: $ERR)"
}

assert_out() {
  [ "$OUT" = "$1" ] || fail_test "expected stdout '$1', got '$OUT'"
}

assert_out_empty() {
  [ -z "$OUT" ] || fail_test "expected empty stdout, got '$OUT'"
}

assert_err_contains() {
  case "$ERR" in
    *"$1"*) return 0 ;;
    *) fail_test "stderr does not contain '$1' (got: $ERR)" ;;
  esac
}

# --- script skeleton and invocation shape --------------------------------

test_unknown_subcommand_exits_2() {
  use_fixture unknown_subcommand
  run_script frobnicate
  assert_rc 2 || return 1
  assert_err_contains "unknown subcommand" || return 1
  [ -z "$(calls)" ] || fail_test "an unknown subcommand must not reach gh: $(calls)" || return 1
}

test_no_arguments_exits_2() {
  use_fixture no_arguments
  run_script
  assert_rc 2 || return 1
  [ -z "$(calls)" ] || fail_test "no arguments must not reach gh" || return 1
}

test_missing_repo_rejected_without_calling_gh() {
  use_fixture skeleton_validation
  run_script probe-bots --pr 71
  assert_rc 2 || return 1
  assert_err_contains "--repo is required" || return 1
  [ -z "$(calls)" ] || fail_test "a validation failure must not reach gh: $(calls)" || return 1
}

# --- probe-bots -----------------------------------------------------------

probe_bots() {
  run_script probe-bots --repo maxkulish/lok --pr 71
}

test_probe_bots_reports_qodo_from_current_pr_comments() {
  # A newly installed bot has no history on earlier PRs; this PR's own comments
  # are the earliest observable proof of installation
  # (.pi/lessons/pr-review-failures.md L1).
  use_fixture probe_bots_qodo_current_pr
  probe_bots
  assert_rc 0 || return 1
  assert_out "qodo-code-review" || return 1
}

test_probe_bots_reports_bot_from_prior_pr_reviews() {
  use_fixture probe_bots_qodo_prior_pr
  probe_bots
  assert_rc 0 || return 1
  assert_out "qodo-code-review" || return 1
}

test_probe_bots_joins_multiple_logins_with_commas() {
  use_fixture probe_bots_two_bots
  probe_bots
  assert_rc 0 || return 1
  assert_out "copilot-pull-request-reviewer,qodo-code-review" || return 1
}

test_probe_bots_prints_none_when_no_bot_activity() {
  use_fixture probe_bots_none
  probe_bots
  assert_rc 0 || return 1
  assert_out "none" || return 1
}

test_probe_bots_fails_closed_when_comments_lookup_errors() {
  use_fixture probe_bots_comments_error
  probe_bots
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_probe_bots_fails_closed_when_prior_pr_review_lookup_errors() {
  use_fixture probe_bots_prior_error
  probe_bots
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_probe_bots_exits_4_when_qodo_billing_blocked() {
  use_fixture probe_bots_billing_blocked
  probe_bots
  assert_rc 4 || return 1
  assert_out "qodo-code-review" || return 1
}

test_probe_bots_rejects_missing_pr_before_any_api_call() {
  use_fixture probe_bots_missing_pr
  run_script probe-bots --repo maxkulish/lok
  assert_rc 2 || return 1
  [ -z "$(calls)" ] || fail_test "a validation failure must not reach gh: $(calls)" || return 1
}

# --- the per-call deadline ------------------------------------------------

test_gh_call_hung_request_fails_closed_after_deadline() {
  use_fixture probe_bots_hang
  PR_REVIEW_CYCLE_CALL_DEADLINE=1
  export PR_REVIEW_CYCLE_CALL_DEADLINE
  start=$(date -u +%s)
  probe_bots
  elapsed=$(( $(date -u +%s) - start ))
  assert_rc 3 || return 1
  [ "$elapsed" -lt 20 ] \
    || fail_test "the per-call deadline did not fire (took ${elapsed}s)" || return 1
}

# --- wait-review and the shared poll -------------------------------------- #

wait_review() {
  run_script wait-review --repo maxkulish/lok --pr 71 --timeout 0
}

test_wait_review_accepts_review_object_on_head() {
  use_fixture wait_review_pass_review_object
  wait_review
  assert_rc 0 || return 1
  assert_out "2026-09-10T10:05:00Z" || return 1
}

test_wait_review_accepts_qodo_completion_comment_naming_head() {
  # A clean pass submits no review object at all; it announces itself by
  # editing a persistent comment and posting a new completion comment naming
  # the head (the PR #80 shape). A reviews-endpoint-only poll times out here.
  use_fixture wait_review_pass_completion_comment
  wait_review
  assert_rc 0 || return 1
  assert_out "2026-09-10T10:06:00Z" || return 1
}

test_wait_review_rejects_review_on_older_commit() {
  use_fixture wait_review_stale_commit
  wait_review
  assert_rc 1 || return 1
  assert_out_empty || return 1
}

test_wait_review_rejects_completion_comment_naming_other_sha() {
  use_fixture wait_review_completion_other_sha
  wait_review
  assert_rc 1 || return 1
  assert_out_empty || return 1
}

test_wait_review_rejects_pass_before_since() {
  use_fixture wait_review_pass_before_since
  wait_review
  assert_rc 1 || return 1
  assert_out_empty || return 1
}

test_wait_review_ignores_persistent_comment_updated_at() {
  # updated_at bumps mid-pass and on post-merge permalink refreshes, so a fresh
  # updated_at is not evidence of a pass.
  use_fixture wait_review_persistent_comment_updated_at
  wait_review
  assert_rc 1 || return 1
}

test_wait_review_gates_on_created_at_not_server_since() {
  # The fake ignores ?since=, so this pins that the jq created_at comparison - # not the query string - is the gate.
  use_fixture wait_review_old_comment_ignored_by_created_at
  wait_review
  assert_rc 1 || return 1
}

test_wait_review_accepts_pass_exactly_at_since() {
  # Pins the inclusive >= boundary: a pass and the request in the same second
  # must still count.
  use_fixture wait_review_pass_exactly_at_since
  wait_review
  assert_rc 0 || return 1
  assert_out "2026-09-10T10:00:00Z" || return 1
}

test_wait_review_times_out_closed() {
  use_fixture wait_review_timeout
  run_script wait-review --repo maxkulish/lok --pr 71 --timeout 0
  assert_rc 1 || return 1
  assert_out_empty || return 1
}

test_wait_review_fails_closed_when_pulls_lookup_errors() {
  use_fixture wait_review_pulls_error
  wait_review
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_wait_review_fails_closed_on_malformed_head_from_lookup() {
  use_fixture wait_review_malformed_head
  wait_review
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_wait_review_call_deadline_respects_overall_timeout() {
  # .pi/lessons/timeout-layering.md L1: the per-call deadline must be clamped to
  # the caller's remaining budget. With a hung call and a 3s overall timeout,
  # an unclamped 30s per-call deadline would keep the gate blocked for 30s.
  use_fixture wait_review_hang
  start=$(date -u +%s)
  run_script wait-review --repo maxkulish/lok --pr 71 --timeout 3
  elapsed=$(( $(date -u +%s) - start ))
  [ "$RC" -ne 0 ] || fail_test "a hung call must not report a pass" || return 1
  [ "$elapsed" -lt 20 ] \
    || fail_test "the inner deadline outlived the outer timeout (took ${elapsed}s)" || return 1
}

# --- request-rereview / wait-rereview -------------------------------------

test_request_rereview_posts_agentic_review_and_returns_post_created_at() {
  use_fixture request_rereview_ok
  run_script request-rereview --repo maxkulish/lok --pr 71 --bots qodo-code-review
  assert_rc 0 || return 1
  assert_out "2026-09-10T11:00:00Z" || return 1
  case "$(calls)" in
    *"-X POST"*) : ;;
    *) fail_test "no POST was issued: $(calls)" || return 1 ;;
  esac
  case "$(calls)" in
    *"body=/agentic_review"*) : ;;
    *) fail_test "the request body was not /agentic_review: $(calls)" || return 1 ;;
  esac
}

test_request_rereview_fails_closed_when_post_returns_no_created_at() {
  # Never fall back to local `date`: the poll compares this bound against
  # GitHub-clock timestamps, and a fast local clock would widen the window past
  # a genuine pass.
  use_fixture request_rereview_no_created_at
  run_script request-rereview --repo maxkulish/lok --pr 71 --bots qodo-code-review
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_request_rereview_requires_bots() {
  # The design's replacement for ${INSTALLED_BOTS+x}: the installed-bot verdict
  # arrives as an argument, so an unset variable cannot be read as "no bots".
  use_fixture request_rereview_ok
  run_script request-rereview --repo maxkulish/lok --pr 71
  assert_rc 2 || return 1
  [ -z "$(calls)" ] || fail_test "the guard must fire before any API call" || return 1
}

test_request_rereview_rejects_bots_without_qodo() {
  # Nothing to ask - posting anyway leaves a stray comment on the PR and then
  # fails the gate ten minutes later.
  use_fixture request_rereview_ok
  run_script request-rereview --repo maxkulish/lok --pr 71 --bots copilot-pull-request-reviewer
  assert_rc 2 || return 1
  case "$(calls)" in
    *"-X POST"*) fail_test "nothing to ask: no POST should be issued" || return 1 ;;
    *) : ;;
  esac
  assert_out_empty || return 1
}

test_request_rereview_rejects_none_bots() {
  use_fixture request_rereview_ok
  run_script request-rereview --repo maxkulish/lok --pr 71 --bots none
  assert_rc 2 || return 1
  case "$(calls)" in
    *"-X POST"*) fail_test "nothing to ask: no POST should be issued" || return 1 ;;
    *) : ;;
  esac
}

test_request_rereview_accepts_a_multi_bot_list() {
  use_fixture request_rereview_ok
  run_script request-rereview --repo maxkulish/lok --pr 71 \
    --bots copilot-pull-request-reviewer,qodo-code-review
  assert_rc 0 || return 1
  assert_out "2026-09-10T11:00:00Z" || return 1
}

test_request_rereview_rejects_malformed_head() {
  use_fixture request_rereview_ok
  run_script request-rereview --repo maxkulish/lok --pr 71 \
    --bots qodo-code-review --head not-a-sha
  assert_rc 2 || return 1
}

test_wait_rereview_accepts_pass_exactly_at_post_bound() {
  use_fixture wait_rereview_boundary
  run_script wait-rereview --repo maxkulish/lok --pr 71 \
    --since 2026-09-10T11:00:00Z --timeout 0
  assert_rc 0 || return 1
  assert_out "1111111111111111111111111111111111111111 2026-09-10T11:00:00Z" || return 1
}

test_wait_rereview_reports_the_head_it_covered() {
  # Both halves are reported: step 9 records the covered head, and phases/pr.md
  # 5.0 re-checks that head against the one being merged.
  use_fixture wait_rereview_boundary
  run_script wait-rereview --repo maxkulish/lok --pr 71 \
    --since 2026-09-10T11:00:00Z --head 2222222222222222222222222222222222222222 \
    --timeout 0
  assert_rc 1 || return 1
  assert_out_empty || return 1
}

test_wait_rereview_fails_closed_when_head_changes_mid_poll() {
  # The caller captured the pre-push head; the pass landed on the pushed head.
  # Reporting a pass here would record re-validation on a commit the bot never
  # saw.
  use_fixture wait_rereview_head_moved
  run_script wait-rereview --repo maxkulish/lok --pr 71 \
    --since 2026-09-10T11:00:00Z --head 1111111111111111111111111111111111111111 --timeout 0
  assert_rc 1 || return 1
  assert_out_empty || return 1
}

test_wait_rereview_requires_since() {
  # An empty bound would compare every timestamp against "" and pass on any
  # prior run's review of the same head.
  use_fixture wait_rereview_timeout
  run_script wait-rereview --repo maxkulish/lok --pr 71 --timeout 0
  assert_rc 2 || return 1
  [ -z "$(calls)" ] || fail_test "a missing bound must fail before any API call" || return 1
}

test_wait_rereview_rejects_malformed_since() {
  use_fixture wait_rereview_timeout
  run_script wait-rereview --repo maxkulish/lok --pr 71 --since 'yesterday' --timeout 0
  assert_rc 2 || return 1
}

test_wait_rereview_times_out_closed() {
  use_fixture wait_rereview_timeout
  run_script wait-rereview --repo maxkulish/lok --pr 71 \
    --since 2026-09-10T11:00:00Z --timeout 0
  assert_rc 1 || return 1
  assert_out_empty || return 1
}

# --- new-comments ---------------------------------------------------------

test_new_comments_reports_comments_after_the_bound() {
  use_fixture new_comments_after_bound
  run_script new-comments --repo maxkulish/lok --pr 71 --me me-user
  assert_rc 1 || return 1
  case "$OUT" in
    *'"id":2'*'qodo-code-review'*'still broken'*) : ;;
    *) fail_test "the later comment was not reported: $OUT" || return 1 ;;
  esac
  # one compact object per line
  [ "$(printf '%s\n' "$OUT" | wc -l | tr -d ' ')" = "1" ] \
    || fail_test "expected exactly one object: $OUT" || return 1
  case "$OUT" in
    *'"user":"qodo-code-review"'*) : ;;
    *) fail_test "body should be flat JSON with a string user: $OUT" || return 1 ;;
  esac
}

test_new_comments_exits_0_when_clean() {
  use_fixture new_comments_clean
  run_script new-comments --repo maxkulish/lok --pr 71 --me me-user
  assert_rc 0 || return 1
  assert_out_empty || return 1
}

test_new_comments_excludes_comment_exactly_at_bound() {
  # Strict `>`: the bound is the caller's own last reply, so a comment stamped
  # at exactly that second is not new.
  use_fixture new_comments_exact_bound
  run_script new-comments --repo maxkulish/lok --pr 71 --since 2026-09-10T10:06:00Z
  assert_rc 0 || return 1
  assert_out_empty || return 1
}

test_new_comments_null_max_falls_back_to_pr_created_at() {
  use_fixture new_comments_null_max
  run_script new-comments --repo maxkulish/lok --pr 71 --me me-user
  assert_rc 1 || return 1
  assert_err_contains "2026-09-10T10:00:00Z" || return 1
}

test_new_comments_never_hides_findings_behind_a_null_bound() {
  # The PR #71 defect: `max` over an empty array is JSON null, `jq -r` prints
  # the literal string "null", and every ISO timestamp sorts before "null" -
  # so the re-check reported clean while hiding every finding.
  use_fixture new_comments_null_max
  run_script new-comments --repo maxkulish/lok --pr 71 --me me-user
  assert_rc 1 || return 1
  case "$OUT" in
    *'"id":5'*) : ;;
    *) fail_test "the finding was hidden by a null bound: $OUT" || return 1 ;;
  esac
  case "$ERR" in
    *'bound: null'*) fail_test "the bound fell through to the literal null: $ERR" || return 1 ;;
    *) : ;;
  esac
}

test_new_comments_resolves_the_login_when_me_is_omitted() {
  use_fixture new_comments_me_lookup
  run_script new-comments --repo maxkulish/lok --pr 71
  assert_rc 1 || return 1
  case "$(calls)" in
    *'user'*) : ;;
    *) fail_test "the login was not resolved from the API: $(calls)" || return 1 ;;
  esac
}

test_new_comments_fails_closed_when_inline_lookup_errors() {
  use_fixture new_comments_error
  run_script new-comments --repo maxkulish/lok --pr 71 --me me-user
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_new_comments_rejects_malformed_since() {
  use_fixture new_comments_exact_bound
  run_script new-comments --repo maxkulish/lok --pr 71 --since 'last tuesday'
  assert_rc 2 || return 1
  [ -z "$(calls)" ] || fail_test "a malformed bound must fail before any API call" || return 1
}

# --- unresolved-threads ---------------------------------------------------

test_unresolved_threads_exits_0_when_only_resolved_threads_exist() {
  use_fixture unresolved_threads_clean
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 0 || return 1
  assert_out_empty || return 1
}

test_unresolved_threads_reports_unresolved_threads() {
  use_fixture unresolved_threads_reports
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 1 || return 1
  case "$OUT" in *PRRT_b*) : ;; *) fail_test "thread id missing: $OUT" || return 1 ;; esac
  case "$OUT" in *'"path":"src/main.rs"'*) : ;; *) fail_test "path missing: $OUT" || return 1 ;; esac
  case "$OUT" in *'"is_outdated":false'*) : ;; *) fail_test "is_outdated missing: $OUT" || return 1 ;; esac
  case "$OUT" in
    *'"latest_author":"qodo-code-review"'*) : ;;
    *) fail_test "latest_author missing: $OUT" || return 1 ;;
  esac
  case "$OUT" in *'off by one'*) : ;; *) fail_test "body missing: $OUT" || return 1 ;; esac
}

test_unresolved_threads_paginates_multiple_threads() {
  # pageInfo says there is a second page; a first-page-only report would hide
  # the second page's thread entirely.
  use_fixture unresolved_threads_paginates_multiple_threads
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 1 || return 1
  [ "$(printf '%s\n' "$OUT" | wc -l | tr -d ' ')" = "2" ] \
    || fail_test "expected a thread from each page, got: $OUT" || return 1
  case "$OUT" in *PRRT_p1*) : ;; *) fail_test "page 1 thread missing: $OUT" || return 1 ;; esac
  case "$OUT" in *PRRT_p2*) : ;; *) fail_test "page 2 thread missing: $OUT" || return 1 ;; esac
  case "$OUT" in
    *'"is_outdated":true'*) : ;;
    *) fail_test "the outdated flag was not carried through: $OUT" || return 1 ;;
  esac
}

test_unresolved_threads_fails_closed_when_graphql_errors() {
  use_fixture unresolved_threads_graphql_errors
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_unresolved_threads_fails_closed_on_missing_data() {
  use_fixture unresolved_threads_missing_data
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_unresolved_threads_fails_closed_on_null_review_threads() {
  use_fixture unresolved_threads_null_threads
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_unresolved_threads_fails_closed_on_malformed_node() {
  use_fixture unresolved_threads_malformed_node
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_unresolved_threads_fails_closed_on_missing_pageinfo() {
  # Without pageInfo.hasNextPage the walk would stop after page 1 and silently
  # truncate the report while still exiting 0.
  use_fixture unresolved_threads_missing_pageinfo
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

test_unresolved_threads_fails_closed_when_gh_call_errors() {
  use_fixture unresolved_threads_gh_error
  run_script unresolved-threads --repo maxkulish/lok --pr 71
  assert_rc 3 || return 1
  assert_out_empty || return 1
}

# --- the fake gh is itself a guard ---------------------------------------

test_fake_gh_rejects_jq_and_arg_flags() {
  use_fixture fake_gh_flags
  fake_gh api user --jq .login >"$FAKE_GH_STATE/o" 2>"$FAKE_GH_STATE/e"
  rc=$?
  [ "$rc" -eq 1 ] || fail_test "expected the fake to reject --jq with exit 1, got $rc" || return 1
  grep -q "unknown flag" "$FAKE_GH_STATE/e" \
    || fail_test "expected an unknown-flag message, got: $(cat "$FAKE_GH_STATE/e")" || return 1

  fake_gh api user --arg x y >/dev/null 2>&1
  [ $? -eq 1 ] || fail_test "expected the fake to reject --arg" || return 1

  # Even a rejected invocation is recorded, so the assertion above cannot be
  # satisfied by the fake silently doing nothing.
  grep -q -- "--jq" "$FAKE_GH_STATE/calls.log" \
    || fail_test "the rejected invocation should still be recorded in calls.log" || return 1
}

# --- call-site status capture (docs/lessons/clo-625-l4, clo-625-l6) -------
#
# The documented call site is `VAR=$(...); RC=$?` with no pipe. These tests
# run that exact shape through both shells an agent may use, and also pin the
# negative: piping the substitution launders the exit status to the last
# pipeline stage, which is why the call sites never pipe.

call_site_status_capture() {
  shell=$1
  command -v "$shell" >/dev/null 2>&1 || skip_test "$shell not found"
  use_fixture "call_site_$shell"

  # shellcheck disable=SC2016  # single quotes deliberate: $1/$RC must expand in the inner shell
  captured=$("$shell" -c 'BOTS=$("$1" no-such-subcommand --repo o/r --pr 1 2>/dev/null); RC=$?; printf "%s" "$RC"' _ "$SCRIPT")
  [ "$captured" = "2" ] \
    || fail_test "$shell: expected RC=2 from the assignment, got '$captured'" || return 1

  # shellcheck disable=SC2016  # single quotes deliberate: this pins that piping launders the status
  piped=$("$shell" -c 'BOTS=$("$1" no-such-subcommand --repo o/r --pr 1 2>/dev/null) | cat; RC=$?; printf "%s" "$RC"' _ "$SCRIPT")
  [ "$piped" = "0" ] \
    || fail_test "$shell: expected the piped form to launder the status to 0, got '$piped'" || return 1
}

test_call_site_status_capture_under_bash() {
  call_site_status_capture bash
}

test_call_site_status_capture_under_zsh() {
  call_site_status_capture zsh
}

# --- the inline-gate guard (CLO-623 acceptance criterion 1) ---------------

GUARD="$HERE/check-inline-gates.sh"

# The guard enumerates its own ban list and this replays it, so a regex that
# stops matching fails a test instead of silently disarming the guard. Writing
# the pattern text into the scratch file would be a useless probe - `\| *jq -r`
# does not match its own spelling - so each row carries an example line.
test_inline_gate_guard_fails_on_each_banned_shape() {
  scratch="$FAKE_GH_STATE/scratch.md"
  patterns="$FAKE_GH_STATE/patterns.txt"

  sh "$GUARD" --patterns > "$patterns" 2>"$FAKE_GH_STATE/err"
  RC=$?
  [ "$RC" -eq 0 ] \
    || { fail_test "guard --patterns exited $RC: $(cat "$FAKE_GH_STATE/err")"; return 1; }

  n=0
  while IFS= read -r row; do
    [ -n "$row" ] || continue
    pat=${row%%@@*}
    ex=${row#*@@}
    [ "$pat" != "$row" ] \
      || { fail_test "enumerated row has no '@@' separator: $row"; return 1; }
    printf '%s\n' "$ex" > "$scratch"
    if sh "$GUARD" "$scratch" >/dev/null 2>&1; then
      fail_test "guard accepted the banned shape '$pat' (example: $ex)"
      return 1
    fi
    n=$(( n + 1 ))
  done < "$patterns"

  [ "$n" -ge 10 ] \
    || { fail_test "only $n banned shapes enumerated; the ban list lost an entry"; return 1; }
}

# The rewritten skill needs no exemption; /pr:review keeps five display-only
# fetches, and the guard must report them rather than passing quietly.
test_inline_gate_guard_exempts_only_the_allowlist() {
  out=$(sh "$GUARD" "$ROOT/.pi/skills/pr-review-cycle.md" 2>&1)
  RC=$?
  [ "$RC" -eq 0 ] \
    || { fail_test "guard rejected the skill file (rc=$RC): $out"; return 1; }
  [ "$out" = "ok: no inline gate shapes" ] \
    || fail_test "expected the clean-file message, got: $out"

  out=$(sh "$GUARD" "$ROOT/.claude/commands/pr/review.md" 2>&1)
  RC=$?
  [ "$RC" -eq 0 ] \
    || { fail_test "guard rejected the allowlisted file (rc=$RC): $out"; return 1; }
  case "$out" in
    *"allowlisted line(s) exempted"*) return 0 ;;
    *) fail_test "guard passed the allowlisted file without reporting the exemptions: $out" ;;
  esac
}

# A guard that cannot find its target must not report success - failing open
# on a missing input is the defect class this task exists to remove.
test_inline_gate_guard_rejects_a_missing_file() {
  sh "$GUARD" "$FAKE_GH_STATE/does-not-exist.md" >/dev/null 2>&1
  RC=$?
  [ "$RC" -eq 2 ] || fail_test "expected exit 2 for a missing file, got $RC"
}

# --- runner --------------------------------------------------------------

run_one() {
  export PATH="$FAKE_GH_DIR:$PATH"
  export PR_REVIEW_CYCLE_POLL_INTERVAL=0
  "$1"
}

self="$HERE/pr-review-cycle.test.sh"
names_file="$WORK/names"
grep -oE '^test_[a-zA-Z0-9_]+' "$self" | sort -u > "$names_file"

total=0
failed=0
failed_names=""

while IFS= read -r name; do
  [ -n "$name" ] || continue
  printf '%s\n' "$name" | grep -qE "$PATTERN" || continue
  total=$((total + 1))
  FAKE_GH_STATE=$(mktemp -d)
  export FAKE_GH_STATE
  if ( run_one "$name" ); then
    printf 'ok   %s\n' "$name"
  else
    printf 'FAIL %s\n' "$name"
    failed=$((failed + 1))
    failed_names="$failed_names $name"
  fi
  rm -rf "$FAKE_GH_STATE"
done < "$names_file"

printf '\n%s test(s) matched, %s failed\n' "$total" "$failed"
if [ -n "$failed_names" ]; then
  printf 'failed:%s\n' "$failed_names"
fi

[ "$failed" -eq 0 ] || exit 1
