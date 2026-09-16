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
