#!/bin/sh
# Acceptance-criterion guard for CLO-623 (design decision 5).
#
# The two markdown files that carried the pr-review-cycle gates must not gain
# a gate shape back. Every gate now lives in .pi/scripts/pr-review-cycle.sh,
# where .pi/scripts/tests/pr-review-cycle.test.sh covers it, so a snippet that
# reappears in a markdown file is an untested gate - which is exactly how the
# original defects survived: one poll existed twice, drifted apart, and the
# weaker copy could not fail.
#
# This is a *shape* check, not a semantics check. It cannot tell a good poll
# from a bad one; it can tell that a poll was rebuilt in prose instead of
# called. That is the property acceptance criterion 1 asks for.
#
# Usage:
#   sh check-inline-gates.sh [FILE...]    scan FILEs; default the two below
#   sh check-inline-gates.sh --patterns   print "<ERE>@@<example>" per shape
#   sh check-inline-gates.sh --help
#
# Exit: 0 no banned shape, 1 a banned shape was found, 2 usage error.
#
# `--patterns` exists for the self-test. A test holding its own copy of the
# patterns would keep passing after the copy and the guard diverged, so the
# guard enumerates its own ban list and the test replays it. Each row carries
# an example line because writing a pattern's own spelling into a file is a
# poor probe: `\| *jq -r` does not match the literal text `\| *jq -r`.
#
# The examples are chosen to match no allowlist entry, because the self-test
# replays them with the allowlist active. An allowlist entry broad enough to
# swallow a real banned shape is a bug in the allowlist, and the self-test is
# where that shows up.

set -u

PROG=check-inline-gates

# The banned shapes, one per line as `<ERE>@@<example line>`.
#
#   wait_for_bot_review   the deleted helper; a caller that names it is
#                         rebuilding the poll it used to provide
#   --slurp               slurps every page into one array, so the jq program
#                         silently changes shape when the page count does
#   --paginate            a paginated read outside the script: an unpaginated
#                         read caps at 30 results and hides comments
#   --jq                  a projection built at the call site, invisible to
#                         every test and to the fail-closed handling
#   --arg                 as above, via variables the caller must set
#   | jq -r               raw jq output consumed as a value (docs/lessons/
#                         clo-625-l6: a pipeline is where exit status dies)
#   DEADLINE=             an inline poll deadline
#   sleep 1               an inline poll tick
#   submitted_at          the review-submission field the poll gates on
#   -f body='/agentic_review'   the inline re-review POST
BANNED='
wait_for_bot_review@@wait_for_bot_review o/r 1 yes
gh api[^|]*--slurp@@gh api repos/o/r/pulls/1/reviews --slurp \
gh api[^|]*--paginate@@gh api repos/o/r/pulls/1/commits --paginate \
gh api[^|]*--jq@@gh api repos/o/r/pulls/1 --jq .head.sha
gh api[^|]*--arg@@gh api graphql --arg h abc123 -f query=@q.graphql
\| *jq -r@@  | jq -r ".created_at"
DEADLINE=@@DEADLINE=1757500000
sleep 1([^0-9]|$)@@sleep 1
submitted_at@@  [.[] | select(.submitted_at) | .submitted_at] | last
-f body='"'"'/agentic_review'"'"'@@  -X POST -f body='"'"'/agentic_review'"'"' --created-at-flag)
'

hits=
ban_re=
allowed_re=

usage() {
  cat <<'EOF'
Usage: check-inline-gates.sh [FILE...]
       check-inline-gates.sh --patterns

Exit: 0 clean, 1 a banned inline gate shape was found, 2 usage error.
EOF
}

# --- ban list and allowlist ---------------------------------------------

# The allowlist sits beside this script, so it is resolved against the
# script's own directory rather than the caller's cwd: CI and the test runner
# invoke this from different places.
here=$(cd "$(dirname "$0")" && pwd)
ALLOWLIST="$here/inline-gate-allowlist.txt"

load_bans() {
  ban_re=$(printf '%s\n' "$BANNED" \
    | sed -e '/^$/d' -e 's/@@.*//' | paste -sd'|' -)
}

# One `<ERE> # <rationale>` per line; `#` starts the rationale, so an
# allowlist ERE may not contain `#`.
load_allowlist() {
  [ -f "$ALLOWLIST" ] || return 0
  allowed_re=$(sed -e 's/#.*//' -e 's/[[:space:]]*$//' "$ALLOWLIST" \
    | grep -v '^$' | paste -sd'|' -)
}

# --- argument handling ---------------------------------------------------

case "${1:-}" in
  -h|--help) usage; exit 0 ;;
  --patterns) printf '%s\n' "$BANNED" | sed -e '/^$/d'; exit 0 ;;
esac

# --- scan ----------------------------------------------------------------

hits=$(mktemp "${TMPDIR:-/tmp}/inline-gate-hits.XXXXXX") || exit 2
trap 'rm -f "$hits" "$hits.exempt"' EXIT INT TERM
: > "$hits"

load_bans
load_allowlist

scan_one() {
  f=$1
  if [ ! -f "$f" ]; then
    printf '%s: %s: no such file\n' "$PROG" "$f" >&2
    exit 2
  fi
  # -n so the failure points at a line number; the filename is prefixed here
  # because -H is a GNU extension and this also runs on macOS.
  grep -nE "$ban_re" "$f" | sed "s|^|$f:|" >> "$hits"
}

if [ "$#" -gt 0 ]; then
  for f in "$@"; do scan_one "$f"; done
else
  scan_one ".pi/skills/pr-review-cycle.md"
  scan_one ".claude/commands/pr/review.md"
fi

if [ ! -s "$hits" ]; then
  printf 'ok: no inline gate shapes\n'
  exit 0
fi

offending="$hits"
if [ -n "$allowed_re" ]; then
  offending="$hits.exempt"
  : > "$offending"
  exempted=0
  while IFS= read -r hit; do
    if printf '%s\n' "$hit" | grep -qE "$allowed_re"; then
      exempted=$(( exempted + 1 ))
    else
      printf '%s\n' "$hit" >> "$offending"
    fi
  done < "$hits"
  if [ "$exempted" -gt 0 ]; then
    printf '%s: %s allowlisted line(s) exempted\n' "$PROG" "$exempted" >&2
  fi
fi

if [ -s "$offending" ]; then
  printf '%s: banned inline gate shape(s):\n' "$PROG" >&2
  cat "$offending" >&2
  printf '%s: move the gate into .pi/scripts/pr-review-cycle.sh, or add a\n' "$PROG" >&2
  printf '%s: justified ERE to %s\n' "$PROG" "$ALLOWLIST" >&2
  exit 1
fi

printf 'ok: no unexempted inline gate shapes\n'
exit 0
