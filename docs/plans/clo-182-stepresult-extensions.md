# CLO-182 Implementation Plan: Extend StepResult with stderr, exit_code, validation fields

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-182
**Design Document**: docs/design-docs/clo-182-stepresult-extensions.md
**Created**: 2026-03-31
**Overall Progress**: 0% (0/20 tasks completed)

---

## Architecture Context

`StepResult` is the output type for every workflow step execution. It flows from `execute_step()` through DAG result collection and into template interpolation. CLO-180 added `QueryOutput { stdout, stderr, exit_code }` to `Backend::query()` but the data is discarded before reaching `StepResult`. This plan threads that data through and adds validation type scaffolding for CLO-183+.

---

## Tasks

### Phase 1: Define New Types (src/workflow.rs)

- [ ] Task 1: Add `ValidationResult` struct and `FailureType` enum after `StepResult`
  - [ ] Define `ValidationResult { passed, failure_type, failure_reason, validator, elapsed_ms }` with `#[derive(Debug, Clone)]`
  - [ ] Define `FailureType` enum with 2 variants: `ValidationFailed`, `EmptyOutput` with `#[derive(Debug, Clone)]`

- [ ] Task 2: Add `ShellOutput` struct (private, near `run_shell` function)
  - [ ] Define `ShellOutput { stdout: String, stderr: Option<String>, exit_code: Option<i32> }`

- [ ] Task 3: Extend `StepResult` with 4 new fields
  - [ ] Add `raw_output: Option<String>` after `backend`
  - [ ] Add `stderr: Option<String>`
  - [ ] Add `exit_code: Option<i32>` (doc comment: None for API backends and signal-killed processes)
  - [ ] Add `validation: Option<ValidationResult>`

- [ ] Task 4: Add `StepResult::error()` constructor helper
  - [ ] Implement `fn error(name, output, elapsed_ms, backend) -> Self` setting all new fields to `None`, `success: false`, `parsed_output: None`

### Phase 2: Update run_shell() (src/workflow.rs)

- [ ] Task 5: Change `run_shell()` return type and implementation
  - [ ] Change signature from `Result<String>` to `Result<ShellOutput>`
  - [ ] Return `ShellOutput { stdout, stderr, exit_code }` instead of `format!("{}{}", stdout, stderr)`
  - [ ] Keep `anyhow::bail!` on non-zero exit code (error path unchanged)

- [ ] Task 6: Update shell step construction sites (3 sites: success, error, timeout)
  - [ ] Shell success (~line 884): Use `shell_output.stdout` for `output`, `shell_output.stderr` and `shell_output.exit_code` for new fields
  - [ ] Shell error (~line 900): Use `StepResult::error()`
  - [ ] Shell timeout (~line 918): Use `StepResult::error()`
  - [ ] Shell fallback (~line 935): Use `StepResult::error()`

### Phase 3: Thread QueryOutput in Single-Backend Path (src/workflow.rs)

- [ ] Task 7: Add stderr/exit_code variables in retry loop scope
  - [ ] Declare `let mut step_stderr: Option<String> = None;` and `let mut step_exit_code: Option<i32> = None;` near `let mut text = String::new();` (~line 1170)
  - [ ] Capture `qo.stderr` and `qo.exit_code` at the success match arm (~line 1190)

- [ ] Task 8: Update fix-loop re-query paths
  - [ ] Line ~1396-1398 (re-query after verify failure): Update `step_stderr` and `step_exit_code` from `qo`
  - [ ] Line ~1444-1446 (re-query after verify timeout): Update `step_stderr` and `step_exit_code` from `qo`

- [ ] Task 9: Update single-backend path StepResult construction sites
  - [ ] Success site (~line 1485): Pass `step_stderr`, `step_exit_code`, `raw_output: None`, `validation: None`
  - [ ] Error after retries (~line 1202): Use `StepResult::error()`
  - [ ] Timeout after retries (~line 1220): Use `StepResult::error()`
  - [ ] Fallback (~line 1496): Use `StepResult::error()`

### Phase 4: Update Remaining Construction Sites (src/workflow.rs)

- [ ] Task 10: Update consensus path construction sites
  - [ ] All-backends-failed (~line 1003): Use `StepResult::error()`
  - [ ] Consensus success (~line 1114): Add `raw_output: None, stderr: None, exit_code: None, validation: None`
  - [ ] Skip result for consensus not reached (~line 577): Use `StepResult::error()` or add None fields
  - [ ] Skip result for failed deps (~line 616): Use `StepResult::error()` or add None fields

- [ ] Task 11: Update for_each path construction site
  - [ ] For_each success (~line 842): Add `raw_output: None, stderr: None, exit_code: None, validation: None`

- [ ] Task 12: Update remaining error-path construction sites
  - [ ] Backend not found (~line 1129): Use `StepResult::error()`
  - [ ] Failed to create backend (~line 1144): Use `StepResult::error()`
  - [ ] Backend not available (~line 1158): Use `StepResult::error()`
  - [ ] Edit failed (~line 1297): Use `StepResult::error()`
  - [ ] Parse failed (~line 1325): Use `StepResult::error()`
  - [ ] Verification failed (~line 1410): Use `StepResult::error()`
  - [ ] Verification timeout (~line 1458): Use `StepResult::error()`

- [ ] Task 13: Update test fixture construction sites (13 sites)
  - [ ] Add `raw_output: None, stderr: None, exit_code: None, validation: None` to all test StepResult literals

### Phase 5: Clean Up backend/mod.rs

- [ ] Task 14: Remove `#[allow(dead_code)]` annotations
  - [ ] Remove from `QueryOutput.stderr` (~line 26)
  - [ ] Remove from `QueryOutput.exit_code` (~line 28)

### Phase 6: Testing & Validation

- [ ] Task 15: Verify compilation
  - [ ] `cargo build` succeeds with no errors

- [ ] Task 16: Run full test suite
  - [ ] `cargo test` passes with 0 failures

- [ ] Task 17: Run clippy
  - [ ] `cargo clippy -- -D warnings` passes with 0 warnings

- [ ] Task 18: Verify acceptance criteria
  - [ ] `grep "pub raw_output\|pub stderr\|pub exit_code\|pub validation" src/workflow.rs` shows 4 matches
  - [ ] `grep "pub struct ValidationResult" src/workflow.rs` returns a match
  - [ ] `grep -c "allow(dead_code)" src/backend/mod.rs` returns 0

### Phase 7: Finalization

- [ ] Task 19: Commit changes
  - [ ] Stage modified files: `src/workflow.rs`, `src/backend/mod.rs`
  - [ ] Commit: `feat(CLO-182): extend StepResult with stderr, exit_code, validation fields`

- [ ] Task 20: Create PR
  - [ ] Push branch: `git push -u origin feat/clo-182-stepresult-extensions`
  - [ ] Create PR via `gh pr create`
  - [ ] Link PR to Linear task CLO-182

---

## Module Structure

- `src/workflow.rs` - StepResult, ValidationResult, FailureType, ShellOutput definitions + 33 construction site updates
- `src/backend/mod.rs` - Remove dead_code annotations from QueryOutput

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

---

## Notes

- Phase 1-4 must be done in order (types defined before construction sites)
- Phase 5 can be done anytime after Phase 3 (when stderr/exit_code are consumed)
- All error-path sites use `StepResult::error()` helper - if a site needs `parsed_output` or `success: true`, use explicit construction instead
- Line numbers are approximate - verify actual locations before editing
