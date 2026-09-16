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

# $owner/$repo/$pr/$cursor are GraphQL variables, not shell ones: they must
# reach gh unexpanded.
# shellcheck disable=SC2016
REVIEW_THREADS_QUERY='query($owner:String!, $repo:String!, $pr:Int!, $cursor:String) {
  repository(owner:$owner, name:$repo) {
    pullRequest(number:$pr) {
      reviewThreads(first:100, after:$cursor) {
        pageInfo { hasNextPage endCursor }
        nodes {
          id
          isResolved
          isOutdated
          path
          line
          comments(last:1) { nodes { author { login } body createdAt } }
        }
      }
    }
  }
}'

REVIEW_THREADS_MAX_PAGES=50

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

require_bots() {
  # --bots takes probe-bots' stdout verbatim, or the literal `none`.
  #
  # This is the design's replacement for the skill's `${INSTALLED_BOTS+x}`
  # shell variable: subcommands read no caller state, so the installed-bot
  # verdict has to arrive as an argument. An unset variable could be silently
  # read as "no bots"; an empty or absent argument cannot.
  # The REST and GraphQL APIs report a GitHub App's login with a `[bot]` suffix
  # (`qodo-code-review[bot]`), while a user account has none. probe-bots prints
  # whatever the API returned and that value is passed straight back in here as
  # `--bots`, so the suffix is stripped once, on the way in. Comparing the raw
  # value against `qodo-code-review` rejects exactly the input this flag exists
  # to carry - found by running probe-bots against PR #89 on 2026-09-16, whose
  # real answer is `qodo-code-review[bot]`, after the whole suite had been green
  # against fixtures that spelled every login bare.
  b=${1:-}
  [ -n "$b" ] \
    || fail "$EX_INVALID" "invalid --bots '' (want a comma-separated login list or 'none')"
  if [ "$b" != "none" ]; then
    printf '%s' "$b" | grep -qE '^[A-Za-z0-9-]+(\[bot\])?(,[A-Za-z0-9-]+(\[bot\])?)*$' \
      || fail "$EX_INVALID" "invalid --bots '$b' (want a comma-separated login list or 'none')"
    b=$(printf '%s' "$b" | sed 's/\[bot\]//g')
  fi
  BOTS=$b
}

require_login() {
  l=${1:-}
  printf '%s' "$l" | grep -qE '^[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?$' \
    || fail "$EX_INVALID" "invalid --me '$l' (want a GitHub login)"
  ME=$l
}

split_repo() {
  OWNER=${REPO%%/*}
  NAME=${REPO##*/}
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

jq_failed() {
  # The shared diagnostic for a jq that exited non-zero on a *gate* input.
  #
  # `gh_capture` checks gh's exit status; this closes the other half of the
  # response path, because an empty result means two different things - "the API
  # said no" and "we could not read the API" - and only the first may pass a
  # gate. Left unchecked, a comments body carrying one `"user": null` (allowed
  # by GitHub's REST schema) made probe-bots print `none` with exit 0 for a
  # billing-blocked PR, and a truncated body made new-comments report a PR
  # clean.
  #
  # `fail` exits, and every call site is a plain statement - never a pipeline
  # and never a command substitution - so the exit reaches the caller. Each call
  # site therefore reads `rc=$?` from the assignment and checks it, which is
  # also why no call site pipes jq's result anywhere.
  fail "$EX_UPSTREAM" "jq exited non-zero on $1 - refusing to read an unreadable body as a negative verdict"
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
       | select((.user.login // "") | test($re))
       | select(.submitted_at >= $since)
       | .submitted_at] | last // empty')
    rc=$?
    [ "$rc" -eq 0 ] || jq_failed "the reviews response"
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
       | select((.user.login // "") | test($re))
       | select(.created_at >= $since)
       | select(.body | test("was updated up to the latest commit"))
       | select(.body | contains($h))
       | .created_at] | last // empty')
    rc=$?
    [ "$rc" -eq 0 ] || jq_failed "the issue-comments response"
    if [ -n "$seen" ]; then
      printf '%s\n' "$seen"
      return "$EX_OK"
    fi

    first=0
    [ "$(date -u +%s)" -lt "$deadline" ] || return "$EX_NEGATIVE"
    sleep "${PR_REVIEW_CYCLE_POLL_INTERVAL:-10}"
  done
}

graphql_query_review_threads() {
  # graphql_query_review_threads <cursor> - sets THREADS_JSON to one raw page.
  #
  # The single copy of the reviewThreads query. It appears verbatim three times
  # in the skill, which is exactly the drift this task exists to stop; CLO-650
  # changes the query in one place. The caller is responsible for walking
  # pageInfo, so the loop below is also the only pagination implementation.
  cur=$1
  if [ -n "$cur" ]; then
    gh_capture graphql \
      -f query="$REVIEW_THREADS_QUERY" \
      -f owner="$OWNER" -f repo="$NAME" -F pr="$PR" \
      -f cursor="$cur"
  else
    gh_capture graphql \
      -f query="$REVIEW_THREADS_QUERY" \
      -f owner="$OWNER" -f repo="$NAME" -F pr="$PR"
  fi
  THREADS_JSON=$GH_BODY
}

validate_threads_page() {
  # validate_threads_page <json> - fails closed (EX_UPSTREAM) on every response
  # that would otherwise degrade into "no unresolved threads".
  #
  # A GraphQL error body, a missing/null data envelope, or a thread node missing
  # a field the report needs all mean the same thing here: the gate does not
  # know. Reporting clean in any of those cases is the fail-open shape this task
  # exists to close.
  page=$1

  errors=$(printf '%s' "$page" | jq -r 'if (.errors // []) | length > 0 then "yes" else "no" end' 2>/dev/null)
  [ "$errors" = "no" ] \
    || fail "$EX_UPSTREAM" "GraphQL returned errors: $(printf '%s' "$page" | jq -c '.errors // .' 2>/dev/null)"

  threads=$(printf '%s' "$page" | jq -r 'if .data.repository.pullRequest.reviewThreads == null then "no" else "yes" end' 2>/dev/null)
  [ "$threads" = "yes" ] \
    || fail "$EX_UPSTREAM" "GraphQL response has no data.repository.pullRequest.reviewThreads"

  # pageInfo has to be a real boolean: a missing one would otherwise end the
  # loop after page 1 and silently truncate the report.
  next=$(printf '%s' "$page" | jq -r '.data.repository.pullRequest.reviewThreads.pageInfo.hasNextPage | if . == null then "missing" else tostring end' 2>/dev/null)
  case "$next" in
    true|false) : ;;
    *) fail "$EX_UPSTREAM" "GraphQL pageInfo.hasNextPage is missing or non-boolean (got '$next')" ;;
  esac

  bad=$(printf '%s' "$page" | jq -r '
    [ .data.repository.pullRequest.reviewThreads.nodes[]
      | select(
          (.id == null) or (.path == null) or (.isResolved == null)
          or ((.comments.nodes | length) == 0)
          or (.comments.nodes[0].author.login == null)
          or (.comments.nodes[0].body == null)
        ) ] | length' 2>/dev/null)
  [ "$bad" = "0" ] \
    || fail "$EX_UPSTREAM" "GraphQL returned malformed review-thread node(s) (count '$bad')"
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

  # `(.user.login // "")` because GitHub's REST schema allows a null user - a
  # deleted account, or a bot whose app was uninstalled. Without it `test($re)`
  # raises, and while the jq status was unchecked that error left this list
  # empty, so a billing-blocked PR came back as `none` with exit 0. Tolerating a
  # null user and checking jq are two different jobs: this one says a null user
  # is "not a bot", and a checked jq makes a malformed body exit 3.
  printf '%s' "$comments" | jq -r --arg re "$re" \
    '.[][] | select((.user.login // "") | test($re)) | .user.login' > "$logins"
  rc=$?
  [ "$rc" -eq 0 ] || jq_failed "this PR's comments response"

  prev_file=$(mktemp "$GH_TMP/prev.XXXXXX") || fail "$EX_UPSTREAM" "could not create a scratch file"
  printf '%s' "$pulls" | jq -r '.[].number' > "$prev_file"
  rc=$?
  [ "$rc" -eq 0 ] || jq_failed "the pull list"
  while IFS= read -r prev; do
    [ -n "$prev" ] || continue
    gh_capture "repos/$REPO/pulls/$prev/reviews" --paginate --slurp
    printf '%s' "$GH_BODY" \
      | jq -r --arg re "$re" '.[][] | select((.user.login // "") | test($re)) | .user.login' >> "$logins"
    rc=$?
    [ "$rc" -eq 0 ] || jq_failed "PR #$prev's reviews response"
  done < "$prev_file"

  bots=$(sort -u "$logins" | jq -Rrs 'split("\n") | map(select(length > 0)) | join(",")')
  rc=$?
  [ "$rc" -eq 0 ] || jq_failed "the login list"

  # Identity is decided by qodo_re() alone, so CLO-650's tightening stays a
  # one-line change. The literal `grep -qi qodo` that used to sit here and the
  # removed QODO_LOGIN constant were a second and third copy of the same fact.
  # The count, not a yes/no, because `-eq 1` let a PR carrying two markers -
  # which happens when Qodo posts its notice and then edits a second comment -
  # fall through to exit 0.
  blocked=0
  if [ -n "$bots" ] && printf '%s\n' "$bots" | grep -qE "$(qodo_re)"; then
    blocked=$(printf '%s' "$comments" | jq -r --arg re "$(qodo_re)" \
      '[.[][] | select((.user.login // "") | test($re))
             | select(.body | contains("<!-- qodo:billing-blocked -->"))] | length')
    rc=$?
    [ "$rc" -eq 0 ] || jq_failed "the billing-marker scan"
  fi

  # Absence is explicit: `none`, never an empty line, so an unset caller
  # variable cannot be read as "no bots installed".
  if [ -z "$bots" ]; then
    printf 'none\n'
  else
    printf '%s\n' "$bots"
  fi

  if [ "$blocked" -gt 0 ]; then
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
  REPO=""; PR=""; HEAD=""; BOTS=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo) require_repo  "${2:-}"; shift 2 ;;
      --pr)   require_pr    "${2:-}"; shift 2 ;;
      --head) require_sha40 "${2:-}"; shift 2 ;;
      --bots) require_bots  "${2:-}"; shift 2 ;;
      -h|--help) usage; exit "$EX_OK" ;;
      *) fail "$EX_INVALID" "unexpected argument '$1'" ;;
    esac
  done
  [ -n "$REPO" ] || fail "$EX_INVALID" "--repo is required"
  [ -n "$PR" ]   || fail "$EX_INVALID" "--pr is required"
  [ -n "$BOTS" ] || fail "$EX_INVALID" "--bots is required (probe-bots' stdout, verbatim)"

  # Qodo does not re-review on push (handle_push_trigger is False), so without an
  # explicit request its findings stay pinned to the pre-fix commit. Asking when
  # there is nothing to ask - no Qodo, or Qodo billing-blocked - leaves a stray
  # comment on the PR and then fails the gate ten minutes later. Whether to skip
  # is the caller's decision; the request is inapplicable, so nothing is posted
  # and the exit status says so.
  # Identity through qodo_re() only, so CLO-650 has one place to change. The
  # comparison used to be `case` equality against a QODO_LOGIN constant, which
  # also rejected this flag's own input: the API reports the app as
  # `qodo-code-review[bot]`.
  printf '%s\n' "$BOTS" | tr ',' '\n' | grep -qE "$(qodo_re)" \
    || fail "$EX_INVALID" "--bots '$BOTS' does not include a qodo login - nothing to request (nothing posted)"

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

  # No --head means the head as of now, which is the post-push head in step 8 -
  # the caller has just pushed the fixes it is asking to have re-reviewed.
  [ -n "$HEAD" ] || { pr_lookup; HEAD=$PR_HEAD; }

  at=$(poll_for_pass "$HEAD" "$SINCE" "$TIMEOUT")
  rc=$?
  [ "$rc" -eq 0 ] || exit "$rc"
  # Both halves are reported: step 9 records the head the pass covers alongside
  # the timestamp, and phases/pr.md 5.0 re-checks that head against the one
  # being merged.
  printf '%s %s\n' "$HEAD" "$at"
  exit "$EX_OK"
}

cmd_new_comments() {
  REPO=""; PR=""; SINCE=""; ME=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --repo)  require_repo  "${2:-}"; shift 2 ;;
      --pr)    require_pr    "${2:-}"; shift 2 ;;
      --since) require_since "${2:-}"; shift 2 ;;
      --me)    require_login "${2:-}"; shift 2 ;;
      -h|--help) usage; exit "$EX_OK" ;;
      *) fail "$EX_INVALID" "unexpected argument '$1'" ;;
    esac
  done
  [ -n "$REPO" ] || fail "$EX_INVALID" "--repo is required"
  [ -n "$PR" ]   || fail "$EX_INVALID" "--pr is required"

  pr_lookup
  gh_capture "repos/$REPO/pulls/$PR/comments" --paginate --slurp
  comments=$GH_BODY

  if [ -n "$SINCE" ]; then
    bound=$SINCE
  else
    if [ -z "$ME" ]; then
      gh_capture user
      jq_field "$GH_BODY" '.login'
      ME=$JQ_VALUE
    fi
    # `max` over an empty array is JSON null, which `jq -r` prints as the
    # literal string "null". Unguarded that reaches the filter below as
    # created_at > "null", and since every ISO timestamp sorts before "null"
    # lexicographically, *every* real comment is dropped and the re-check
    # reports clean while hiding all of them (the PR #71 defect). `// empty`
    # turns the null into an empty string so the fallback can fire.
    bound=$(printf '%s' "$comments" | jq -r --arg me "$ME" \
      '[.[][] | select(.user.login == $me) | .created_at] | max // empty')
    rc=$?
    [ "$rc" -eq 0 ] || jq_failed "the inline comments response"
    # No replies of our own yet - scope the window to the PR instead of
    # comparing every timestamp against "".
    [ -n "$bound" ] || bound=$PR_CREATED_AT
  fi

  diag "$SUB" "new-comment bound: $bound"

  # gh_api runs under a command substitution, so its scratch dir (and its EXIT
  # trap) live only in that subshell. The parent needs its own.
  gb_tmp_ensure
  out=$(mktemp "$GH_TMP/new.XXXXXX") || fail "$EX_UPSTREAM" "could not create a scratch file"
  # Strict `>`: the bound is the caller's own last reply, and reporting that
  # reply back as new would never terminate. One compact object per line.
  found=$(printf '%s' "$comments" | jq -c --arg since "$bound" \
    '.[][] | select(.created_at > $since) | {id, user: (.user.login // ""), body}')
  rc=$?
  [ "$rc" -eq 0 ] || jq_failed "the inline comments response"
  # An empty result is written as no file content at all, not as a blank line:
  # the `-s` test below treats a one-newline file as "findings present".
  [ -n "$found" ] && printf '%s\n' "$found" > "$out"

  if [ -s "$out" ]; then
    cat "$out"
    exit "$EX_NEGATIVE"
  fi
  exit "$EX_OK"
}

cmd_unresolved_threads() {
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

  # The PR must exist before the GraphQL query runs, so a typo cannot look like
  # an empty thread list.
  pr_lookup
  split_repo
  gb_tmp_ensure

  out=$(mktemp "$GH_TMP/threads.XXXXXX") || fail "$EX_UPSTREAM" "could not create a scratch file"
  : > "$out"

  cursor=""
  seen_cursors=""
  page_no=0
  while :; do
    page_no=$(( page_no + 1 ))
    [ "$page_no" -le "$REVIEW_THREADS_MAX_PAGES" ] \
      || fail "$EX_UPSTREAM" "reviewThreads pagination exceeded $REVIEW_THREADS_MAX_PAGES pages"

    graphql_query_review_threads "$cursor"
    validate_threads_page "$THREADS_JSON"

    # Only unresolved threads are reported; the latest comment is the one a
    # reply would land on, so it is the one whose author decides the action.
    printf '%s' "$THREADS_JSON" | jq -c '
      .data.repository.pullRequest.reviewThreads.nodes[]
      | select(.isResolved == false)
      | {id, path, line, is_outdated: .isOutdated,
         latest_author: .comments.nodes[0].author.login,
         latest_body: (.comments.nodes[0].body[0:120])}' >> "$out"

    next=$(printf '%s' "$THREADS_JSON" | jq -r '.data.repository.pullRequest.reviewThreads.pageInfo.hasNextPage')
    [ "$next" = "true" ] || break

    cursor=$(printf '%s' "$THREADS_JSON" | jq -r '.data.repository.pullRequest.reviewThreads.pageInfo.endCursor')
    if [ -z "$cursor" ] || [ "$cursor" = "null" ]; then
      fail "$EX_UPSTREAM" "pageInfo.hasNextPage is true but endCursor is empty - would truncate the report"
    fi
    case "$seen_cursors" in
      *"|$cursor|"*) fail "$EX_UPSTREAM" "reviewThreads pagination revisited cursor '$cursor'" ;;
      *) seen_cursors="$seen_cursors|$cursor|" ;;
    esac
  done

  if [ -s "$out" ]; then
    cat "$out"
    exit "$EX_NEGATIVE"
  fi
  exit "$EX_OK"
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
