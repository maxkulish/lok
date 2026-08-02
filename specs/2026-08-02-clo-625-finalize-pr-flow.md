# Spec: `/pr:finalize` opens a PR instead of pushing to `main`

**Created**: 2026-08-02
**Linear**: [CLO-625](https://linear.app/cloud-ai/issue/CLO-625)
**Estimated scope**: S/M (2 files edited, 1 audit note, 5 sub-tasks)

## 1. Problem Statement

`.claude/commands/pr/finalize.md` is the last command in the task lifecycle. It updates
`docs/PROJECT.md`, `docs/ROADMAP.md`, `docs/DEPENDENCIES.md` and `docs/status/` after a PR merges,
then commits those edits **directly to `main`**. That has been impossible on this repository since
CLO-600 landed the branch ruleset:

```
! [remote rejected] main -> main (GH013: Repository rule violations found for refs/heads/main)
Required status check "CI Gate" is expected.
```

Ruleset `20153405` on `refs/heads/main` is `enforcement=active` with `required_status_checks:
["CI Gate"]`, **zero** `bypass_actors`, plus `deletion` and `non_fast_forward` rules. With no bypass
actors the repository owner cannot push to `main` either. This is the intended shape, not a
permissions gap, so the command has to change.

The command is built end to end around the protected branch, not merely at one line:

| File:line | Step | What it does |
| -- | -- | -- |
| `.claude/commands/pr/finalize.md:96` | Step 4 | Asserts `git rev-parse --abbrev-ref HEAD` is `main` |
| `.claude/commands/pr/finalize.md:250` | Step 7, worktree mode | `git push origin main` (aggregation files) |
| `.claude/commands/pr/finalize.md:266` | Step 7, standard mode | `git push origin main` (aggregation files) |
| `.claude/commands/pr/finalize.md:335` | Step 9 | `git push origin main` (`docs/status/` workflow state) |
| `.claude/commands/pr/finalize.md:500` | Checklist | `- [ ] Aggregation updates committed and pushed to main` |

Fixing only Step 7 leaves Step 9 failing identically. Anyone running `/pr:finalize` today gets a
rejected push after the Linear task has already been flipped to Done (Step 5), leaving the task
half-finalized: Linear says complete, the repository has no record.

Two facts constrain the fix and were verified against the live repository rather than assumed:

- `allow_auto_merge: false` — `gh pr merge --auto` is unavailable. The command must poll for
  `CI Gate` itself.
- `delete_branch_on_merge: false` — the finalize branch survives merge unless deleted explicitly.
- `.github/workflows/ci.yml` has **no `paths:` filter**, so a docs-only PR does run `CI Gate` and
  can therefore satisfy the required check. A full Rust build/test on two runners for a docs commit
  is wasteful but it is the price of the gate being unconditional, and it is what makes the new
  flow work at all.

## 2. Acceptance Criteria

- [ ] **AC1**: No `git push origin main` remains anywhere in `.claude/commands/pr/`.
- [ ] **AC2**: `.claude/commands/pr/finalize.md` Step 4 no longer asserts `HEAD == main`; it creates
      and checks out `docs/clo-XX-finalize` from an up-to-date `main`.
- [ ] **AC3**: The aggregation files and `docs/status/` files reach `main` through exactly **one**
      pull request per finished task, not three pushes.
- [ ] **AC4**: The command waits for the `CI Gate` check to conclude and merges only on success;
      it does not use `gh pr merge --auto` (unavailable on this repo). On a `CI Gate` failure it
      aborts before merging, leaves the Linear task in its pre-merge state, and reports the failing
      run URL. The wait is bounded — it does not block forever on a stuck runner.
- [ ] **AC4b**: When there is nothing to commit (the aggregation files are already current), the
      command opens **no** PR and still completes: it updates Linear to Done, writes the workflow
      YAML, and renders the summary with the finalize-PR line omitted.
- [ ] **AC4c**: When the finalize branch or an open PR for it already exists from an aborted run,
      the command detects it and prompts with reuse/delete options instead of hard-failing on
      `git checkout -b` or `gh pr create`.
- [ ] **AC5**: The completion summary names the finalize PR it opened, distinct from the
      implementation PR already shown in the 🔗 Pull Request block.
- [ ] **AC6**: `/pr:finalize` runs to completion on this repository against the active ruleset,
      demonstrated by finalizing a real task, not by reading the file.
- [ ] **AC7**: The resulting PR passes `CI Gate` and merges.
- [ ] **AC8**: Sibling commands under `.claude/commands/` are audited for the same direct-push
      assumption and the result is recorded in the workflow YAML.

**Verification method**: AC1 and AC2 by `rg`; AC3–AC5 by reading the rewritten file against this
spec; AC6 and AC7 by running the rewritten command to finalize CLO-625 itself and observing the PR
merge; AC8 by the audit table in §4 sub-task 4 being present in `docs/status/clo-625-workflow.yaml`.

## 3. Constraints

**Must**:
- Keep the branch-and-PR path as the **normal** flow, not an error path or a fallback. There is no
  direct-push path left to fall back to.
- Use `docs/clo-XX-finalize` as the branch name. It matches the global branch convention
  (`<type>/<short-kebab-title>`, type drawn from the conventional-commit set) and marks the branch
  as documentation-only.
- Collapse the Step 7 and Step 9 commits into **one** PR. One PR per finished task is enough and it
  removes two of the three pushes outright.
- Poll `gh pr checks <n> --watch` (or equivalent) before `gh pr merge`, because `allow_auto_merge`
  is `false` on this repository. Bound the wait: pass `--interval 30` and wrap it so it gives up
  after roughly 20 minutes, then report the PR URL and stop rather than blocking forever on a
  GitHub Actions outage or a stuck runner. An unbounded `--watch` is a hang, not a wait.
- Check `gh pr list --head docs/clo-XX-finalize --state open` before `gh pr create`. If a PR is
  already open for the branch, reuse it instead of hard-failing.
- Pass `--delete-branch` to `gh pr merge`, because `delete_branch_on_merge` is `false`.
- Return the repository to `main` and pull after the merge, so a worktree's parent repo is not left
  sitting on a branch that was just deleted on the remote.
- Preserve worktree mode. Both modes keep working; they differ only in which directory the git
  operations happen in, as they do today.
- Keep Step 5 (Linear → Done) **after** the merge succeeds, not before. Today the Linear flip
  happens at Step 5 and the push fails at Step 7, which is exactly how the failure produced a
  half-finalized task.

**Must-not**:
- Do not request, add, or document a ruleset bypass actor. The ruleset shape is the intended one.
- Do not force-push, and do not use `--admin` on `gh pr merge`.
- Do not add a `paths:` filter to `.github/workflows/ci.yml` to make the docs PR skip CI. That
  would make `CI Gate` non-reporting on docs PRs and the required check would block forever.
- Do not touch the implementation PR's fields in the workflow YAML (`phases.pr.*`). The finalize PR
  is a second, separate PR and needs its own keys.

**Prefer**:
- Reuse `gh` invocations already present in sibling commands (`gh pr merge [number] --squash` at
  `.claude/commands/task/phases/complete.md:25`) rather than inventing new spellings.
- Keep the special-case blocks at the bottom of `finalize.md` intact; add to them rather than
  rewriting them.

**Escalate when**:
- `CI Gate` fails on the finalize PR. A docs-only change cannot legitimately break the Rust build,
  so a failure means `main` is red for an unrelated reason and the task should stop rather than
  merge around it. Abort before merging and leave Linear untouched.
- The finalize branch, or an open PR for it, already exists from an earlier aborted run — prompt
  the user with options to reuse or delete it.
- The bounded CI wait expires. Report the PR URL and hand back to the user; do not merge blind.

## 4. Decomposition

1. **Rewrite the branch handling (Steps 2–4)** — Step 4 stops asserting `main`. In both modes it
   pulls `main`, then creates `docs/clo-XX-finalize`. Handle the pre-existing-branch case.
   Files: `.claude/commands/pr/finalize.md`
2. **Collapse Steps 7 and 9 into one commit on the branch** — a single `git add` of the aggregation
   files plus `docs/status/`, one commit message, one `git push -u origin docs/clo-XX-finalize`.
   Delete the second commit block and the "Alternative" note at line 338, which exists only because
   there were two commits.
   Files: `.claude/commands/pr/finalize.md`
3. **Add the PR step** — check for an already-open PR on the branch, otherwise open one; capture its
   number, record it in the workflow YAML, push that second commit, wait for `CI Gate` under a
   bounded timeout, merge with `--squash --delete-branch`, then return to `main` and pull. Move the
   Linear → Done update to after the merge. Handle the two non-merge exits explicitly: **empty
   diff** skips PR creation entirely and jumps straight to the Linear update and the summary;
   **CI failure or wait timeout** stops before both the merge and the Linear update.
   Files: `.claude/commands/pr/finalize.md`
4. **Audit siblings and record the result** — grep every command for `push origin main`,
   `checkout main`, and `HEAD == main` assertions; record the table in the workflow YAML.
   Files: `docs/status/clo-625-workflow.yaml`
5. **Teach the completion summary about the finalize PR** — add `phases.complete.finalize_pr_url`
   and `finalize_pr_number` to the field-mapping table with **omit the line** as the missing-data
   rule, and add that line to the 📂 Aggregation Files block in the canonical template. Update the
   finalize checklist line 500.
   Files: `.claude/templates/completion-summary.md`, `.claude/commands/pr/finalize.md`

**Dependency order**: 1 → 2 → 3 are sequential edits to overlapping regions of one file and should
be done in that order. 4 is independent. 5 depends on 3 for the YAML key names.

### Ordering decision: why two commits on the branch, not one

The workflow YAML must record the finalize PR's URL, but that URL does not exist until
`gh pr create` has run, and `gh pr create` needs a pushed branch. The sequence is therefore:

1. commit the aggregation files and `docs/status/` (workflow YAML without the PR keys), push
2. `gh pr create`, capture number and URL
3. write `phases.complete.finalize_pr_url` / `finalize_pr_number` into the workflow YAML, commit,
   push
4. **now** wait for `CI Gate` on the final head SHA, then merge

Waiting for CI after the second push rather than between steps 2 and 3 means CI is waited on once,
not twice. Amending the first commit instead would require a force-push and buys nothing.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | No direct push to main remains | no output | `rg -n "push origin main" .claude/commands/pr/` |
| 2 | No `HEAD == main` assertion remains in finalize | no output | `rg -n 'abbrev-ref HEAD.*main\|Should be "main"' .claude/commands/pr/finalize.md` |
| 3 | Finalize branch is created | one match, `docs/clo-XX-finalize` | `rg -n "checkout -b docs/" .claude/commands/pr/finalize.md` |
| 4 | Exactly one commit-and-push block for aggregation + status | one `git push -u origin docs/` | `rg -n "git push" .claude/commands/pr/finalize.md` |
| 5 | CI wait precedes merge, no `--auto` | `gh pr checks` appears before `gh pr merge`; no `--auto` | `rg -n "gh pr checks\|gh pr merge" .claude/commands/pr/finalize.md` |
| 6 | Merge deletes the branch | `--delete-branch` present | `rg -n "delete-branch" .claude/commands/pr/finalize.md` |
| 7 | Summary template carries the finalize PR | `finalize_pr_url` in both the mapping table and the template block | `rg -n "finalize_pr" .claude/templates/completion-summary.md` |
| 8 | **End-to-end on a real task** | PR opens, `CI Gate` passes, PR merges, `main` contains the aggregation update | run `/pr:finalize CLO-625` for this task |
| 9 | Linear flip is after the merge | Step ordering shows Linear update following `gh pr merge` | read `.claude/commands/pr/finalize.md` |
| 10 | Empty diff still reaches Done | with the aggregation files already current, no PR opens, Linear goes to Done, summary omits the finalize-PR line | dry-run the empty-diff branch of the command by hand |
| 11 | Branch exists **and** a PR is open for it | command detects both and prompts reuse/delete; does not hard-fail | `rg -n "gh pr list --head" .claude/commands/pr/finalize.md` |
| 12 | CI wait is bounded | a timeout or interval bound is present, no bare `--watch` | `rg -n "gh pr checks" .claude/commands/pr/finalize.md` |
| 13 | CI failure aborts before Linear | failure path shows no merge and no Linear update | read `.claude/commands/pr/finalize.md` |

Test 8 is the criterion the Linear issue insists on — "verified by running it on a real task rather
than by reading". CLO-625 finalizes itself with the rewritten command, so the fix proves itself.

**Edge cases to verify**:
- **Finalize branch already exists** from an aborted run: the command must say so and prompt the
  user with reuse/delete options rather than failing on `git checkout -b`.
- **Branch exists and a PR is already open for it**: the compound case. Reusing the branch is not
  enough — `gh pr create` hard-fails when a PR already exists for the head. Check
  `gh pr list --head docs/clo-XX-finalize --state open` first and adopt that PR's number.
- **Nothing to commit**: if the aggregation files were already updated (for example
  `/project:sync --complete` ran in this repo, not the worktree), `git commit` fails with "nothing
  to commit". The command must detect the empty diff and skip PR creation entirely — but the task
  is still finished, so it must still flip Linear to Done, write the workflow YAML, and render the
  summary. Skipping the PR must not skip completion.
- **`finalize_pr_url` absent**: on the empty-diff path there is no finalize PR, so the summary line
  is **omitted** rather than rendered as `(none)` — the same rule the template already applies to
  the Lessons block. A missing line means "no PR was needed", not "a PR failed".
- **CI wait expires**: report the PR URL, do not merge, do not flip Linear. The user resumes by
  re-running the command once the check reports.
- **Worktree mode**: the git operations happen in the main repo directory (`../lok`), and the
  branch, PR, merge, and the return to `main` all happen there. The worktree itself is untouched
  and the user still runs `gd` afterwards.
- **`CI Gate` fails**: stop, do not merge, do not flip Linear to Done, report the failing run URL.
- **Dirty main repo** (existing Case 6): unchanged behaviour, but it now matters more because the
  command creates a branch rather than committing in place.

### Sibling audit (input for sub-task 4)

Grep of `.claude/` for `push origin main`, `checkout main`, and branch assertions:

| File:line | Finding | Verdict |
| -- | -- | -- |
| `pr/finalize.md:96,250,266,335,500` | the defect | **fix** |
| `task/phases/complete.md:42–43` | `git checkout main; git pull origin main` — read-only, no push | clean, but see note |
| `pr/create.md:92` | pushes the feature branch | clean |
| `pr/create.md:421` | `git fetch origin main` — read-only | clean |
| `pr/review.md:263` | pushes the feature branch | clean |
| `plan/create.md:225`, `plan/implement.md:659,844`, `task/phases/implement.md:20`, `design-doc/create.md:381` | push feature branches | clean |

**Note on `complete.md`**: Step 3 invokes `/project:sync --complete`, which edits the aggregation
files, and Step 4 then runs `git checkout main` with those edits uncommitted. In worktree mode the
edits land in the worktree while `/pr:finalize` edits the copy in the main repo — the same three
files are edited twice in two places. This is a real ordering defect but it is not a direct-push
defect, so it is **out of scope here**; file it as a follow-up.
