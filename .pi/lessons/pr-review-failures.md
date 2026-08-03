# Lessons: PR review failures

Durable rules extracted from PR-review incidents on this repo. Phase
scripts cite this file by name rather than restating the rationale.

These rules are non-negotiable inside the `pr` phase. Loosening them
requires landing a new incident report here first.

---

## L1 - "No CI configured" is not equivalent to "no bot reviewers"

**Source incident:** PR #4 (CLO-332) merged with 7 unaddressed inline
comments after the orchestrator concluded "No CI or bot reviewers
configured" within seconds of PR creation. Gemini-code-assist had in
fact posted 7 comments roughly 90 seconds later.

**Rule:** CI presence and bot-reviewer presence are independent facts
and must be verified independently. Inferring one from the other - in
either direction - is a fatal class of error.

**How to apply:**
- Bot-reviewer presence is verified by `pr-review-cycle` step 1a:
  the last 10 PRs in any state, plus the current PR's own issue
  comments, scanned for `qodo-code-review|copilot-pull-request-reviewer`.
  (`gemini-code-assist` was dropped on 2026-08-02 - the app is sunset;
  see L3.)
- **`qodo-code-review` has been installed on this repo since
  2026-08-02.** The absence branch of step 1a is therefore not the
  expected path here; a run that takes it should be treated as a
  probe failure worth investigating, not as a clean skip.
- **A closed-PR-only scan is not sufficient.** It was the original form
  of this check and it fails for a newly installed bot, which by
  definition has no history. `qodo-code-review` was installed
  2026-08-02 and first reviewed PR #71; the old scan would have
  reported "not installed" while Qodo was actively reviewing that very
  PR. Include open PRs and the current PR's comments - Qodo posts its
  summary as an issue comment about a minute before the review lands,
  which is the earliest observable proof of installation.
- "No CI configured" by itself is never a valid rationale for skipping
  the review-fetch step. If CI is absent but bots are installed, bots
  still need to be waited for.

---

## L2 - 180 seconds minimum wait before any "no comments" conclusion

**Source incident:** Same PR #4 above. The decision to skip review
processing was logged before bots had a chance to post.

**Rule:** No "no inline comments" rationale may be recorded sooner than
180 seconds after PR creation. Gemini and Copilot routinely post in the
60-180s window; an empty fetch before then is meaningless.

**How to apply:**
- Superseded in mechanism by L6: the wait is no longer a clock floor at
  all. `pr-review-cycle` step 1b polls for an *observed* review on the
  current head SHA, which is strictly stronger than any elapsed-time
  minimum - a bot review cannot be observed before it exists.
- What survives is the prohibition: an empty comment fetch taken
  moments after PR creation is not evidence of anything, and must never
  be recorded as "no comments".

---

## L3 - ~~Every author reply ends with a re-review trailer~~ (SUPERSEDED 2026-08-02)

> **SUPERSEDED.** The reviewer app that consumed the trailer is sunset
> and has ceased all review activity - on this repo its last review was
> PR #55 (2026-05-26); PRs #58+ have none. The trailer now triggers
> nothing, so the rule and its regex gate were removed from
> `skills/pr-review-cycle.md` and `phases/pr.md`. The incident below is
> kept because the underlying failure mode still exists; only the remedy
> changed.

**Source incident:** Pre-CLO-332, replies to Copilot and human
reviewers were not re-validated by Gemini, leaving threads in
ambiguous resolved-but-unvalidated state. Several merged with
unaddressed concerns the author had implicitly declined.

**What still applies:** the risk is a thread resolved without its
rationale being recorded anywhere. With no universal validator left,
the author is the closer, so the rationale has to be written down
rather than delegated to a bot re-read.

**How to apply now:**
- Every reply states the fix commit, or the explicit rationale for
  declining. "Intentionally kept as-is: `<rationale>`" is still the
  required shape for a declined suggestion - a bare resolve is not.
- Resolve the thread yourself after replying. Exception: a human
  `CHANGES_REQUESTED` thread is left for that human to resolve.
- Automated review moved earlier: the `implement.md` Step 4 gate
  (Codex + synthesis, with a Claude fallback) runs before the PR exists
  and is now the automated review of record. A quiet PR is not an
  unreviewed one - confirm that gate ran.
- A reply is not a re-review request under any reviewer. See L7 for the
  one command that is.

---

## L4 - Re-fetch comments immediately before merge

**Source incident:** Late bot comments posted between the last
addressing pass and the merge button. Without a final re-fetch they
were missed.

**Rule:** Before merging, re-run the comment fetch one last time. If
new unresolved threads appeared since the last addressing pass, return
to the addressing loop. The merge button is the final gate.

**How to apply:**
- `pr.md` §5.0 (mandatory pre-merge re-fetch) hits the same
  `gh api repos/.../pulls/<n>/comments --paginate` call used in
  `pr-review-cycle` step 3, and gates the merge command on a clean
  result. It also compares `phases.pr.bot_rereview_head_sha` against the
  head being merged, so a skipped step 8 fails the gate instead of
  passing silently.

---

## L5 - Stale comments are not auto-skipped

**Rule:** When the code under a comment has changed since the comment
was posted (line within 5 of `original_line` modified in
`<original_commit_id>..HEAD`), flag the comment as `[STALE?]` and
confirm with the user before acting. Do not silently drop it - the user
may want the stale feedback addressed against the new code shape.

**How to apply:** `pr-review-cycle` step 5 contains the `git diff` check
and gating language.

---

## L6 - Bot-review completion is an observed review, not a timer

**Source incident:** PR #24 (CLO-382) was merged at 07:07:48Z, 261s
after PR creation. Gemini posted its review at 07:08:16Z and the
blocking inline comment at 07:08:17Z. A 180s minimum wait would not
have caught this; merging before observing the bot review missed a
high-priority inline comment.

**Rule:** For repos with installed bot reviewers, the PR phase must
wait for a bot review on the current head commit (`pull_request.head.sha`)
before concluding review fetching. A timeout is not proof that review is
clean. If installed bots do not produce a current-head review by the
10-minute deadline, block for user guidance rather than recording
`reviews_addressed: true`.

**How to apply:** `pr-review-cycle.md` step 1 polls
`repos/<owner>/<repo>/pulls/<n>/reviews` for any installed bot on the
current head SHA, then fetches inline comments and GraphQL review
threads. Step 1a runs the installation probe first so that a repo with
no bots skips the poll instead of stalling ten minutes; step 2 blocks
only when a bot IS installed and did not deliver.

The current-head requirement is what makes this rule bite, and it is
also what makes L7 necessary: on a repo whose bot does not re-review on
push, "wait for a review on the current head" is unsatisfiable by
waiting alone.

---

## L7 - A bot that does not re-review on push turns every wait into a deadlock

**Source incident:** CLO-625 / PR #73 ran five Qodo passes, each one
requiring an explicit request. Auditing the procedure afterwards
surfaced the latent case nobody had hit yet: `pr.md` Step 3 waits for CI
*before* the review cycle starts, and a CI fix pushed there moves the
head past the commit Qodo reviewed at PR open. The wait loop then polls
for a current-head review that cannot arrive, times out, and blocks the
task for user guidance - with nothing actually wrong.

**Rule:** Wherever a procedure waits for a bot review on the current
head, it must be able to *ask* for one. A wait with no request path is a
deadlock on any reviewer whose `handle_push_trigger` is `False` -
Qodo's documented default. Verified on this repo by posting `/config`
to PR #71 on 2026-08-02:

```
config.handle_push_trigger  = False
config.pr_commands          = ['/agentic_describe', '/agentic_review']
config.push_commands        = ['/agentic_review']
```

`pr_commands` fires on PR open; `push_commands` never fires because the
trigger is off. `/agentic_review` is the configured request command -
`/review` is the legacy PR-Agent name and is not wired up here.

**How to apply:**
- `pr-review-cycle` step 1b posts `/agentic_review` and re-polls when
  the first wait times out and Qodo is installed. Step 8 does the same
  after the fix commits. Both use one `wait_for_bot_review` helper.
- The poll matches on head SHA **and** `submitted_at >= REQUESTED_AT`.
  SHA alone would accept a previous run's review on the same commit and
  report a re-validation that never happened.
- Take `REQUESTED_AT` from the POST response, never from local `date` -
  `submitted_at` comes from GitHub's clock, and a fast local clock
  filters out the very review being waited on.
- Do **not** fix this by enabling `handle_push_trigger`. It would fire a
  full review on every intermediate commit with a 60s dedup TTL. One
  explicit request at a known point is cheaper and gives a deterministic
  signal to poll for.
- Record the outcome: `phases.pr.bot_rereview_head_sha` /
  `bot_rereview_at` exist so the pre-merge gate can prove the bot
  reviewed the commit being merged. Procedure prose is not a gate.

---

## L8 - Qodo only reads a reply that mentions it

**Source incident:** PR #71, where a finding was declined in a thread
reply and no response ever came. Qodo routes on mentions: `@qodo` (or a
bare `qodo`) is what makes a comment reach the bot, every time,
including follow-ups in a thread it is already part of. A plain reply is
recorded on the thread and never read.

**Rule:** Mention `@qodo` in exactly the replies you want an answer to -
a declined finding, or a question about its reasoning. Omit it for
"fixed in SHA"; the re-review is what confirms a fix, not a thread
reply.

**How to apply:**
- `pr-review-cycle` step 7 carries both reply templates. The declined
  one is `@qodo Keeping as-is. <evidence>`.
- **Declines carry evidence, not assertion.** Qodo reasons from the diff
  without executing anything, so its premises are the thing to check
  first. On PR #71 the `timeout --kill-after` finding was declined with
  pasted `timeout --version` output; two of its other three findings
  were real, and chasing one of those surfaced a further defect it had
  not seen. Verify before acting, in both directions.
