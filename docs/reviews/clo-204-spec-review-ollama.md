# Spec Review: clo-204

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-04
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Strong**: The problem statement is well-defined and accurately describes the current state. The line references (workflow.rs:82-151, 2263, 2372, 2502) are accurate - I verified all regex patterns and interpolation functions exist at those locations. The limitation of "no filters, no conditionals, no extensibility" is correctly identified. The separation from CLO-209 (regex replacement) is clear.

**Minor Gap**: The spec references `StepResult` fields including `parsed_output: Option<Value>`, but doesn't mention that `parsed_output` is only populated when `output_format` is JSON - this matters for `steps.{name}.{field}` resolution.

## 2. Acceptance Criteria Review

**Strong**:
- Dependency addition is explicit and correctly specifies version constraint (`minijinja = "2"`)
- Module structure follows existing patterns (`src/template/mod.rs`, `src/template/context.rs`, `src/template/filters.rs`)
- Filter naming avoids collisions with MiniJinja builtins (uses `json_encode` not `json`, `default_val` not `default`)
- Lazy env lookup via Object trait is correctly specified
- Context paths match existing regex patterns (verified: `steps.{name}.output`, `env.{VAR}`, `arg.{N}`, etc.)

**Gaps**:
1. **Missing acceptance criterion for `steps.{name}.success`** - The Must constraint states "Expose `StepResult.success` as `steps.{name}.success` (bool)" but no acceptance criterion or test verifies this.
2. **No criterion for error handling behavior** - What happens when a template references `{{ steps.nonexistent.output }}`? MiniJinja's default behavior (render as empty? error?) should be specified.
3. **No criterion for `shell_escape` edge cases** - Test #1 covers escaping, but no criterion mandates handling null bytes, newlines, and unicode explicitly listed in Must constraints.
4. **No criterion for thread safety of `LazyEnv`** - Environment variables are process-global; `LazyEnv::get_value()` reads from `std::env::var()`. No criterion addresses whether `TemplateEngine` needs to be thread-safe.

## 3. Constraints Check

**Aligned**:
- `minijinja = "2"` - correct choice (Object trait API changed in 2.0)
- Filter naming avoids builtin collisions - MiniJinja has `tojson` and `default` builtins; spec correctly uses `json_encode` and `default_val`
- `thiserror` for errors - consistent with `BackendError` and `WorkflowError` patterns
- No `unsafe` code - follows project patterns
- No modification to `workflow.rs` - correct scope boundary

**Concerns**:
1. **"Context must produce the same variable paths" is imprecise** - The current regex system escapes `{{ }}` in step outputs (line 2281: `escape_braces(&r.output)`). MiniJinja by default would re-expand `{{ }}` in substituted values. The spec should clarify whether `TemplateContext` should pre-escape `steps.{name}.output` values to prevent double-expansion.
   
2. **Escalation condition for Object trait may not be valid** - MiniJinja 2's Object trait supports custom `get_value()` without enumeration. The escalation "MiniJinja 2.x Object trait API doesn't support lazy attribute enumeration" seems incorrect for the use case.

3. **"args are 1-indexed as string keys" needs clarification** - The existing regex (`arg\.(\d+)`) captures digits, and usage likely expects `{{ arg.1 }}` for the first argument. But MiniJinja uses string keys for maps, so `arg` would need to be an object with string keys `"1"`, `"2"`, etc. This should be explicit.

## 4. Decomposition Quality

**Well-scoped**:
- Sub-task 1 (skeleton) is appropriately minimal
- Sub-task 2 (filters) is independent and testable in isolation
- Sub-task 5 (TemplateEngine + integration tests) correctly depends on 3 and 4

**Issues**:
1. **Sub-task 4 (TemplateContext) missing `steps.{name}.success`** - The Must constraint requires exposing `success` but Sub-task 4's description doesn't mention it. The context builder input includes `&HashMap<String, StepResult>` which has `.success`, but the sub-task should explicitly list all fields being exposed.

2. **Sub-task 5 should include error handling test scenarios** - Integration tests cover mixed variables and filters, but malformed templates (`{{ steps..output }}`, unclosed braces `{{ steps.x`) should have explicit test cases.

3. **Sub-task 4 scope for loop context** - The description mentions "optionally `item`/`index` for loop contexts" but doesn't specify how `TemplateContext` receives these. Is there a `with_loop_vars(item: Value, index: usize)` method? The interface should be explicit.

## 5. Evaluation Coverage

**Covered**:
- All filter functions have unit tests
- Core context building scenarios (step output, field access, env lookup, arg access, workflow.backends, loop vars)
- Mixed template rendering

**Gaps**:
1. **Missing test for `steps.{name}.success`** - No test case verifies that `{{ steps.fetch.success }}` renders `true` or `false` for a successful/failed step.

2. **Missing test for step without `parsed_output`** - When `parsed_output: None`, accessing `{{ steps.fetch.field }}` should fall back to string parsing (matching existing `interpolate_with_fields` behavior). Test #4 doesn't cover this fallback.

3. **Missing test for malformed template syntax** - What happens with `{{ steps.x` (unclosed), `{{ steps..output }}` (empty name), or `{{ unknown.VAR }}`? MiniJinja will error - should the test verify error type/message?

4. **Missing test for concurrent `TemplateEngine` use** - If `TemplateEngine::render()` is called concurrently (from multiple threads), is it safe? `LazyEnv` reads `std::env::var()` which is thread-safe, but this should be verified.

5. **Edge case "arg.0" needs test for error path, not just undefined** - Test case says "returns undefined" but the acceptance criteria say nothing about error handling behavior.

## 6. Codebase Alignment

**Violations**: None found. The spec correctly follows established patterns:

- `src/template/mod.rs` mirrors `src/backend/mod.rs` structure
- `thiserror::Error` for `TemplateError` mirrors `BackendError` and `WorkflowError`
- Doc comments on all `pub` items is consistent with codebase style
- Unit tests in same file (or `#[cfg(test)]` module) follows existing pattern

**Alignment**:
- Filter naming convention follows the pattern of avoiding collisions while remaining idiomatic
- Context building from `StepResult` follows the existing data flow pattern
- Module is standalone infrastructure (no execution path integration) - correctly scoped

**Note**: The spec says "Return `thiserror` errors, not `anyhow` - consistent with `BackendError` pattern (`src/workflow.rs`)" - but I observe that `WorkflowError` (line 23-61) uses `thiserror::Error`, and `BackendError` (src/backend/mod.rs:28) also uses `thiserror::Error`. This is correct.

## 7. Blind Spots

1. **Error handling for undefined variables** - MiniJinja by default renders undefined variables as empty string or can be configured to error. The spec should specify which behavior and whether `TemplateError::UndefinedVariable` should be an explicit error variant.

2. **Context value escaping** - The existing `interpolate()` function escapes `{{` and `}}` in step outputs (line ~2281) to prevent double-expansion. The spec must clarify whether `TemplateContext` should pre-escape string values, or whether MiniJinja's auto-escaping handles this.

3. **`TemplateEngine::render` signature ambiguity** - The signature shows `render(template_str, &TemplateContext) -> Result<String>` but doesn't specify:
   - Is `TemplateContext` borrowed or moved?
   - Is `TemplateEngine` stateless (can be created once and reused)?
   - Should there be a `TemplateEngine::new()` that registers filters, or is that per-render?

4. **Test file location** - The spec says "Unit tests for each filter" but doesn't specify whether tests go in `src/template/filters.rs` with `#[cfg(test)]` module or in a separate `tests/` file. The codebase uses inline `#[cfg(test)]` modules.

5. **`workflow.backends` format** - Test #8 expects "Capitalized, deduplicated, ' + '-joined". The spec should clarify what "Capitalized" means for backend names like "claude" -> "Claude"? What about "bedrock" -> "Bedrock"? Is this `str::to_uppercase()` on first char?

6. **Integration with existing `interpolate_loop_vars`** - The spec doesn't mention how `TemplateContext` will handle `for_each` loop variables (`item`, `index`). These are currently handled separately in `interpolate_loop_vars()`. Is this in scope for the context builder?

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, accurately references existing code, and correctly scopes the work as infrastructure-only. The decomposition is reasonable and testable. However, several gaps should be addressed before implementation:

1. Add acceptance criterion and test for `steps.{name}.success`
2. Clarify error handling for undefined variables and malformed templates
3. Clarify whether step output values need escaping to prevent double-expansion
4. Specify `TemplateContext` interface for loop variables (`with_loop_vars` method?)
5. Add test for `parsed_output: None` fallback path

## 9. Actionable Feedback

**P0 - Must fix before implementation:**

1. **Add acceptance criterion for `steps.{name}.success`**:
   - Add: "- [ ] `{{ steps.fetch.success }}` renders `true` for successful step, `false` for failed step"
   - Add Test #12: `{{ steps.fetch.success }}` returns bool value

2. **Specify error handling behavior**:
   - Add `TemplateError` variants: `UndefinedVariable { name: String }`, `RenderError { message: String }`, `ParseError { message: String }`
   - Add acceptance criterion: "- [ ] Undefined variable `{{ steps.nonexistent.output }}` returns `TemplateError::UndefinedVariable`"
   - Clarify in constraints: "Must return error for undefined variables (MiniJinja default is to render empty)"

3. **Add `steps.{name}.success` to Sub-task 4 description**:
   - Update: "Populates `steps.*` (output, success, field access via parsed_output), `env` (LazyEnv), ..."

**P1 - Should fix:**

4. **Clarify context escaping strategy**:
   - Add Must constraint: "Step output values containing `{{ }}` must NOT be re-expanded by MiniJinja. Context must store values as MiniJinja `Value::String` without nested template markers."

5. **Specify loop variable interface**:
   - Add to Sub-task 4: "`TemplateContext::with_loop_item(item: Value, index: usize) -> Self` for for_each contexts. When set, `{{ item }}`, `{{ item.field }}`, `{{ index }}` resolve to passed values."

6. **Add test for `parsed_output: None` fallback**:
   - Add Test #13: "`{{ steps.fetch.field }}` where `parsed_output: None` falls back to JSON string parsing (matches existing `interpolate_with_fields` behavior)"

**P2 - Nice to have:**

7. **Specify thread safety** - Add note: "`TemplateEngine` is stateless and can be reused. `LazyEnv::get_value()` reads from `std::env::var()` which is thread-safe."

8. **Add test for malformed template** - Add Test #14: "Malformed template `{{ steps.x` returns `TemplateError::ParseError`"

9. **Clarify `workflow.backends` capitalization** - Add note: "Capitalize first letter only: `claude` -> `Claude`, `bedrock` -> `Bedrock`"
