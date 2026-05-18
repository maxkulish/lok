---
name: pr-review-cycle
description: Bot-review wait, fetch, address, reply, verify, re-fetch - the 10-step PR review procedure owned by the pi `pr` phase. Enforces the 180s wait, CI/bot independence, mandatory `/gemini review` trailer, and trailer-verification gate.
---

# Skill: pr-review-cycle

Bot-review wait, fetch, address, reply, verify, re-fetch. Owned by the
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

It writes one `review_addressed` history event on success.

---

## 1 - Wait for bot reviewers to post

Bot reviewers (`gemini-code-assist`, `copilot-pull-request-reviewer`)
post inline review comments within minutes of a green CI run. Poll
rather than blocking-sleep so the agent runtime does not truncate.

**Three rules, all mandatory** (see `lessons/pr-review-failures.md`
L1 + L2):

1. **CI presence is independent of bot reviewers.** Bots are GitHub
   Apps installed at repo/org level and post regardless of
   `.github/workflows/`. "No CI configured" is NEVER a valid reason to
   skip review fetching.
2. **Minimum wait is 180s** from `pull_request.created_at`. Bots
   routinely post in the 60-180s window. An empty poll inside that
   window means nothing.
3. **Loop early-exits only when comments are found.** When the loop
   reaches max iterations with zero comments, proceed to step 2 below
   (it confirms whether bots are installed at all).

```bash
PR=<n>
REPO=maxkulish/lok
PR_CREATED_AT=$(gh api repos/${REPO}/pulls/${PR} --jq .created_at)
MIN_WAIT_UNTIL=$(date -u -j -v+180S -f "%Y-%m-%dT%H:%M:%SZ" "$PR_CREATED_AT" "+%s" 2>/dev/null \
                 || date -u -d "$PR_CREATED_AT + 180 seconds" "+%s")

for i in $(seq 1 60); do
  count=$(gh api repos/${REPO}/pulls/${PR}/comments --paginate --jq 'length' 2>/dev/null || echo 0)
  if [ "$count" -gt 0 ]; then
    echo "Found ${count} inline review comment(s) after ${i} poll(s)"
    break
  fi
  now=$(date -u +%s)
  if [ "$now" -ge "$MIN_WAIT_UNTIL" ] && [ "$i" -ge 60 ]; then
    echo "10 min elapsed with zero inline comments; proceed to step 2"
    break
  fi
  sleep 10
done
```

## 2 - Confirm bots are (or are not) installed

Only run when step 1 finished with zero inline comments. Distinguishes
"bots installed but had nothing to say" from "bots not installed for
this repo". Conflating these is the PR #4 failure mode
(`lessons/pr-review-failures.md` L1).

```bash
gh api "repos/${REPO}/pulls?state=closed&per_page=5" --jq '.[].number' \
  | while read prev_pr; do
      gh api "repos/${REPO}/pulls/${prev_pr}/reviews" \
        --jq '.[] | select(.user.login | test("gemini-code-assist|copilot-pull-request-reviewer")) | .user.login'
    done | sort -u
```

Record the result in the `review_addressed` history event with an
explicit, factual rationale.

Acceptable rationales:

- `"Bots installed (gemini-code-assist, copilot-pull-request-reviewer
  active on PR #<n-1>); waited 10 min on PR #<n>; zero inline comments
  posted this run."`
- `"Bots not installed (no gemini-code-assist or
  copilot-pull-request-reviewer reviews on last 5 closed PRs); zero
  inline comments expected."`

Unacceptable rationales (see `lessons/pr-review-failures.md` L1 + L2):

- `"No CI configured."`
- `"No CI or bot reviewers configured."`
- Any rationale logged sooner than 180s after PR creation.

## 3 - Fetch all inline comments

```bash
gh api repos/${REPO}/pulls/${PR}/comments --paginate \
  --jq '.[] | {id, path, line: .original_line, body, user: .user.login, commit_id: .original_commit_id}'

gh pr view ${PR} --json comments \
  --jq '.comments[] | {id: .databaseId, body, author: .author.login}'
```

`--paginate` is required. Omitting it silently caps results at 30 and
hides comments on large PRs.

## 4 - Categorize comments

| Reviewer | Severity signal | Priority |
|---|---|---|
| `gemini-code-assist` | `**Severity**: high/medium/low` in body | Parse; default medium |
| `copilot-pull-request-reviewer` | None | Treat as medium |
| Human | `CHANGES_REQUESTED` state | High; `COMMENTED` = medium |

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

### Mandatory trailer rule

Every author reply to ANY review comment - Gemini, Copilot, human, or
any other reviewer - MUST end with `/gemini review` on its own line.
Rationale and enforcement: `lessons/pr-review-failures.md` L3.

This is a hard precondition on every reply post. Step 8 below verifies
it after pushes and fails the gate if any reply is missing it.

### Decision per thread (reviewer-agnostic)

| Thread state | Action |
|---|---|
| Already resolved | Skip |
| Latest reviewer comment approves the fix ("looks good", "this is sound", "no further action", "LGTM") | Resolve only, no reply |
| Awaiting author response (no author reply yet) | Post reply with `/gemini review` trailer, then resolve after Gemini approves |
| Author replied but no validator re-review yet | Post `/gemini review` reply to trigger re-review |
| Declined suggestion | Post "Intentionally kept as-is: `<rationale>`" reply with `/gemini review` trailer |

Copilot does not re-review on demand the way Gemini does; routing every
reply through `/gemini review` gives every thread - Copilot's included -
a consistent validator.

**CRITICAL: one reply per thread, maximum. NEVER post a second
standalone comment to add the trigger after the fact.** Construct the
reply body completely (content + trailer) before calling
`gh api .../replies`. If the trailer is missing, fix the body and
retry; never patch with a follow-up comment.

Resolve a thread (no reply needed when validator already approved):

```bash
gh api graphql -f query='
mutation($id:ID!) {
  resolveReviewThread(input:{threadId:$id}) {
    thread { id isResolved }
  }
}' -f id="<thread_graphql_id>"
```

Reply when fix needs validator re-review:

```bash
COMMIT_SHA=$(git rev-parse --short HEAD)

gh api repos/${REPO}/pulls/${PR}/comments/<comment_id>/replies \
  -X POST -f body="Fixed in ${COMMIT_SHA}. <one-line explanation>

/gemini review"
```

Reply for declined suggestions:

```bash
gh api repos/${REPO}/pulls/${PR}/comments/<comment_id>/replies \
  -X POST -f body="Intentionally kept as-is: <rationale>.

/gemini review"
```

Record the UTC timestamp of the most recent reply push so step 8 can
scope its verification window:

```bash
REPLY_PUSH_TS=$(date -u +%Y-%m-%dT%H:%M:%SZ)
```

## 8 - Verify the trailer landed on every reply (MANDATORY)

Fetch every author reply made since `REPLY_PUSH_TS` and confirm each
ends with `/gemini review` on its own line. If any reply is missing
it, the gate fails. Do NOT patch with a follow-up comment (step 7's
"one reply per thread" rule); stop, escalate to the user, treat the
thread as unresolved.

```bash
AUTHOR=$(gh api user --jq .login)

MISSING=$(gh api repos/${REPO}/pulls/${PR}/comments --paginate \
  --jq --arg author "$AUTHOR" --arg since "$REPLY_PUSH_TS" '
    .[]
    | select(.user.login == $author)
    | select(.created_at >= $since)
    | select((.body | test("(^|\\n)/gemini review\\s*$")) | not)
    | {id, created_at, body_preview: (.body[0:120])}
  ')

if [ -n "$MISSING" ]; then
  echo "GATE FAIL: replies missing /gemini review trailer:"
  echo "$MISSING"
  exit 1
fi

echo "GATE OK: every reply since ${REPLY_PUSH_TS} ends with /gemini review"
```

The regex `(^|\n)/gemini review\s*$` requires the trailer on its own
line at the end of the body (trailing whitespace tolerated). A trailer
buried mid-body does not satisfy the gate - Gemini only triggers when
the marker is on its own line.

If verification passes, proceed to step 9. If it fails, log a
`workflow_blocked` event, surface the offending comment IDs to the
user, wait for guidance.

## 9 - Re-check for new comments

After pushing and replying, check for new unresolved threads (bots
re-review after the `/gemini review` trigger):

```bash
gh pr view ${PR} --json reviews,reviewDecision
gh api repos/${REPO}/pulls/${PR}/comments --paginate \
  --jq '.[] | select(.created_at > "<push_timestamp>") | {id, user: .user.login, body}'
```

If new comments exist in unresolved threads, return to step 4 and
repeat. Threads already resolved by validator approval can be skipped.

## 10 - Log state

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "review_addressed",
  details: "<N> threads resolved; replies posted N/N; /gemini review trailer verified on every reply (Gemini, Copilot, human alike).",
  phase_updates: { reviews_addressed: true }
})
```
