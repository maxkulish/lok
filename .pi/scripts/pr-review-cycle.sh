#!/bin/sh
# pr-review-cycle.sh - executable merge-gate helpers for the pr-review-cycle
# skill (.pi/skills/pr-review-cycle.md) and /pr:review
# (.claude/commands/pr/review.md).
#
# Design: docs/designs/clo-623-pr-review-cycle-tested.md
#
# POSIX shell, deliberately. There is no `set -e` and no `pipefail`: several
# subcommands exit non-zero as their gate verdict, so an ambient `set -e`
# would turn a verdict into an abort, and a gate that depends on an ambient
# shell setting is one paste away from losing it (docs/lessons/clo-625-l6).
# Every status that matters is checked explicitly. `set -u` is on, with
# `:-` defaults on every optional variable.

set -u

PROG=pr-review-cycle

# --- exit codes (design: "Public API surface") ---------------------------
# The published contract the markdown call sites switch on. Rendered through
# usage() so the table cannot drift from the implementation.
EX_OK=0          # condition met
EX_NEGATIVE=1    # negative gate verdict: no pass before the deadline, or new comments found
EX_INVALID=2     # invalid or inapplicable input; nothing was called or posted
EX_UPSTREAM=3    # gh failed, returned an empty body, or returned malformed/missing data
EX_BILLING=4     # probe-bots only: qodo-code-review is billing-blocked on this PR

usage() {
  cat <<'EOF'
usage: pr-review-cycle.sh <subcommand> [options]

subcommands:
  probe-bots         --repo <OWNER/REPO> --pr <N>
  wait-review        --repo <OWNER/REPO> --pr <N> [--timeout <SECONDS>]
  request-rereview   --repo <OWNER/REPO> --pr <N> --bots <LIST|none>
  wait-rereview      --repo <OWNER/REPO> --pr <N> --since <ISO8601Z> [--head <SHA40>] [--timeout <SECONDS>]
  new-comments       --repo <OWNER/REPO> --pr <N> [--since <ISO8601Z>]
  unresolved-threads --repo <OWNER/REPO> --pr <N>

environment (test seams, not for operators):
  PR_REVIEW_CYCLE_POLL_INTERVAL  seconds between poll ticks; default 10
  PR_REVIEW_CYCLE_CALL_DEADLINE  per-request cap for any single gh call; default 30
EOF
  printf '\nexit codes:\n'
  printf '  %s  condition met\n' "$EX_OK"
  printf '  %s  negative gate verdict: no pass before the deadline, or new comments found\n' "$EX_NEGATIVE"
  printf '  %s  invalid or inapplicable input; nothing was called or posted\n' "$EX_INVALID"
  printf '  %s  upstream failure: gh failed, returned an empty body, or returned malformed data\n' "$EX_UPSTREAM"
  printf '  %s  qodo-code-review is billing-blocked (probe-bots only)\n' "$EX_BILLING"
  printf '\nOnly exit 0 may be recorded as a passed gate.\n'
}

diag() {
  # diag <subcommand> <message...>
  sub=$1
  shift
  printf '%s: %s: %s\n' "$PROG" "$sub" "$*" >&2
}

fail() {
  # fail <exit-code> <message...> - always names the current subcommand
  rc=$1
  shift
  diag "${SUB:-?}" "$@"
  exit "$rc"
}

# --- input validation ----------------------------------------------------
# Validators run before any API call, so an invalid invocation can never be
# reported as an upstream failure and never reaches `gh`. Each one assigns the
# validated value to a global after checking it.

require_repo() {
  r=${1:-}
  [ -n "$r" ] || fail "$EX_INVALID" "--repo is required"
  case "$r" in
    */*/*) fail "$EX_INVALID" "invalid --repo '$r' (want OWNER/REPO)" ;;
    */*) : ;;
    *) fail "$EX_INVALID" "invalid --repo '$r' (want OWNER/REPO)" ;;
  esac
  REPO=$r
}

require_pr() {
  p=${1:-}
  [ -n "$p" ] || fail "$EX_INVALID" "--pr is required"
  case "$p" in
    *[!0-9]*) fail "$EX_INVALID" "invalid --pr '$p' (want a number)" ;;
    *) : ;;
  esac
  PR=$p
}

# --- subcommands ---------------------------------------------------------
# Each command resets the globals it owns, so an inherited environment
# variable can never stand in for a required flag.

cmd_probe_bots() {
  REPO=""; PR=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo) require_repo "${2:-}"; shift 2 ;;
      --pr)   require_pr   "${2:-}"; shift 2 ;;
      -h|--help) usage; exit "$EX_OK" ;;
      *) fail "$EX_INVALID" "unexpected argument '$1'" ;;
    esac
  done
  [ -n "$REPO" ] || fail "$EX_INVALID" "--repo is required"
  [ -n "$PR" ]   || fail "$EX_INVALID" "--pr is required"
  fail "$EX_UPSTREAM" "probe-bots is not implemented in this revision"
}

cmd_wait_review() {
  fail "$EX_UPSTREAM" "wait-review is not implemented in this revision"
}

cmd_request_rereview() {
  fail "$EX_UPSTREAM" "request-rereview is not implemented in this revision"
}

cmd_wait_rereview() {
  fail "$EX_UPSTREAM" "wait-rereview is not implemented in this revision"
}

cmd_new_comments() {
  fail "$EX_UPSTREAM" "new-comments is not implemented in this revision"
}

cmd_unresolved_threads() {
  fail "$EX_UPSTREAM" "unresolved-threads is not implemented in this revision"
}

main() {
  [ $# -ge 1 ] || { usage >&2; exit "$EX_INVALID"; }
  SUB=$1
  shift
  case "$SUB" in
    probe-bots)         cmd_probe_bots "$@" ;;
    wait-review)        cmd_wait_review "$@" ;;
    request-rereview)   cmd_request_rereview "$@" ;;
    wait-rereview)      cmd_wait_rereview "$@" ;;
    new-comments)       cmd_new_comments "$@" ;;
    unresolved-threads) cmd_unresolved_threads "$@" ;;
    -h|--help|help)     usage; exit "$EX_OK" ;;
    *)
      diag "$SUB" "unknown subcommand '$SUB'"
      usage >&2
      exit "$EX_INVALID"
      ;;
  esac
}

main "$@"
