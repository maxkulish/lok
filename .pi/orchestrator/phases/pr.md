# Phase: pr

Open the pull request, run pre-flight checks, and shepherd reviews until
CI is green. Mirrors `.claude/commands/task/phases/pr.md`.

## Required exit state

```yaml
phases:
  pr:
    status: complete
    pr_url: "https://github.com/maxkulish/lok/pull/<n>"
    pr_number: <n>
    ci_passed: true
    bot_review_wait_completed: true
    bot_review_wait_completed_at: "<ISO-8601>"
    reviews_addressed: true
    pre_merge_refetch_passed: true
    pre_merge_refetch_at: "<ISO-8601>"
    merged_at: "<ISO-8601>"   # optional
    merge_commit: "<sha>"     # optional
```

History events required: `pre_flight_checks_passed`, `pr_created`, `ci_passed`, `bot_review_wait_completed`, `review_addressed`, `pre_merge_refetch_passed`.
Optional: `pr_merged`.

## Step 4.0 - Pre-flight checks (MANDATORY)

These run before opening the PR. They must all pass:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo clippy --tests
cargo test
```

The pre-merge gate is the concatenation `cargo fmt --check && cargo clippy -- -D warnings && cargo test`. lok has no Makefile.

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "pre_flight_checks_passed",
  details: "Pre-merge gate green: fmt + clippy + test all pass",
  phase_updates: { status: "in_progress" }
})
```

## Step 0.5 - Stage and commit remaining changes (MANDATORY)

Before pushing, check for any unstaged or untracked files. The implement
phase may leave post-implementation artifacts dirty (workflow YAML updates,
lessons files, review reports). These must be committed to the PR branch.

```bash
git status --short
```

If the working tree is clean, proceed directly to Step 1.

Review each dirty file:

| If files are... | Then... |
|---|---|
| Task artifacts (workflow YAML, lessons, review reports, code changes) | Stage and commit them |
| Unrelated to this task (different branch's work, uncommitted experiments) | Warn the user and stop — cross-task boundary |
| `.pi/lessons/` files | **Stage and commit them.** Lessons are cross-task memory; losing them to a `git branch -D` in the complete phase is data loss. |

```bash
git add -A
git commit -m "chore(CLO-XX): update workflow state and post-implementation artifacts"
```

⚠️ **Untracked files are especially vulnerable.** `git branch -D` in
the `complete.md` phase destroys any file that was never staged. If you
skip this step for untracked `.pi/lessons/*` or `docs/status/*` files,
you will lose them permanently. Stage them now.

## Step 1 - Push the branch

```bash
git push -u origin feat/clo-XX-<slug>
```

## Step 2 - Open the PR

```bash
gh pr create \
  --title "feat(CLO-XX): <one-line summary>" \
  --body "$(cat <<'EOF'
## Summary
<2-3 bullets describing the change>

## Plan
- docs/plans/clo-XX-<slug>.md

## Validation
- Codex: docs/reviews/clo-XX-codex-validation.md (verdict: approve)
- Gemini: docs/reviews/clo-XX-gemini-validation.md (verdict: approve)
- Pre-merge gate green locally (fmt + clippy + test)

Closes CLO-XX
EOF
)"
```

Capture the URL and number, then:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "pr_created",
  details: "PR #<n> opened: <url>",
  phase_updates: {
    pr_url: "<url>",
    pr_number: <n>
  }
})
```

Update Linear:

```
mcp__linear__save_issue(id="CLO-XX", state="In Review")
mcp__linear__save_comment(issueId="CLO-XX", body="PR #<n>: <url>")
```

## Step 3 - Wait for CI

Poll until CI completes:

```bash
gh pr checks <n> --watch
```

If CI fails, fix locally, push, repeat. Update state on each iteration:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "ci_iteration",
  details: "<what failed>; <how fixed>; pushed <sha>"
})
```

When CI is green:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "ci_passed",
  details: "All required checks passing",
  phase_updates: { ci_passed: true }
})
```

## Step 3.5 - Address all PR review comments (skill: pr-review-cycle)

The full procedure for waiting on bot reviewers, fetching comments,
categorizing them, addressing them, replying, and re-fetching
post-reply lives in
[`.pi/skills/pr-review-cycle.md`](../../skills/pr-review-cycle.md).
Run that skill in order from step 1 to step 9. Do not reinvent any
part of it inline here.

Replies carry **no `/gemini review` trailer** - Gemini Code Assist is
sunset and the marker triggers nothing. The author replies and resolves
the thread.

`qodo-code-review` (installed 2026-08-02) is the PR-side automated
validator. It does not re-review on push: its `handle_push_trigger` is
`False`, verified by posting `/config` to PR #71. After pushing fixes,
post `/agentic_review` as a PR comment and poll for a review whose
`commit_id` matches the current head SHA. Skipping that leaves Qodo's
findings pinned to the pre-fix commit.

Qodo submits as `COMMENTED`, never `CHANGES_REQUESTED`, so it cannot
block a merge on its own, and its findings are claims to verify rather
than verdicts to obey. The pre-PR validation gate in `implement.md`
Step 4 therefore remains the automated review of record; Qodo is a
second pass on top of it.

The skill cites `.pi/lessons/pr-review-failures.md` for the durable
rules behind its non-negotiables (current-head bot-review completion,
CI/bot independence). Read both before short-circuiting any step. L3 in
that file mandated the reply trailer and is superseded.

Exit state on success: the skill writes `bot_review_wait_completed`
and `review_addressed` history events with
`phase_updates: { bot_review_wait_completed: true,
bot_review_wait_completed_at: "<ISO-8601>", reviews_addressed: true }`.
If any verification step in the skill fails, the workflow goes to
`blocked` and pauses for user guidance - do NOT proceed to Step 4.

## Step 4 - Address escalated review comments

If Step 3.5 surfaces a comment that requires a design change or contradicts the
existing plan, surface the conflict in the PR thread rather than silently
complying. Options:

- Post a PR comment explaining the tension and asking for guidance.
- Link to the relevant design doc or ADR.
- Tag the user for a decision if blocking.

When all threads are resolved and `reviews_addressed: true` is set, proceed.

## Step 5 - Approval checkpoint

Auto Mode may merge once:

- `ci_passed: true`
- `bot_review_wait_completed: true`
- `reviews_addressed: true`
- `pre_merge_refetch_passed: true`
- All required reviewers approved (or no reviewers required)
- **Step 5.0 pre-merge re-fetch passes** (see below)

Otherwise wait for the user.

### Step 5.0 - Mandatory pre-merge re-fetch

Immediately before transitioning to `complete`, re-fetch inline comments
and GraphQL review threads one last time. Confirm both that no new
inline comments appeared since the last `review_addressed` event and
that no review thread remains unresolved. Bots sometimes post after the
initial wait window; merging without this check is what caused PR #4 and
PR #24 to ship with missed inline comments.

```bash
PR=<n>
REPO=maxkulish/lok
OWNER=maxkulish
NAME=lok

# Timestamp of the most recent review_addressed event in the workflow YAML.
LAST_ADDRESSED=$(yq '.history[] | select(.action == "review_addressed") | .timestamp' \
  docs/status/clo-XX-workflow.yaml | tail -1)

NEW=$(gh api repos/${REPO}/pulls/${PR}/comments --paginate --slurp \
  | jq -c --arg since "$LAST_ADDRESSED" '
    .[][] | select(.created_at > $since) | {id, user: .user.login, body_preview: (.body[0:120])}
  ')

UNRESOLVED=$(gh api graphql -f query='
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
}' -f owner="$OWNER" -f repo="$NAME" -F pr="$PR" \
  --jq '.data.repository.pullRequest.reviewThreads.nodes[]
        | select(.isResolved == false)
        | {id, path, line, isOutdated, latest_author: .comments.nodes[0].author.login, latest_body: (.comments.nodes[0].body[0:120])}')

if [ -n "$NEW" ]; then
  echo "GATE FAIL: new inline comments since ${LAST_ADDRESSED}:"
  echo "$NEW"
  # Return to 3.5.3 - do NOT transition to complete.
  exit 1
fi

if [ -n "$UNRESOLVED" ]; then
  echo "GATE FAIL: unresolved review threads remain:"
  echo "$UNRESOLVED"
  # Return to 3.5.3 - do NOT transition to complete.
  exit 1
fi

echo "GATE OK: no new inline comments and no unresolved review threads"
```

If the gate fails, return to Step 3.5.3 and address the new comments.
Log the iteration as `review_addressed` again with the updated count,
then re-run Step 5.0. Only when the gate passes may Step 6 fire.

When the gate passes, record the explicit runtime gate:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "pre_merge_refetch_passed",
  details: "Pre-merge re-fetch passed: no new inline comments and no unresolved review threads.",
  phase_updates: {
    pre_merge_refetch_passed: true,
    pre_merge_refetch_at: "<ISO-8601>",
    status: "complete"
  }
})
```

## Step 6 - Exit guard: verify clean working tree

Before transitioning, confirm the working tree has no uncommitted files
that would be lost by `complete.md`'s `git branch -D`:

```bash
git status --porcelain
```

If the tree is dirty, you MUST either:
1. Commit the files to the current branch (preferred), OR
2. Stash them (`git stash push -m "clo-XX: uncommitted artifacts before merge"`)

Do NOT transition with dirty untracked files in `.pi/lessons/` or
`docs/status/`. Those are the files most likely to be lost.

Only once `git status --porcelain` is empty:

```ts
transition_phase({
  task_id: "CLO-XX",
  from_phase: "pr",
  to_phase: "complete"
})
```

The actual merge happens in `complete.md` (squash + cleanup are coupled).
