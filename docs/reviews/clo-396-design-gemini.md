# Design Review: clo-396-opencode-migration-docs

**Reviewer**: Gemini 3.5 Flash
**Reviewed**: 2026-05-25
**Pipeline**: lok design-review (manual bypass)

---

Verdict: `APPROVE_WITH_SUGGESTIONS`

## Key Findings

1. **Completeness & Structure**: Excellent. Every section covers exactly what needs to change.
2. **Security Posture**: Strong improvement — moving from mandatory API keys to OAuth reduces token-leakage risks.
3. **Alignment**: Sandbox capabilities cleanly mapped to opencode flags, preserving the existing security model.

## Actionable Suggestions

### 1. Document headless/CI-CD authentication (HIGH)
`opencode auth login` requires a browser. Headless environments (SSH, CI, Docker) need fallback instructions. Add to the migration callout or tips section.

### 2. Specify minimum opencode version (MEDIUM)
Sandbox mappings rely on specific CLI flags. State minimum version (e.g., `opencode >= X.Y.Z`) in prerequisites.

### 3. Support for non-macOS platforms (MEDIUM)
`brew install anomalyco/tap/opencode` is macOS-specific. Add Linux/WSL installation alternatives.

### 4. PATH and shell troubleshooting (LOW)
`npx` auto-resolves, `opencode` requires `$PATH`. Add troubleshooting tip for `command not found`.
