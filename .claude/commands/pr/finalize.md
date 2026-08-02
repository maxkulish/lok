# /pr:finalize - Post-Merge Cleanup and Task Completion

**Purpose**: Handle post-merge cleanup after a PR is merged. Updates aggregation files, Linear task status, and marks the task as complete. Supports both regular branches and git worktrees.

`main` is protected and takes no direct pushes, so these updates land through a second, docs-only pull request from `docs/clo-XX-finalize`. Opening that PR is the normal path, not a fallback.

**Usage**:
- `/pr:finalize CLO-XX` - Finalize specific task after merge
- `/pr:finalize` - Interactive mode

---

## When to Use

This command should be run:
1. After PR is merged to main
2. To complete the task lifecycle
3. To update project documentation

---

## Command Execution Instructions

### Step 1: Extract Task Number

1. **Get task number** from argument or detect from context
2. **If not provided**: Ask user or check workflow state

### Step 2: Detect Git Worktree Mode

Check if we're running in a git worktree:

```bash
# Check current directory name for worktree pattern (repo--branch)
basename "$PWD"

# List all worktrees to confirm
git worktree list
```

**Worktree Detection Logic**:
- If directory name matches pattern `*--*` (e.g., `lok--feat-clo-52-state-machine`), we're in a worktree
- Extract main repo name: everything before `--` (e.g., `lok`)
- Main repo path: `../<main-repo-name>` (e.g., `../lok`)

**Set variables for later steps**:
```bash
# Example for worktree: lok--feat-clo-52-state-machine
CURRENT_DIR=$(basename "$PWD")
if [[ "$CURRENT_DIR" == *"--"* ]]; then
    IS_WORKTREE=true
    MAIN_REPO_NAME="${CURRENT_DIR%%--*}"
    MAIN_REPO_PATH="../$MAIN_REPO_NAME"
else
    IS_WORKTREE=false
    MAIN_REPO_PATH="."
fi
```

Display:
```
Git Mode: [Worktree / Regular Branch]
Main Repo: [path]
Current Branch: [branch name]
```

### Step 3: Verify PR is Merged

```bash
gh pr list --head "feat/clo-XX-description" --json number,state,mergedAt
```

**If PR not merged**:
```
PR #[number] is not yet merged.

State: [Open/Closed]

Options:
1. [merge] - Merge the PR now
2. [wait] - Exit and wait for merge
3. [force] - Continue anyway (cleanup without merge)

Your choice:
```

**If PR is merged**: Continue

### Step 4: Create the Finalize Branch

`main` is protected. Ruleset `20153405` on `refs/heads/main` is `enforcement=active` with
`required_status_checks: ["CI Gate"]` and **no** `bypass_actors`, so nobody — including the
repository owner — can push to it. The aggregation updates reach `main` through a pull request like
any other change. That is the normal path here, not an error path; there is no direct-push fallback.

Set the branch name once and reuse it:

```bash
FINALIZE_BRANCH="docs/clo-XX-finalize"
```

#### If Worktree Mode:

```bash
# Go to main repo folder — all git work happens there, the worktree is untouched.
# Bail if the cd fails: continuing would branch and commit inside the worktree
# instead, which is Case 5.
cd "$MAIN_REPO_PATH" || { echo "MAIN_REPO_NOT_FOUND"; exit 1; }

# Refresh main (includes the merged PR)
git checkout main
git pull origin main
```

Display:
```
Working in main repo: [path]
Pulled latest changes including merged PR.
```

**IMPORTANT**: All subsequent file operations (Steps 6-9) happen in the main repo folder.

#### If Regular Branch Mode:

```bash
git checkout main
git pull origin main
```

Display:
```
Switched to main branch.
Pulled latest changes including merged PR.
```

#### Both modes: branch off the refreshed main

```bash
# Local branch from an aborted run in this clone
if git show-ref --verify --quiet "refs/heads/$FINALIZE_BRANCH"; then
    echo "BRANCH_EXISTS_LOCAL — stopping. Resolve via Case 7 before continuing."
    exit 1
fi

# Remote branch from an aborted run elsewhere — another worktree, another machine,
# or this one after the local branch was deleted. Creating a fresh local branch here
# would push into a remote that already has commits, and `git push -u` would be
# rejected non-fast-forward.
#
# Three outcomes, not two. `--exit-code` gives 0 found / 2 absent, and anything else
# (128 for auth, DNS or network trouble) means the question went unanswered. Treating
# an unanswered check as "absent" is a fail-open: it would branch, edit, commit, and
# only discover the problem at push time.
git ls-remote --exit-code --heads origin "$FINALIZE_BRANCH" >/dev/null 2>&1
LS_REMOTE_RC=$?
case "$LS_REMOTE_RC" in
    0) echo "BRANCH_EXISTS_REMOTE — stopping. Resolve via Case 7 before continuing."; exit 1 ;;
    2) : ;;   # absent, which is what we want
    *) echo "REMOTE_CHECK_FAILED — git ls-remote exit $LS_REMOTE_RC; cannot verify. Stopping."; exit 1 ;;
esac

git checkout -b "$FINALIZE_BRANCH" || { echo "BRANCH_CREATE_FAILED"; exit 1; }
git rev-parse --abbrev-ref HEAD   # confirm: $FINALIZE_BRANCH
```

Both stops are deliberate and must stay non-zero. If the branch already exists, an earlier run
aborted partway; see **Case 7** — prompt the user, do not silently reuse or delete. Merely printing
the marker and continuing would leave `HEAD` on `main` while Step 9 assumes the finalize branch, so
the aggregation edits would be committed to the wrong branch and the push would be rejected by the
ruleset all over again.

Local and remote are checked separately because they fail differently. A local branch means this
clone aborted; a remote-only branch means the abort happened somewhere else and this clone knows
nothing about the commits already on it.

On the Case 7 `reuse` path, check the branch out (`git checkout "$FINALIZE_BRANCH"`) **before** any
file edits in Steps 6-8.

### Step 5: Aggregation Files (Linear update moved to Step 9g)

Linear is **not** updated here. It used to be, and that ordering is what made the original failure
so damaging: the task was flipped to Done at this point and the push to `main` was rejected two
steps later, leaving Linear claiming completion that the repository had no record of. The Linear
transition now happens in Step 9g, after the PR has actually merged.

### Step 6: Update Aggregation Files

**IMPORTANT**: In worktree mode, all file paths are relative to the main repo folder (set in Step 4).

#### 6.1: Update PROJECT.md

Read `docs/PROJECT.md` (in main repo) and update:

**Move task from Active Work to Recently Completed**:

Before:
```markdown
## Active Work
| Task | Status | Phase | Blocked By |
|------|--------|-------|------------|
| [CLO-XX](link) | In Progress | Phase 1 | - |
```

After:
```markdown
## Recently Completed
| Task | Completed | Summary |
|------|-----------|---------|
| [CLO-XX](link) | [Today's date] | [Brief summary] |
```

#### 6.2: Update ROADMAP.md

Read `docs/ROADMAP.md` and update:

**Change task status to Done**:

Before:
```markdown
| CLO-XX | [Title] | In Progress | CLO-YY |
```

After:
```markdown
| CLO-XX | [Title] | Done | CLO-YY |
```

**Update phase completion count** in Summary table:

```markdown
| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 1 | 4 | 3 | In Progress |  <- Update count
```

#### 6.3: Update DEPENDENCIES.md

Read `docs/DEPENDENCIES.md` and update:

**Remove from Current Blockers** (if CLO-XX was blocking anything):

Before:
```markdown
## Current Blockers
| Blocked Task | Blocked By | Blocker Status | Notes |
|--------------|------------|----------------|-------|
| CLO-ZZ | CLO-XX | In Progress | ... |
```

After:
```markdown
## Unblocked & Ready
| Task | Dependencies Satisfied | Ready Since |
|------|------------------------|-------------|
| CLO-ZZ | CLO-XX complete | [Today's date] |
```

### Step 7: Update Status File

**In worktree mode**: These files are in the main repo folder.

Update `docs/status/clo-XX-[description].md`:

```markdown
**Last Updated**: [Current Date/Time]

## Current Status: Complete

**Overall Progress**: 100% (X/X tasks)
**Completed**: [Date/Time]
**PR Merged**: [Date/Time]

---

## Final Summary

**Implementation**: Successfully completed all tasks.

**Modules**:
- [List of modules created/modified]

**Total Commits**: [count]
**PR**: #[number] (merged)
```

### Step 8: Update Workflow State (if exists)

**In worktree mode**: This file is in the main repo folder.

Update `docs/status/clo-XX-workflow.yaml`:

```yaml
workflow:
  current_phase: complete
  status: complete

phases:
  complete:
    status: complete
    aggregation_files_updated: true
    merged_at: [ISO timestamp]

history:
  - timestamp: [ISO timestamp]
    action: workflow_complete
    phase: complete
    details: "Task CLO-XX fully completed"
```

The finalize PR's own URL is written here too, but not yet — it does not exist until Step 9b has
created it. Step 9c comes back and fills it in.

### Step 9: Commit, Open the PR, Merge It

Steps 6, 7 and 8 edited files only. Everything reaches `main` here, as **one** pull request. The
old command committed twice and pushed to `main` three times; all three of those pushes were
rejected by the ruleset, and one PR carrying both the aggregation files and `docs/status/` replaces
them.

#### 9a: Commit the aggregation and status files, push the branch

**IMPORTANT**: Ensure you're in the main repo folder (Step 4 put you there) and on
`$FINALIZE_BRANCH`, not `main`.

```bash
pwd                                # main repo path

# Fail fast if Step 4 left us somewhere else — committing the aggregation files
# onto `main` is the exact failure this whole command exists to remove.
if [ "$(git rev-parse --abbrev-ref HEAD)" != "$FINALIZE_BRANCH" ]; then
    echo "WRONG_BRANCH: on $(git rev-parse --abbrev-ref HEAD), expected $FINALIZE_BRANCH"
    exit 1
fi

# Stage each path that exists, one at a time. Two traps here, both load-bearing:
#   - `git add a b c` aborts with exit 128 having staged NOTHING when any single
#     pathspec is missing, and the emptiness check below would then read that as
#     "already current" — skipping the PR while still marking the task Done.
#   - collecting the paths into one string and expanding it unquoted relies on word
#     splitting, which bash does and zsh does not. Per-path quoting works in both.
FOUND=0
STAGE_ERR=0
for p in docs/PROJECT.md docs/ROADMAP.md docs/DEPENDENCIES.md docs/status; do
    [ -e "$p" ] || continue
    FOUND=1
    git add -- "$p" || STAGE_ERR=1
done

if [ "$FOUND" -eq 0 ]; then
    echo "NO_AGGREGATION_FILES"      # Case 1
    exit 1
elif [ "$STAGE_ERR" -ne 0 ]; then
    echo "STAGING_FAILED"            # do NOT read this as "nothing to commit"
    exit 1
# Nothing staged means the files were already current — see Case 8
elif git diff --cached --quiet; then
    echo "NOTHING_TO_COMMIT"         # the one non-error early exit; route to Case 8
else
    git commit -m "$(cat <<'EOF'
docs(CLO-XX): update aggregation and status files for completed task

- PROJECT.md: Moved CLO-XX to Recently Completed
- ROADMAP.md: Updated task status to Done
- DEPENDENCIES.md: Updated blockers and unblocked tasks
- docs/status/: Final summary and workflow marked complete
EOF
)" || { echo "COMMIT_FAILED"; exit 1; }
    git push -u origin "$FINALIZE_BRANCH" || { echo "PUSH_FAILED"; exit 1; }
fi
```

Every error branch here exits non-zero. These markers are load-bearing, not advisory: falling
through to Step 9b after a failed stage or push would run `gh pr create` against a branch with no
commit on it, which is the partial-finalization this rewrite exists to prevent. `NOTHING_TO_COMMIT`
is the only early exit that is **not** an error.

If `NOTHING_TO_COMMIT`, skip to **Case 8**. The task is still finished — it just needs no PR.

#### 9b: Open the PR (or adopt one that is already open)

An aborted earlier run can leave a PR open on this branch, and `gh pr create` fails outright when
one exists. Look first:

```bash
PR_NUMBER=$(gh pr list --head "$FINALIZE_BRANCH" --state open --json number --jq '.[0].number // empty')

if [ -z "$PR_NUMBER" ]; then
    PR_URL=$(gh pr create \
      --base main \
      --head "$FINALIZE_BRANCH" \
      --title "docs(CLO-XX): finalize task — aggregation and status updates" \
      --body "$(cat <<'EOF'
Post-merge finalization for CLO-XX, opened by `/pr:finalize`.

`main` is protected by ruleset 20153405 (`CI Gate` required, no bypass actors), so the aggregation
and status updates arrive by pull request rather than a direct push.

- `docs/PROJECT.md` — CLO-XX moved to Recently Completed
- `docs/ROADMAP.md` — status set to Done, phase count updated
- `docs/DEPENDENCIES.md` — blockers cleared, dependents unblocked
- `docs/status/` — final summary, workflow marked complete

Docs only. No source changes.
EOF
)")
    PR_NUMBER=$(echo "$PR_URL" | grep -oE '[0-9]+$')
else
    PR_URL=$(gh pr view "$PR_NUMBER" --json url --jq '.url')
    echo "Reusing already-open PR #$PR_NUMBER"
fi

echo "Finalize PR: $PR_URL (#$PR_NUMBER)"
```

#### 9c: Record the PR in the workflow YAML, then push

The PR number only exists now, which is why this is a second commit rather than part of 9a.
Amending 9a instead would need a force-push and buy nothing.

Add to `docs/status/clo-XX-workflow.yaml`:

```yaml
phases:
  complete:
    finalize_pr_url: [PR_URL]
    finalize_pr_number: [PR_NUMBER]
```

```bash
git add docs/status/
git commit -m "docs(CLO-XX): record finalize PR #$PR_NUMBER in workflow state"
git push
```

Waiting for CI now, after this second push, means waiting once instead of twice.

#### 9d: Wait for `CI Gate`

`allow_auto_merge` is `false` on this repository, so `gh pr merge --auto` is unavailable and the
command polls for itself. Bound the wait — a bare `--watch` against a stuck runner or an Actions
outage is a hang, not a wait:

```bash
# `timeout` is GNU coreutils — present as `timeout` or `gtimeout` on a brew-equipped
# macOS, absent on a stock one. Fall back to an unbounded watch rather than failing.
TIMEOUT_BIN=$(command -v timeout || command -v gtimeout || true)

if [ -n "$TIMEOUT_BIN" ]; then
    "$TIMEOUT_BIN" 1200 gh pr checks "$PR_NUMBER" --watch --fail-fast --interval 30 \
      && echo "CI_PASSED" || echo "CI_NOT_PASSED"
else
    echo "WARNING: no timeout(1); watching unbounded. Ctrl-C and re-run if it hangs."
    gh pr checks "$PR_NUMBER" --watch --fail-fast --interval 30 \
      && echo "CI_PASSED" || echo "CI_NOT_PASSED"
fi
```

`--fail-fast` leaves watch mode the moment a check fails, so a known-bad run is not waited out for
the full 20 minutes.

`ci.yml` carries no `paths:` filter, so this docs-only PR does run the full Rust matrix and `CI Gate`
does report. That is what makes the required check satisfiable at all — see Case 9 before ever
"optimizing" it away.

**If `CI_NOT_PASSED`**: stop. Do not merge, and do not update Linear — see **Case 9**.

#### 9e: Merge

```bash
gh pr merge "$PR_NUMBER" --squash --delete-branch
```

`--delete-branch` is required because `delete_branch_on_merge` is `false` on this repository; without
it the finalize branches accumulate.

Never pass `--admin`, and never force-push. The ruleset shape is intended, not an obstacle to route
around.

#### 9f: Return to `main`

The merge just deleted `$FINALIZE_BRANCH` on the remote, and in worktree mode this repo is the
user's main checkout — do not leave it sitting on a dead branch:

```bash
git checkout main
git pull origin main
git branch -d "$FINALIZE_BRANCH" 2>/dev/null || true
```

#### 9g: Update Linear Task Status

Only now, with the finalize PR merged, is the task genuinely complete.

```
mcp__linear-server__update_issue(
  id="CLO-XX",
  state="Done"
)
```

Post final comment:

```
mcp__linear-server__create_comment(
  issueId="CLO-XX",
  body="## Task Complete

**Status**: Done
**PR**: #[number] (merged)
**Finalize PR**: #[PR_NUMBER] (merged)
**Merged At**: [timestamp]

**Summary**:
[Brief summary of what was implemented]

**Documents**:
- Design: `docs/design-docs/clo-XX-[description].md`
- Plan: `docs/plans/clo-XX-[description].md`
- Status: `docs/status/clo-XX-[description].md`

This task is now complete."
)
```

### Step 10: Display Completion Summary

Render the canonical summary defined in `.claude/templates/completion-summary.md`.

- Load `docs/status/clo-XX-workflow.yaml`.
- Resolve every placeholder using the field-mapping table in the template (`Source of Data` section).
- Apply the phase-skip rules for the current `task_type`.
- Print the rendered block exactly as specified — preserve box-drawing characters, indentation, emojis, and separator widths.

The footer line `Status: ✅ DONE` is the single authoritative completion signal — emit it only when `workflow.current_phase == complete` AND `workflow.status == complete`.

#### Worktree Mode addendum

After the canonical summary, append exactly this block on a new line (only in worktree mode):

```
NEXT STEPS:
1. Exit Claude Code
2. Run `gd` to delete this worktree and branch
3. You'll be returned to the main repo folder
```

In regular branch mode, do not append anything after the canonical summary.

---

## Aggregation File Templates

### PROJECT.md Recently Completed Entry

```markdown
| [CLO-XX](https://linear.app/cloud-ai/issue/CLO-XX) | [YYYY-MM-DD] | [One line summary] |
```

### ROADMAP.md Updated Entry

```markdown
| [CLO-XX](https://linear.app/cloud-ai/issue/CLO-XX) | [Title] | Done | [Dependencies] |
```

### DEPENDENCIES.md Unblocked Entry

```markdown
| CLO-ZZ | CLO-XX complete | [YYYY-MM-DD] |
```

---

## Special Cases

### Case 1: Aggregation Files Don't Exist

```
WARNING: Aggregation files not found

Expected files:
- docs/PROJECT.md [missing]
- docs/ROADMAP.md [missing]
- docs/DEPENDENCIES.md [found]

Options:
1. [create] - Create missing files from templates
2. [skip] - Skip aggregation updates
3. [cancel] - Cancel finalization

Your choice:
```

### Case 2: Task Not in Aggregation Files

```
NOTE: CLO-XX not found in ROADMAP.md

The task may not have been added to project tracking.

Options:
1. [add] - Add task to files now
2. [skip] - Skip this file
3. [manual] - I'll update manually

Your choice:
```

### Case 3: Merge Conflicts in Aggregation Files

```
WARNING: Aggregation files have conflicts

docs/PROJECT.md has conflicts after git pull.

Options:
1. [resolve] - Attempt auto-resolve
2. [manual] - Exit and resolve manually
3. [skip] - Skip this file

Your choice:
```

### Case 4: Multiple Tasks Were Blocking

When CLO-XX was blocking multiple tasks:

```
UNBLOCKED TASKS

CLO-XX completion unblocks:
- CLO-12: Add authentication
- CLO-15: Implement caching

All will be moved to "Unblocked & Ready" in DEPENDENCIES.md.

Proceed? (yes/edit/cancel)
```

### Case 5: Worktree Main Repo Not Found

```
WARNING: Main repo folder not found

Current directory: lok--feat-clo-52-state-machine
Expected main repo: ../lok

Options:
1. [path] - Specify main repo path manually
2. [skip] - Skip aggregation updates (update manually later)
3. [cancel] - Cancel finalization

Your choice:
```

### Case 6: Worktree Main Repo Has Uncommitted Changes

This one matters more than it used to. Step 4 now creates a branch rather than committing in place,
and `git checkout -b` carries uncommitted changes onto the new branch. Resolve it before Step 4.


```
WARNING: Main repo has uncommitted changes

Changes in ../lok:
- docs/PROJECT.md (modified)
- src/main.rs (modified)

Options:
1. [stash] - Stash changes, proceed, then unstash
2. [skip] - Skip aggregation updates
3. [cancel] - Cancel and resolve manually

Your choice:
```

### Case 7: Finalize Branch Already Exists

An earlier run aborted between Step 4 and Step 9.

```
NOTE: Finalize branch already exists

Branch:   docs/clo-XX-finalize
Where:    [local only / remote only / both]
Open PR:  [#number / none]

Options:
1. [reuse]  - Check it out and continue; an open PR is adopted rather than recreated
2. [delete] - Delete the branch and start the finalization fresh
3. [cancel] - Stop and inspect manually

Your choice:
```

Do not pick for the user. `reuse` inherits whatever partial state the aborted run left, and
`delete` discards it; which is right depends on why the earlier run stopped.

**Where it exists changes what each option means.** Determine that first:

```bash
git show-ref --verify --quiet "refs/heads/$FINALIZE_BRANCH" && LOCAL=yes || LOCAL=no
git ls-remote --exit-code --heads origin "$FINALIZE_BRANCH" >/dev/null 2>&1 && REMOTE=yes || REMOTE=no
echo "local=$LOCAL remote=$REMOTE"
```

**Local only** — the abort happened in this clone and nothing was pushed:

```bash
git checkout "$FINALIZE_BRANCH"                    # reuse
git branch -D "$FINALIZE_BRANCH"                   # delete, then re-run Step 4
```

**Remote only** — the abort happened elsewhere and this clone has none of those commits:

```bash
# reuse: adopt what is already there
git fetch origin "$FINALIZE_BRANCH" \
  && git checkout -b "$FINALIZE_BRANCH" "origin/$FINALIZE_BRANCH"

# delete: discard it, then re-run Step 4
git push origin --delete "$FINALIZE_BRANCH"
```

**Both** — reuse needs the local branch reconciled with the remote, and delete must remove *both*
sides. Deleting only the remote leaves the local branch behind and Step 4 keeps aborting on
`BRANCH_EXISTS_LOCAL`:

```bash
# reuse: fast-forward local onto the remote; if it refuses, the two have diverged —
# inspect rather than force
git checkout "$FINALIZE_BRANCH" && git merge --ff-only "origin/$FINALIZE_BRANCH"

# delete: remote first, then local — both, or Step 4 aborts again
git checkout main
git push origin --delete "$FINALIZE_BRANCH"
git branch -D "$FINALIZE_BRANCH"
```

Deleting a finalize branch is safe — it only ever carries docs commits, and the ruleset's `deletion`
rule protects `main`, not it.

### Case 8: Nothing to Commit

Step 9a staged the files and found no difference. Usually this means the aggregation files were
already updated elsewhere — `/project:sync --complete` running in this repo rather than in the
worktree is the common cause.

```
NOTE: Aggregation files already current

Nothing to commit, so no finalize PR is needed.

The task is still complete. Continuing with:
- Linear -> Done (Step 9g)
- Completion summary (Step 10, finalize-PR line omitted)
```

**No PR is not the same as not finished.** Skip 9a's push through 9f, then run 9g and Step 10
normally. Delete `$FINALIZE_BRANCH` since it carries no commits:

```bash
git checkout main
git branch -d "$FINALIZE_BRANCH" 2>/dev/null || true
```

Leave `phases.complete.finalize_pr_url` and `finalize_pr_number` unset; Step 10 omits that line.

### Case 9: `CI Gate` Did Not Pass

Either a check failed or the 20-minute wait expired.

```
STOP: Finalize PR not merged

PR:     [PR_URL] (#[PR_NUMBER])
Reason: [CI Gate failed / wait timed out after 20 minutes]
Run:    [failing run URL, from `gh run list --branch docs/clo-XX-finalize`]

Linear has NOT been set to Done and the PR has NOT been merged.
```

A docs-only change cannot legitimately break the Rust build, so a failure here means `main` is red
for an unrelated reason. Fix that first. A timeout usually means an Actions outage or a stuck
runner, and the PR is fine — re-run `/pr:finalize CLO-XX` once the check reports, and Case 7's
`reuse` path picks it back up.

Do not merge around this with `--admin`, and do not add a `paths:` filter to `ci.yml` to make docs
PRs skip CI. A filtered-out `CI Gate` never reports at all, and a required check that never reports
blocks the PR permanently rather than letting it through.

---

## Cleanup Checklist

Before marking complete, verify:

- [ ] Implementation PR is merged to main
- [ ] Main branch is up-to-date locally (pulled in main repo)
- [ ] PROJECT.md updated
- [ ] ROADMAP.md updated
- [ ] DEPENDENCIES.md updated
- [ ] Status file has final summary
- [ ] Aggregation and status updates committed to `docs/clo-XX-finalize`
- [ ] Finalize PR opened, `CI Gate` passed, PR merged and branch deleted (or Case 8: nothing to commit)
- [ ] Repo returned to `main` and pulled
- [ ] Linear task status is "Done" (set only after the finalize PR merged)

**If Worktree Mode** (user handles after exiting):
- [ ] User runs `gd` to delete worktree and branch

---

## Integration Notes

**Called by**: `/task:orchestrate` as final step

**Follows**: PR merge

**Final step in workflow chain**

**Supports**:
- Regular branches (switches to main, optional branch deletion)
- Git worktrees (updates main repo folder, user runs `gd` to cleanup)

**Updates** (all via one pull request from `docs/clo-XX-finalize`, never a direct push to `main`):
- Aggregation files (all three)
- Status file
- Workflow state file
- Linear task (Done status, set only after that PR merges)

**Branch Cleanup**:
- **Regular mode**: Optional branch deletion offered
- **Worktree mode**: User runs `gd` after exiting Claude to delete worktree and branch
