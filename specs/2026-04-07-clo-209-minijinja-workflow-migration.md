# Spec: Replace regex interpolation in workflow.rs with MiniJinja rendering

**Created**: 2026-04-07
**Estimated scope**: M (1 file modified + small `TemplateEngine` API extension, 7 sub-tasks, target: existing tests pass + 11 new tests)
**Prerequisite**: CLO-204 must be merged to `main` (provides `TemplateEngine`, `TemplateContext`, `LazyEnv`, filters)

## 1. Problem Statement

`src/workflow.rs` interpolates templates and evaluates conditions using 14 hand-written `LazyLock<Regex>` patterns (lines 83-151) and four functions:

- `interpolate()` (`src/workflow.rs:2263-2295`) - replaces `{{ steps.X.output }}` via `INTERPOLATE_RE`
- `interpolate_with_fields()` (`src/workflow.rs:2372-2498`) - chains 6 regex passes for `steps.X.field`, `env.VAR`, `arg.N`, `workflow.backends`, plus an `UNKNOWN_VAR_RE` validation pass
- `evaluate_condition()` (`src/workflow.rs:2304-2366`) - parses 5 condition forms via separate regexes and recurses for `not(...)`
- `interpolate_loop_vars()` (`src/workflow.rs:2502-2528`) - replaces `{{ item }}`, `{{ item.field }}`, `{{ index }}` for `for_each` blocks

This approach is fragile (every new feature is another regex), can't compose (no `{% if %}` blocks, filters, or `default()`), and duplicates logic that already lives in `src/template/` (CLO-204): `TemplateEngine`, `TemplateContext`, `LazyEnv`, and 8 custom filters (`shell_escape`, `json_encode`, `join`, `first`, `last`, `default_val`, `trim`, `lines`).

This task replaces all four functions and their regex backends with a single `TemplateEngine::render()` call per interpolation site, plus a small condition translator that maps legacy condition syntax onto MiniJinja `eval_expr` for backward compatibility. Existing acceptance criteria (`{{ steps.X.output }}`, `{{ env.VAR }}`, `{{ arg.N }}`) keep working unchanged so no workflow TOML files need editing.

**Key types involved**:
- `TemplateEngine` (`src/template/mod.rs:44-67`) - `render(template, ctx) -> Result<String, TemplateError>`
- `TemplateContext` (`src/template/context.rs:32-143`) - `new(steps, args, backends)`, `with_loop_item(item, index)`, `as_value()`
- `TemplateError` (`src/template/mod.rs:9-22`) - `UndefinedVariable`, `ParseError`, `RenderError`
- `WorkflowError::UnknownVariable` (existing) - error currently raised by `interpolate_with_fields()`
- `StepResult` (`src/workflow.rs:174-184` neighborhood) - `output`, `parsed_output`, `success`, etc.
- `WorkflowRunner` (`impl WorkflowRunner` in `src/workflow.rs`) - holds `args`, `config`, `context`

**Note**: This task is a refactor. CLO-204 already provides the MiniJinja infrastructure. CLO-211 (apply-verify pipeline wiring) is unrelated.

## 2. Acceptance Criteria

- [ ] `interpolate()`, `interpolate_with_fields()`, `evaluate_condition()`, and `interpolate_loop_vars()` all delegate to `TemplateEngine` for variable substitution
- [ ] Public signatures of `interpolate()`, `interpolate_with_fields()`, and `evaluate_condition()` are unchanged (existing call sites at `src/workflow.rs:1096`, `1246`, `1255` continue to compile without edits)
- [ ] `interpolate_loop_vars()` either keeps its existing free-function signature (`fn interpolate_loop_vars(template, item, index) -> String`) OR is replaced with a private helper that produces an enriched `TemplateContext` - call site at `src/workflow.rs:1343` is the only place that needs updating
- [ ] All 14 LazyLock regex constants (lines 83-151) related to interpolation/conditions are removed - the only regexes remaining in `workflow.rs` are unrelated ones (e.g., command parsing) or none
- [ ] `escape_braces`, `unescape_braces`, and `ESCAPED_OPEN_BRACE` (`src/workflow.rs:154-167`) are deleted. MiniJinja already prevents re-expansion of literal `{{ }}` in step outputs - this is verified by the existing test `test_no_reexpansion_of_braces_in_output` (`src/template/mod.rs:104-115`).
- [ ] Backward-compatible condition syntax (translated to MiniJinja expressions before eval):
  - `contains(step.field, "string")` -> `"string" in steps.step.field`
  - `equals(step.field, "string")` -> `steps.step.field == "string"`
  - `not(<inner>)` -> `not (<translated inner>)`
  - `steps.X.output contains 'Y'` (legacy) -> `"Y" in steps.X.output`
  - `steps.X.success` -> `steps.X.success` (already valid Jinja)
- [ ] New condition syntax works directly as MiniJinja expressions: `"foo" in steps.X.output`, `steps.X.success and not steps.Y.success`, `steps.X.field == "value"`, etc.
- [ ] `evaluate_condition()` keeps its lenient default: an unparseable/erroring condition returns `true` (matches current behavior at `src/workflow.rs:2304-2366` and `test_condition_unparseable_returns_true`)
- [ ] `for_each` loop variables (`{{ item }}`, `{{ item.field }}`, `{{ index }}`) work via `TemplateContext::with_loop_item()` - the call site at `src/workflow.rs:1343` builds an enriched context per iteration
- [ ] Strict undefined behavior is preserved for prompt/shell interpolation: referencing an undefined `{{ steps.X.output }}` or `{{ steps.X.field }}` produces `WorkflowError::UnknownVariable` with workflow name, step name, AND the offending variable name in the error message. The variable name is extracted from the `minijinja::Error` (via `Error::name()` or by parsing `Display` if `name()` is unavailable for the error kind) and re-contextualized at the call site by the wrapper that constructs `WorkflowError::UnknownVariable`.
- [ ] When `steps.X.field` is referenced and the step's `parsed_output` is `None`, `TemplateContext` falls back to parsing `output` as JSON and extracting the field (existing behavior of `TemplateContext::new()`). If the fallback also fails, MiniJinja strict mode raises `UndefinedError` and the call site converts it to `WorkflowError::UnknownVariable` (matching current strict behavior of `interpolate_with_fields()`).
- [ ] Environment variable lookup uses `LazyEnv` (`src/template/context.rs:10-23`) - missing env vars produce a `WorkflowError::UnknownVariable` error (NOT the legacy `[env VAR not set]` placeholder string), matching existing strict behavior in `interpolate_with_fields()`
- [ ] `{{ workflow.backends }}` continues to render the formatted backend list (already wired through `TemplateContext`)
- [ ] `{{ arg.1 }}` continues to be 1-indexed (`arg.0` is undefined)
- [ ] All existing tests (the ones listed in Section 4 below) pass unchanged. No test edits unless the test asserts on a regex error message that no longer applies, in which case the test is updated to assert the new MiniJinja error and the change is called out in the commit message.
- [ ] New Jinja features verified by 11+ new tests:
  - `{% if cond %}...{% endif %}` blocks render correctly inside prompts
  - `{{ steps.X.output | default_val("fallback") }}` returns fallback when undefined (note: filter is `default_val`, registered in `src/template/filters.rs`)
  - `{{ steps.X.output | trim }}` strips whitespace
  - `{{ items | join(", ") }}` joins lists
  - `{{ steps.X.field | shell_escape }}` quotes safely
  - `{{ steps.X.output | lines | first }}` chains filters
  - `{% for item in items %}{{ item }}{% endfor %}` works inside a single template (loop body)
  - `{{ steps.X.field | default_val("none") }}` works for missing JSON fields
  - **Mixed legacy/new condition**: `not(contains(steps.X.field, "x")) and steps.Y.success` translates and evaluates correctly (translator is recursive into `not(...)`, the `and` clause passes through unchanged)
  - **Condition error recovery**: A condition that produces a MiniJinja eval error (e.g., `steps.nonexistent.field == "x"`) returns `true` from `evaluate_condition` (lenient default)
  - **`parsed_output: None` JSON field fallback**: A step with `parsed_output: None` and `output: r#"{"verdict":"PASS"}"#` allows `{{ steps.X.verdict }}` to render `"PASS"`
- [ ] `cargo test -p lokomotiv` passes
- [ ] `cargo clippy -p lokomotiv -- -D warnings` clean

**Verification method**:
```bash
cargo test -p lokomotiv 2>&1 | tee /tmp/clo-209-test.log
cargo clippy -p lokomotiv -- -D warnings
grep -c "LazyLock" src/workflow.rs   # should be 0 or only unrelated patterns
```

## 3. Constraints

**Must**:
- Use the existing `TemplateEngine` and `TemplateContext` from `src/template/`. Do NOT reimplement template rendering.
- Build a single `TemplateContext` per call to `interpolate()`/`interpolate_with_fields()` from `(&self.args, &results, &self.workflow_backends_for_context())` - don't construct multiple contexts. The `WorkflowRunner` -> `TemplateContext::new()` integration is: `self.args` (Vec<String>) maps to `args`, the `results` parameter (HashMap<String, StepResult>) maps to `steps`, and a backends slice (built from the active backend list at the call site - check existing logic in `interpolate_with_fields()` near `WORKFLOW_BACKENDS_RE`) maps to `backends`.
- Cache or reuse a single `TemplateEngine` instance per `WorkflowRunner` as a field initialized in `WorkflowRunner::new()`. Per-call construction is forbidden (filter registration cost is non-trivial).
- **Extend `TemplateEngine` with a public `eval_expression()` method** (`src/template/mod.rs`) that wraps `Environment::compile_expression()` + `Expression::eval()`, returning `Result<bool, TemplateError>` (the condition use case only needs a bool). Signature: `pub fn eval_expression(&self, expr: &str, ctx: &TemplateContext) -> Result<bool, TemplateError>`. Add 2-3 unit tests in `src/template/mod.rs` for this method (truthy, falsy, undefined error path).
- Map `TemplateError` to `WorkflowError::UnknownVariable` so callers don't see a new error type. The `WorkflowError::UnknownVariable` message must include the workflow name, step name, and the offending variable name. The current format in `src/workflow.rs:2485-2497` should be preserved verbatim where possible. If MiniJinja's error doesn't expose the variable name cleanly via `Error::name()`, parse it from the `Display` representation as a fallback.
- Keep `evaluate_condition()`'s lenient default: any error from translation OR `eval_expression` returns `true`. This preserves the documented behavior tested by `test_condition_unparseable_returns_true`.
- Evaluate condition strings via the new `TemplateEngine::eval_expression()` method, NOT by wrapping in `{{ ... }}` template strings.
- All `pub` items keep their existing doc comments. New `pub` items (like `eval_expression`) need doc comments. New private helpers do not need doc comments unless their purpose is non-obvious.
- The condition translator must be implemented as a private free function in `src/workflow.rs` with its own unit tests.
- Before deleting the helper `extract_json_field()` (`src/workflow.rs` near line 2530), grep for remaining call sites. If the only callers were the four interpolation/condition functions being rewritten, delete it. If anything else uses it, keep it.

**Must-not**:
- Do NOT change the public signatures of `interpolate()`, `interpolate_with_fields()`, or `evaluate_condition()`. Existing call sites must compile unchanged.
- Do NOT add new dependencies. `minijinja` is already in `Cargo.toml` from CLO-204.
- Do NOT change workflow TOML semantics. Existing `.lok/workflows/*.toml` files must run unchanged.
- Do NOT use `unsafe` code.
- Do NOT add the legacy `[env VAR not set]` placeholder string. The current `interpolate_with_fields()` already errors on missing env vars via `UNKNOWN_VAR_RE` validation - we keep that strict behavior.
- Do NOT introduce backwards-compatibility shims for the regex constants themselves. Once the migration is verified, the regexes get deleted, not deprecated.
- Do NOT keep `escape_braces`/`unescape_braces`/`ESCAPED_OPEN_BRACE` (`src/workflow.rs:154-167`). MiniJinja already prevents brace re-expansion natively.
- Do NOT make `TemplateEngine.env` field public. Add a public method instead (`eval_expression`).

**Prefer**:
- Translate legacy condition syntax once at the start of `evaluate_condition()` via a `translate_legacy_condition(&str) -> Cow<str>` helper, then feed the result to `eval_expr`. This keeps the translator testable in isolation.
- When translating, prefer minimal regex use - one regex per legacy form is acceptable, but the translator should be a small focused function (target: <80 lines).
- Use `tracing::debug!` (or `eprintln!` if `tracing` isn't already used in workflow.rs) when a legacy condition syntax is translated, so users have a path to discover deprecated forms.
- Group all new helpers (translator, context builder) near the existing interpolation functions, not at the top of the file.

**Escalate when**:
- A legacy condition form in the existing test suite cannot be expressed as a MiniJinja expression (would force a behavior change beyond the spec).
- `TemplateContext` is missing a namespace that an existing test relies on (e.g., a `{{ workflow.X }}` form other than `backends`).
- Strict mode breaks more than 2-3 tests in non-trivial ways - in that case stop, ask whether to keep strict mode or relax to `UndefinedBehavior::Lenient`.

## 4. Decomposition

1. **Add `eval_expression` to `TemplateEngine`**: Add a public method `eval_expression(&self, expr: &str, ctx: &TemplateContext) -> Result<bool, TemplateError>` to `src/template/mod.rs`. Implementation wraps `self.env.compile_expression(expr)?.eval(ctx.as_value())?` and converts the resulting `Value` to `bool` via `Value::is_true()` (or equivalent). Add 3 unit tests in `src/template/mod.rs`: truthy expression, falsy expression, undefined variable error. - files: `src/template/mod.rs`

2. **Add helper: `translate_legacy_condition`**: Create a private free function `translate_legacy_condition(condition: &str) -> std::borrow::Cow<'_, str>` in `src/workflow.rs` near the existing `evaluate_condition`. Detects:
   - `contains(X.Y, "Z")` -> `"Z" in steps.X.Y`
   - `equals(X.Y, "Z")` -> `steps.X.Y == "Z"`
   - `not(<inner>)` -> `not (<recursively translated inner>)`
   - Legacy `steps.X.output contains 'Y'` -> `"Y" in steps.X.output`
   
   Returns `Cow::Borrowed` if no translation needed. Add 9 unit tests: each form individually, nested `not()`, mixed legacy + new (`not(contains(steps.X.field, "x")) and steps.Y.success`), pass-through for already-valid expressions, single quotes vs double quotes. Emit `tracing::debug!` (or `eprintln!` if tracing isn't already imported in this file) when a translation occurs. - files: `src/workflow.rs`

3. **Wire `TemplateEngine` into `WorkflowRunner`**: Add a `template_engine: TemplateEngine` field to `WorkflowRunner`, initialize in `WorkflowRunner::new()`. Add a private helper method or inline logic to compute the backends slice that `TemplateContext::new()` needs (look at the existing `WORKFLOW_BACKENDS_RE` substitution in `interpolate_with_fields()` to find where the backend list is currently built). - files: `src/workflow.rs` (struct definition + constructor)

4. **Replace `interpolate()`**: Rewrite the function body to build a `TemplateContext` from `(&self.args, results, &backends)` and call `self.template_engine.render(template, &ctx)`. Map `TemplateError::UndefinedVariable` to `WorkflowError::UnknownVariable` with the workflow + step + variable name. Delete `INTERPOLATE_RE` constant (lines 83-84). Run `cargo test -p lokomotiv -- interpolate` and verify all tests pass. - files: `src/workflow.rs:2263-2295`

5. **Replace `interpolate_with_fields()` and delete brace-escape helpers**: Rewrite `interpolate_with_fields()` to use a single `TemplateEngine::render()` call. The context already exposes `steps.X.output`, `steps.X.field`, `env.VAR`, `arg.N`, `workflow.backends`. Map `TemplateError::UndefinedVariable` to `WorkflowError::UnknownVariable` with the existing error message format from `src/workflow.rs:2485-2497`. Delete `FIELD_RE`, `ENV_RE`, `ARG_RE`, `WORKFLOW_BACKENDS_RE`, `UNKNOWN_VAR_RE` (regex constants). Delete `escape_braces`, `unescape_braces`, and `ESCAPED_OPEN_BRACE` (`src/workflow.rs:154-167`) - these are no longer needed because MiniJinja prevents re-expansion natively. Run `cargo test -p lokomotiv -- interpolate` and verify all tests pass, including the existing template-engine test `test_no_reexpansion_of_braces_in_output`. - files: `src/workflow.rs:2372-2498`, regex removals lines 110-134, helper removals lines 154-167

6. **Replace `evaluate_condition()`**: Rewrite to call `translate_legacy_condition()` first, then `self.template_engine.eval_expression(&translated, &ctx)`. On any `Err(_)` from translation OR eval, return `true` (preserving lenient default tested by `test_condition_unparseable_returns_true`). Delete `CONDITION_LEGACY_RE`, `CONDITION_CONTAINS_RE`, `CONDITION_EQUALS_RE`, `CONDITION_NOT_RE`, `CONDITION_SUCCESS_RE` (regex constants at lines 87-107, 150-151). Run `cargo test -p lokomotiv -- condition` and verify all tests pass. Investigate `extract_json_field()` callers - if only used by the now-rewritten functions, delete it. - files: `src/workflow.rs:2304-2366`

7. **Replace `interpolate_loop_vars()`**: Either (a) keep the function signature `fn interpolate_loop_vars(template: &str, item: &serde_json::Value, index: usize) -> String` but rebuild it as: build a fresh `TemplateContext` with `with_loop_item()`, render via a new local `TemplateEngine`, return the result (note: this requires constructing a new engine since the function is a free fn, not on `WorkflowRunner` - alternative is to convert it to a method). OR (b) preferred: convert to a method on `WorkflowRunner` that uses `self.template_engine` and an enriched context, then update the single call site at `src/workflow.rs:1343`. Delete `ITEM_RE`, `ITEM_FIELD_RE`, `INDEX_RE` (lines 137-146). Run `cargo test -p lokomotiv -- loop_vars` and `cargo test -p lokomotiv -- for_each` and verify all tests pass. - files: `src/workflow.rs:2502-2528`, call site `src/workflow.rs:1343`

8. **Add new Jinja feature tests + final verification**: Add the 11 new tests listed in Acceptance Criteria (filter chains, if blocks, default_val, mixed condition, error recovery, parsed_output fallback) to the existing `#[cfg(test)] mod tests` block in `src/workflow.rs`. Run full `cargo test -p lokomotiv` and `cargo clippy -p lokomotiv -- -D warnings`. Verify `grep -c "LazyLock.*Regex" src/workflow.rs` returns 0 or only documents unrelated regexes. Run a real workflow (e.g., `lok run .lok/workflows/spec-review.toml [args] --dir .`) as a smoke test to confirm end-to-end behavior is unchanged. - files: `src/workflow.rs` (test block)

**Dependency order**: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8 (strictly sequential - each step deletes regexes that the next step's tests should not depend on, and the test suite is the safety net between steps). Sub-task 1 (`eval_expression`) must come first because sub-task 6 (`evaluate_condition` rewrite) depends on it.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | All tests in `test_interpolate*` (lines 3375+) pass unchanged | Pass | `cargo test -p lokomotiv -- interpolate` |
| 2 | All `test_condition_*` tests pass (lines 3686-3755, 4384) | Pass including `test_condition_unparseable_returns_true` | `cargo test -p lokomotiv -- condition` |
| 3 | All `test_interpolate_loop_vars_*` tests pass (lines 3826-3877) | Pass | `cargo test -p lokomotiv -- loop_vars` |
| 4 | `test_extract_json_field_*` tests pass (lines 3321-3375) | Pass (these test the helper still used by TemplateContext) | `cargo test -p lokomotiv -- extract_json_field` |
| 5 | All `test_parse_for_each_*` and `test_for_each_*` tests pass | Pass | `cargo test -p lokomotiv -- for_each` |
| 6 | New: `{% if steps.fetch.success %}A{% else %}B{% endif %}` renders "A" when success | "A" rendered | `cargo test -p lokomotiv -- test_jinja_if_block` |
| 7 | New: `{{ steps.fetch.output \| default("fallback") }}` returns "fallback" when step missing | "fallback" rendered | `cargo test -p lokomotiv -- test_jinja_default_filter` |
| 8 | New: `{{ steps.fetch.output \| trim }}` strips whitespace | trimmed string | `cargo test -p lokomotiv -- test_jinja_trim_filter` |
| 9 | New: `{{ items \| join(", ") }}` joins arrays | "a, b, c" | `cargo test -p lokomotiv -- test_jinja_join_filter` |
| 10 | New: `{{ steps.fetch.field \| shell_escape }}` produces single-quoted string | `'value with spaces'` | `cargo test -p lokomotiv -- test_jinja_shell_escape_filter` |
| 11 | New: `{{ steps.fetch.output \| lines \| first }}` chains filters | first line of output | `cargo test -p lokomotiv -- test_jinja_chained_filters` |
| 12 | New: `translate_legacy_condition` rewrites `contains(step.field, "x")` to `"x" in steps.step.field` | Cow::Owned with translated expression | `cargo test -p lokomotiv -- test_translate_contains` |
| 13 | New: `translate_legacy_condition` returns Cow::Borrowed for already-valid expression | borrowed unchanged | `cargo test -p lokomotiv -- test_translate_passthrough` |
| 14 | New: `translate_legacy_condition` handles nested `not(contains(...))` | translated correctly | `cargo test -p lokomotiv -- test_translate_nested_not` |
| 15 | New: `translate_legacy_condition` handles mixed legacy/new: `not(contains(steps.X.field, "x")) and steps.Y.success` | translates the `not(contains(...))` portion only, leaves `and steps.Y.success` intact | `cargo test -p lokomotiv -- test_translate_mixed_legacy_new` |
| 16 | New: `evaluate_condition` returns `true` when MiniJinja `eval_expression` errors (e.g., `steps.nonexistent.field == "x"`) | true (lenient default preserved) | `cargo test -p lokomotiv -- test_evaluate_condition_error_recovery` |
| 17 | New: `parsed_output: None` with `output: r#"{"verdict":"PASS"}"#` allows `{{ steps.X.verdict }}` to render `"PASS"` | "PASS" | `cargo test -p lokomotiv -- test_interpolate_parsed_output_none_fallback` |
| 18 | New: `TemplateEngine::eval_expression` returns Ok(true) for truthy expression | Ok(true) | `cargo test -p lokomotiv -- test_eval_expression_truthy` |
| 19 | New: `TemplateEngine::eval_expression` returns Ok(false) for falsy expression | Ok(false) | `cargo test -p lokomotiv -- test_eval_expression_falsy` |
| 20 | New: `TemplateEngine::eval_expression` returns `Err(UndefinedVariable)` for undefined variable in expression | Err | `cargo test -p lokomotiv -- test_eval_expression_undefined` |
| 21 | `grep -c "LazyLock.*Regex" src/workflow.rs` | Returns 0 (or only unrelated regexes - verify each remaining one is documented) | `grep -c "LazyLock.*Regex" src/workflow.rs` |
| 22 | **Mandatory** workflow file integration: `.lok/workflows/spec-review.toml` runs end-to-end | Same output as before migration (smoke test) | `lok run .lok/workflows/spec-review.toml [args] --dir .` |
| 23 | `cargo clippy` clean | No warnings | `cargo clippy -p lokomotiv -- -D warnings` |
| 24 | Strict mode error: undefined `{{ steps.missing.output }}` | `WorkflowError::UnknownVariable` with workflow + step + var name | `cargo test -p lokomotiv -- test_interpolate_unknown_step` |
| 25 | The existing `test_no_reexpansion_of_braces_in_output` (in `src/template/mod.rs`) still passes (regression check after deleting `escape_braces`) | Pass | `cargo test -p lokomotiv -- test_no_reexpansion_of_braces_in_output` |

**Edge cases to verify**:
- Step output containing literal `{{` or `}}` characters renders without re-expansion (MiniJinja handles this natively - no need for `escape_braces` shim)
- `{{ env.NONEXISTENT_VAR_XYZ }}` produces `WorkflowError::UnknownVariable`, not the legacy placeholder
- `{{ arg.99 }}` (out of range) produces `WorkflowError::UnknownVariable`
- Condition `not(steps.X.success)` works after translation (translator wraps inner in parens)
- Legacy `steps.X.output contains 'Y'` (with single quotes) is translated correctly
- A condition like `steps.X.success and not steps.Y.success` (already valid Jinja) passes through `translate_legacy_condition` unchanged via `Cow::Borrowed`
- `for_each` with object items: `{{ item.name }}` and `{{ index }}` both render correctly inside loop iterations
- `for_each` with string items: `{{ item }}` renders the string directly
