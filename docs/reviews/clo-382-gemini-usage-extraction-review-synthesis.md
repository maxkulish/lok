# Review Synthesis: clo-382-gemini-usage-extraction

**Synthesized**: 2026-05-20
**Pipeline**: lok design-review
**Reviewers**: Gemini 3.1 Pro, Codex/Ollama (glm-5:cloud), Claude (fallback)

---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | REVIEW_FAILED | Gemini CLI unreachable (pre-flight health check failed) |
| Ollama/Codex | REVIEW_FAILED | lok workflow template error (`{{ steps.health_check.output }}` variable resolution failed) |
| Claude (fallback) | OK | Produced full review |

## Single Review — Source: Claude (fallback)

Since only one review (Claude fallback) succeeded, the synthesis follows the **Single Review** format.

## Key Findings

| # | Finding | Severity |
|---|---------|----------|
| 1 | Design is architecturally sound: private envelope types, zero public API change, text-mode fallback preserves backward compat | Low |
| 2 | Fixture capture is the #1 schedule risk — `GeminiStats` field names are unverified against real Gemini CLI output | Medium |
| 3 | Partial-stats behaviour (missing `promptTokenCount` or `candidatesTokenCount`) should return `None` — conservative and correct | Low |
| 4 | Security surface is unchanged: trusted stdout boundary, no new auth/secrets/network code | Low |
| 5 | No ADRs to violate — design aligns with existing Codex/Ollama backend patterns | Low |

## Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is approved for implementation with the noted suggestions (inline comments, optional envelope size guard, PR documentation for older CLI versions). The fixture capture risk is tracked and does not change the verdict.

## Priority Actions

1. **Capture real Gemini JSON fixture within 24h** (or proceed with documented field names and file follow-up issue).
2. Implement the design as specified — no architecture changes needed.
3. Add inline comment clarifying that error-envelope parsing is handled by the pre-existing exit-code check.
