# Spec Review Synthesis: clo-637

**Synthesized**: 2026-08-06
**Pipeline**: lok spec-review

---

Recorded as F6. Here is the synthesis.

---

# Spec Review Synthesis: CLO-637

**Reviewer status:** Ollama leg FAILED - not a model verdict, a harness defect. The `sed` template substitutes the Linear title inside a single-quoted expression, and the apostrophe in `/pr:review's` terminated the quote, so the shell errored before the model ran (recorded as F6 in `.session/notes.md`). Synthesized from the Claude fallback alone; every finding below is single-source but was verified by that reviewer against the actual files (`​.claude/commands/pr/review.md`, `.pi/skills/pr-review-cycle.md`) with all line references confirmed.

## Findings (Single Valid Reviewer: Claude)

| # | Finding | Severity |
|---|---------|----------|
| 1 | AC2 and AC4 are not fully pinned: truncated SHAs and a placeholder `REQUESTED_AT` force the AFK executor to reconstruct evidence mid-run | Medium |
| 2 | Clean *initial* pass at call site 1b is an unevidenced hole - the spec's own "review object iff new inline findings" rule predicts neither signal there, so the dual-shape wait could still burn its full 600s (self-heals via re-request; no worse than today) | Medium |
| 3 | Verification probes run against live mutable PR data (#71, #80); probe JSON should be snapshotted into PR evidence for reproducibility | Low |
| 4 | Two-shape rule is induced from a small base (6 passes, 2 PRs, ~2 days); covered by the escalation clause and re-checked by decomposition item 4 | Low |
| 5 | Path B hinges on Qodo's exact phrase "was updated up to the latest commit"; a wording change regresses clean passes to full timeout (fails closed, correct direction; mitigated by the Prefer-tier tolerant filter) | Low |
| 6 | AC5's `rg -c "poll for a review"` check is a brittle phrase proxy; the paired manual read must stay authoritative | Minor |

Strengths the reviewer confirmed: the problem statement resolves a real documented contradiction with evidence; the deviation from the Linear issue (completion comment instead of `updated_at`) is defensible and documented; the fail-closed constraint set is solid; the scope extension to `.pi/skills/pr-review-cycle.md` is justified by CLO-633's Defect 2; the four-item decomposition order and M estimate are right, and its line-reference enumeration is exhaustive.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (sole valid reviewer's verdict; no NEEDS_REVISION present)

## Priority Actions

1. **Pin AC2/AC4 fully** before implementation starts: full 40-char SHAs and the exact `REQUESTED_AT` for PR #71's request - two `gh api` calls (tracked as T1 in session notes).
2. **Add one sentence acknowledging the clean-initial-pass hole at 1b** - completion comment is evidenced for re-review passes only; the `/agentic_review` re-request is the recovery path (F5).
3. **Snapshot probe JSON into PR evidence** during decomposition item 4 so AC1-AC4 remain verifiable after live data drifts.
4. *(Minor)* Mark AC5's `rg` check as advisory, manual read authoritative.
5. **Harness follow-up (outside this spec):** fix the Ollama review template's quoting so titles with apostrophes don't silently drop that leg (F6) - worth a small Linear issue so future syntheses get two reviewers again.
