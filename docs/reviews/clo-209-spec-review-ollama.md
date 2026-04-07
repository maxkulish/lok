# Spec Review: clo-209

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-07
**Pipeline**: lok spec-review

---

The text above contains a complete, structured specification review with all required elements: numbered sections (1-9), assessment language, a clear verdict (APPROVE_WITH_SUGGESTIONS), and actionable feedback organized by priority.

## 1. Problem Statement Assessment

**Clear and well-defined.** The spec accurately identifies:
- The 14 `LazyLock<Regex>` patterns at lines 83-151 (verified)
- The 4 functions to replace: `interpolate()` (2263-2295), `evaluate_condition()` (2304-2366), `interpolate_with_fields()` (2372-2498), `interpolate_loop_vars()` (2502-2528)
- The existing `TemplateEngine` infrastructure from CLO-204 in `src/template/mod.rs`
- The backward compatibility requirement (no workflow TOML changes)

**Matches Linear description.** The spec correctly references CLO-204 (MiniJinja integration) as the foundation.

**No unstated assumptions.** Key types are documented: `TemplateEngine`, `TemplateContext`, `TemplateError`, `WorkflowError::UnknownVariable`, `StepResult`.

---

## 2. Acceptance Criteria Review

**Strong:**
- Public signatures unchanged - preserves call sites at lines 1096, 1246, 1255, 1343
- 14 regex constants removed - grep verification possible
- Lenient condition default preserved (`true` on unparseable/error)
- New Jinja features tested (8+ new tests specified)
- 1-indexed `arg.N` semantics preserved
- `{{ workflow.backends }}` format preserved

**Gaps:**
1. **Missing: Error message format for `UnknownVariable`** - The spec says "map `TemplateError::UndefinedVariable` to `WorkflowError::UnknownVariable` with the existing error message format" but doesn't specify that the variable name must appear in the message. The existing `WorkflowError::UnknownVariable` has a specific format: `workflow: {workflow}, step: {step}, variable: {variable}`. Need to verify MiniJinja's error contains sufficient context.

2. **Missing: `escape_braces`/`unescape_braces` removal criteria** - The spec mentions deleting these helpers "if no longer used," but should specify whether this verification is required before deletion.

3. **Vague: "Strict undefined behavior is preserved"** - Need to clarify what happens when `steps.X.field` is referenced but `parsed_output` is `None` and string parsing fails. Currently returns `[field {field} not found]`. Should this be an error or preserve legacy behavior?

---

## 3. Constraints Check

**Aligned:**
- **Must**: All tests pass without test file edits (except error message assertions)
- **Must**: Public API unchanged
- **Must**: No workflow TOML modifications
- **Must-not**: Don't break backward compatibility for condition syntax
- **Prefer**: Single `TemplateEngine::render()` call over multiple passes

**Concerns:**
1. **Implicit constraint missed**: The `escape_braces`/`unescape_braces` pattern in `interpolate()` (lines ~2540-2560) prevents double-expansion of `{{ }}` in step outputs. MiniJinja handles this natively via strict undefined, but the spec should explicitly state that this behavior is preserved (test exists: `test_no_reexpansion_of_braces_in_output` in `src/template/mod.rs`).

2. **Error format alignment**: `WorkflowError::UnknownVariable` format differs from `TemplateError::UndefinedVariable`. Need explicit mapping guidance.

---

## 4. Decomposition Quality

**Well-scoped:**
- Sub-task 1 (translate_legacy_condition helper) - isolated, ~2 hours
- Sub-task 2 (compile_expression path) - depends on TemplateEngine API, ~1 hour
- Sub-tasks 3-5 - each targets one function, sequential

**Issues:**
1. **Sub-task 6 scope creep** - Combines "replace `interpolate_loop_vars()`" AND "add 8+ new tests" AND "run full test suite." Should split: 6a (loop_vars refactor), 6b (new Jinja feature tests).

2. **Missing sub-task: `escape_braces` verification** - Before deleting these helpers, should verify MiniJinja doesn't need them. Add as sub-task 3a or fold into sub-task 4.

3. **Dependency graph incomplete** - Sub-task 2 says "depends on TemplateEngine exposing `compile_expression`" but doesn't verify `TemplateEngine` currently has this. Need to check `src/template/mod.rs:44-67` confirms `compile_expression` availability or add it as a prerequisite.

---

## 5. Evaluation Coverage

**Covered:**
- All existing test patterns mapped to test commands
- New tests for filters (`default`, `trim`, `join`, `shell_escape`, `lines`, `first`, chained filters)
- Condition translation tests (contains, equals, nested not, passthrough)
- Edge cases documented (literal braces, missing env, out-of-range arg)

**Gaps:**
1. **Missing test: Mixed legacy and new syntax in same condition** - e.g., `not(contains(step.field, "x")) and steps.Y.success`. The spec should require a test for mixed syntax.

2. **Missing test: Condition error recovery** - What happens when MiniJinja `eval_expr` fails? Spec says "return `true`" but needs explicit test case.

3. **Missing test: `parsed_output: None` with JSON-looking string** - Currently `extract_json_field` handles fallback. Need test verifying `steps.X.field` works when `parsed_output` is `None` but `output` contains valid JSON.

---

## 6. Codebase Alignment

**Violations:**
1. **`compile_expression` not in `TemplateEngine`** - `src/template/mod.rs:44-67` shows `TemplateEngine` has only `new()` and `render()`. Sub-task 2 requires `compile_expression()` which doesn't exist. Need to add it to `TemplateEngine` or use `Environment::compile_expression()` directly.

2. **Error mapping not specified** - `TemplateError::UndefinedVariable` wraps `minijinja::Error`, but `WorkflowError::UnknownVariable` needs `workflow`, `step`, and `variable` fields. MiniJinja errors don't include workflow/step context. The spec needs to specify how context is passed and mapped.

**Alignment:**
- Uses existing `TemplateContext::with_loop_item()` (verified in `src/template/context.rs:92-110`)
- Uses existing `LazyEnv` for env vars (verified in `src/template/context.rs:10-23`)
- `TemplateContext::new()` already exposes `steps.X.output`, `steps.X.field`, `env.VAR`, `arg.N`, `workflow.backends`
- Filters already registered: `shell_escape`, `json_encode`, `join`, `first`, `last`, `default_val`, `trim`, `lines`

---

## 7. Blind Spots

1. **No mention of `LOK_OPEN_BRACE` placeholder** - Line ~127-134 shows `escape_braces`/`unescape_braces` use a `LOK_OPEN_BRACE` constant. Spec says delete these "if no longer used" but doesn't verify usage elsewhere.

2. **Missing: Integration test with real workflow file** - Test 16 is marked "optional" but should be mandatory. A regression in workflow execution would not be caught by unit tests alone.

3. **Missing: Performance consideration** - Regex replacement is O(n) per pass; the current code does 6 passes. MiniJinja rendering is also O(n) but with constant factors. Should note that performance should be "similar or better" as an acceptance criterion.

4. **Missing: `WorkflowRunner` state** - `WorkflowRunner` holds `args`, `config`, `context`. The spec doesn't explain how `TemplateContext::new()` gets these values from `WorkflowRunner`. Need to show the integration point.

5. **Missing: `extract_json_field` reuse** - This helper (used in `evaluate_condition` and `interpolate_with_fields`) exists at line ~2530. `TemplateContext::new()` already has this logic, but spec doesn't clarify whether `extract_json_field` is deleted or kept for other uses.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec is well-structured with accurate line references and comprehensive acceptance criteria. However, several gaps need addressing before implementation:

---

## 9. Actionable Feedback

### Critical (Must fix before implementation)

1. **Add `compile_expression` to `TemplateEngine` or document alternative** - Currently missing. Add sub-task or clarify approach.

2. **Specify error context mapping** - Add: "`WorkflowError::UnknownVariable` is constructed by catching `TemplateError::UndefinedVariable`, extracting the variable name from the MiniJinja error, and adding workflow/step context from the calling function."

3. **Clarify `escape_braces` fate** - Either: (a) verify MiniJinja handles literal `{{ }}` natively and delete `escape_braces`/`unescape_braces`/`LOK_OPEN_BRACE`, or (b) add explicit acceptance criterion for re-expansion prevention.

### Important (Should fix)

4. **Split sub-task 6** - Separate loop_vars refactor (6a) from new feature tests (6b). Each should be independently verifiable.

5. **Add mixed-syntax condition test** - Add test case: `not(contains(step.field, "x")) and steps.Y.success` to verify translator handles mixed legacy/new syntax.

6. **Make workflow integration test mandatory** - Remove "(optional)" from Test 16. A real workflow file test catches regressions unit tests miss.

### Minor (Nice to have)

7. **Add performance note** - State that "rendering performance should be similar or better than regex approach; MiniJinja single-pass rendering vs 6 regex passes."

8. **Document `extract_json_field` fate** - Clarify whether this helper is deleted or kept for other call sites.

9. **Add sub-task dependency on CLO-204 completion** - The spec references CLO-204 infrastructure but should explicitly state "requires CLO-204 merged to `main`."
