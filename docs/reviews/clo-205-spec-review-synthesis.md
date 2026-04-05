# Spec Review Synthesis: clo-205

**Synthesized**: 2026-04-04
**Pipeline**: lok spec-review

---

## Synthesis: CLO-205 Edit Parser Spec Review

**Note:** Gemini review failed (no structured output returned). Synthesis based on Ollama review only.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | Auto-detection cascade error handling is ambiguous - unclear if parsers return `Result<Option<...>>` for fallthrough or use one-shot heuristics | Ollama | High |
| 2 | Newline normalization underspecified - no mention of `\r\n` or multiple trailing newlines | Ollama | High |
| 3 | No test count target unlike other specs (e.g., CLO-204 had 48 tests) | Ollama | High |
| 4 | Sub-task 5 combines two concerns (full-file parser + `EditParser::parse()` wiring) - should split | Ollama | High |
| 5 | No input size limit - regex on unbounded LLM output is a potential DoS vector | Ollama | Medium |
| 6 | `sanitize_json_strings()` and `find_closing_fence()` are re-implemented from private fns rather than made `pub(crate)` shared - creates maintenance burden | Ollama | Medium |
| 7 | No logging/observability for format detection decisions | Ollama | Medium |
| 8 | No path traversal validation on parsed `FileEdit.file` paths (deferred to CLO-210 but worth documenting) | Ollama | Low |
| 9 | `ParsedEdits` missing trait derives (`Clone`, `Debug`, `Serialize`) that `FileEdit` already has | Ollama | Low |
| 10 | No test for `\ No newline at end of file` diff marker or ambiguous format inputs | Ollama | Low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (single reviewer verdict, Gemini unavailable)

## Priority Actions

1. **Clarify auto-detection cascade** - Define whether each parser returns `Option` for fallthrough or detection is heuristic-first. This affects the core architecture.
2. **Specify newline normalization** - Change to explicit `.trim_end()` semantics or document `\r\n` handling.
3. **Add test count target** - Set minimum (e.g., 25 tests) to match project conventions.
4. **Split sub-task 5** into full-file parser implementation and `EditParser::parse()` wiring - the dependency graph already implies this separation.
5. **Add input size limit** - Reject inputs >1MB with `EditParseError::InputTooLarge` to prevent regex DoS.
6. **Consider shared utility for duplicated functions** - Move `sanitize_json_strings()` to `pub(crate)` in a shared module, or explicitly document why duplication is preferred.
7. **Add debug logging** for format detection path taken.
