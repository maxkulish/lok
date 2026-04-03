# Spec: Heuristic Validators for Step Validation

**Created**: 2026-04-03
**Estimated scope**: M (3 files, ~5 sub-tasks)

## 1. Problem Statement

Lok workflows currently have no way to validate step output before passing it downstream. A step that returns empty text, truncated output, or output missing expected sections silently succeeds and corrupts dependent steps. CLO-182 added the data structures (`ValidationResult`, `FailureType`) but left `validation: None` everywhere.

This task adds the cheapest validation layer: string-based heuristic checks that run in <1ms with zero cost. These are configured via a `[steps.validate]` TOML section on any step, with a `check` field containing one of three validators: `not_empty`, `min_length(N)`, or `contains('text')`.

The `ValidateConfig` struct also carries `backend`, `model`, and `prompt` fields needed by CLO-184 (LLM validation), but this task only implements the `check` path.

**Key files**:
- `src/workflow.rs:248-332` - `Step` struct (needs `validate` field)
- `src/workflow.rs:449-469` - `ValidationResult`, `FailureType` (already defined by CLO-182)
- `src/workflow.rs:1443-1465` - StepResult construction point (needs validation wiring)
- `src/workflow.rs:555-690` - `continue_on_error` logic (already works with `success = false`)

## 2. Acceptance Criteria

- [ ] `ValidateConfig` struct defined with fields: `check: Option<String>`, `backend: Option<String>`, `model: Option<String>`, `prompt: Option<String>`
- [ ] `Step` struct has `validate: Option<ValidateConfig>` field, deserialized from `[steps.validate]` TOML sections
- [ ] `check = "not_empty"` rejects output that is empty or whitespace-only; sets `failure_type: EmptyOutput`
- [ ] `check = "min_length(200)"` rejects output shorter than 200 chars; sets `failure_type: ValidationFailed`
- [ ] `check = "contains('## Summary')"` rejects output missing the marker text; sets `failure_type: ValidationFailed`
- [ ] On check failure: `StepResult.success = false`, `StepResult.validation` populated with `ValidationResult`, `StepResult.output` retains original backend/shell output (not replaced with error message), `StepResult.raw_output` remains `None` (heuristic checks do not mutate output)
- [ ] On check pass: `StepResult.validation` populated with `passed: true`, output unchanged, `StepResult.raw_output` remains `None`
- [ ] `ValidationResult.validator` uses format `"heuristic:<check_name>"` (e.g., `"heuristic:not_empty"`, `"heuristic:min_length"`, `"heuristic:contains"`)
- [ ] `ValidationResult.failure_reason` provides human-readable context (e.g., `"Output is empty or whitespace-only"`, `"Output length 42 is less than minimum 200"`, `"Output is missing expected string '## Summary'"`)
- [ ] Heuristic validation runs AFTER the `fix_retries` loop (after line 1443), not inside it - heuristic checks validate final output quality, not edit application correctness
- [ ] Steps without `[steps.validate]` behave identically to before (`validation: None`)
- [ ] Heuristic failure integrates with existing `continue_on_error` logic (no new code needed - already checks `success`)
- [ ] Unit tests for each heuristic check (not_empty, min_length, contains) covering pass and fail cases
- [ ] Unit tests for check string parsing (valid and invalid inputs)
- [ ] Integration test with shell step + validate check in a TOML workflow
- [ ] All existing tests pass without modification

**Verification method**:
- `cargo test` - all unit and integration tests pass
- `cargo clippy` - no new warnings
- Manual: run a test workflow with `check = "not_empty"` on a shell step that produces empty output; confirm step fails

## 3. Constraints

**Must**:
- Use existing `ValidationResult` and `FailureType` types from CLO-182 without modification
- Parse `check` field from the same `ValidateConfig` struct that CLO-184 will use for LLM validation (`backend`, `model`, `prompt` fields present but unused here)
- Follow existing code patterns in `workflow.rs` for struct definitions, serde attributes, and test organization
- Keep all validation logic deterministic and allocation-minimal (string ops only)

**Must-not**:
- Add any LLM or network calls - this is strictly string-based validation
- Modify `ValidationResult` or `FailureType` definitions
- Break any existing test or change existing StepResult construction for steps without `validate`

**Prefer**:
- Implement heuristic checks as a standalone function `run_heuristic_check(check: &str, output: &str) -> ValidationResult` that can be unit tested independently
- Use `contains` with single-quoted argument to match TOML ergonomics (e.g., `contains('## Summary')`)
- Also support double quotes in `contains("## Summary")` for flexibility
- Keep the check parser simple: regex or hand-written parser, whichever is shorter

**Escalate when**:
- The `Step` deserialization of nested `[steps.validate]` TOML tables doesn't work with serde's default derive (TOML nested table syntax)
- Any existing test breaks due to the new field addition

## 4. Decomposition

1. **Define ValidateConfig and wire into Step** - files: `src/workflow.rs`
   - Add `ValidateConfig` struct with serde Deserialize/Serialize/Clone/Debug derives
   - Add `validate: Option<ValidateConfig>` to `Step` struct with `#[serde(default)]`
   - Verify TOML parsing works with a unit test

2. **Implement heuristic check parser and validators** - files: `src/workflow.rs` (or new `src/validate.rs` if workflow.rs is already too large)
   - Parse check strings: `not_empty`, `min_length(N)`, `contains('text')`/`contains("text")`
   - Implement each check as a function returning `ValidationResult`
   - Unit tests for parser and each validator (pass + fail cases)

3. **Wire validation into step execution** - files: `src/workflow.rs`
   - After line 1443 (verify passed / `break 'fix_loop`), before StepResult construction at line 1454
   - Intentionally OUTSIDE `'fix_loop` - heuristic checks validate final output quality, not edit application; `fix_retries` is for `apply_edits` + `verify` workflow, not output validation
   - If `step.validate.check` is `Some`, run heuristic check on `current_text`
   - On failure: set `success = false`, populate `validation` with `ValidationResult { passed: false, validator: "heuristic:<name>", failure_reason: Some("<descriptive>"), .. }`, keep `output` as original text, keep `raw_output` as `None`
   - On pass: populate `validation` with `ValidationResult { passed: true, validator: "heuristic:<name>", .. }`, keep `raw_output` as `None`
   - No `validate` clause: leave `validation: None` (current behavior)

4. **Add TOML parsing test** - files: `src/workflow.rs` (test module)
   - Test that `[steps.validate]` section parses correctly into Step struct
   - Test that missing validate section leaves it as None

5. **Add integration test** - files: `tests/integration.rs`, `tests/workflows/test_validate.toml`
   - Shell step producing empty output with `check = "not_empty"` - should fail
   - Shell step producing sufficient output with `check = "min_length(5)"` - should pass
   - Shell step with `continue_on_error = true` and failing validation - workflow continues

**Dependency order**: 1 -> 2 -> 3 -> 4, 5 (4 and 5 are independent after 3)

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Parse `check = "not_empty"` from TOML | `ValidateConfig { check: Some("not_empty"), backend: None, model: None, prompt: None }` | `cargo test test_parse_validate_config` |
| 2 | `not_empty` on `""` | `ValidationResult { passed: false, failure_type: Some(EmptyOutput), .. }` | `cargo test test_heuristic_not_empty` |
| 3 | `not_empty` on `"hello"` | `ValidationResult { passed: true, .. }` | `cargo test test_heuristic_not_empty` |
| 4 | `not_empty` on `"   \n  "` | `ValidationResult { passed: false, failure_type: Some(EmptyOutput), .. }` | `cargo test test_heuristic_not_empty` |
| 5 | `min_length(10)` on `"short"` | `ValidationResult { passed: false, failure_type: Some(ValidationFailed), .. }` | `cargo test test_heuristic_min_length` |
| 6 | `min_length(3)` on `"hello"` | `ValidationResult { passed: true, .. }` | `cargo test test_heuristic_min_length` |
| 7 | `contains('## Summary')` on `"no marker"` | `ValidationResult { passed: false, failure_type: Some(ValidationFailed), .. }` | `cargo test test_heuristic_contains` |
| 8 | `contains('## Summary')` on `"has ## Summary here"` | `ValidationResult { passed: true, .. }` | `cargo test test_heuristic_contains` |
| 9 | Invalid check string `"unknown_check"` | Error or `ValidationResult { passed: false, failure_reason: Some("Unknown check: unknown_check"), .. }` | `cargo test test_heuristic_unknown` |
| 10 | Step without validate clause | `StepResult.validation == None`, `success == true` | `cargo test` (existing tests) |
| 11 | Integration: shell step + not_empty on empty output | Step marked as failed in workflow output | `cargo test test_validate` |
| 12 | Integration: failed validation + continue_on_error | Workflow continues past failed step | `cargo test test_validate` |
| 13 | Mixed config: both `check` and `backend` set | Only `check` runs, `backend` ignored | `cargo test test_validate_mixed_config` |
| 14 | `validator` field format | Returns `"heuristic:not_empty"`, `"heuristic:min_length"`, `"heuristic:contains"` | `cargo test test_heuristic_*` |
| 15 | `failure_reason` descriptive | Returns human-readable string with context (e.g., output length) | `cargo test test_heuristic_*` |

**Edge cases to verify**:
- `min_length(0)` - should always pass (any string >= 0 chars)
- `contains('')` with empty search string - should always pass
- `contains` with special regex characters in text (e.g., `contains('price: $10')`) - must be literal match, not regex
- Whitespace-only output with `min_length(5)` where whitespace is 5+ chars - passes (min_length counts raw chars, not trimmed)
- `check` field present but empty string - treat as no check (pass through)
- Both `check` and `backend` set in validate config - only `check` runs in this task (backend ignored)
- Validation failure on a step with `fix_retries > 0` - validation runs after the fix loop, so fix_retries has no effect on heuristic failures
