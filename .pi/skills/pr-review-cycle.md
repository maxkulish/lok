---
name: pr-review-cycle
description: Bot-review wait, fetch, address, reply, re-fetch - the 9-step PR review procedure owned by the pi `pr` phase. Every gate is executed by the tested .pi/scripts/pr-review-cycle.sh, not by snippets pasted from this file. Enforces current-head bot-review completion, CI/bot independence, and one-reply-per-thread. Recognizes qodo-code-review and copilot-pull-request-reviewer, and fails fast when Qodo is billing-blocked. Qodo never re-reviews on push, so every wait ends in an explicit `/agentic_review` request when the head has moved.
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
- **Invocation from the repository or worktree root.** The script is
  addressed by its repo-relative path below; a call from a subdirectory
  fails with `not found` rather than with a verdict, and a gate that
  cannot be reached is not a gate that passed.

```bash
PR=<n>
REPO=maxkulish/lok
```

It writes `bot_review_wait_completed`, `review_addressed` and
`bot_rereview_verified` history events on success.

## How the gates run

**Every gate in this procedure is executed by
`.pi/scripts/pr-review-cycle.sh`.** The snippets this skill used to
carry were pasted into two different files, drifted apart (one waited
20s and matched only one reviewer login; the other waited 10s and
matched both), and included paths that *failed open* - an unset
`INSTALLED_BOTS` read as "no bots installed", a jq `max` over an empty
array becoming the string `null` and hiding every comment. The script is
covered by `.pi/scripts/tests/pr-review-cycle.test.sh`, so a gate cannot
change behaviour without a test changing with it.

Subcommands, all taking `--repo` and `--pr`:

| Subcommand | Answers |
|---|---|
| `probe-bots` | Which reviewer bots are installed, and is Qodo billing-blocked? |
| `wait-review` | Has a bot finished a pass on the current head since the PR opened? |
| `request-rereview` | Post `/agentic_review` and return the bound GitHub assigned it |
| `wait-rereview` | Has a fresh pass landed on the post-push head since that bound? |
| `new-comments` | Which inline comments appeared after your own last reply? |
| `unresolved-threads` | Which review threads are still unresolved? |

Every subcommand states its verdict through its exit status:

| Exit | Meaning |
|---|---|
| 0 | Condition met - the only status that may be recorded as a passed gate |
| 1 | Negative verdict: no pass before the deadline, or comments/threads found |
| 2 | Invalid or inapplicable input. Nothing was called or posted |
| 3 | Upstream failure: `gh` failed, or a response was empty, malformed, or missing a required field |
| 4 | `probe-bots` only: Qodo is billing-blocked on this PR |

Call sites take this form, and the two statements must stay separate -
`RC` is read from the assignment, never from a pipeline
(`docs/lessons/clo-625-l6`):

```bash
INSTALLED_BOTS=$(.pi/scripts/pr-review-cycle.sh probe-bots --repo "$REPO" --pr "$PR"); RC=$?
```

Nothing in this file decides anything. Where a decision remains - "the
user approved proceeding without a bot review", "reviews are back" -
it is made by the user and written down in prose and in the workflow
state, never inferred by a shell fragment.

---

## 1 - Probe for installed bots, then wait for their review

> Recognized reviewers are `qodo-code-review` (installed 2026-08-02,
> first review on PR #71) and `copilot-pull-request-reviewer` (not
> currently installed here). The former `gemini-code-assist` app is
> sunset and reviews nothing; no trigger of any kind reaches it.

Bot reviewers make a finished pass observable in one of two shapes: a
GitHub review object on the current head commit when the pass carries
new inline findings, or - Qodo only - a completion comment naming that
head when it does not (the per-pass delivery rule in 1b). Poll for
those observable signals, not merely for elapsed time or CI status.
PR #24 showed why: it was merged at +261s while the bot posted its
review and inline comment at +289s/+290s.

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
   bot review exists for the PR's current `head.sha`, proceed
   immediately to fetch inline comments and review threads.
3. **10 minutes is a hard timeout, not a success condition.** If bots
   are installed but no current-head bot review appears within the
   deadline, block for user guidance instead of silently marking
   reviews addressed.
4. **Only confirmed absence of installed bots may skip bot review.**
   Absence is confirmed by 1a below.
5. **A billing-blocked Qodo fails fast, not after 20 minutes.** When the
   workspace is out of credits, Qodo posts a notice instead of a review,
   and no amount of polling changes that. 1a detects the notice and the
   gate stops for user guidance before 1b starts.

### 1a - Probe which bots are installed

`probe-bots` scans the last 10 PRs in any state, plus the current PR's
own issue comments. Both halves matter: a newly installed bot has no
history on closed PRs, and Qodo posts its summary as an issue comment
about a minute before it submits the review. Scanning only closed PRs
would have reported "not installed" for Qodo on PR #71, its first
review.

Its stdout is the verdict and is also what step 8 needs, so keep it:

```bash
INSTALLED_BOTS=$(.pi/scripts/pr-review-cycle.sh probe-bots --repo "$REPO" --pr "$PR"); RC=$?
```

It prints `<login>[,<login>...]` or the literal `none` - never an empty
line, so an empty string can never be mistaken for "no bots".

- **`RC=4`** - Qodo is billing-blocked on this PR (`INSTALLED_BOTS` still
  names it). Go to the billing branch below.
- **`RC=3`** - the probe could not determine anything. This is a gate
  failure, not an absence: fix the `gh` problem and re-run rather than
  proceeding to step 3.
- **`RC=2`** - bad arguments. Fix the invocation.
- **`RC=0`** - proceed.
  - `INSTALLED_BOTS=none`: no reviewer bots are installed. Record the
    wait gate with the absence rationale below and go straight to
    **step 3**. Do not run 1b.
  - Otherwise a bot is installed: continue to 1b.

### Billing-blocked Qodo

Qodo posts one comment per PR opening with `<!-- qodo:billing-blocked -->`
("Qodo reviews are paused because your workspace is out of credits") and
edits that same comment when `/agentic_review` is posted, so both 600s
waits are guaranteed to time out. PR #100 lost 20 minutes that way. Stop
and ask the user; do not start a wait.

The user decides. If they approve proceeding without a bot review,
record the wait gate with that rationale and go to **step 3**; step 8
then skips its request as well:

```ts
update_workflow_state({
  task_id: "CLO-XX",
  phase: "pr",
  action: "bot_review_wait_completed",
  details: "qodo-code-review is billing-blocked on PR #<n> (<!-- qodo:billing-blocked --> comment, workspace out of credits); user approved proceeding without a bot review.",
  phase_updates: {
    bot_review_wait_completed: true,
    bot_review_wait_completed_at: "<ISO-8601>"
  }
})
```

The absence rationale, for the `INSTALLED_BOTS=none` branch:

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

The notice stays on the PR after credits are restored. If the user says
reviews are back, go straight to the `/agentic_review` request in 1b: the
PR-open review never ran, so the first wait has nothing to find.

### 1b - Poll for a current-head review

Only run when 1a found at least one installed bot and Qodo is not
billing-blocked. The first wait covers a pass since the PR opened -
`wait-review` reads both the head and the lower bound from
`pulls/<PR>` itself.

```bash
BOT_REVIEW_AT=$(.pi/scripts/pr-review-cycle.sh wait-review --repo "$REPO" --pr "$PR"); RC=$?
```

A completed pass arrives in one of **two shapes**, and the wait accepts
both. A new **review object** appears only when the pass has new inline
findings to attach (all six on PR #71 carried 1-3 inline comments). A
**clean pass** submits no review object at all - Qodo edits its
persistent "Code Review by Qodo" comment in place and announces
completion with a *new* issue comment reading
`[Code review](...) by qodo was updated up to the latest commit <sha>`
(observed on PR #80, where a reviews-endpoint-only poll ran to full
timeout while the clean pass had already landed). Each shape pairs a
freshness bound with a covered-commit check, so an older pass - on the
same commit or an old one - can never be mistaken for a fresh one. The
persistent comment's own edit time is deliberately not a condition: it
bumps mid-pass and on post-merge permalink refreshes, so it would pass
runs that never happened.

`BOT_REVIEW_AT` is the timestamp `wait-review` observed, taken from the
API response and printed on stdout.

**If that exited 1, ask for a pass before calling it a failure.** Qodo
reviews on PR open (`pr_commands`) and never on push
(`handle_push_trigger = False`). Step 3 of `phases/pr.md` waits for CI
*before* this skill runs, so any CI fix pushed there moved the head past
the commit Qodo reviewed - and the first wait was then waiting for a
review that is never coming. One explicit request is the only exit.
`request-rereview` posts it and returns the bound GitHub assigned the
request:

```bash
REQUESTED_AT=$(.pi/scripts/pr-review-cycle.sh request-rereview --repo "$REPO" --pr "$PR" --bots "$INSTALLED_BOTS"); RC=$?
```

`--bots` carries 1a's answer forward: this skill no longer passes gate
state through shell variables, and a run that skipped 1a would have to
pass `--bots` explicitly rather than silently defaulting to "no bots".

The bound comes from the POST response, never from local `date` - the
comparison on the other side is against GitHub's own clock, and a fast
local clock would exclude the very review it is waiting for. `RC=3`
means the request failed; do not invent a bound, fix the failure and
re-run. Expect 3-4 minutes.

Then wait on the requested pass, scoped to that bound and the head as of
now:

```bash
BOT_REREVIEW=$(.pi/scripts/pr-review-cycle.sh wait-rereview --repo "$REPO" --pr "$PR" --since "$REQUESTED_AT"); RC=$?
```

`BOT_REREVIEW` prints `<head_sha> <detected_at>` on success - the head
the pass covers, which step 9 records and `phases/pr.md` §5.0 re-checks
against the commit being merged.

Known limit: a clean **initial** pass may deliver neither shape - the
review object needs findings, and the completion comment is only
evidenced for *re*-review passes ("was updated"). The first wait then
times out and this explicit request is the recovery: the requested pass
is a re-review, which does announce itself. No worse than a plain
timeout, and the gate still fails closed if nothing lands.

If the requested wait also exits 1, that is a real gate failure - go to
step 2.

If a pass was observed, record the wait gate and proceed to step 3:

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

There is no snippet to run. A non-zero exit from `wait-review` or
`wait-rereview` **is** the failure - the loop that produced it runs
inside the tested script, so there is no `INSTALLED_BOTS` to be unset
and no `BOT_REVIEW_SEEN` to default to the wrong value. A resumed
session cannot reach this step with a stale verdict: it re-runs 1a and
1b, and re-running either is cheap.

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
  current-head review and the deadline path was hit.

## 3 - Fetch all inline comments and review threads

Two calls, both paginated and filtered inside the script. `unresolved-threads`
prints one compact JSON object per line for every thread still open, with
the file, line, and the *latest* comment - the comment a reply would land
on, and therefore the one that decides the action in step 7:

```bash
THREADS=$(.pi/scripts/pr-review-cycle.sh unresolved-threads --repo "$REPO" --pr "$PR"); RC=$?
```

`RC=1` means there are unresolved threads to work through (the objects are
on stdout); `RC=0` means the PR is clean at thread level and stdout is
empty; `RC=3` means the thread state could not be read - treat that as a
gate failure, never as "no unresolved threads".

And the inline comment bodies that step 4 categorizes:

```bash
NEW_COMMENTS=$(.pi/scripts/pr-review-cycle.sh new-comments --repo "$REPO" --pr "$PR"); RC=$?
```

With no `--since`, `new-comments` bounds the window at your own latest
inline comment, falling back to the PR's `created_at` when you have not
replied yet - so on a first pass this is every inline comment on the PR.
It prints the bound it used to stderr, and exits 1 when anything is in
window.

GraphQL thread state is required in addition to the comment list because
a comment can exist while its thread has already been resolved or marked
outdated. Both calls are paginated; an unpaginated read silently caps at
30 results and hides comments on large PRs, which is why the pagination
lives in the script where a test can hold it.

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

Re-fetch the thread state, because step 6's push may have changed it -
`unresolved-threads` returns the GraphQL node ids needed to resolve, plus
the latest comment per thread:

```bash
THREADS=$(.pi/scripts/pr-review-cycle.sh unresolved-threads --repo "$REPO" --pr "$PR"); RC=$?
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

You do **not** need to record a reply timestamp by hand. `new-comments`
in step 8 recomputes the window from your latest inline comment on the
PR, taking it from GitHub rather than from local `date` - it is compared
against the creation time of other comments, and mixing clock domains
lets a fast local clock hide comments that arrived just after your
replies.

## 8 - Re-check for new comments

**Request the re-review first.** Qodo does not re-review on push - its
`handle_push_trigger` is `False`, verified by posting `/config` to
PR #71 on 2026-08-02 and consistent with Qodo's documented default.
Without an explicit request its findings stay pinned to the pre-fix
commit and this pass sees nothing new. `/agentic_review` is the
configured command; `/review` is the legacy PR-Agent name and is not
wired up here.

**Skip this whole request-and-poll when 1a found no installed bots.**
There is nothing to ask and nothing to wait for; posting the request into
a repo with no Qodo app just leaves a stray comment and then fails the
gate ten minutes later. Jump to the new-comment check at the end of this
step. The same applies when 1a found Qodo billing-blocked and the user
approved proceeding: the request would only refresh the billing notice.

The command refuses that call for you: `--bots` without
`qodo-code-review` (including `none`) exits 2 and posts nothing, so a
session that lost 1a's answer stops rather than leaving the stray
comment. Pass `--bots "$INSTALLED_BOTS"` from 1a.

```bash
REQUESTED_AT=$(.pi/scripts/pr-review-cycle.sh request-rereview --repo "$REPO" --pr "$PR" --bots "$INSTALLED_BOTS"); RC=$?

BOT_REREVIEW=$(.pi/scripts/pr-review-cycle.sh wait-rereview --repo "$REPO" --pr "$PR" --since "$REQUESTED_AT"); RC=$?
```

`BOT_REREVIEW` is `<head_sha> <detected_at>`. Carry both into step 9; they
are what the workflow YAML records and what the pre-merge gate in
`phases/pr.md` §5.0 re-checks against the head being merged.

Both conditions the wait enforces are load-bearing here:

- The SHA must be the **post-push** head. A review object on the old
  SHA - or a completion comment naming it - is the stale pass.
- SHA alone is not proof of a fresh pass. Re-running step 8 without an
  intervening push, or running it after the request failed, would match
  the *previous* run's pass on the same SHA and report re-validation that
  never happened. The bound is what makes the poll observe this run
  rather than any run - and it comes from the POST response, not local
  `date`, so both sides stay in GitHub's clock domain. A missing bound or
  a head that is not a 40-hex SHA, as after a failed pulls lookup, exits 2
  or fails closed rather than polling with a vacuous condition.

Then check for new comments and unresolved threads, including any human
reviewer's response:

```bash
NEW_COMMENTS=$(.pi/scripts/pr-review-cycle.sh new-comments --repo "$REPO" --pr "$PR"); RC=$?

THREADS=$(.pi/scripts/pr-review-cycle.sh unresolved-threads --repo "$REPO" --pr "$PR"); RC=$?
```

New findings arrive as new inline comments; the superseded ones remain
attached to the old commit. `new-comments` uses a strict bound - a
comment created at the same second as your own last reply is not new -
so your own reply is never reported back to you as feedback.

The bound it derives is the PR #71 defect, fixed in the script: `max`
over an empty array returns JSON `null`, which `jq -r` prints as the
literal string `null`. Left unguarded that lands in the filter as a
comparison against `"null"`, and since every ISO timestamp sorts before
`"null"` lexicographically, **every** real comment is filtered out and
this re-check reports clean while hiding all of them. The script guards
it with `// empty` and falls back to the PR's `created_at`, and a test
asserts the findings are still reported rather than only that the bound
string looks right.

Three behaviours to expect from Qodo, all observed on PR #71:

- It **edits its existing "Code Review by Qodo" issue comment in
  place**. Watching that comment for a new id will miss the re-review;
  the observable completion signals are the two shapes the wait polls -
  a review object on the head, or the completion comment naming it. Do
  not fall back to the comment's edit time: it bumps mid-pass and on
  post-merge permalink refreshes, so it passes runs that never happened.
- It posts a transient "Qodo is busy working" comment and then
  **deletes** it. A comment id that 404s on fetch is normal, not an
  error.
- The edited comment passes through **intermediate states**. During the
  third pass on PR #71 it briefly read `Bugs (0)` before settling on
  `Bugs (1)`. Never read a count while a "busy working" comment is
  present; wait for it to disappear, then read.

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
than implying a pass that never ran. A billing-blocked Qodo also records
`"none"`, and `details` must say it was billing-blocked and that the user
approved proceeding.
