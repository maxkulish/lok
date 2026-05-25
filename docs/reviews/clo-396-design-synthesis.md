# Review Synthesis: clo-396-opencode-migration-docs

**Synthesized**: 2026-05-25
**Pipeline**: lok design-review (manual bypass)
**Reviewers**: Gemini 3.5 Flash (1/1 successful)

---

**Verdict:** APPROVE_WITH_SUGGESTIONS

## Strengths

- Completeness & structure — design covers the expected surface area for FR-12c.
- Security posture — strong improvement over prior baseline (API keys → OAuth).
- Requirements alignment — clean mapping between design sections and FR-12c requirements.

## Priority Actions

### HIGH — address before merge
1. **Document headless/CI-CD authentication path.** Interactive Google OAuth covers desktop; the docs must address headless environments (SSH, CI, Docker) with either explicit fallback instructions or a clear statement that headless auth is deferred to a follow-up.

### MEDIUM — address before implementation lands
2. **Pin minimum `opencode` version.** Add the minimum supported version to prerequisites since sandbox mappings rely on specific `opencode` CLI flags.
3. **Clarify non-macOS platform support.** The setup guide currently lists `brew install anomalyco/tap/opencode` and `curl -fsSL https://opencode.ai/install | bash` — make the Linux alternative more prominent, not a footnote.

### LOW — nice-to-have
4. **PATH troubleshooting tip.** Add a brief note in the setup guide about updating shell profile after install.
