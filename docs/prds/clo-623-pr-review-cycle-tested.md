# PRD: CLO-623 - Make pr-review-cycle shell snippets executable and tested

## Problem

The PR review gates that decide whether a lok PR may merge live as bash
snippets embedded in two markdown files: `.pi/skills/pr-review-cycle.md`
and `.claude/commands/pr/review.md`. Neither file is compiled, executed,
or tested by anything, so defects in the gate logic survive until a human
reads the prose closely. PR #71 demonstrated this concretely: seven
findings formed a chain where each fix introduced the next defect, and two
of them (`gh api --jq --arg` being invalid; `jq max` over an empty array
yielding the string `null`) would each have been caught by executing the
snippet once. Several of these gates fail **open** - they report success
from absent or stale evidence, which is the opposite of what a merge gate
should do.

## Who is affected and how today

Every lok task that reaches the `pr` phase: the agent running
`/task:orchestrate` copies these snippets into a live shell and the
workflow YAML gates (`bot_review_wait_completed`, `bot_rereview_*`,
`pre_merge_refetch_passed`) are only as sound as the copied code. A
defective gate either stalls the task for ten minutes or - worse -
silently passes a PR that the bots never re-reviewed.

## Desired outcome

- One implementation of the gate logic: a real script
  (`.pi/scripts/pr-review-cycle.sh`) with subcommands (`probe-bots`,
  `wait-review`, `request-rereview`, `wait-rereview`, `new-comments`)
  that both the skill and the `/pr:review` command call.
- `shellcheck` over the script runs in CI.
- Unit tests with recorded `gh` fixtures prove every gate fails **closed**
  on missing/stale/empty input (the cases behind PR #71 items 6 and 7).
- The markdown shrinks to prose plus script calls - one implementation,
  not two drifting copies.

## Non-goals

- Changing the gate semantics (timeouts, delivery shapes, billing-block
  handling). The script ports the behavior the prose currently specifies.
- Reworking reviewer-output handling (`REVIEW_FAILED` on empty stdout) in
  `.lok/workflows/` - that is the follow-up noted in the issue body.

## Acceptance criteria

* No executable bash remains inline in `pr-review-cycle.md` or `/pr:review`
  beyond illustrative one-liners.
* `shellcheck` passes in CI.
* Each gate has a test proving it fails closed on missing/stale input.
* The skill and the command call the same implementation.

## Context

- Linear: https://linear.app/cloud-ai/issue/CLO-623/
- Incident: https://github.com/maxkulish/lok/pull/71
