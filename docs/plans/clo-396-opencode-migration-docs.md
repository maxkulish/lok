# Plan: CLO-396 — FR-12c: opencode migration docs + setup guide refresh

## Context

- Design: `docs/designs/clo-396-opencode-migration-docs.md`
- Discovery: `docs/discovery/clo-396.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-396/fr-12c-opencode-migration-docs-setup-guide-refresh
- Blocked by: [CLO-394](https://linear.app/cloud-ai/issue/CLO-394/fr-12a-replace-gemini-cli-backend-with-opencode-subprocess) (must ship before docs are finalized)

## Sub-tasks

### ST1 — Update `docs/guides/lok-setup-guide.md` Gemini backend section

**Files:** `docs/guides/lok-setup-guide.md`

**Scope:**
- Replace Gemini install instructions (remove `npm install -g @google/gemini-cli`, add `brew install anomalyco/tap/opencode` + `curl -fsSL https://opencode.ai/install | bash`)
- Replace auth section (remove `GEMINI_API_KEY`/`GOOGLE_API_KEY` required, add `opencode auth login` with Google OAuth; note env vars as optional fallback)
- Replace full config reference Gemini `args` to show opencode-compatible defaults (once CLO-394 default config is finalized)
- Update backend types table: remove "npx" association from CLI backend detection
- Update Per-Step Sandbox table: replace `Gemini --approval-mode default|auto_edit|yolo` with `opencode --agent plan|build` mapping and matching `When to use` descriptions
- Update Token Usage observability matrix Gemini row: reference opencode JSONL usage fields
- Update Pattern 1 shell step: replace `gemini --model ... -y --sandbox` with opencode equivalent shell command or `backend = "gemini"` step
- Update Pattern 3 note: update "When the backend is Codex or Gemini" sandbox defaulting text
- Add "Migrating from Gemini CLI" callout covering install delta, auth delta, and sandbox delta
- Add minimum opencode version requirement to prerequisites
- Add PATH troubleshooting tip after install section
- Clean up Tips section: remove/replace gemini-cli-specific gotchas

**Acceptance:** After applying all edits, run `grep -rn '@google/gemini-cli\|npx.*gemini\|GEMINI_API_KEY.*required\|--approval-mode' docs/guides/lok-setup-guide.md` — must return zero matches in content sections (exclude legacy cross-references in code examples that intentionally show old config). Verify sandbox table shows `--agent plan|build` not `--approval-mode`.

**Estimate:** M

### ST2 — Update `README.md` prerequisites and examples

**Files:** `README.md`

**Scope:**
- Prerequisites table: Replace Gemini row with opencode showing both macOS (`brew install anomalyco/tap/opencode`) and Linux (`curl -fsSL https://opencode.ai/install | bash`) install paths. Add minimum version note. Add auth instruction (`opencode auth login`).
- `lok doctor` output example: Ensure the example reflects the current doctor output format (verify against CLO-395 final output at implementation time).
- Backend strengths table: Update Gemini row description if opencode-driven backend has different characteristics.
- Example workflow: Update any shell commands in the gemini step that reference old CLI flags.

**Acceptance:** `grep -rn '@google/gemini-cli\|npx.*@google\|GEMINI_API_KEY.*required' README.md` — returns zero matches.

**Estimate:** S

### ST3 — Cross-reference audit and cleanup

**Files:** `docs/guides/lok-setup-guide.md`, `README.md`

**Scope:**
- Full cross-reference search across entire `docs/` tree for any remaining `@google/gemini-cli`, `npx.*gemini`, or `GEMINI_API_KEY`/`GOOGLE_API_KEY` in user-facing docs
- Review for semantic consistency: ensure all references to the gemini backend's behavior are accurate for the opencode-driven path
- Verify "Migrating from Gemini CLI" callout appears in the setup guide with correct install + auth + sandbox delta

**Acceptance:** Cross-reference search returns zero matches in user-facing docs. Design docs, investigations, and specs explicitly excluded (they document the old state by design — noted in `docs/discovery/clo-396.md`).

**Estimate:** S

### ST4 — Pre-merge gate

**Files:** All modified files

**Scope:**
- No Rust code changed, so no `cargo test`/`clippy` gate needed
- Run `grep` checks from ST1-ST3 acceptance criteria as final verification
- Manual review: read through both modified docs to ensure:
  - No dangling references to removed tools/flags
  - Sandbox mapping is internally consistent across all sections
  - Config examples match CLO-394's final default config
  - Migrating callout is coherent and helpful

**Acceptance:** All grep checks pass, manual review identifies no issues.

**Estimate:** S

## Pre-merge gate

No Rust code changes — pre-merge gate is documentation review:

1. `grep -rn '@google/gemini-cli\|npx.*@google/gemini-cli' docs/guides/lok-setup-guide.md README.md` — must return zero relevant matches
2. `grep -rn '--approval-mode' docs/guides/lok-setup-guide.md` — must return zero matches (all replaced with `--agent`)
3. `grep -rn 'GEMINI_API_KEY\|GOOGLE_API_KEY' docs/guides/lok-setup-guide.md README.md | grep -i 'required\|must\|need'` — must return zero matches in instructional content (env vars mentioned as optional fallback is OK)
4. Manual review: sandbox mapping table shows all 3 opencode agent mappings, migration callout present, config examples internally consistent

## Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| CLO-394 ships with different opencode flags/args than assumed in design | Medium | Blocked-by relation ensures docs are finalized after CLO-394 merges. Verify actual opencode flags before landing CLO-396. |
| CLO-395 changes `lok doctor` output format | Medium | ST2 defers doctor output example update until CLO-395 is merged. Both tasks are in the same release window. |
| Open questions (minimum opencode version, headless auth) not resolved by implementation time | Low | Open questions are documented in the design doc. Mark as "TBD — verify at implementation" if unresolved; do not block merge on unknowns that don't affect the core doc structure. |
| Docs-only PR skipped by reviewer ("just docs") — CI-only PRs may get less scrutiny | Low | Flag in PR description that this is part of FR-12 migration series and carries real user-facing correctness. |
