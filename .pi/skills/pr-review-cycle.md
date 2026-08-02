---
name: pr-review-cycle
description: Bot-review wait, fetch, address, reply, re-fetch - the 9-step PR review procedure owned by the pi `pr` phase. Enforces current-head bot-review completion, CI/bot independence, and one-reply-per-thread. Recognizes qodo-code-review and copilot-pull-request-reviewer. No `/gemini review` trailer - Gemini Code Assist is sunset.
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

It writes `bot_review_wait_completed` and `review_addressed` history events on success.

---

## 1 - Probe for installed bots, then wait for their review

> **Gemini Code Assist is sunset.** The consumer GitHub app has ceased
> all code review activity; on this repo its last review was PR #55
> (2026-05-26). There is no `/gemini review` trigger and no automated
> validator from it. Recognized reviewers are now
> `qodo-code-review` (installed 2026-08-02, first review on PR #71) and
> `copilot-pull-request-reviewer` (not currently installed here).

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
  details: "No reviewer bots installed (no qodo-code-review or copilot-pull-request-reviewer activity on the last 10 PRs or this PR's comments; Gemini Code Assist is sunset); zero inline comments expected.",
  phase_updates: {
    bot_review_wait_completed: true,
    bot_review_wait_completed_at: "<ISO-8601>"
  }
})
```

### 1b - Poll for a current-head review

Only run when 1a found at least one installed bot.

```bash
DEADLINE=$(date -u -j -v+600S -f "%Y-%m-%dT%H:%M:%SZ" "$PR_CREATED_AT" "+%s" 2>/dev/null \
           || date -u -d "$PR_CREATED_AT + 600 seconds" "+%s")
BOT_REVIEW_SEEN=0

for i in $(seq 1 60); do
  BOT_REVIEW=$(gh api repos/${REPO}/pulls/${PR}/reviews --paginate --slurp \
    | jq -c --arg head "$HEAD_SHA" --arg re "$BOT_RE" '
      [.[][]
       | select(.commit_id == $head)
       | select(.user.login | test($re))
       | {user:.user.login,state,submitted_at,commit_id}]
      | last // empty
    ')

  if [ -n "$BOT_REVIEW" ]; then
    echo "Current-head bot review observed: ${BOT_REVIEW}"
    BOT_REVIEW_SEEN=1
    break
  fi

  now=$(date -u +%s)
  if [ "$now" -ge "$DEADLINE" ]; then
    echo "10 min elapsed with no current-head bot review; go to step 2 (gate failure)"
    break
  fi

  sleep 10
done
```

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

Only reached when 1a found installed bots and the 1b loop hit the
deadline without a current-head review. The absence case was already
settled in 1a, so there is nothing left to distinguish here: an installed
bot that did not finish is a gate failure, not a pass. Conflating the two
is the PR #4 / PR #24 failure mode
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

**Qodo findings are claims, not verdicts.** Verify each against the code
before acting; it reasons from the diff without running anything. On
PR #71 it filed three bugs, of which two were real and one rested on a
false premise about `timeout` portability. Reply with the evidence either
way - see step 7.

**Human `CHANGES_REQUESTED` remains the only externally blocking
signal.** The pre-PR validation gate in `phases/implement.md` (Codex +
Gemini-via-opencode + synthesis) is still the primary automated review
for this repo; it runs before the PR exists. Do not treat a quiet PR as
an unreviewed one - check that the gate ran.

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

### No reply trailer

Replies carry **no trailer**. `/gemini review` is deleted: Gemini Code
Assist is sunset, so the marker triggers nothing and only adds noise to
the thread. `lessons/pr-review-failures.md` L3, which mandated it, is
superseded - see that entry for what still applies.

No bot re-reviews on demand. Copilot never did, and Gemini no longer
exists to act as the universal validator. **The author is now the
closer**: state the fix, then resolve the thread yourself. What L3 was
protecting against - threads resolved without the rationale being
recorded - is now guarded by writing the reasoning into the reply
before resolving, not by a trailer.

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

Reply for declined suggestions:

```bash
gh api repos/${REPO}/pulls/${PR}/comments/<comment_id>/replies \
  -X POST -f body="Intentionally kept as-is: <rationale>."
```

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
PR #71 on 2026-08-02. Without an explicit request its findings stay
pinned to the pre-fix commit and this pass sees nothing new.

Post the request and keep the timestamp **GitHub** assigns to it:

```bash
REQUESTED_AT=$(gh api repos/${REPO}/issues/${PR}/comments \
  -X POST -f body='/agentic_review' --jq .created_at)
```

Take the timestamp from the POST response rather than from local
`date`. `submitted_at` on the review comes from GitHub's clock, so a
locally generated bound compares across two clock domains: a local clock
running fast would exclude the very re-review it is waiting for and time
the gate out. Reading `created_at` off the response keeps both sides in
GitHub's domain and costs no extra call.

Then poll for a review whose `commit_id` equals the **post-push** head
SHA **and** whose `submitted_at` is at or after `REQUESTED_AT`. Both
conditions are load-bearing:

- Do not reuse the 1b loop. Its `HEAD_SHA` predates the push and its
  `DEADLINE` is `PR_CREATED_AT + 600s`, already in the past by step 8.
- SHA alone is not proof of a fresh pass. Re-running step 8 without an
  intervening push, or running it after the `/agentic_review` post
  failed, would match the *previous* run's review on the same SHA and
  report re-validation that never happened. The timestamp is what makes
  the poll observe this run rather than any run.

```bash
NEW_HEAD=$(gh api repos/${REPO}/pulls/${PR} --jq .head.sha)
DEADLINE=$(( $(date -u +%s) + 600 ))

while :; do
  REREVIEW=$(gh api repos/${REPO}/pulls/${PR}/reviews --paginate --slurp \
    | jq -r --arg h "$NEW_HEAD" --arg since "$REQUESTED_AT" '.[][]
        | select(.commit_id == $h)
        | select(.user.login | test("qodo"))
        | select(.submitted_at >= $since)
        | .submitted_at' | tail -1)

  [ -n "$REREVIEW" ] && { echo "Re-review on ${NEW_HEAD:0:7} at ${REREVIEW}"; break; }
  [ "$(date -u +%s)" -ge "$DEADLINE" ] && { echo "GATE FAIL: no re-review on ${NEW_HEAD:0:7} since ${REQUESTED_AT}"; exit 1; }
  sleep 20
done
```

Both timestamps are ISO-8601 UTC with the same `Z` suffix, so the `>=`
string comparison in jq is a valid chronological one.

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

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "review_addressed",
  details: "<N> threads resolved; replies posted N/N; unresolved-thread re-check clean. No bot validator (Gemini Code Assist sunset); pre-PR validation gate carried automated review.",
  phase_updates: { reviews_addressed: true }
})
```
