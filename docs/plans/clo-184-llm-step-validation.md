# CLO-184 Implementation Plan: LLM-Based Step Validation

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-184
**Design Document**: docs/design-docs/clo-184-llm-step-validation.md
**Created**: 2026-04-03
**Overall Progress**: 0% (0/20 tasks completed)

---

## Architecture Context

This task adds Layer 2 (LLM-based semantic validation) to Lok's validation pipeline. It builds on CLO-183 (heuristic validators) and CLO-181 (per-step model override). The implementation extends `ValidateConfig` with new fields, adds response parsing functions, creates a unified `run_step_validation()` entry point, and wires it into the three existing validation sites in `workflow.rs`.

---

## Tasks

### Phase 1: ValidateConfig & Type Extensions (src/workflow.rs)

- [ ] Task 1: Extend ValidateConfig struct with new fields
  - [ ] Add `on_error: Option<String>` field with `#[serde(default)]`
  - [ ] Add `max_input_length: Option<usize>` field with `#[serde(default)]`
  - [ ] Add `replace_output: bool` field with `#[serde(default)]`
  - [ ] Add `timeout_ms: Option<u64>` field with `#[serde(default)]`

- [ ] Task 2: Extend FailureType enum
  - [ ] Add `ValidatorError` variant to `FailureType` enum

### Phase 2: Core Validation Functions (src/workflow.rs)

- [ ] Task 3: Implement `interpolate_validation_prompt()`
  - [ ] Single-pass replacement of `{{ output }}` and `{{ stderr }}`
  - [ ] Truncation logic with `[TRUNCATED]` marker when `max_input_length` exceeded

- [ ] Task 4: Implement `strip_markdown_fences()`
  - [ ] Handle ` ```json ... ``` ` pattern
  - [ ] Handle ` ``` ... ``` ` pattern (no language tag)
  - [ ] Pass through text with no fences unchanged

- [ ] Task 5: Implement `parse_validation_response()`
  - [ ] Define `ValidationResponse` struct (status, output, reason)
  - [ ] JSON parsing (after fence stripping) as primary path
  - [ ] `REVIEW_FAILED:` prefix as fallback path
  - [ ] Fail-closed error for unrecognized formats (not a silent pass)

- [ ] Task 6: Implement `run_llm_validation()`
  - [ ] Create validation backend via `create_backend()`
  - [ ] Build prompt via `interpolate_validation_prompt()`
  - [ ] Query backend with model override and optional `timeout_ms`
  - [ ] Parse response and construct `ValidationResult`
  - [ ] Handle `on_error` policy (pass/fail/skip) for infrastructure failures
  - [ ] Return `(Option<ValidationResult>, Option<String>)` tuple

- [ ] Task 7: Implement `run_step_validation()`
  - [ ] Orchestrate heuristic check first (if configured)
  - [ ] Skip LLM if heuristic fails (cost optimization)
  - [ ] Call `run_llm_validation()` if backend is configured
  - [ ] Return combined validation result and optional cleaned output

### Phase 3: Wire Into Step Execution (src/workflow.rs)

- [ ] Task 8: Replace shell step validation block (~L1076)
  - [ ] Replace inline heuristic block with `run_step_validation()` call
  - [ ] Handle `replace_output` and `raw_output` preservation
  - [ ] Maintain existing print output for validation pass/fail

- [ ] Task 9: Replace multi-backend validation block (~L1304)
  - [ ] Replace inline heuristic block with `run_step_validation()` call
  - [ ] Handle `replace_output` and `raw_output` preservation

- [ ] Task 10: Replace apply/verify validation block (~L1624)
  - [ ] Replace inline heuristic block with `run_step_validation()` call
  - [ ] Handle `replace_output` and `raw_output` preservation

### Phase 4: Unit Tests (src/workflow.rs)

- [ ] Task 11: Unit tests for `interpolate_validation_prompt()`
  - [ ] Basic `{{ output }}` replacement
  - [ ] `{{ stderr }}` replacement
  - [ ] Truncation with `max_input_length`
  - [ ] Injection safety: output containing `{{ stderr }}` literal not expanded

- [ ] Task 12: Unit tests for `strip_markdown_fences()`
  - [ ] JSON with ` ```json ` fence
  - [ ] Content with plain ` ``` ` fence
  - [ ] Content with no fence (passthrough)

- [ ] Task 13: Unit tests for `parse_validation_response()`
  - [ ] Valid JSON pass response
  - [ ] Valid JSON fail response
  - [ ] JSON wrapped in markdown fences
  - [ ] REVIEW_FAILED: prefix (backward compat)
  - [ ] Unrecognized format -> error (fail-closed)
  - [ ] Invalid status value -> error

### Phase 5: Integration Tests (tests/integration.rs)

- [ ] Task 14: Integration test - LLM validation pass (shell mock)
  - [ ] Shell script echoes `{"status":"pass","output":"cleaned"}` as validation backend
  - [ ] Verify step succeeds and output optionally replaced

- [ ] Task 15: Integration test - LLM validation fail (shell mock)
  - [ ] Shell script echoes `{"status":"fail","reason":"invalid content"}` as validation backend
  - [ ] Verify step fails with correct ValidationResult

- [ ] Task 16: Integration test - combined heuristic + LLM
  - [ ] Heuristic fails -> LLM not invoked
  - [ ] Heuristic passes -> LLM invoked

- [ ] Task 17: Integration test - on_error policy
  - [ ] Backend not found + `on_error = "pass"` -> step passes
  - [ ] Backend not found + `on_error = "fail"` (default) -> step fails

- [ ] Task 18: TOML parsing tests for new ValidateConfig fields
  - [ ] Parse `on_error`, `max_input_length`, `replace_output`, `timeout_ms`
  - [ ] Verify defaults when fields absent

### Phase 6: Verification & Finalization

- [ ] Task 19: Full test suite and clippy
  - [ ] `cargo test` - all tests pass (existing + new)
  - [ ] `cargo clippy` - no warnings
  - [ ] Remove `#[allow(dead_code)]` from `raw_output` field if now used

- [ ] Task 20: Create PR
  - [ ] Push branch: `git push -u origin feat/clo-184-llm-step-validation`
  - [ ] Create PR via `/pr:create CLO-184`

---

## Module Structure

- `src/workflow.rs` - Core changes: ValidateConfig, FailureType, validation functions, wiring
- `src/backend/mod.rs` - Read-only: uses existing `create_backend()`, `Backend::query()`
- `tests/integration.rs` - New integration tests for LLM validation

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage will be recalculated based on completed tasks.

---

## Notes

- Use shell backend as mock validator in integration tests (echo JSON responses)
- Validation functions are `async` because they call `Backend::query()`
- The three wiring points share identical validation call patterns - extract once, call three times
- `replace_output = false` is the safe default; output replacement is opt-in
- Heuristic-first gating saves cost: failed heuristic skips LLM call entirely
