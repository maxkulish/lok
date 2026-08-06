# /pr:review - Handle PR Review Feedback

**Purpose**: Check for PR review comments, analyze feedback, make necessary changes, and respond to reviewers. Automates the review feedback cycle.

**Usage**:
- `/pr:review CLO-XX` - Check and address reviews for specific task
- `/pr:review` - Interactive mode (detects from branch)

---

## Workflow

```
┌─────────────────────────────────────────────────────────────────┐
│                    PR Review Cycle                              │
├─────────────────────────────────────────────────────────────────┤
│  1. Fetch PR reviews and comments (with pagination)             │
│  2. Analyze feedback (blocking vs suggestions)                  │
│  3. VERIFY each bot claim before acting on it                   │
│  4. Detect stale comments (line changed since comment)          │
│  5. Make code changes to address verified feedback              │
│  6. Commit changes with descriptive message                     │
│  7. Push to branch (BEFORE replying)                            │
│  8. Reply to EVERY comment (MANDATORY - track N/N)              │
│  9. Request re-validation (/agentic_review - Qodo needs it)     │
│ 10. Poll for the re-review pass on the CURRENT head SHA         │
│ 11. Repeat if new comments arrive                               │
└─────────────────────────────────────────────────────────────────┘
```

### MANDATORY: Reply to Every Comment

**Every review comment MUST receive a reply.** This is not optional.
No comment may be left without a response - whether the fix was applied, declined with rationale, or acknowledged. The PR review is not complete until all comments have replies posted via the GitHub API.

| Decision | Required Reply |
|----------|---------------|
| Fixed | "Fixed in [SHA]. [what changed]" |
| Declined | "Intentionally kept as-is: [rationale]" |
| Question answered | "[explanation]. [reference to design doc if relevant]" |
| Deferred | "Tracked as follow-up in [task/issue]. [reason for deferral]" |

**Addressing Qodo in a reply.** Qodo only answers a reply that mentions it. Per Qodo's docs you must mention `@qodo` (or `/qodo`, or a bare `qodo`) *each* time you want a response, including follow-ups in a thread it already participates in. A plain reply is recorded but Qodo will not read or respond to it.

Mention `@qodo` only when you actually want an answer - a declined finding where you want its assessment, or a question about its reasoning. For a straightforward "fixed in SHA", leave the mention off; a re-review is what confirms the fix, not a thread reply.

There is no `/gemini review` trailer any more. Gemini Code Assist is sunset.

---

## Command Execution Instructions

### Step 1: Extract Task and PR Info

1. **Get task number** from argument or branch name
2. **Find PR number**:

```bash
gh pr list --head "feat/clo-XX-description" --json number,url,state
```

**If no PR found**:
```
ERROR: No PR found for CLO-XX

Expected PR from branch: feat/clo-XX-*

Create one first: /pr:create CLO-XX
```
Exit command.

### Step 2: Fetch PR Status

```bash
gh pr view [number] --json state,reviews,reviewDecision,comments,mergeable
```

Extract:
- `state`: open, closed, merged
- `reviews`: List of reviews with state (APPROVED, CHANGES_REQUESTED, COMMENTED)
- `reviewDecision`: Overall decision
- `comments`: General PR comments
- `mergeable`: Whether PR can be merged

**If PR is merged**:
```
PR #[number] is already merged.

Merged at: [timestamp]

No review action needed.
```
Exit command.

### Step 3: Fetch Review Comments

**Use `--paginate` on all gh api calls** to ensure no comments are missed on PRs with many reviews (default page size is 30).

```bash
# Get review comments (inline code comments) - paginated
gh api repos/{owner}/{repo}/pulls/[number]/comments --paginate \
  --jq '.[] | {id, path, line, original_line, body, user: .user.login, created_at, commit_id: .original_commit_id}'

# Get review threads - paginated
gh api repos/{owner}/{repo}/pulls/[number]/reviews --paginate \
  --jq '.[] | {id, state, body, user: .user.login}'

# Get issue comments (general discussion)
gh pr view [number] --json comments --jq '.comments[] | {id, body, author: .author.login}'
```

**Note**: The `commit_id` and `original_line` fields are used for stale comment detection in Step 4.5.

### Step 4: Categorize Feedback

Group comments by type:

| Category | Priority | Action Required |
|----------|----------|-----------------|
| `CHANGES_REQUESTED` | High | Must address before merge |
| `COMMENTED` (blocking) | Medium | Should address |
| `COMMENTED` (suggestion) | Low | Optional, acknowledge |
| `APPROVED` | None | No action needed |

**Identify blocking feedback**:
- Explicit change requests
- Questions about implementation
- Security concerns
- Bug reports

**Identify non-blocking**:
- Style suggestions
- "Nice to have" improvements
- Positive feedback

### Step 4.5: Detect Stale Comments

For each inline comment, check if the referenced code has changed since the comment was posted:

1. The comment's `original_commit_id` tells you what commit the reviewer saw
2. Compare the file at that commit vs HEAD:
   ```bash
   git diff [original_commit_id]..HEAD -- [file_path]
   ```
3. If the diff includes changes around the commented line (within 5 lines), flag the comment as **potentially stale**

**Stale comments are presented to the user but marked clearly:**
```
[STALE?] @reviewer on src/backend/retry.rs:45
  "Consider adding jitter to retry delay"
  Note: Lines 40-50 of this file changed in commit abc1234 after this comment.
```

The user decides whether stale comments still need action. Do not auto-skip them.

### Step 5: Display Review Summary

```
========================================
PR REVIEW STATUS: CLO-XX
========================================

PR #[number]: [title]
State: Open
Mergeable: Yes/No

Reviews:
  - @reviewer1: APPROVED
  - @reviewer2: CHANGES_REQUESTED

Overall Decision: [APPROVED / CHANGES_REQUESTED / PENDING]

Comments to Address: [count]

---

BLOCKING FEEDBACK:

1. @reviewer2 on src/websocket/handler.rs:45
   "Consider using async/await pattern here"
   Status: Unresolved

2. @reviewer2 general comment
   "Please add documentation for the public API"
   Status: Unresolved

---

SUGGESTIONS (optional):

1. @reviewer1 on src/websocket/parser.rs:12
   "Consider adding validation for input"
   Status: Unresolved

---

Options:
1. [address] - Address blocking feedback
2. [address-all] - Address all feedback including suggestions
3. [skip] - Skip for now
4. [details] - Show full comment details

Your choice:
```

### Step 6: Address Feedback

For each piece of blocking feedback:

#### 6.1: Analyze the Comment

Read the comment and:
1. Identify the file and line referenced
2. Understand the requested change
3. Read surrounding code for context
4. Determine the fix

#### 6.2: Make Code Changes

Use appropriate tools to implement the fix:

```bash
# Read the file
Read tool: [file path]

# Make changes
Edit tool: [modifications]

# Validate
cargo build && cargo test
```

#### 6.3: Track Changes Made

Keep a list of changes for commit message:
- `[file]: [change description]`

### Step 7: Create Review Response Commit

After addressing feedback, commit with descriptive message:

```bash
git add [modified files]
git commit -m "$(cat <<'EOF'
fix(CLO-XX): address PR review feedback

Changes:
- src/websocket/handler.rs: Use async/await pattern
- src/websocket/mod.rs: Add public API documentation

Resolves review comments from @reviewer2

Related: PR #[number]
EOF
)"
```

### Step 8: Push Changes

Push BEFORE replying so the commit SHA is visible on GitHub when reviewers read your replies.

```bash
git push origin feat/clo-XX-description
```

### Step 9: Reply to EVERY Comment (MANDATORY)

**This step is REQUIRED. Do not skip it. Do not proceed to Step 10 until every comment has a reply.**

For EACH comment (fixed, declined, or question), post a reply via the GitHub API:

```bash
# Reply to a review comment
gh api repos/{owner}/{repo}/pulls/[number]/comments/[comment_id]/replies \
  -X POST -f body="Fixed in [commit SHA]. [Brief explanation of change]"
```

**Rules**:
1. Every comment gets a reply - no exceptions
2. Reference the commit SHA that contains the fix
3. If declining a suggestion, explain why (reference design docs, ADRs, or project constraints)
4. Declining a bot finding requires evidence, not assertion. Paste the command output, file:line, or config value that disproves it. `qodo-code-review` reasons from the diff without running anything, so its premises are the thing to check first
5. For `qodo-code-review`: add `@qodo` only when you want it to answer (a disputed finding). Omit it otherwise - see the addressing rule above
6. For `copilot-pull-request-reviewer`: reply with fix details, no trigger needed
7. Re-validation is a separate step - a reply never triggers it. See Step 9.5
8. Track reply count - the final summary must show `Replies Posted: N/N`

**Reply templates by reviewer type**:

| Reviewer | Decision | Reply Template |
|----------|----------|----------------|
| Human | Bug fix | "Fixed in abc1234. Good catch!" |
| Human | Declined | "Intentionally kept as-is: [rationale]. Happy to discuss." |
| `qodo-code-review` | Fixed | "Fixed in abc1234. [what changed, and why it satisfies the finding]" |
| `qodo-code-review` | Declined | "@qodo Keeping as-is. [evidence: command output / file:line / config value that disproves the premise]" |
| `copilot-pull-request-reviewer` | Fixed | "Fixed in abc1234. [details]" |
| `copilot-pull-request-reviewer` | Declined | "Intentionally kept as-is: [rationale]" |

**Batch replies** (for multiple comments):

```bash
COMMIT_SHA=$(git rev-parse --short HEAD)
COMMENTS=(
  "COMMENT_ID_1|Fixed: description of change"
  "COMMENT_ID_2|Declined: rationale for keeping as-is"
)

for item in "${COMMENTS[@]}"; do
  ID="${item%%|*}"
  MSG="${item#*|}"
  gh api repos/{owner}/{repo}/pulls/[number]/comments/${ID}/replies \
    -X POST -f body="${MSG} (${COMMIT_SHA})"
done
```

No per-reply trigger suffix is needed for any reviewer. Re-validation is requested once, in Step 9.5, not per comment.

### Step 9.5: Request Re-validation from Qodo

**Qodo does not re-review on push in this repo.** Verified on 2026-08-02 by posting `/config` to PR #71:

```
config.handle_push_trigger  = False
config.pr_commands          = ['/agentic_describe', '/agentic_review']
config.push_commands        = ['/agentic_review']
```

`pr_commands` runs on PR open. `push_commands` would run on each new commit, but only when `handle_push_trigger` is `True`, and it is `False`. Pushing a fix therefore leaves Qodo's findings sitting against the old head SHA forever unless you ask for a new pass.

After pushing fixes and posting replies, request one re-review for the whole PR:

Post the request and keep the timestamp **GitHub** assigns it:

```bash
REQUESTED_AT=$(gh api repos/{owner}/{repo}/issues/[number]/comments \
  -X POST -f body='/agentic_review' --jq .created_at)
```

Read the timestamp off the POST response rather than from local `date`. The review's `submitted_at` comes from GitHub's clock, so a locally generated bound compares two clock domains - a local clock running fast would filter out the very re-review it is waiting for and time out. This costs no extra API call.

Then poll for the completed pass on the **post-push** head SHA. A completed pass arrives in one of **two shapes** (the per-pass delivery rule in "How Qodo posts" below): a new **review object** appears only when the pass has new inline findings to attach; a **clean pass** edits the existing "Code Review by Qodo" comment in place and announces completion with a *new* issue comment reading `[Code review](...) by qodo was updated up to the latest commit <sha>`. Polling only the reviews endpoint therefore times out precisely when the re-review came back clean - the success case (observed on PR #80).

Each shape pairs a freshness bound with a covered-commit check, and both conditions matter. Freshness alone would accept a previous run's pass - re-running this step without an intervening push, or after the `/agentic_review` post failed, must not report a re-validation that never happened. The commit check alone would accept the stale pass on the old head.

```bash
NEW_HEAD=$(gh api repos/{owner}/{repo}/pulls/[number] --jq .head.sha)
printf '%s' "$NEW_HEAD" | grep -qE '^[0-9a-f]{40}$' || { echo "Bad head SHA '${NEW_HEAD}' - the pulls lookup failed; fix that first"; exit 1; }
[ -n "$REQUESTED_AT" ] || { echo "Empty REQUESTED_AT - the /agentic_review POST failed; fix that first"; exit 1; }
DEADLINE=$(( $(date -u +%s) + 600 ))

while :; do
  # Shape 1: a new review object - submitted only when the pass carries new inline findings
  REREVIEW=$(gh api repos/{owner}/{repo}/pulls/[number]/reviews --paginate --slurp \
    | jq -r --arg h "$NEW_HEAD" --arg since "$REQUESTED_AT" \
      '[.[][] | select(.commit_id==$h) | select(.user.login|test("qodo"))
        | select(.submitted_at >= $since) | .submitted_at] | last // empty')
  [ -n "$REREVIEW" ] && { echo "Re-review (new findings) on ${NEW_HEAD:0:7} at ${REREVIEW}"; break; }

  # Shape 2: a clean pass - a new completion comment naming the covered commit
  UPDATED=$(gh api repos/{owner}/{repo}/issues/[number]/comments --paginate --slurp \
    | jq -r --arg h "$NEW_HEAD" --arg since "$REQUESTED_AT" \
      '[.[][] | select(.user.login|test("qodo")) | select(.created_at >= $since)
        | select(.body|test("was updated up to the latest commit"))
        | select(.body|contains($h)) | .created_at] | last // empty')
  [ -n "$UPDATED" ] && { echo "Re-review (clean pass) covering ${NEW_HEAD:0:7} at ${UPDATED}"; break; }

  [ "$(date -u +%s)" -ge "$DEADLINE" ] && {
    echo "No re-review since ${REQUESTED_AT}: no qodo review object on ${NEW_HEAD:0:7} and no completion comment naming it. Inspect the PR before overriding - do not treat this as a pass."
    exit 1
  }
  sleep 20
done
```

**Why not the persistent comment's `updated_at`?** It bumps when a re-review has *not* landed: the comment cycles through intermediate states during a pass, and Qodo also refreshes it outside passes entirely - on PR #80 it was edited 10 seconds after merge, with no `/agentic_review` posted and no completion comment, just permalinks refreshed to a head no pass had covered. `updated_at >= REQUESTED_AT` is corroborating evidence, never the gate; gating on it reports re-validations that never happened.

Expect 3-4 minutes per pass. Whichever shape lands, the "Code Review by Qodo" comment passes through intermediate states while Qodo works - it briefly showed `Bugs (0)` before settling on `Bugs (1)` during one pass on PR #71. The gate above only proves the pass **landed**; never read the finding count while a "Qodo is busy working" comment is present.

**Command notes** (Qodo's managed app is PR-Agent under the hood):

| Command | Effect |
|---|---|
| `/agentic_review` | Full re-review. This is the recheck command |
| `/agentic_describe` | Regenerate the PR summary comment |
| `/ask <question>` | Ask about the diff |
| `/config` | Dump effective configuration - use it to re-verify these trigger settings if behavior changes |
| `/checks` | Analyze CI failures |

`/review` is the legacy PR-Agent name and is **not** the configured command here; `pr_commands` lists `/agentic_review`. Do not encode `/review` in any procedure.

**Do not enable `handle_push_trigger` to avoid this step.** It would fire a full review on every intermediate push, including work-in-progress commits, and `push_trigger_dedup_ttl_seconds` is only 60. One explicit request at a known point is cheaper and gives a deterministic signal to poll for, which is what `pr-review-cycle` is built around.

### Step 10: Update Workflow State (if exists)

```yaml
phases:
  pr:
    reviews_addressed: [increment count]

history:
  - timestamp: [ISO timestamp]
    action: review_addressed
    phase: pr
    details: "Addressed [count] review comments, pushed [commit SHA]"
```

### Step 11: Re-check Review Status

After pushing, check if new comments appeared:

```bash
gh pr view [number] --json reviews,reviewDecision
```

**If new changes requested**:
```
New feedback received after your push.

[New comments]

Would you like to address these? (yes/no)
```

**If approved**:
```
SUCCESS: PR is now approved!

All reviewers have approved.
Ready to merge.

Next steps:
1. Merge: gh pr merge [number] --squash
2. Or continue with orchestrator: /task:orchestrate CLO-XX
```

### Step 12: Update Linear

Post update to Linear:

```
mcp__linear-server__create_comment(
  issueId="CLO-XX",
  body="## PR Review Update

**PR**: #[number]
**Status**: [Addressed feedback / Approved]

**Changes Made**:
- [Change 1]
- [Change 2]

**Commits**: [SHA]

**Review Status**:
- @reviewer1: Approved
- @reviewer2: [Updated status]"
)
```

### Step 13: Confirm to User

**GATE**: Do not display this summary until ALL comments have replies posted.
If any comment is missing a reply, go back to Step 9 and post it.

```
========================================
REVIEW FEEDBACK ADDRESSED
========================================

PR #[number]: [title]

Changes Made:
- [file1]: [change]
- [file2]: [change]

Commits: [count]
Pushed: Yes

Replies Posted: [N/N] (MUST be N/N - all comments replied to)
- Comment [id]: [fixed/declined/answered] (@reviewer)
- Comment [id]: [fixed/declined/answered] (@qodo-code-review)
- Comment [id]: [fixed/declined/answered] (@copilot-pull-request-reviewer)
- ...

Stale Comments: [count skipped with user approval]

Review Status:
- @reviewer1: APPROVED
- @reviewer2: CHANGES_REQUESTED -> [pending re-review]

Next steps:
1. Wait for re-review
2. Run /pr:review CLO-XX again if needed
3. After approval: /task:orchestrate CLO-XX
```

---

## Handling Different Feedback Types

### Type 1: Code Change Request

```markdown
Reviewer: "Use async/await instead of callbacks"
File: src/websocket/handler.rs:45

Action:
1. Read src/websocket/handler.rs
2. Find line 45
3. Refactor to async/await
4. Commit and reply
```

### Type 2: Missing Functionality

```markdown
Reviewer: "Please add input validation"

Action:
1. Identify the relevant function
2. Add validation logic
3. Add tests for validation
4. Commit and reply
```

### Type 3: Documentation Request

```markdown
Reviewer: "Add usage examples to the module documentation"

Action:
1. Find or create doc comments
2. Add usage examples
3. Commit and reply
```

### Type 4: Question (No Code Change)

```markdown
Reviewer: "Why did you choose this approach?"

Action:
1. Reply with explanation
2. Reference design doc if applicable
3. No code change needed
```

### Type 5: Suggestion (Optional)

```markdown
Reviewer: "Nice to have: could add logging here"

Action:
1. Evaluate effort vs value
2. If quick: implement and reply
3. If complex: reply explaining decision to defer
```

---

## Batch Processing

When multiple comments exist:

1. **Group by file**: Address all comments on same file together
2. **Order by priority**: Blocking first, then suggestions
3. **Single commit per file group**: Avoid many small commits
4. **Batch replies**: Reply to all addressed comments

Example commit for batch:

```
fix(CLO-XX): address PR review feedback (batch)

src/websocket/handler.rs:
- Line 45: Use async/await pattern
- Line 67: Add error context

src/websocket/parser.rs:
- Add input validation
- Add documentation

Resolves: 4 review comments from @reviewer2
```

---

## Special Cases

### Case 1: Conflicting Feedback

When two reviewers give conflicting feedback:

```
CONFLICT DETECTED: Reviewers disagree

@reviewer1: "Use sync approach for simplicity"
@reviewer2: "Use async for performance"

Please decide:
1. Follow @reviewer1's suggestion
2. Follow @reviewer2's suggestion
3. Find a compromise
4. Discuss in PR comments

Your choice:
```

### Case 2: Feedback Requires Design Change

```
SIGNIFICANT CHANGE REQUESTED

@reviewer1: "This should use a completely different architecture"

This feedback suggests changes beyond implementation fixes.

Options:
1. [discuss] - Comment asking for clarification
2. [update-design] - Revisit design document
3. [escalate] - Tag project lead for decision
4. [implement] - Try to implement anyway

Your choice:
```

### Case 3: Stale Comments

Stale comments are now detected automatically in Step 4.5. When a comment references code that has changed since the comment was posted, it is flagged with `[STALE?]` in the review summary.

The user decides how to handle stale comments:
- **Already addressed**: Reply explaining the change resolves the concern
- **Still relevant**: Address as normal
- **No longer applicable**: Reply noting the code has been restructured

### Case 4: All Approved

```
SUCCESS: All reviews approved!

No action needed.

PR is ready to merge.

Options:
1. [merge] - Merge the PR
2. [wait] - Wait for more reviews
3. [exit] - Exit (merge manually)

Your choice:
```

---

## AI Code Review: qodo-code-review

`qodo-code-review` is the repository's automated reviewer, installed 2026-08-02. Its first review on this repo was PR #71. It replaced `gemini-code-assist`, whose consumer GitHub app is sunset and no longer reviews anything.

### How Qodo posts

On the first pass, two artifacts in order:

1. **A summary issue comment** ("PR Summary by Qodo") - AI description, a mermaid diagram, alternative approaches, and a per-file table. Informational; there is nothing to address in it.
2. **The review** ("Code Review by Qodo") - a persistent issue comment carrying the findings header, plus a review object with the inline findings attached, a few minutes later.

Only the second carries actionable content. On PR #71 the summary arrived at 12:31 and the review at 12:35. Reading the summary and concluding "no findings" is a mistake - wait for the review.

**Per-pass delivery rule** (verified against PRs #71 and #80): a **review object** is submitted only when a pass has new inline findings to attach - all six on PR #71 carried 1-3 inline comments; PR #80's clean re-review submitted none. Every pass updates the persistent "Code Review by Qodo" comment in place, and every completed *re*-review pass additionally announces itself with a new issue comment reading `[Code review](...) by qodo was updated up to the latest commit <sha>`. Step 9.5's poll is built on this rule - both shapes, fail closed. Do not describe or poll Qodo's re-reviews any other way.

The review is always submitted as `COMMENTED`, never `CHANGES_REQUESTED`, so `reviewDecision` stays `PENDING` no matter how severe the findings. Never infer severity from review state.

### Fetching Qodo findings

```bash
# Inline findings - the actionable ones
gh api repos/{owner}/{repo}/pulls/[number]/comments --paginate \
  --jq '.[] | select(.user.login|test("qodo")) | {id, path, line, body}'

# The summary + any command responses
gh api repos/{owner}/{repo}/issues/[number]/comments --paginate \
  --jq '.[] | select(.user.login|test("qodo")) | {id, created_at}'
```

### Severity

Severity lives in a badge image at the top of each inline comment body, not in a `**Severity**:` line.

| Badge | Priority | Action |
|---|---|---|
| `Action required` | High | Address before merge |
| `Review recommended` | Medium | Address or decline with evidence |

Findings are also tagged by category (`🐞 Bug`, `☼ Reliability`, `≡ Correctness`) and the review header counts them (`🐞 Bugs (3)`, `📘 Rule violations (0)`).

Parse the badge with:

```bash
grep -o 'alt="[^"]*"' <<<"$BODY" | head -1
```

Each finding also ships an **Agent Prompt** block containing a ready-made remediation prompt with issue description, context, focus areas, and a suggested fix. It is a useful starting point, not an instruction to follow blindly.

### Verify before you act

**Qodo findings are claims, not verdicts.** It reasons from the diff without executing anything, so it cannot tell whether a command exists on the target platform, whether a flag is supported, or whether a helper elsewhere in the repo already solves the problem. Check the premise before writing code.

PR #71 is the worked example - three findings, two real:

| Finding | Verdict | How it was settled |
|---|---|---|
| Doc/workflow model prefix mismatch produced `google/google/...` | Real | Read the README line against the workflow's `--model "google/$VAR"` construction |
| `pr-review-cycle` step 1 and step 2 gave contradictory ordering | Real | Read both sections; they could not both be followed |
| `timeout --kill-after` is nonportable | False premise | Ran `timeout --version` and the flag itself; GNU coreutils 9.11, works. macOS ships no `timeout` at all, so every call site already required coreutils |

Note the second one: Qodo found a real contradiction, and chasing it surfaced a *further* defect it had not seen - the installation probe scanned only closed PRs, so it would have reported "no bots installed" while Qodo was actively reviewing. Treat a confirmed finding as a thread to pull, not a checkbox.

When a premise is wrong, decline with the evidence that disproves it - command output, `file:line`, or a config value. "Works on my machine" is not evidence; the pasted `timeout --version` output is.

### Workflow for Qodo feedback

```
┌─────────────────────────────────────────────────────────────────┐
│                   qodo-code-review Cycle                        │
├─────────────────────────────────────────────────────────────────┤
│  1. Wait for the review, not just the summary comment           │
│  2. Fetch inline findings; read the badge for severity          │
│  3. VERIFY each claim against the code before acting            │
│  4. Fix what is real; gather evidence for what is not           │
│  5. Commit and push                                             │
│  6. Reply to every finding (@qodo only where you want an answer)│
│  7. Post /agentic_review - it does NOT re-review on push        │
│  8. Poll for the pass: review object OR completion comment      │
└─────────────────────────────────────────────────────────────────┘
```

Steps 7 and 8 are the ones people skip. See Step 9.5 for the verified trigger configuration and the polling snippet.

### Asking Qodo a question

Qodo answers only when addressed. Mention `@qodo` (or `/qodo`, or a bare `qodo`) every time you want a response, including follow-ups in a thread it is already part of. A reply without a mention is recorded but never read.

```bash
gh api repos/{owner}/{repo}/pulls/[number]/comments/[comment_id]/replies \
  -X POST -f body="@qodo This assumes GNU coreutils is absent, but macOS ships no timeout at all - every call site here already depends on it. Does that change the finding?"
```

Use this for disputed findings. For a plain "fixed in SHA", skip the mention: the re-review is what confirms the fix.

### Qodo Summary Display

```
========================================
QODO CODE REVIEW: CLO-XX
========================================

PR #[number]: [title]
Review commit: [sha]  (MUST equal current head)

Findings: 3  (Bugs 3 | Rule violations 0 | Skill insights 0)

ACTION REQUIRED:
1. [ID: 3699023245] .lok/workflows/pre-pr-validation.toml:117
   "Nonportable timeout kill-after"
   Verified: NO - GNU coreutils 9.11 present, flag works
   Status: Decline with evidence

2. [ID: 3699023248] .pi/skills/pr-review-cycle.md:45
   "Contradictory step ordering"
   Verified: YES - step 1 and step 2 conflict
   Status: Needs fix

REVIEW RECOMMENDED:
3. [ID: 3699023250] .pi/agents/README.md:45
   "Model prefix docs mismatch"
   Verified: YES - produces google/google/...
   Status: Needs fix

---

Options:
1. [address-all] - Fix verified findings, draft declines for the rest
2. [address-high] - Action-required only
3. [details ID]  - Show full finding including Agent Prompt
4. [skip]        - Skip for now

Your choice:
```

### After Addressing All Feedback

```
========================================
QODO FEEDBACK ADDRESSED
========================================

PR #[number]: [title]

Verified real: 2/3   Declined with evidence: 1/3
Commit: 1d0f984

Replies Posted: 3/3
- 3699023250: fixed
- 3699023248: fixed
- 3699023245: declined (evidence: timeout --version output)

Re-validation: /agentic_review posted
Re-review observed covering head 1d0f984 (review object or completion comment): [yes/no]

Next steps:
1. Confirm the re-review covered the current head SHA (either shape)
2. Address any new findings: /pr:review CLO-XX
3. After approval: merge or continue workflow
```
## AI Code Review: copilot-pull-request-reviewer

Copilot is **not currently installed on this repo** - `qodo-code-review` is the active reviewer. This section applies only if Copilot is added later. When configured, it leaves inline code suggestions but carries no severity signal.

### Fetching copilot Comments

```bash
gh api repos/{owner}/{repo}/pulls/[number]/comments --paginate \
  --jq '.[] | select(.user.login == "copilot-pull-request-reviewer") | {id, path, line, body}'
```

### Key Differences from qodo-code-review

| Aspect | qodo-code-review | copilot-pull-request-reviewer |
|--------|------------------|-------------------------------|
| Severity signal | Badge alt text (`Action required` / `Review recommended`) | None - treat all as medium |
| Re-validation trigger | `/agentic_review` as a PR comment | None needed - auto re-reviews on push |
| Re-review on push | No (`handle_push_trigger = False`) | Yes |
| Suggestion format | Badge + category tags + Agent Prompt block | Markdown, often with code blocks |
| Reply format | Mention `@qodo` only when you want an answer | Standard reply, no mention needed |

### Handling copilot Feedback

1. **Fetch comments** filtered by `copilot-pull-request-reviewer`
2. **Treat all as medium priority** (no severity parsing needed)
3. **Verify the claim** before acting, same as any bot finding
4. **Address issues** the same way as other review comments
5. **Reply to each comment** with fix details (no trigger suffix needed)
6. Copilot automatically re-reviews when new commits are pushed - unlike Qodo, no explicit request is required

---

## Integration Notes

**Called by**: `/task:orchestrate` during PR phase

**Follows**: `/pr:create`

**Precedes**: Merge (via orchestrator or manual)

**Updates**:
- Code files (to address feedback)
- Git repository (commits)
- PR comments (replies)
- Linear task (status update)
- Workflow state file
