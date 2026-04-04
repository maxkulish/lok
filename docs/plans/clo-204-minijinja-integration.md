# CLO-204 Implementation Plan: MiniJinja Integration with TemplateContext and Custom Filters

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-204
**Specification**: specs/2026-04-04-clo-204-minijinja-integration.md
**Created**: 2026-04-04
**Overall Progress**: 0% (0/21 tasks completed)

---

## Architecture Context

Add MiniJinja 2.0 as template engine infrastructure alongside the existing regex interpolation system. Creates `src/template/` module with `TemplateEngine`, `TemplateContext`, `LazyEnv`, custom filters, and `TemplateError`. Does NOT modify `src/workflow.rs` or any execution paths - CLO-209 handles the migration.

---

## Tasks

### Phase 1: Add dependency and module skeleton (3 tasks)

- [ ] 1.1: Add `minijinja = "2"` to `[dependencies]` in `Cargo.toml`

- [ ] 1.2: Create `src/template/mod.rs` with module structure
  - [ ] `mod context;` and `mod filters;` declarations
  - [ ] `pub use context::TemplateContext;`
  - [ ] Stub `TemplateEngine` struct (empty, will be filled in Phase 5)
  - [ ] `TemplateError` enum with `thiserror`: `UndefinedVariable`, `ParseError`, `RenderError` variants wrapping `minijinja::Error`

- [ ] 1.3: Add `mod template;` to `src/main.rs` (after existing mod declarations)

### Phase 2: Implement custom filters (9 tasks)

- [ ] 2.1: Create `src/template/filters.rs` with `register_filters(env: &mut minijinja::Environment)` function

- [ ] 2.2: Implement `shell_escape` filter
  - [ ] Wrap value in single quotes, escape embedded single quotes with `'\''`
  - [ ] Handle null bytes (strip), newlines (preserve but quoted), backticks, `$()`

- [ ] 2.3: Implement `json_encode` filter
  - [ ] Serialize minijinja Value to JSON string via `serde_json::to_string`

- [ ] 2.4: Implement `join` filter
  - [ ] Join sequence values with separator argument (default: `""`)

- [ ] 2.5: Implement `first` and `last` filters
  - [ ] Return first/last element of a sequence, undefined if empty

- [ ] 2.6: Implement `default_val` filter
  - [ ] Return value if defined and non-empty, otherwise return the argument

- [ ] 2.7: Implement `trim` filter
  - [ ] Strip leading/trailing whitespace from string value

- [ ] 2.8: Implement `lines` filter
  - [ ] Split string into sequence of lines

- [ ] 2.9: Unit tests for all 8 filters
  - [ ] `shell_escape`: quotes, backticks, `$()`, newlines, null bytes, unicode
  - [ ] `json_encode`: string, number, nested object
  - [ ] `join`: with separator, default separator, empty sequence
  - [ ] `first`/`last`: normal, empty, single element
  - [ ] `default_val`: defined value, undefined, empty string
  - [ ] `trim`: whitespace, newlines, already trimmed
  - [ ] `lines`: multiline, single line, empty

### Phase 3: Implement LazyEnv and TemplateContext (6 tasks)

- [ ] 3.1: Create `src/template/context.rs` with `LazyEnv` struct
  - [ ] Implement `minijinja::value::Object` trait
  - [ ] `get_value(&self, key: &Value) -> Option<Value>`: calls `std::env::var(key)`, returns `Some(Value::from(val))` on hit, `None` on miss
  - [ ] No eager enumeration of env vars

- [ ] 3.2: Implement `TemplateContext` struct
  - [ ] Fields: `values: minijinja::Value` (the root context value)
  - [ ] `pub fn new(steps: &HashMap<String, StepResult>, args: &[String], backends: &[String]) -> Self`

- [ ] 3.3: Build `steps.*` namespace in context builder
  - [ ] For each step: `steps.{name}.output` (string), `steps.{name}.success` (bool)
  - [ ] For each step with `parsed_output: Some(value)`: spread JSON fields as `steps.{name}.{field}`
  - [ ] For each step with `parsed_output: None`: parse `output` as JSON string, extract fields (matching `extract_json_field` behavior)

- [ ] 3.4: Build `env`, `arg`, and `workflow` namespaces
  - [ ] `env`: `Value::from_object(LazyEnv)`
  - [ ] `arg`: 1-indexed sequence with `UNDEFINED` at index 0, so `arg.1`, `arg.2` resolve via sequence indexing
  - [ ] `workflow.backends`: capitalized (first letter), deduplicated, " + "-joined string

- [ ] 3.5: Implement `with_loop_item(self, item: Value, index: usize) -> Self`
  - [ ] Adds `item` and `index` to root context
  - [ ] Returns new `TemplateContext` with loop vars included

- [ ] 3.6: Unit tests for TemplateContext
  - [ ] Step output access, step field access (with parsed_output)
  - [ ] Step field access with `parsed_output: None` fallback
  - [ ] `steps.{name}.success` for true and false
  - [ ] Env var lookup (hit and miss)
  - [ ] Arg access (valid index, invalid index 0)
  - [ ] Workflow backends formatting
  - [ ] Loop vars (item string, item object field, index)

### Phase 4: Implement TemplateEngine and integration tests (3 tasks)

- [ ] 4.1: Implement `TemplateEngine` in `src/template/mod.rs`
  - [ ] `pub fn new() -> Self`: creates `minijinja::Environment`, registers filters, sets `UndefinedBehavior::Strict`
  - [ ] `pub fn render(&self, template: &str, ctx: &TemplateContext) -> Result<String, TemplateError>`
  - [ ] Map minijinja errors to appropriate `TemplateError` variants

- [ ] 4.2: Integration tests in `src/template/mod.rs`
  - [ ] Mixed template: `{{ steps.x.output | trim | default_val("none") }}`
  - [ ] Template with step output containing `{{ }}` literal braces (no re-expansion)
  - [ ] Template with env var + arg + step output combined

- [ ] 4.3: Error path tests
  - [ ] Malformed template `{{ steps.x` returns `TemplateError::ParseError`
  - [ ] Undefined variable `{{ steps.nonexistent.output }}` returns `TemplateError::UndefinedVariable`

---

## Dependency Order

```
Phase 1 -> Phase 2 (parallel with Phase 3) -> Phase 4
```

Phase 2 (filters) and Phase 3 (context) are independent and can be implemented in either order. Phase 4 (engine + integration) depends on both.

---

## Verification

After all phases:
```bash
cargo test -p lokomotiv -- template
cargo clippy -p lokomotiv -- -D warnings
```
