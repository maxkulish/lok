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

# The re-review request command. `/agentic_review` is Qodo's configured trigger;
# `/review` is the legacy PR-Agent name and is not wired up here.
REQUEST_REREVIEW_COMMAND='/agentic_review'

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

require_timeout() {
  n=${1:-}
  case "$n" in
    ''|*[!0-9]*) fail "$EX_INVALID" "invalid --timeout '$n' (want whole seconds)" ;;
    *) : ;;
  esac
  TIMEOUT=$n
}

require_since() {
  # Only the whole-second UTC shape these endpoints actually return is accepted.
  # Chronological string comparison in jq is valid within that shape, and only
  # within that shape - and it is the comparison the gate depends on.
  t=${1:-}
  printf '%s' "$t" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$' \
    || fail "$EX_INVALID" "invalid --since '$t' (want YYYY-MM-DDTHH:MM:SSZ from the request POST response)"
  SINCE=$t
}

require_sha40() {
  h=${1:-}
  printf '%s' "$h" | grep -qE '^[0-9a-f]{40}$' \
    || fail "$EX_INVALID" "invalid --head '$h' (want a 40-hex SHA)"
  HEAD=$h
}

require_installed_bots() {
  # The `${INSTALLED_BOTS+x}` guard, ported. `probe-bots` must have run in the
  # same shell and its result exported. An unset list is not "no bots": with it
  # merely unset the qodo grep finds nothing, the else-branch fires, and the run
  # records a clean status for a re-review that never happened.
  if [ -z "${INSTALLED_BOTS+x}" ]; then
    fail "$EX_NEGATIVE" "GATE FAIL: INSTALLED_BOTS is unset - run \`pr-review-cycle.sh probe-bots\` first and export its output"
  fi
}

# --- required-field extraction ------------------------------------------

jq_field() {
  # jq_field <json> <jq-expression> - sets JQ_VALUE from the response.
  #
  # Fails closed with EX_UPSTREAM when jq errors (malformed JSON) or the field
  # is absent or null. A missing field must never read as "nothing to see":
  # that is the shape the `null` bound in PR #71 took to hide every comment.
  src=$1
  expr=$2
  JQ_VALUE=$(printf '%s' "$src" | jq -r "$expr" 2>/dev/null)
  rc=$?
  if [ "$rc" -ne 0 ] || [ -z "$JQ_VALUE" ] || [ "$JQ_VALUE" = "null" ]; then
    fail "$EX_UPSTREAM" "required field '$expr' is missing, null, or malformed in the API response"
  fi
}

# --- gh plumbing ---------------------------------------------------------
# The only place the script talks to `gh`. No status that matters is ever read
# from a pipeline (.pi/lessons/pr-review-failures.md, docs/lessons/clo-625-l6).

reviewer_re() {
  # The single reviewer-identity seam. Today it is a substring match; CLO-650
  # tightens it to exact bot identity, and every subcommand reaches identity
  # through these two functions so that lands as one change.
  printf '%s' 'qodo-code-review|copilot-pull-request-reviewer'
}

qodo_re() {
  printf '%s' 'qodo'
}

gb_tmp_ensure() {
  if [ -z "${GH_TMP:-}" ]; then
    GH_TMP=$(mktemp -d) || fail "$EX_UPSTREAM" "could not create a scratch directory"
    trap 'rm -rf "$GH_TMP"' EXIT INT TERM
  fi
}

call_deadline() {
  # Per-request cap in seconds. PR_REVIEW_CYCLE_CALL_DEADLINE is the configured
  # maximum; when the caller has set an overall remaining budget the inner
  # deadline is clamped to it, so an inner deadline can never outlive the
  # authoritative outer one (.pi/lessons/timeout-layering.md L1). A budget that
  # has already run out does not shorten the call to zero - the poll decides
  # whether to start another tick; this only bounds a single hung request.
  d=${PR_REVIEW_CYCLE_CALL_DEADLINE:-30}
  case "$d" in ''|*[!0-9]*) d=30 ;; esac
  b=${CALL_BUDGET:-}
  if [ -n "$b" ] && [ "$b" -gt 0 ] && [ "$b" -lt "$d" ]; then
    d=$b
  fi
  printf '%s' "$d"
}

gh_api() {
  # gh_api <gh api args...> - run one API call under a per-request deadline and
  # print the response body on stdout. Returns EX_UPSTREAM, with a diagnostic on
  # stderr, when gh exits non-zero, the call outlives its deadline, or the body
  # is empty. Empty is a failure, not "no results": the fail-open paths this
  # task exists to close all start with an empty body being read as a fact.
  #
  # `gh api` has no request-timeout flag and macOS ships no `timeout` binary, so
  # the deadline is a watchdog process that signals a hung gh.
  gb_tmp_ensure
  body_file=$(mktemp "$GH_TMP/body.XXXXXX") || return "$EX_UPSTREAM"
  err_file=$(mktemp "$GH_TMP/err.XXXXXX")   || return "$EX_UPSTREAM"
  done_file=$(mktemp "$GH_TMP/done.XXXXXX")  || return "$EX_UPSTREAM"
  timed_out=$(mktemp "$GH_TMP/tout.XXXXXX")  || return "$EX_UPSTREAM"
  rm -f "$done_file" "$timed_out"
  limit=$(call_deadline)

  gh api "$@" >"$body_file" 2>"$err_file" &
  gh_pid=$!
  # The watchdog checks the done-marker before signalling, so it can never
  # target a PID that has already been reaped.
  (
    # Reset the inherited EXIT/INT/TERM cleanup trap first. A trapped signal is
    # deferred until the current foreground command finishes, so an inherited
    # trap would make this watchdog unkillable for the whole sleep and let a
    # finished call block for the full deadline anyway.
    #
    # The redirects are load-bearing: gh_api runs inside a command substitution,
    # and a background child that inherits that pipe keeps its write end open, so
    # the caller would block on EOF until this sleep finished even though the
    # call returned immediately.
    trap - EXIT INT TERM
    sleep "$limit"
    if [ ! -f "$done_file" ]; then
      : > "$timed_out"
      kill -TERM "$gh_pid" 2>/dev/null
    fi
  ) >/dev/null 2>&1 &
  watchdog_pid=$!

  wait "$gh_pid"
  rc=$?
  : > "$done_file"
  kill "$watchdog_pid" 2>/dev/null
  wait "$watchdog_pid" 2>/dev/null

  if [ -f "$timed_out" ]; then
    diag "${SUB:-?}" "gh api $* exceeded the ${limit}s per-call deadline"
    rm -f "$body_file" "$err_file" "$done_file" "$timed_out"
    return "$EX_UPSTREAM"
  fi
  if [ "$rc" -ne 0 ]; then
    diag "${SUB:-?}" "gh api $* failed (exit $rc): $(cat "$err_file")"
    rm -f "$body_file" "$err_file" "$done_file" "$timed_out"
    return "$EX_UPSTREAM"
  fi
  if [ ! -s "$body_file" ]; then
    diag "${SUB:-?}" "gh api $* returned an empty body"
    rm -f "$body_file" "$err_file" "$done_file" "$timed_out"
    return "$EX_UPSTREAM"
  fi
  cat "$body_file"
  rm -f "$body_file" "$err_file" "$done_file" "$timed_out"
  return 0
}

gh_capture() {
  # gh_capture <gh api args...> - sets GH_BODY, or aborts the script with
  # EX_UPSTREAM. POSIX sh has no nameref, so the body lands in a fixed global
  # rather than in a caller-named variable; the call-site contract is unchanged.
  GH_BODY=$(gh_api "$@")
  rc=$?
  [ "$rc" -eq 0 ] || exit "$rc"
  return 0
}

pr_lookup() {
  # Sets PR_HEAD and PR_CREATED_AT from pulls/<PR>. Both are validated here, so
  # a failed or partial lookup can never become a vacuous comparison downstream
  # (an empty head makes `contains($h)` true for any completion comment).
  gh_capture "repos/$REPO/pulls/$PR"
  jq_field "$GH_BODY" '.head.sha'
  PR_HEAD=$JQ_VALUE
  jq_field "$GH_BODY" '.created_at'
  PR_CREATED_AT=$JQ_VALUE
  printf '%s' "$PR_HEAD" | grep -qE '^[0-9a-f]{40}$' \
    || fail "$EX_UPSTREAM" "pulls/$PR returned a head.sha that is not 40-hex ('$PR_HEAD')"
  printf '%s' "$PR_CREATED_AT" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$' \
    || fail "$EX_UPSTREAM" "pulls/$PR returned a created_at that is not whole-second UTC ('$PR_CREATED_AT')"
}

poll_for_pass() {
  # poll_for_pass <head_sha> <since_iso8601> <timeout_seconds>
  #
  # Prints the detection timestamp and returns EX_OK when a reviewer pass is
  # observed on <head_sha> no earlier than <since>; returns EX_NEGATIVE on
  # timeout. Ports the skill's wait_for_bot_review, including both delivery
  # shapes:
  #
  #   - a review object on <head_sha> from a reviewer bot whose submitted_at
  #     is >= <since> (a pass that carried inline findings);
  #   - qodo only: a completion comment with created_at >= <since> whose body
  #     says "was updated up to the latest commit" and names <head_sha>
  #     (a clean pass, which submits no review object at all).
  #
  # The persistent review comment's updated_at is deliberately not a condition:
  # it bumps mid-pass and on post-merge permalink refreshes, so gating on it
  # would pass runs that never happened.
  head=$1
  since=$2
  limit=$3

  printf '%s' "$head" | grep -qE '^[0-9a-f]{40}$' \
    || fail "$EX_INVALID" "head '$head' is not a 40-hex SHA - the pulls lookup failed?"
  [ -n "$since" ] \
    || fail "$EX_INVALID" "empty since-bound - the request POST (or PR lookup) failed?"

  re=$(reviewer_re)
  qre=$(qodo_re)
  deadline=$(( $(date -u +%s) + limit ))
  first=1

  while :; do
    now=$(date -u +%s)
    # The first tick always runs, so `--timeout 0` still consults the API once
    # and reports a real negative verdict instead of skipping the check.
    if [ "$first" -eq 0 ] && [ "$now" -ge "$deadline" ]; then
      return "$EX_NEGATIVE"
    fi
    remaining=$(( deadline - now ))
    [ "$remaining" -ge 0 ] || remaining=0
    CALL_BUDGET=$remaining

    gh_capture "repos/$REPO/pulls/$PR/reviews" --paginate --slurp
    seen=$(printf '%s' "$GH_BODY" | jq -r --arg h "$head" --arg since "$since" --arg re "$re" '
      [.[][]
       | select(.commit_id == $h)
       | select(.user.login | test($re))
       | select(.submitted_at >= $since)
       | .submitted_at] | last // empty')
    if [ -n "$seen" ]; then
      printf '%s\n' "$seen"
      return "$EX_OK"
    fi

    # ?since= is a server-side prefilter on updated_at (never earlier than
    # created_at, so it cannot drop a comment the created_at gate below would
    # accept); it keeps each tick from re-downloading the whole history. The jq
    # comparison is the gate, never the query string.
    gh_capture "repos/$REPO/issues/$PR/comments?since=$since&per_page=100" --paginate --slurp
    seen=$(printf '%s' "$GH_BODY" | jq -r --arg h "$head" --arg since "$since" --arg re "$qre" '
      [.[][]
       | select(.user.login | test($re))
       | select(.created_at >= $since)
       | select(.body | test("was updated up to the latest commit"))
       | select(.body | contains($h))
       | .created_at] | last // empty')
    if [ -n "$seen" ]; then
      printf '%s\n' "$seen"
      return "$EX_OK"
    fi

    first=0
    [ "$(date -u +%s)" -lt "$deadline" ] || return "$EX_NEGATIVE"
    sleep "${PR_REVIEW_CYCLE_POLL_INTERVAL:-10}"
  done
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

  re=$(reviewer_re)
  gb_tmp_ensure

  # This PR's own comments are half the evidence: a newly installed bot has no
  # history on earlier PRs, and qodo posts its summary as an issue comment about
  # a minute before it submits the review
  # (.pi/lessons/pr-review-failures.md L1).
  gh_capture "repos/$REPO/issues/$PR/comments" --paginate --slurp
  comments=$GH_BODY

  gh_capture "repos/$REPO/pulls?state=all&per_page=10"
  pulls=$GH_BODY

  logins=$(mktemp "$GH_TMP/logins.XXXXXX") || fail "$EX_UPSTREAM" "could not create a scratch file"
  printf '%s' "$comments" \
    | jq -r --arg re "$re" '.[][] | select(.user.login | test($re)) | .user.login' >> "$logins"

  prev_file=$(mktemp "$GH_TMP/prev.XXXXXX") || fail "$EX_UPSTREAM" "could not create a scratch file"
  printf '%s' "$pulls" | jq -r '.[].number' > "$prev_file"
  while IFS= read -r prev; do
    [ -n "$prev" ] || continue
    gh_capture "repos/$REPO/pulls/$prev/reviews" --paginate --slurp
    printf '%s' "$GH_BODY" \
      | jq -r --arg re "$re" '.[][] | select(.user.login | test($re)) | .user.login' >> "$logins"
  done < "$prev_file"

  bots=$(sort -u "$logins" | jq -Rrs 'split("\n") | map(select(length > 0)) | join(",")')

  blocked=0
  if [ -n "$bots" ] && printf '%s\n' "$bots" | grep -qi qodo; then
    blocked=$(printf '%s' "$comments" | jq -r --arg re "$(qodo_re)" \
      '[.[][] | select(.user.login | test($re))
             | select(.body | contains("<!-- qodo:billing-blocked -->"))] | length')
  fi

  # Absence is explicit: `none`, never an empty line, so an unset caller
  # variable cannot be read as "no bots installed".
  if [ -z "$bots" ]; then
    printf 'none\n'
  else
    printf '%s\n' "$bots"
  fi

  if [ "$blocked" = "1" ]; then
    diag "$SUB" "qodo-code-review is billing-blocked on PR #$PR (workspace out of credits)"
    exit "$EX_BILLING"
  fi
  exit "$EX_OK"
}

cmd_wait_review() {
  REPO=""; PR=""; TIMEOUT=600
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo)    require_repo    "${2:-}"; shift 2 ;;
      --pr)      require_pr      "${2:-}"; shift 2 ;;
      --timeout) require_timeout "${2:-}"; shift 2 ;;
      -h|--help) usage; exit "$EX_OK" ;;
      *) fail "$EX_INVALID" "unexpected argument '$1'" ;;
    esac
  done
  [ -n "$REPO" ] || fail "$EX_INVALID" "--repo is required"
  [ -n "$PR" ]   || fail "$EX_INVALID" "--pr is required"

  # Head and since-bound both come from the PR itself: the first wait covers a
  # pass from PR open, so the bound is the PR's created_at.
  pr_lookup
  at=$(poll_for_pass "$PR_HEAD" "$PR_CREATED_AT" "$TIMEOUT")
  rc=$?
  [ "$rc" -eq 0 ] || exit "$rc"
  printf '%s\n' "$at"
  exit "$EX_OK"
}

cmd_request_rereview() {
  REPO=""; PR=""; HEAD=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo) require_repo   "${2:-}"; shift 2 ;;
      --pr)   require_pr     "${2:-}"; shift 2 ;;
      --head) require_sha40  "${2:-}"; shift 2 ;;
      -h|--help) usage; exit "$EX_OK" ;;
      *) fail "$EX_INVALID" "unexpected argument '$1'" ;;
    esac
  done
  [ -n "$REPO" ] || fail "$EX_INVALID" "--repo is required"
  [ -n "$PR" ]   || fail "$EX_INVALID" "--pr is required"
  require_installed_bots

  # Qodo does not re-review on push (handle_push_trigger is False), so without
  # an explicit request its findings stay pinned to the pre-fix commit. Skip the
  # request entirely when there is nothing to ask: posting into a repo with no
  # Qodo app leaves a stray comment and then fails the gate ten minutes later,
  # and when Qodo is billing-blocked the request only refreshes the notice.
  if [ "${QODO_BILLING_BLOCKED:-0}" = "1" ] \
     || ! printf '%s\n' "$INSTALLED_BOTS" | grep -qi qodo; then
    printf 'none\n'
    exit "$EX_OK"
  fi

  if [ -z "$HEAD" ]; then
    pr_lookup
    HEAD=$PR_HEAD
  fi

  # created_at comes from the POST response, never from local `date`: the poll
  # compares it against submitted_at/created_at, which come from GitHub's clock,
  # and a fast local clock would widen the window past a genuine pass.
  gh_capture "repos/$REPO/issues/$PR/comments" -X POST -f "body=$REQUEST_REREVIEW_COMMAND"
  jq_field "$GH_BODY" '.created_at'
  printf '%s\n' "$JQ_VALUE"
  exit "$EX_OK"
}

cmd_wait_rereview() {
  REPO=""; PR=""; SINCE=""; HEAD=""; TIMEOUT=600
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo)    require_repo    "${2:-}"; shift 2 ;;
      --pr)      require_pr      "${2:-}"; shift 2 ;;
      --since)   require_since   "${2:-}"; shift 2 ;;
      --head)    require_sha40   "${2:-}"; shift 2 ;;
      --timeout) require_timeout "${2:-}"; shift 2 ;;
      -h|--help) usage; exit "$EX_OK" ;;
      *) fail "$EX_INVALID" "unexpected argument '$1'" ;;
    esac
  done
  [ -n "$REPO" ]  || fail "$EX_INVALID" "--repo is required"
  [ -n "$PR" ]    || fail "$EX_INVALID" "--pr is required"
  # A missing bound would let any prior pass on the same head satisfy the poll:
  # re-running after a failed POST would report re-validation that never
  # happened. The whole point of this subcommand is the exogenous since-bound.
  [ -n "$SINCE" ] || fail "$EX_INVALID" "--since is required (the POST response's created_at)"

  if [ -z "$HEAD" ]; then
    pr_lookup
    HEAD=$PR_HEAD
  fi

  at=$(poll_for_pass "$HEAD" "$SINCE" "$TIMEOUT")
  rc=$?
  [ "$rc" -eq 0 ] || exit "$rc"
  printf '%s\n' "$at"
  exit "$EX_OK"
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
