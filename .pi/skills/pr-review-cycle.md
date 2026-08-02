---
name: pr-review-cycle
description: Bot-review wait, fetch, address, reply, re-fetch - the 9-step PR review procedure owned by the pi `pr` phase. Enforces current-head bot-review completion, CI/bot independence, and one-reply-per-thread. No `/gemini review` trailer - Gemini Code Assist is sunset.
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

## 1 - Wait for bot reviewers deterministically

> **Gemini Code Assist is sunset.** The consumer GitHub app has ceased
> all code review activity; on this repo its last review was PR #55
> (2026-05-26) and no bot has reviewed since. There is no `/gemini review`
> trigger and no automated validator. `copilot-pull-request-reviewer`
> remains the only bot this skill recognizes, and it is not currently
> installed here - so step 2's absence path is the expected route.

Bot reviewers (currently only `copilot-pull-request-reviewer`) post a
GitHub review on the current head commit when their pass finishes. Poll
for that observable review, not merely for elapsed time or CI status.
PR #24 showed why: it was merged at +261s while the bot posted its
review and inline comment at +289s/+290s.

**Check installation before polling.** With no bot installed, the
10-minute loop below can only ever time out, stalling every PR for ten
minutes. Run the step-2 installation probe first and skip straight to
step 3 when it confirms absence.

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
   Absence is confirmed by inspecting recent closed PRs for bot reviews.

```bash
PR=<n>
REPO=maxkulish/lok
PR_CREATED_AT=$(gh api repos/${REPO}/pulls/${PR} --jq .created_at)
HEAD_SHA=$(gh api repos/${REPO}/pulls/${PR} --jq .head.sha)
BOT_RE='copilot-pull-request-reviewer'
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
    echo "10 min elapsed with no current-head bot review; proceed to step 2"
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

## 2 - Confirm bots are absent or block on timeout

Only run when step 1 timed out without a current-head bot review.
Distinguish "bots are not installed" from "bots are installed but did
not finish in time". Conflating these is the PR #4 / PR #24 failure
mode (`lessons/pr-review-failures.md` L1, L2, L6).

```bash
INSTALLED_BOTS=$(gh api "repos/${REPO}/pulls?state=closed&per_page=5" --jq '.[].number' \
  | while read prev_pr; do
      gh api "repos/${REPO}/pulls/${prev_pr}/reviews" --paginate --slurp \
        | jq -r --arg re "$BOT_RE" '.[][] | select(.user.login | test($re)) | .user.login'
    done | sort -u)

if [ -n "$INSTALLED_BOTS" ]; then
  echo "GATE FAIL: bots installed but no current-head bot review by 10 min deadline:"
  echo "$INSTALLED_BOTS"
  exit 1
fi
```

If bots are confirmed absent, record the wait gate and continue:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "bot_review_wait_completed",
  details: "Bots not installed (no copilot-pull-request-reviewer reviews on last 5 closed PRs; Gemini Code Assist is sunset); zero inline comments expected.",
  phase_updates: {
    bot_review_wait_completed: true,
    bot_review_wait_completed_at: "<ISO-8601>"
  }
})
```

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
| `copilot-pull-request-reviewer` | None | Treat as medium |
| Human | `CHANGES_REQUESTED` state | High; `COMMENTED` = medium |

With no bot validator left, **human `CHANGES_REQUESTED` is the only
blocking signal that arrives from outside**. The pre-PR validation gate
in `phases/implement.md` (Codex + Gemini-via-opencode + synthesis) is now
the primary automated review for this repo; it runs before the PR exists.
Do not treat a quiet PR as an unreviewed one - check that the gate ran.

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

Record the UTC timestamp of the most recent reply push so step 8 can
scope its re-check window:

```bash
REPLY_PUSH_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)
```

## 8 - Re-check for new comments

After pushing and replying, check for new unresolved threads. Nothing
re-reviews on demand any more, so this pass exists to catch a human
reviewer's response and any thread left unresolved - not to wait on a
bot re-run:

```bash
gh pr view ${PR} --json reviews,reviewDecision

gh api repos/${REPO}/pulls/${PR}/comments --paginate \
  --jq '.[] | select(.created_at > "<push_timestamp>") | {id, user: .user.login, body}'

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
