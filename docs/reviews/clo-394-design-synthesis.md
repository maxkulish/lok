# Review Synthesis: CLO-394

**Synthesized**: 2026-05-24
**Pipeline**: manual fallback after `.lok/workflows/design-review.toml` exited 1 before writing review files
**Reviewers**: Gemini architecture persona (manual fallback)

---

## Reviewer Status

| Reviewer | Status | Detail |
|---|---|---|
| Gemini | OK | Manual fallback review produced structured feedback. |
| Ollama/Codex | SKIPPED | lok design-review workflow exited 1 before review files were produced. |
| Claude fallback | SKIPPED | lok design-review workflow exited 1 before fallback wrote review files. |

## Source

Gemini architecture persona (manual fallback).

## Key Findings

| # | Finding | Severity |
|---|---|---|
| 1 | Preserve legacy Gemini-envelope parsing or explicitly de-support pinned legacy `command = "npx"` configs. | Medium |
| 2 | Normalize model IDs to avoid double-prefixing provider-qualified models while prefixing bare Gemini names. | Medium |
| 3 | Capture real opencode JSON/NDJSON fixtures before writing parser structs. | High |

## Verdict

APPROVE_WITH_SUGGESTIONS

## Priority Actions

1. **Applied in design:** `parse_backend_output` now tries opencode first, then legacy Gemini envelope fallback, then raw stdout fallback.
2. **Applied in design:** model normalization behavior and tests are explicit.
3. **Carried into plan:** first implementation sub-task must capture/scrub opencode fixtures and lock parser shape from observed output.
