# Spec Review Synthesis: clo-204

**Synthesized**: 2026-04-04
**Pipeline**: lok spec-review

---

## Spec Review Synthesis - CLO-204 MiniJinja Integration

> **Note:** Gemini review failed (returned only system initialization messages). Synthesis based on Ollama review only.

---

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | No acceptance criterion or test for `steps.{name}.success` despite Must constraint requiring it | Ollama | P0 |
| 2 | Error handling for undefined variables unspecified - MiniJinja defaults to empty string, spec doesn't declare behavior | Ollama | P0 |
| 3 | Missing `steps.{name}.success` from Sub-task 4 description | Ollama | P0 |
| 4 | Double-expansion risk: existing `interpolate()` escapes `{{ }}` in step outputs, but spec doesn't clarify whether `TemplateContext` pre-escapes values | Ollama | P1 |
| 5 | Loop variable interface (`with_loop_vars`) not specified in Sub-task 4 despite being mentioned | Ollama | P1 |
| 6 | No test for `parsed_output: None` fallback path | Ollama | P1 |
| 7 | `args` 1-indexed as string keys needs explicit clarification for MiniJinja map representation | Ollama | P1 |
| 8 | `TemplateEngine::render` signature ambiguity - borrow vs move, statefulness, filter registration timing | Ollama | P1 |
| 9 | Thread safety of `TemplateEngine` / `LazyEnv` not addressed | Ollama | P2 |
| 10 | No test for malformed template syntax error paths | Ollama | P2 |
| 11 | `workflow.backends` capitalization rule underspecified | Ollama | P2 |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec is well-structured with accurate code references, correct dependency choice, and proper scope boundaries. The decomposition is testable and follows existing codebase patterns. However, three P0 gaps need attention before implementation begins.

## Priority Actions

| # | Action | Severity | Rationale |
|---|--------|----------|-----------|
| 1 | Add acceptance criterion + test for `{{ steps.{name}.success }}` rendering bool | P0 | Must constraint exists but nothing verifies it |
| 2 | Specify error handling: add `TemplateError` variants (`UndefinedVariable`, `ParseError`, `RenderError`) and declare behavior for undefined references | P0 | Without this, implementer must guess; wrong default breaks compatibility |
| 3 | Add `success` field to Sub-task 4 description alongside `output` and `parsed_output` | P0 | Sub-task omits a Must constraint - implementer could miss it |
| 4 | Clarify context escaping strategy - step output values with `{{ }}` must not be re-expanded | P1 | Double-expansion would silently corrupt outputs containing template-like syntax |
| 5 | Define `TemplateContext::with_loop_item(item, index)` interface in Sub-task 4 | P1 | Currently mentioned but interface is unspecified |
| 6 | Add test for `parsed_output: None` fallback to string parsing | P1 | Ensures backward compatibility with existing `interpolate_with_fields` behavior |
| 7 | Clarify `arg` key representation as string keys `"1"`, `"2"` in MiniJinja map | P1 | MiniJinja uses string keys; implicit assumption could cause runtime errors |
