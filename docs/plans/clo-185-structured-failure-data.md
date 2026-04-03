# CLO-185 Implementation Plan: Structured Failure Data for Step Errors

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-185
**Design Document**: docs/design-docs/clo-185-structured-failure-data.md
**Created**: 2026-04-03
**Overall Progress**: 0% (0/30 tasks completed)

---

## Architecture Context

Add `failure: Option<StepFailure>` to `StepResult` with a `StepFailureKind` enum (6 variants). This is a separate domain from `ValidationResult`/`FailureType` per CLO-182 contract. All changes are in `src/workflow.rs` only.

---

## Tasks

### Phase 1: Define Types and Update StepResult (6 tasks)

- [ ] 1.1: Add `StepFailureKind` enum after `FailureType` enum (~line 948)
  - [ ] 6 variants: `Timeout`, `BackendError`, `EmptyOutput`, `Skipped`, `EditFailed`, `VerifyFailed`
  - [ ] Derives: `Debug, Clone, Copy, PartialEq, Eq`
  - [ ] Annotation: `#[allow(dead_code)]`

- [ ] 1.2: Add `impl fmt::Display for StepFailureKind`
  - [ ] Map each variant to lowercase snake_case string (e.g., `Timeout` -> `"timeout"`)

- [ ] 1.3: Add `StepFailure` struct after `StepFailureKind`
  - [ ] Fields: `kind: StepFailureKind`, `message: String`, `backend: Option<String>`, `exit_code: Option<i32>`, `elapsed_ms: u64`
  - [ ] Derives: `Debug, Clone`
  - [ ] Annotation: `#[allow(dead_code)]`

- [ ] 1.4: Add `failure: Option<StepFailure>` field to `StepResult` struct
  - [ ] Annotation: `#[allow(dead_code)]`
  - [ ] Doc comment referencing separation from `validation`

- [ ] 1.5: Update `StepResult::error()` to accept `failure_kind: StepFailureKind` parameter
  - [ ] Build `StepFailure` inside the constructor
  - [ ] Clone `output` into `StepFailure.message` and `backend` into `StepFailure.backend`

- [ ] 1.6: Add `failure: None` to all 4 success-path `StepResult { ... }` constructions
  - [ ] Line ~1374: `for_each` aggregate result (success case only; failure case handled in Phase 2)
  - [ ] Line ~1444: Shell step success result
  - [ ] Line ~1674: Multi-backend consensus success result
  - [ ] Line ~2006: Single-backend success result

### Phase 2: Classify All Failure Paths (7 tasks)

- [ ] 2.1: Update skip/dependency call sites with `StepFailureKind::Skipped`
  - [ ] Line ~1112: Consensus not reached (continue_on_error)
  - [ ] Line ~1149: Hard dependency failed (continue_on_error)

- [ ] 2.2: Update backend error call sites with `StepFailureKind::BackendError`
  - [ ] Line ~1464: Shell step error (last retry)
  - [ ] Line ~1485: Shell step fallback
  - [ ] Line ~1546: All backends failed in consensus
  - [ ] Line ~1693: Backend not found
  - [ ] Line ~1701: Backend creation failed
  - [ ] Line ~1708: Backend not available
  - [ ] Line ~1749: LLM query error (last retry)
  - [ ] Line ~2021: Single-backend fallback

- [ ] 2.3: Update timeout call sites with `StepFailureKind::Timeout`
  - [ ] Line ~1475: Shell step timeout (last retry)
  - [ ] Line ~1760: LLM query timeout (last retry)

- [ ] 2.4: Update edit failure call sites with `StepFailureKind::EditFailed`
  - [ ] Line ~1830: Edit apply failed
  - [ ] Line ~1848: Edit parse failed

- [ ] 2.5: Update verify failure call sites with `StepFailureKind::VerifyFailed`
  - [ ] Line ~1925: Verify failed (retries exhausted)
  - [ ] Line ~1965: Verify timeout (retries exhausted)

- [ ] 2.6: Update `for_each` aggregate result (~line 1374) for failure case
  - [ ] When `all_success` is false, populate `failure: Some(StepFailure { kind: BackendError, ... })`

- [ ] 2.7: Verify compilation: `cargo build`

### Phase 3: Thread Additional Context (4 tasks)

- [ ] 3.1: Thread `elapsed_ms` accurately for timeout paths
  - [ ] Line ~1475: Use actual elapsed time, not just error message
  - [ ] Line ~1760: Use actual elapsed time

- [ ] 3.2: Thread `exit_code` from shell output into `StepFailure` for shell error paths
  - [ ] Line ~1464: Capture exit code before entering error branch if available

- [ ] 3.3: Ensure `backend` field is populated for all backend-related failures
  - [ ] Lines ~1693, 1701, 1708: Already pass `Some(backend_name)` - verify carried to `StepFailure`
  - [ ] Line ~1546: Consensus failure - set `backend` to `None` (multi-backend)

- [ ] 3.4: Verify compilation: `cargo build`

### Phase 4: Testing (11 tasks)

- [ ] 4.1: Update ~12 existing test `StepResult { ... }` constructions with `failure: None`
  - [ ] Lines ~3297, 3558, 3573, 3665, 3680, 3803, 3827, 3869, 3896, 4004, 4031, 4288, 4305

- [ ] 4.2: Unit test: `StepResult::error()` produces correct `StepFailure`
  - [ ] Assert `failure.is_some()` with correct `kind` for each of 5 used variants

- [ ] 4.3: Unit test: `StepFailureKind::Display` outputs expected strings
  - [ ] Each variant maps to its lowercase snake_case name

- [ ] 4.4: Unit test: `StepFailureKind::Timeout` for timeout scenario

- [ ] 4.5: Unit test: `StepFailureKind::BackendError` for backend error scenario

- [ ] 4.6: Unit test: `StepFailureKind::Skipped` for skip scenario

- [ ] 4.7: Unit test: `StepFailureKind::EditFailed` for edit failure scenario

- [ ] 4.8: Unit test: `StepFailureKind::VerifyFailed` for verify exhaustion scenario

- [ ] 4.9: Contract test: `success=false` implies `failure.is_some()` OR `validation.passed==false`

- [ ] 4.10: Run full test suite: `cargo test`

- [ ] 4.11: Run clippy: `cargo clippy -- -D warnings`

### Phase 5: Finalization (2 tasks)

- [ ] 5.1: Commit with conventional message: `feat(CLO-185): add structured failure data for step errors`

- [ ] 5.2: Create PR via `/pr:create CLO-185`

---

## Module Structure

- `src/workflow.rs` - All type definitions and production code changes
- Tests inline in `src/workflow.rs` `#[cfg(test)]` module

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked

---

## Notes

- Line numbers are approximate (referenced at design time 2026-04-03) and may shift
- `EmptyOutput` variant is defined but has no current call site - it is a forward-looking placeholder
- `for_each` aggregate failures use `BackendError` kind with a summary message
- Non-verify retry exhaustion -> `BackendError`; verify loop exhaustion -> `VerifyFailed`
