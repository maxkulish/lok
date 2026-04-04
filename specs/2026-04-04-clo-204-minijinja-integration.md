# Spec: Add MiniJinja integration with TemplateContext and custom filters

**Created**: 2026-04-04
**Estimated scope**: M (4 new files + 1 edit, ~5 sub-tasks)

## 1. Problem Statement

lok's template interpolation is spread across 14 static regex patterns (`src/workflow.rs:82-151`) and three functions:
- `interpolate()` (line 2263) - `{{ steps.X.output }}`
- `interpolate_with_fields()` (line 2372) - `{{ steps.X.field }}`, `{{ env.VAR }}`, `{{ arg.N }}`, `{{ workflow.backends }}`
- `interpolate_loop_vars()` (line 2502) - `{{ item }}`, `{{ item.field }}`, `{{ index }}`

Each new variable namespace requires a new regex, a new replacement pass, and careful ordering to avoid collisions. There are no filters (`| default("N/A")`), no conditional expressions in templates, and no way to extend the system without touching workflow.rs.

This task adds MiniJinja 2.0 as a template engine alongside the existing regex system. It creates `src/template/` with `TemplateContext` (builds the Jinja context from workflow state) and custom filters. It does NOT replace the regex interpolation - that's CLO-209.

**Key types involved**:
- `StepResult` (`src/workflow.rs:885-913`) - has `name`, `output`, `parsed_output: Option<Value>`, `success`, `elapsed_ms`, `backend`, `raw_output`, `stderr`, `exit_code`, `validation`, `failure`
- `WorkflowRunner` (`src/workflow.rs:1034-1039`) - has `config`, `cwd`, `args: Vec<String>`, `context`
- `HashMap<String, StepResult>` - the step results map passed to interpolation

## 2. Acceptance Criteria

- [ ] `minijinja = "2"` added to `[dependencies]` in `Cargo.toml`
- [ ] `mod template;` declared in `src/main.rs`
- [ ] `src/template/mod.rs` exports `TemplateEngine` with a `render(template_str, &TemplateContext) -> Result<String>` method
- [ ] `src/template/context.rs` exports `TemplateContext` that builds a MiniJinja `Value` tree from `&HashMap<String, StepResult>`, `&[String]` (args), and a backends list
- [ ] `src/template/filters.rs` registers 8 custom filters: `shell_escape`, `json_encode`, `join`, `first`, `last`, `default_val`, `trim`, `lines`
- [ ] Lazy `{{ env.VAR }}` lookup works via `minijinja::value::Object` trait on a `LazyEnv` struct - env vars are only read when the template accesses them
- [ ] Loop variables work: `{{ item }}`, `{{ item.field }}`, `{{ index }}` render correctly when passed in context
- [ ] `{{ workflow.backends }}` renders the capitalized (first letter only: `claude` -> `Claude`), deduplicated, " + "-joined backend list
- [ ] `{{ steps.{name}.success }}` renders `true` for successful step, `false` for failed step
- [ ] Undefined variable `{{ steps.nonexistent.output }}` returns `TemplateError::UndefinedVariable`
- [ ] Malformed template `{{ steps.x` returns `TemplateError::ParseError`
- [ ] Every custom filter has at least one unit test
- [ ] `TemplateContext` building has unit tests covering: step output access, JSON field access, missing step, env var lookup, arg access, loop vars
- [ ] `TemplateEngine::render` has integration tests covering a template with mixed variables and filters
- [ ] `cargo test` passes, `cargo clippy` clean

**Verification method**: `cargo test -p lokomotiv -- template && cargo clippy -p lokomotiv -- -D warnings`

## 3. Constraints

**Must**:
- Use `minijinja = "2"` (not 1.x - the Object trait API changed significantly)
- Name custom filters to avoid colliding with MiniJinja builtins: use `json_encode` (not `json` - MiniJinja has a builtin `tojson`), `default_val` (not `default` - MiniJinja has a builtin `default`)
- `shell_escape` must handle: single quotes, double quotes, backticks, `$()`, newlines, null bytes
- `LazyEnv` Object implementation must not eagerly read all env vars - only resolve on attribute access
- Context must produce the same variable paths as current regex patterns: `steps.{name}.output`, `steps.{name}.{field}`, `env.{VAR}`, `arg.{N}` (1-indexed, stored as string keys `"1"`, `"2"` in a MiniJinja map), `workflow.backends`, `item`, `item.{field}`, `index`
- Expose `StepResult.success` as `steps.{name}.success` (bool) in the context - needed by condition evaluation
- Step output values containing `{{ }}` must NOT be re-expanded by MiniJinja - context stores values as `minijinja::Value::from()` strings (MiniJinja does not re-parse substituted values, unlike the regex system which needed explicit brace escaping)
- Return error for undefined variables - configure MiniJinja with `set_undefined_behavior(UndefinedBehavior::Strict)` rather than defaulting to empty string
- `TemplateError` enum must have variants: `UndefinedVariable`, `ParseError`, `RenderError` (all wrapping MiniJinja's error type with context)
- `TemplateEngine` is stateless after construction and safe to reuse across calls
- All `pub` items must have doc comments

**Must-not**:
- Do NOT modify `src/workflow.rs` - the regex system stays untouched (CLO-209 scope)
- Do NOT add the `template` module to any execution path yet - it's infrastructure only
- Do NOT use `minijinja`'s `source` feature or file-based template loading - all templates are inline strings
- Do NOT use `unsafe` code

**Prefer**:
- Return `thiserror` errors, not `anyhow` - consistent with `BackendError` pattern (`src/workflow.rs`)
- Use `minijinja::Value::from_object()` for lazy lookups rather than eagerly converting everything
- Keep filter functions as standalone `fn` items (not closures) for testability

**Escalate when**:
- MiniJinja 2.x Object trait API doesn't support lazy attribute enumeration without listing all env vars
- `steps.{name}.output` conflicts with `steps.{name}.{field}` resolution in MiniJinja's dot access

## 4. Decomposition

1. **Add dependency and module skeleton**: Add `minijinja = "2"` to `Cargo.toml`, create `src/template/mod.rs` with `mod context; mod filters;` and stub `TemplateEngine`, add `mod template;` to `src/main.rs` - files: `Cargo.toml`, `src/main.rs`, `src/template/mod.rs`

2. **Implement custom filters**: Create `src/template/filters.rs` with 8 filter functions (`shell_escape`, `json_encode`, `join`, `first`, `last`, `default_val`, `trim`, `lines`) plus a `register_filters(env: &mut minijinja::Environment)` function. Each filter is a standalone `fn`. Unit tests for each filter. - files: `src/template/filters.rs`

3. **Implement LazyEnv object**: Create a `LazyEnv` struct in `src/template/context.rs` that implements `minijinja::value::Object`. On `get_value(key)`, calls `std::env::var(key)`. Unit tests for hit/miss/special chars. - files: `src/template/context.rs`

4. **Implement TemplateContext builder**: In `src/template/context.rs`, create `TemplateContext` with a `build()` method that produces a `minijinja::Value` tree. Inputs: `&HashMap<String, StepResult>`, `&[String]` (args), `&[String]` (backends). Populates `steps.*` (output, parsed fields, success), `env` (LazyEnv), `arg.*` (string keys "1", "2", ...), `workflow.backends`. Provide `with_loop_item(item: Value, index: usize) -> Self` to set `item`/`item.{field}`/`index` for for_each contexts. For `steps.{name}.{field}` - use `parsed_output` when available, fall back to JSON string parsing (matching existing `extract_json_field` behavior). Unit tests for each namespace including `parsed_output: None` fallback. - files: `src/template/context.rs`

5. **Implement TemplateEngine and integration tests**: In `src/template/mod.rs`, implement `TemplateEngine::new()` (creates `minijinja::Environment` with filters registered) and `render(&self, template: &str, ctx: &TemplateContext) -> Result<String, TemplateError>`. Define `TemplateError` enum. Integration tests rendering templates with mixed variables and filters. - files: `src/template/mod.rs`

**Dependency order**: 1 -> 2, 3 (parallel) -> 4 -> 5

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `shell_escape` filter escapes `'; rm -rf /; echo '` | Returns `''\'''; rm -rf /; echo '\'''` or equivalent safe string | `cargo test -- template::filters::test_shell_escape` |
| 2 | `json_encode` filter on a struct | Returns valid JSON string | `cargo test -- template::filters::test_json_encode` |
| 3 | `{{ steps.fetch.output }}` renders step output | Step's output string verbatim | `cargo test -- template::context::test_step_output` |
| 4 | `{{ steps.fetch.verdict }}` renders JSON field | Extracted field value | `cargo test -- template::context::test_step_field` |
| 5 | `{{ env.HOME }}` via LazyEnv | Returns `$HOME` value | `cargo test -- template::context::test_env_lookup` |
| 6 | `{{ env.NONEXISTENT_LOK_VAR }}` | Returns undefined (MiniJinja handles gracefully) | `cargo test -- template::context::test_env_missing` |
| 7 | `{{ arg.1 }}` returns first arg | First positional argument | `cargo test -- template::context::test_arg_access` |
| 8 | `{{ workflow.backends }}` | Capitalized, deduplicated, " + "-joined | `cargo test -- template::context::test_workflow_backends` |
| 9 | `{{ item }}` and `{{ index }}` in loop context | Item value and iteration index | `cargo test -- template::context::test_loop_vars` |
| 10 | Mixed template with filters: `{{ steps.x.output \| trim \| default_val("none") }}` | Rendered correctly | `cargo test -- template::test_render_mixed` |
| 11 | `cargo clippy` clean | No warnings | `cargo clippy -p lokomotiv -- -D warnings` |
| 12 | `{{ steps.fetch.success }}` for successful step | Renders `true` | `cargo test -- template::context::test_step_success` |
| 13 | `{{ steps.fetch.verdict }}` where `parsed_output: None` | Falls back to JSON string parsing | `cargo test -- template::context::test_field_fallback` |
| 14 | Malformed template `{{ steps.x` | Returns `TemplateError::ParseError` | `cargo test -- template::test_parse_error` |
| 15 | `{{ steps.nonexistent.output }}` | Returns `TemplateError::UndefinedVariable` | `cargo test -- template::test_undefined_var` |

**Edge cases to verify**:
- Step output containing `{{ }}` literal braces (MiniJinja does not re-expand substituted values - verified by test)
- Empty step output with `default_val` filter
- `shell_escape` with null bytes, newlines, unicode
- `arg.0` (invalid - args are 1-indexed) returns undefined
- `json_encode` on nested objects
