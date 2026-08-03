---
name: pr-review-cycle
description: Bot-review wait, fetch, address, reply, re-fetch - the 9-step PR review procedure owned by the pi `pr` phase. Enforces current-head bot-review completion, CI/bot independence, and one-reply-per-thread. Recognizes qodo-code-review and copilot-pull-request-reviewer. Qodo never re-reviews on push, so every wait ends in an explicit `/agentic_review` request when the head has moved.
---

# Skill: pr-review-cycle

Bot-review wait, fetch, address, reply, re-fetch. Owned by the
`pr` phase; lifted out of `orchestrator/phases/pr.md` to keep that file
focused on phase orchestration.

Authoritative incident lessons cited inline live in
`.pi/lessons/pr-review-failures.md`. Do not duplicate that rationale
here - link to it.

This skill expects:

- A PR number `PR=<n>` and `REPO=<owner>/<repo>`.
- `ci_passed` already logged on the workflow.
- The author has push access and `gh` is authenticated as the PR
  author.

It writes `bot_review_wait_completed`, `review_addressed` and
`bot_rereview_verified` history events on success.

---

## 1 - Probe for installed bots, then wait for their review

> Recognized reviewers are `qodo-code-review` (installed 2026-08-02,
> first review on PR #71) and `copilot-pull-request-reviewer` (not
> currently installed here). The former `gemini-code-assist` app is
> sunset and reviews nothing; no trigger of any kind reaches it.

Bot reviewers post a GitHub review on the current head commit when their
pass finishes. Poll for that observable review, not merely for elapsed
time or CI status. PR #24 showed why: it was merged at +261s while the
bot posted its review and inline comment at +289s/+290s.

**Probe first, then poll.** With no bot installed, the polling loop can
only ever time out, stalling every PR for ten minutes. Run 1a and skip
straight to step 3 when it confirms absence; only run the 1b loop when a
bot is actually installed. Step 2 handles the remaining case - a bot is
installed but misses the deadline.

**Rules, all mandatory** (see `lessons/pr-review-failures.md` L1, L2,
L6):

1. **CI presence is independent of bot reviewers.** Bots are GitHub
   Apps installed at repo/org level and post regardless of
   `.github/workflows/`. "No CI configured" is NEVER a valid reason to
   skip review fetching.
2. **Current-head bot review is the primary completion signal.** If a
   bot review exists for `pull_request.head.sha`, proceed immediately
   to fetch inline comments and review threads.
3. **10 minutes is a hard timeout, not a success condition.** If bots
   are installed but no current-head bot review appears by
   `pull_request.created_at + 600s`, block for user guidance instead of
   silently marking reviews addressed.
4. **Only confirmed absence of installed bots may skip bot review.**
   Absence is confirmed by 1a below.

### 1a - Probe which bots are installed

Scan the last 10 PRs in any state, plus the current PR's own issue
comments. Both halves matter: a newly installed bot has no history on
closed PRs, and Qodo posts its summary as an issue comment about a
minute before it submits the review. Scanning only closed PRs would have
reported "not installed" for Qodo on PR #71, its first review.

```bash
PR=<n>
REPO=maxkulish/lok
PR_CREATED_AT=$(gh api repos/${REPO}/pulls/${PR} --jq .created_at)
HEAD_SHA=$(gh api repos/${REPO}/pulls/${PR} --jq .head.sha)
BOT_RE='qodo-code-review|copilot-pull-request-reviewer'

INSTALLED_BOTS=$( { gh api "repos/${REPO}/issues/${PR}/comments" --paginate --slurp \
    | jq -r --arg re "$BOT_RE" '.[][] | select(.user.login | test($re)) | .user.login'
  gh api "repos/${REPO}/pulls?state=all&per_page=10" --jq '.[].number' \
    | while read prev_pr; do
        gh api "repos/${REPO}/pulls/${prev_pr}/reviews" --paginate --slurp \
          | jq -r --arg re "$BOT_RE" '.[][] | select(.user.login | test($re)) | .user.login'
      done
  } | sort -u)

if [ -z "$INSTALLED_BOTS" ]; then
  echo "No reviewer bots installed; skipping the wait loop and going to step 3"
fi
```

If `INSTALLED_BOTS` is empty, record the wait gate with the absence
rationale below and go straight to **step 3**. Do not run 1b.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "bot_review_wait_completed",
  details: "No reviewer bots installed (no qodo-code-review or copilot-pull-request-reviewer activity on the last 10 PRs or this PR's comments); zero inline comments expected.",
  phase_updates: {
    bot_review_wait_completed: true,
    bot_review_wait_completed_at: "<ISO-8601>"
  }
})
```

### 1b - Poll for a current-head review

Only run when 1a found at least one installed bot.

Define the poll once - step 8 reuses it verbatim. It matches on the head
SHA **and** a lower bound on `submitted_at`, so it can never mistake an
older pass on the same commit for a fresh one:

```bash
# wait_for_bot_review <head_sha> <since_iso8601> [timeout_seconds]
# Echoes the review's submitted_at and returns 0; returns 1 on timeout.
wait_for_bot_review() {
  head="$1"; since="$2"; limit="${3:-600}"
  deadline=$(( $(date -u +%s) + limit ))
  while :; do
    seen=$(gh api repos/${REPO}/pulls/${PR}/reviews --paginate --slurp \
      | jq -r --arg h "$head" --arg since "$since" --arg re "$BOT_RE" '
          [.[][]
           | select(.commit_id == $h)
           | select(.user.login | test($re))
           | select(.submitted_at >= $since)
           | .submitted_at] | last // empty')
    [ -n "$seen" ] && { printf '%s\n' "$seen"; return 0; }
    [ "$(date -u +%s)" -ge "$deadline" ] && return 1
    sleep 10
  done
}
```

Both timestamps are ISO-8601 UTC with the same `Z` suffix, so the `>=`
string comparison in jq is a valid chronological one.

```bash
BOT_REVIEW_SEEN=0
BOT_REVIEW_AT=$(wait_for_bot_review "$HEAD_SHA" "$PR_CREATED_AT" 600) && BOT_REVIEW_SEEN=1
```

**If that timed out, ask for a pass before calling it a failure.** Qodo
reviews on PR open (`pr_commands`) and never on push
(`handle_push_trigger = False`). Step 3 of `phases/pr.md` waits for CI
*before* this skill runs, so any CI fix pushed there moved the head past
the commit Qodo reviewed - and the poll above would then be waiting for a
review that is never coming. One explicit request is the only exit:

```bash
if [ "$BOT_REVIEW_SEEN" -eq 0 ] && printf '%s\n' "$INSTALLED_BOTS" | grep -qi qodo; then
  REQUESTED_AT=$(gh api repos/${REPO}/issues/${PR}/comments \
    -X POST -f body='/agentic_review' --jq .created_at)
  echo "No review on ${HEAD_SHA:0:7}; requested /agentic_review at ${REQUESTED_AT}"
  BOT_REVIEW_AT=$(wait_for_bot_review "$HEAD_SHA" "$REQUESTED_AT" 600) && BOT_REVIEW_SEEN=1
fi
```

Take `REQUESTED_AT` from the POST response, never from local `date` -
`submitted_at` comes from GitHub's clock, and a fast local clock would
exclude the very review it is waiting for. Expect 3-4 minutes.

If `BOT_REVIEW_SEEN` is still 0 after the requested pass, that is a real
gate failure - go to step 2.

If `BOT_REVIEW_SEEN=1`, immediately record the wait gate, then proceed
to step 3:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "bot_review_wait_completed",
  details: "Current-head bot review observed for PR #<n> at head <sha>; fetching inline comments and review threads.",
  phase_updates: {
    bot_review_wait_completed: true,
    bot_review_wait_completed_at: "<ISO-8601>"
  }
})
```

## 2 - Block when an installed bot misses the deadline

Only reached when 1a found installed bots and 1b hit the deadline
without a current-head review **including after the explicit
`/agentic_review` request**. The absence case was already settled in 1a
and the stale-head case in 1b, so there is nothing left to distinguish
here: an installed bot that did not finish is a gate failure, not a pass.
Conflating the two is the PR #4 / PR #24 failure mode
(`lessons/pr-review-failures.md` L1, L2, L6).

`INSTALLED_BOTS` comes from 1a and `BOT_REVIEW_SEEN` from 1b. Run all
three blocks in one shell. If this block is reached in a fresh shell -
a resumed session, or a partial re-run - the defaults below make it fail
closed rather than silently pass an unset `BOT_REVIEW_SEEN` off as a
success.

```bash
BOT_REVIEW_SEEN="${BOT_REVIEW_SEEN:-0}"

if [ -z "${INSTALLED_BOTS+x}" ]; then
  echo "GATE FAIL: INSTALLED_BOTS unset - re-run 1a in this shell before evaluating the gate"
  exit 1
fi

if [ "$BOT_REVIEW_SEEN" -eq 0 ]; then
  echo "GATE FAIL: bots installed but no current-head bot review by 10 min deadline:"
  echo "$INSTALLED_BOTS"
  exit 1
fi
```

Stop and ask the user how to proceed. Do not record
`bot_review_wait_completed`.

Unacceptable rationales (see `lessons/pr-review-failures.md` L1, L2,
L6):

- `"No CI configured."`
- `"No CI or bot reviewers configured."`
- `"Waited 180 seconds; no comments."`
- `"Qodo reviewed an earlier commit."` - that is the stale pass, and 1b
  already gave it a chance to produce a fresh one.
- Any success rationale when installed bots have not produced a
  current-head review and the 10-minute timeout path was hit.

## 3 - Fetch all inline comments and review threads

```bash
gh api repos/${REPO}/pulls/${PR}/reviews --paginate \
  --jq '.[] | {id, state, submitted_at, commit_id, user: .user.login, body_preview: (.body[0:120])}'

gh api repos/${REPO}/pulls/${PR}/comments --paginate \
  --jq '.[] | {id, path, line: .original_line, body, user: .user.login, commit_id: .original_commit_id, in_reply_to_id}'

gh api graphql -f query='
query($owner:String!, $repo:String!, $pr:Int!) {
  repository(owner:$owner, name:$repo) {
    pullRequest(number:$pr) {
      reviewThreads(first:100) {
        nodes {
          id
          isResolved
          isOutdated
          path
          line
          comments(first:20) {
            nodes { databaseId createdAt author { login } body }
          }
        }
      }
    }
  }
}' -f owner=maxkulish -f repo=lok -F pr=${PR}

gh pr view ${PR} --json comments \
  --jq '.comments[] | {id: .databaseId, body, author: .author.login}'
```

`--paginate` is required. Omitting it silently caps results at 30 and
hides comments on large PRs. GraphQL thread state is required because a
comment can exist while its thread has already been resolved or marked
outdated.

## 4 - Categorize comments

| Reviewer | Severity signal | Priority |
|---|---|---|
| `qodo-code-review` | `Action required` badge | High; `Review recommended` = medium |
| `copilot-pull-request-reviewer` | None | Treat as medium |
| Human | `CHANGES_REQUESTED` state | High; `COMMENTED` = medium |

Qodo submits its review as `COMMENTED`, never `CHANGES_REQUESTED`, so
its state tells you nothing about severity - read the badge in each
inline comment body instead. It posts twice per PR: a summary issue
comment first, then the review carrying the inline findings. Only the
second one matters here.

**The badge is an image, not a `**Severity**:` line.** Severity lives in
the alt text at the top of each inline comment body:

```bash
grep -o 'alt="[^"]*"' <<<"$BODY" | head -1
```

`Action required` is high, `Review recommended` is medium. Findings also
carry category tags (`🐞 Bug`, `☼ Reliability`, `≡ Correctness`) and the
review header counts them (`🐞 Bugs (3)`, `📘 Rule violations (0)`). Each
finding ships an **Agent Prompt** block with a ready-made remediation
prompt - a useful starting point, not an instruction to follow blindly.

**Qodo findings are claims, not verdicts.** Verify each against the code
before acting; it reasons from the diff without running anything. On
PR #71 it filed three bugs, of which two were real and one rested on a
false premise about `timeout` portability. Reply with the evidence either
way - see step 7.

**Human `CHANGES_REQUESTED` remains the only externally blocking
signal.** The pre-PR validation gate in `phases/implement.md` (Codex +
synthesis) is still the primary automated review for this repo; it runs
before the PR exists. Do not treat a quiet PR as an unreviewed one -
check that the gate ran.

High-severity and `CHANGES_REQUESTED` comments are blocking. Medium /
low may be addressed or declined with rationale.

## 5 - Stale comment detection

For each inline comment, check whether the referenced code has changed:

```bash
git diff <original_commit_id>..HEAD -- <path>
```

If lines within 5 of the commented line changed, flag as `[STALE?]`
and confirm with the user before acting. Do NOT auto-skip stale
comments (`lessons/pr-review-failures.md` L5).

## 6 - Address feedback, commit, push

Group comments by file. Address all comments on a file together, then
commit:

```bash
git add <modified files>
git commit -m "$(cat <<'EOF'
fix(CLO-XX): address PR review feedback

- <file>: <change> (<reviewer>)

Resolves <N> review comments
EOF
)"
git push origin feat/clo-XX-<slug>
```

Push **before** replying so commit SHAs are live on GitHub when
reviewers read the replies.

## 7 - Reply or resolve each thread

Fetch thread state (GraphQL node IDs are required to resolve):

```bash
gh api graphql -f query='
query($owner:String!, $repo:String!, $pr:Int!) {
  repository(owner:$owner, name:$repo) {
    pullRequest(number:$pr) {
      reviewThreads(first:100) {
        nodes {
          id
          isResolved
          comments(first:20) {
            nodes { author { login } body }
          }
        }
      }
    }
  }
}' -f owner=maxkulish -f repo=lok -F pr=<n>
```

### No reply trailer, but Qodo must be addressed to answer

Replies carry **no trailer**. Nothing re-reviews on reply;
re-validation is requested once, in step 8.
`lessons/pr-review-failures.md` L3, which mandated a `/gemini review`
trailer, is superseded - see that entry for what still applies.

**The author is the closer**: state the fix, then resolve the thread
yourself. What L3 was protecting against - threads resolved without the
rationale being recorded - is now guarded by writing the reasoning into
the reply before resolving.

**Qodo only reads a reply that mentions it.** Observed on PR #71 and
consistent with Qodo's documented command model: a mention (`@qodo`, or a
bare `qodo`) is what routes a comment to the bot. A plain reply is
recorded on the thread and never read.

Mention `@qodo` only where you actually want an answer - a finding you
are declining, or a question about its reasoning. For a plain "fixed in
SHA", leave the mention off: the re-review in step 8 is what confirms the
fix, not a thread reply.

### Decision per thread (reviewer-agnostic)

| Thread state | Action |
|---|---|
| Already resolved | Skip |
| Latest reviewer comment approves the fix ("looks good", "this is sound", "no further action", "LGTM") | Resolve only, no reply |
| Awaiting author response (no author reply yet) | Post reply citing the fix commit, then resolve |
| Declined suggestion | Post "Intentionally kept as-is: `<rationale>`", then resolve |
| Human `CHANGES_REQUESTED` | Reply, but do **not** self-resolve - leave it for the human to resolve |

The last row is the one exception to author-closes: a human who
requested changes owns their own thread.

**CRITICAL: one reply per thread, maximum.** Construct the reply body
completely before calling `gh api .../replies`. Never patch a posted
reply with a second standalone comment; edit or escalate instead.

Resolve a thread (no reply needed when the reviewer already approved):

```bash
gh api graphql -f query='
mutation($id:ID!) {
  resolveReviewThread(input:{threadId:$id}) {
    thread { id isResolved }
  }
}' -f id="<thread_graphql_id>"
```

Reply citing the fix, then resolve the thread:

```bash
COMMIT_SHA=$(git rev-parse --short HEAD)

gh api repos/${REPO}/pulls/${PR}/comments/<comment_id>/replies \
  -X POST -f body="Fixed in ${COMMIT_SHA}. <one-line explanation>"
```

Reply for declined suggestions. **Declining a bot finding requires
evidence, not assertion** - paste the command output, `file:line`, or
config value that disproves the premise. Qodo reasons from the diff
without running anything, so its premises are the thing to check first:

```bash
# Human or Copilot - no mention needed.
gh api repos/${REPO}/pulls/${PR}/comments/<comment_id>/replies \
  -X POST -f body="Intentionally kept as-is: <rationale>."

# Qodo - mention it so the decline actually reaches it.
gh api repos/${REPO}/pulls/${PR}/comments/<comment_id>/replies \
  -X POST -f body="@qodo Keeping as-is. <evidence: command output / file:line / config value>."
```

The PR #71 worked example: the `timeout --kill-after` finding was
declined with pasted `timeout --version` output (GNU coreutils 9.11),
not with "works on my machine".

Record the timestamp of your most recent reply so step 8 can scope its
re-check window. Take it from GitHub, not local `date` - it is compared
against `created_at` on other comments, and mixing clock domains lets a
fast local clock hide comments that arrived just after your replies:

```bash
ME=$(gh api user --jq .login)
REPLY_PUSH_TS=$(gh api repos/${REPO}/pulls/${PR}/comments --paginate --slurp \
  | jq -r --arg me "$ME" '[.[][] | select(.user.login == $me) | .created_at] | max // empty')

# No replies of your own yet - scope the window to the PR instead of leaving
# the bound empty, which would compare every timestamp against "".
: "${REPLY_PUSH_TS:=$(gh api repos/${REPO}/pulls/${PR} --jq .created_at)}"
```

`max` over an empty array returns JSON `null`, which `jq -r` prints as
the literal string `null`. Left unguarded that lands in the step 8
filter as `created_at > "null"`, and since `"2026-…" < "null"`
lexicographically, **every** real comment is filtered out and the
re-check reports clean while hiding all of them. `// empty` turns the
null into an empty string so the `:=` default can fire.

## 8 - Re-check for new comments

**Request the re-review first.** Qodo does not re-review on push - its
`handle_push_trigger` is `False`, verified by posting `/config` to
PR #71 on 2026-08-02 and consistent with Qodo's documented default.
Without an explicit request its findings stay pinned to the pre-fix
commit and this pass sees nothing new. `/agentic_review` is the
configured command; `/review` is the legacy PR-Agent name and is not
wired up here.

**Skip this whole request-and-poll when 1a found no installed bots.**
There is nothing to ask and nothing to wait for; posting
`/agentic_review` into a repo with no Qodo app just leaves a stray
comment and then fails the gate ten minutes later. Jump to the new-
comment check at the end of this step.

Run this in the **same shell as step 1** - it needs `INSTALLED_BOTS` and
the `wait_for_bot_review` helper. The guard below is what keeps a
resumed session or a partial re-run from failing open: with
`INSTALLED_BOTS` merely unset, the `grep` finds nothing, the else-branch
fires, and the run records `bot_rereview_head_sha: "none"` - a clean
gate for a re-review that never happened.

```bash
if [ -z "${INSTALLED_BOTS+x}" ]; then
  echo "GATE FAIL: INSTALLED_BOTS unset - re-run 1a in this shell before requesting the re-review"
  exit 1
fi

BOT_REREVIEW_SHA=""
BOT_REREVIEW_AT=""

if printf '%s\n' "$INSTALLED_BOTS" | grep -qi qodo; then
  NEW_HEAD=$(gh api repos/${REPO}/pulls/${PR} --jq .head.sha)

  REQUESTED_AT=$(gh api repos/${REPO}/issues/${PR}/comments \
    -X POST -f body='/agentic_review' --jq .created_at)

  if BOT_REREVIEW_AT=$(wait_for_bot_review "$NEW_HEAD" "$REQUESTED_AT" 600); then
    BOT_REREVIEW_SHA="$NEW_HEAD"
    echo "Re-review on ${NEW_HEAD:0:7} at ${BOT_REREVIEW_AT}"
  else
    echo "GATE FAIL: no re-review on ${NEW_HEAD:0:7} since ${REQUESTED_AT}"
    exit 1
  fi
else
  BOT_REREVIEW_SHA="none"
  BOT_REREVIEW_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)
fi
```

`wait_for_bot_review` is the helper defined in step 1b. Both of its
conditions are load-bearing here:

- The SHA must be the **post-push** head. A review whose `commit_id` is
  the old SHA is the stale pass.
- SHA alone is not proof of a fresh pass. Re-running step 8 without an
  intervening push, or running it after the `/agentic_review` post
  failed, would match the *previous* run's review on the same SHA and
  report re-validation that never happened. `REQUESTED_AT` is what makes
  the poll observe this run rather than any run - and it comes from the
  POST response, not local `date`, so both sides stay in GitHub's clock
  domain.

Carry `BOT_REREVIEW_SHA` and `BOT_REREVIEW_AT` into step 9; they are
what the workflow YAML records and what the pre-merge gate in
`phases/pr.md` §5.0 re-checks against the head being merged.

Observed latency on PR #71 was 3-4 minutes per pass.

Three behaviors to expect, all observed on PR #71:

- Qodo **edits its existing "Code Review by Qodo" issue comment in
  place** rather than posting a new one. Watching for a new comment id
  will miss the re-review; compare `commit_id` on the review object, or
  the comment's `updated_at`.
- It posts a transient "Qodo is busy working" comment and then
  **deletes** it. A comment id that 404s on fetch is normal, not an
  error.
- The edited comment passes through **intermediate states**. During the
  third pass on PR #71 it briefly read `Bugs (0)` before settling on
  `Bugs (1)`. Never read a count while a "busy working" comment is
  present; wait for it to disappear, then read.

New findings arrive as new inline comments; the superseded ones remain
attached to the old commit. Then check for new unresolved threads,
including any human reviewer's response:

```bash
gh pr view ${PR} --json reviews,reviewDecision

gh api repos/${REPO}/pulls/${PR}/comments --paginate --slurp \
  | jq -r --arg since "$REPLY_PUSH_TS" \
    '.[][] | select(.created_at > $since) | {id, user: .user.login, body}'

gh api graphql -f query='
query($owner:String!, $repo:String!, $pr:Int!) {
  repository(owner:$owner, name:$repo) {
    pullRequest(number:$pr) {
      reviewThreads(first:100) {
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
}' -f owner=maxkulish -f repo=lok -F pr=${PR} \
  --jq '.data.repository.pullRequest.reviewThreads.nodes[]
        | select(.isResolved == false)
        | {id, path, line, isOutdated, latest_author: .comments.nodes[0].author.login, latest_body: (.comments.nodes[0].body[0:120])}'
```

If new comments or unresolved threads exist, return to step 4 and
repeat. Threads already resolved can be skipped.

## 9 - Log state

Two events. The first records what was addressed, the second records the
re-validation - keep them separate so a run that addressed comments but
never got a fresh pass cannot look like a complete one.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "review_addressed",
  details: "<N> threads resolved (<n> qodo, <m> human); replies posted N/N; <k> declined with evidence; unresolved-thread re-check clean.",
  phase_updates: { reviews_addressed: true }
})

update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "bot_rereview_verified",
  details: "qodo-code-review re-reviewed <BOT_REREVIEW_SHA> at <BOT_REREVIEW_AT> after /agentic_review; <j> new findings.",
  phase_updates: {
    bot_rereview_head_sha: "<BOT_REREVIEW_SHA>",
    bot_rereview_at: "<BOT_REREVIEW_AT>"
  }
})
```

Write what actually happened. `details` is the record a later reader
trusts: if the re-review produced new findings you looped back to step 4
for, say so; if 1a confirmed no bots are installed, record
`bot_rereview_head_sha: "none"` and give the absence rationale rather
than implying a pass that never ran.
