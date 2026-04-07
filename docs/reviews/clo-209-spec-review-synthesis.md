# Spec Review Synthesis: clo-209

**Synthesized**: 2026-04-07
**Pipeline**: lok spec-review

---

**Note:** Gemini review failed (timeout after 120s). Synthesis is based on Ollama review only.

## Agreement (High Confidence)
*No cross-reference possible — only one reviewer succeeded.*

## Disagreement (Needs Human Decision)
*No cross-reference possible — only one reviewer succeeded.*

## Novel Insights (Single Reviewer — Ollama)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `compile_expression` does not exist on `TemplateEngine` (`src/template/mod.rs:44-67` only exposes `new()` and `render()`); sub-task 2 depends on it | Ollama | Critical |
| 2 | Error context mapping unspecified: `WorkflowError::UnknownVariable` requires `workflow`, `step`, `variable` fields, but MiniJinja errors lack workflow/step context | Ollama | Critical |
| 3 | `escape_braces`/`unescape_braces`/`LOK_OPEN_BRACE` fate is ambiguous — spec says delete "if no longer used" without verification criteria; re-expansion prevention behavior must be explicitly preserved (test exists: `test_no_reexpansion_of_braces_in_output`) | Ollama | Critical |
| 4 | Sub-task 6 has scope creep: combines loop_vars refactor + 8 new feature tests + full suite run; should split into 6a/6b | Ollama | Important |
| 5 | Missing test: mixed legacy and new syntax in same condition (e.g., `not(contains(step.field, "x")) and steps.Y.success`) | Ollama | Important |
| 6 | Missing test: condition error recovery when `eval_expr` fails (spec says return `true` but no explicit test) | Ollama | Important |
| 7 | Missing test: `parsed_output: None` with JSON-looking string fallback via `extract_json_field` | Ollama | Important |
| 8 | Test 16 (real workflow integration test) marked "optional" but should be mandatory — unit tests alone won't catch workflow execution regressions | Ollama | Important |
| 9 | `WorkflowRunner` -> `TemplateContext::new()` integration point not shown — how `args`, `config`, `context` flow through | Ollama | Important |
| 10 | `extract_json_field` helper (~line 2530) fate unclear — delete or keep? | Ollama | Minor |
| 11 | `UnknownVariable` error message format requirement (variable name must appear) not stated | Ollama | Minor |
| 12 | "Strict undefined behavior preserved" vague — what happens when `steps.X.field` referenced but `parsed_output` is `None` and string parsing fails? | Ollama | Minor |
| 13 | Missing performance acceptance criterion (MiniJinja single-pass vs 6 regex passes — "similar or better") | Ollama | Minor |
| 14 | Missing explicit prerequisite: "requires CLO-204 merged to main" | Ollama | Minor |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** (from Ollama; Gemini unavailable)

Single-reviewer confidence — recommend re-running Gemini before final sign-off on critical items.

## Priority Actions

**Critical (block implementation until resolved):**
1. Verify `TemplateEngine::compile_expression()` exists or add it as a prerequisite sub-task; check `src/template/mod.rs:44-67`.
2. Specify how `WorkflowError::UnknownVariable` is constructed from `TemplateError::UndefinedVariable` — define how workflow/step context is injected at the call site.
3. Resolve `escape_braces`/`unescape_braces`/`LOK_OPEN_BRACE` fate: either verify MiniJinja handles literal `{{ }}` natively and delete, or add explicit re-expansion-prevention acceptance criterion. Confirm `test_no_reexpansion_of_braces_in_output` still passes.

**Important (fix before implementation starts):**
4. Split sub-task 6 into 6a (`interpolate_loop_vars` refactor) and 6b (new Jinja feature tests).
5. Add mixed legacy/new syntax condition test case.
6. Add explicit condition-error-recovery test (eval_expr failure → `true`).
7. Add `parsed_output: None` + JSON-string fallback test.
8. Make Test 16 (real workflow file integration test) mandatory.
9. Document `WorkflowRunner` -> `TemplateContext::new()` integration point with field mappings.

**Minor (nice to have):**
10. Document `extract_json_field` fate (delete vs keep).
11. State `UnknownVariable` message must include variable name.
12. Clarify behavior when `steps.X.field` resolves against `None` parsed_output.
13. Add performance expectation note.
14. Add explicit CLO-204-merged prerequisite.

**Recommendation:** Re-run Gemini review to cross-validate the 3 Critical findings before treating this synthesis as final.
