# Spec Review: clo-637

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-08-06
**Pipeline**: lok spec-review
**Note**: The external reviewer failed; this is the fallback review

---

Review complete. I read the spec and verified every line reference and factual claim against both target files (`.claude/commands/pr/review.md`, `.pi/skills/pr-review-cycle.md`) - all of them check out. Full structured review below.

---

# Spec Review: CLO-637 - Qodo re-review poll dual-shape detection

## 1. Problem Statement

**Strong.** The spec does three things well that most specs skip:

- **It resolves a real internal contradiction with evidence.** The current Step 9.5 poll (`review.md:343-355`) waits for a review object on the new head, while line 357 of the same file states Qodo edits its comment in place. I confirmed both passages exist as described. The spec explains the contradiction with observed data (review objects appear iff the pass carries new inline findings) rather than picking a side arbitrarily.
- **It deviates from the Linear issue deliberately and defensibly.** Linear proposed detecting the comment *update*; the spec shows `updated_at` bumps outside review passes (PR #80 edited 10s after merge with no pass) and would fail open. Choosing the append-only completion comment instead is the right call, and documenting the deviation in the spec itself is exactly how a deviation should be handled.
- **The scope addition is justified, not smuggled.** Extending to `.pi/skills/pr-review-cycle.md` goes beyond the Linear issue, but the rationale (CLO-623 not landed, CLO-633's Defect 2 showed what single-file fixes of duplicated snippets cause) is concrete. I verified `wait_for_bot_review` at lines 119-134 and both call sites (157, 500-507) match the spec's description.

One evidential soft spot: the two-shape rule is induced from 6 re-review passes across 2 PRs over roughly 2 days. Small sample, but the escalation clause covers the case where live verification contradicts it.

## 2. Acceptance Criteria

**Good coverage, two pinning gaps.** AC1-AC4 cover both detection paths, fail-closed absence, and the cosmetic-edit trap - each executable against recorded GitHub data. AC7 catches a genuine fail-open (an empty `REQUESTED_AT` satisfies `>=` for every string in jq, so today's snippet would match everything). AC5/AC6 make the documentation reconciliation and cross-file parity testable.

Gaps:

- **AC2 is not fully pinned**: `NEW_HEAD=1d0f984...` is truncated and `REQUESTED_AT=<the 12:50 request>` is a placeholder; eval row 2 says "recover head/request from PR #71 comment log" at verification time. **AC4** likewise uses a truncated `0903322...` while Path B matches on the *full* SHA substring. For an AFK-labeled task, the executor should not be reconstructing evidence mid-run.
- AC5's automated check (`rg -c "poll for a review"`) is a brittle phrase proxy; the spec correctly pairs it with a manual read, so this is minor - just keep the manual read authoritative.

## 3. Constraints & Assumptions

**The constraint set is the spec's best section.** Fail-closed as a Must, the clock-domain rule preserved, every path required to pair a freshness bound with a covered-commit check, the CLO-623 boundary respected (no new script files), and `updated_at` explicitly banned as a success signal. The Must-nots block the two tempting shortcuts (SHA-alone, timestamp-alone) that the current files already warn against.

Unstated assumptions worth surfacing:

- **Wording stability**: Path B hinges on the phrase `was updated up to the latest commit`. A Qodo product update that rewords it silently regresses every clean pass back to today's 600s timeout. It fails closed (correct), but nothing will distinguish "wording changed" from "no re-review happened" except the operator following the timeout message. The Prefer-tier "tolerant jq filter" and diagnostic timeout message mitigate this; acceptable.
- **Initial-pass shape** - see Risks below.

## 4. Decomposition / Phases

**Correct order, right granularity.** Four items with an explicit dependency rationale (wording follows the final snippet, so 2 must not precede 1; 3 mirrors 1 into the second file; 4 is evidence-only). I verified every line the reconciliation item targets: "How Qodo posts" (639-648), the workflow-cycle box step 8 (710), line 714, and the Step 12 template line 785 all exist and say what the spec claims - item 2's enumeration is exhaustive as far as I can find. The M estimate (2 files, 4 sub-tasks) is realistic.

## 5. Risks & Open Questions

1. **Clean *initial* pass is an unevidenced hole at call site 1b** (the one real gap). The completion comment was observed on re-review passes only - both PRs' *first* passes produced review objects (they had findings) and no completion comment. If a first pass ever finds nothing, the spec's own "review object iff new inline findings" rule predicts *neither* signal, and 1b's dual-shape wait still burns its full 600s before the `/agentic_review` re-request recovers it (that pass does emit the comment). Not a blocker - behavior is no worse than today and it self-heals - but the spec asserts "both call sites must get dual-shape behaviour" without acknowledging that dual-shape may be insufficient at 1b. One sentence would close this.
2. **Verification depends on live mutable data.** The probes run against PRs #71/#80 as they exist today. The completion comments are append-only in observation, so risk is low, but snapshotting the raw probe JSON into the PR evidence would make AC1-AC4 reproducible after the fact.
3. **Small induction base** for the two-shape rule (6 passes, 2 PRs, ~2 days). Covered by the escalation clause; live verification in item 4 doubles as a re-check.
4. **Double `/agentic_review` race** - already handled in the edge-case list (`last` semantics); no action needed.

## Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec is implementable as written: evidence-based problem statement, fail-closed design, testable ACs, justified deviations from the Linear issue, and a decomposition whose line references all verify. The suggestions are pre-flight tightening, not design changes.

## Priority Actions

1. **Pin AC2 and AC4 fully** (full 40-char SHAs, exact `REQUESTED_AT` for AC2's PR #71 request) - two `gh api` calls, done before or at the start of implementation so the AFK executor never reconstructs evidence mid-run.
2. **Add one sentence on the clean-initial-pass hole at 1b** - state that the completion comment is evidenced for re-review passes only and that the re-request path is the recovery for a hypothetical signal-less clean first pass.
3. **Snapshot probe JSON into the PR evidence** when executing decomposition item 4, so verification survives live-data drift.
4. *(Minor)* Note in AC5 that the `rg` phrase check is advisory and the manual read of the three sections is the authoritative check.

Findings F5 (initial-pass hole) and follow-up T1 (pin AC2/AC4) are recorded in `.session/notes.md`.
