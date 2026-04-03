# CLO-185: Implement Structured Failure Data for Step Errors

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-185
**Status**: Finalized
**Finalized**: 2026-04-03
**Approved By**: Mk Km
**Author**: Mk Km
**Created**: 2026-04-03

---

## Summary

Add a `failure: Option<StepFailure>` field to `StepResult` that carries structured failure metadata for every failed step. This separates execution-level failures (timeouts, backend errors, skipped steps) from validation-level failures (already handled by `ValidationResult`), following the architectural contract established in CLO-182. The new field enables downstream steps to make programmatic decisions based on failure type without string parsing.

---

## Background

After CLO-182/183/184, the validation pipeline is complete: steps with a `validate` clause produce structured `ValidationResult` with `FailureType`. However, the ~17 non-validation failure paths (timeouts, backend errors, missing backends, dependency failures) all funnel through `StepResult::error()`, which sets `success: false`, puts an error message string in `output`, and leaves `validation: None`.

The CLO-182 design doc explicitly anticipated this gap and recommended the path forward:
- Line 32: *"FailureType must be scoped to validation failures only"*
- Line 134: *"If structured execution failure metadata is needed later, a separate `failure_info: Option<FailureInfo>` field can be added."*

This task implements that recommendation.

### Prior Research

**Discovery report**: `docs/prds/discovery-report-2026-04-03-clo-185.md`

Key findings that shaped this design:

1. **CLO-182 design contract**: The discovery phase found that the original PRD approach (extending `FailureType` with `Timeout`/`BackendError`) directly contradicts CLO-182's design. The approved approach is a separate struct.

2. **Temporal SDK pattern**: Temporal uses a typed failure hierarchy (`ApplicationFailure`, `TimeoutFailure`, `CancelledFailure`) with a `nonRetryable` flag. Dagster separates `Failure` (permanent) from `RetryRequested` (transient). Both validate the two-domain approach: validation failures are separate from execution failures.

3. **BackendError overloading risk**: The discovery stress test identified that a single `BackendError` variant covering 5+ distinct failure modes would recreate the original problem. The design uses 6 specific variants to preserve distinguishability.

4. **Contract enforcement**: Without a consumer or test asserting `failure.is_some()` on failed steps, the classification would drift. This design includes a contract-enforcement test.

---

## Architecture

### Component Overview

```
Backend::query() -> Result<QueryOutput>
       |
       |--- Success path (unchanged)
       |      -> run_validation() -> StepResult { success, validation, failure: None }
       |
       |--- Error path (CHANGED)
       |      -> StepResult::error() now also builds StepFailure
       |      -> StepResult { success: false, failure: Some(StepFailure { kind, ... }) }
       |
StepResult {
    success: bool,
    validation: Option<ValidationResult>,  // validation-domain only (unchanged)
    failure: Option<StepFailure>,          // NEW: execution-domain failures
}
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/workflow.rs` | Modified | Add `StepFailure`/`StepFailureKind` types. Add `failure` field to `StepResult`. Update `StepResult::error()` signature. Update 16 `error()` call sites + 1 `for_each` aggregate path. |
| Tests in `workflow.rs` | Modified | Update test construction sites with new field. Add tests per failure kind. |

### Dependencies

- **Internal**: `StepResult`, `ValidationResult`, `FailureType` (all in `workflow.rs`)
- **External**: None (no new crates needed)

---

## Detailed Design

### New Types

```rust
/// Why a step failed at the execution level (not validation).
/// Scoped to execution-domain failures only. Validation failures
/// are represented by ValidationResult.failure_type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum StepFailureKind {
    /// Step or backend timed out
    Timeout,
    /// Backend returned error, non-zero exit, or could not be created.
    /// Used for: backend creation failures, query errors (non-timeout),
    /// all-backends-failed in consensus, and retry exhaustion on non-verify paths.
    BackendError,
    /// Backend returned empty/whitespace output (no validate clause present).
    /// NOTE: No current call site produces this variant. It is a forward-looking
    /// placeholder for when execution-level empty-output detection is added.
    /// When a validate clause IS present, empty output flows through
    /// ValidationResult with FailureType::EmptyOutput instead.
    EmptyOutput,
    /// Step skipped due to unmet condition or failed dependency
    Skipped,
    /// Edit parse or apply failed
    EditFailed,
    /// Verify/fix loop exhausted all retries.
    /// Distinct from BackendError: verify failures mean the backend produced
    /// output but it failed verification checks, not that the backend itself errored.
    VerifyFailed,
}

/// Structured failure metadata for a failed step.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StepFailure {
    /// Classification of the failure
    pub kind: StepFailureKind,
    /// Human-readable error message (same content as StepResult.output)
    pub message: String,
    /// Backend that failed, if applicable
    pub backend: Option<String>,
    /// Process exit code, if applicable (CLI backends only)
    pub exit_code: Option<i32>,
    /// Time elapsed before failure (milliseconds)
    pub elapsed_ms: u64,
}
```

### StepResult Changes

```rust
pub struct StepResult {
    // ... existing fields unchanged ...
    /// Structured failure data. Populated for every failed step (success=false).
    /// None when step succeeds. Separate from `validation` which is scoped
    /// to validation-clause outcomes only.
    #[allow(dead_code)]
    pub failure: Option<StepFailure>,
}
```

### StepResult::error() Changes

```rust
impl StepResult {
    /// Create an error result with structured failure data.
    fn error(
        name: String,
        output: String,
        elapsed_ms: u64,
        backend: Option<String>,
        failure_kind: StepFailureKind,
    ) -> Self {
        Self {
            name,
            output: output.clone(),
            parsed_output: None,
            success: false,
            elapsed_ms,
            backend: backend.clone(),
            raw_output: None,
            stderr: None,
            exit_code: None,
            validation: None,
            failure: Some(StepFailure {
                kind: failure_kind,
                message: output,
                backend,
                exit_code: None,
                elapsed_ms,
            }),
        }
    }
}
```

### Failure Path Mapping (Complete Enumeration)

Every `StepResult::error()` call site mapped to its target `StepFailureKind`:

| # | Line | Context | Current Error Message | Target Kind |
|---|------|---------|----------------------|-------------|
| 1 | 1112 | Consensus not reached (continue_on_error) | `"Skipped: Consensus not reached: N/M..."` | `Skipped` |
| 2 | 1149 | Hard dependency failed (continue_on_error) | `"Skipped: dependency failed (X)"` | `Skipped` |
| 3 | 1464 | Shell step error (last retry) | `"Error: {e}"` | `BackendError` |
| 4 | 1475 | Shell step timeout (last retry) | `"Error: {last_error}"` | `Timeout` |
| 5 | 1485 | Shell step fallback (should not reach) | `"Error: {last_error}"` | `BackendError` |
| 6 | 1546 | All backends failed in consensus | `"All backends failed: ..."` | `BackendError` |
| 7 | 1693 | Backend not found | `"Backend not found: {name}"` | `BackendError` |
| 8 | 1701 | Backend creation failed | `"Failed to create backend: {e}"` | `BackendError` |
| 9 | 1708 | Backend not available | `"Backend {name} not available"` | `BackendError` |
| 10 | 1749 | LLM query error (last retry) | `"Error: {e}"` | `BackendError` |
| 11 | 1760 | LLM query timeout (last retry) | `"Error: {last_error}"` | `Timeout` |
| 12 | 1830 | Edit apply failed | `"Edit failed: {e}\n\nOriginal..."` | `EditFailed` |
| 13 | 1848 | Edit parse failed | `"Parse failed: {e}\n\nOriginal..."` | `EditFailed` |
| 14 | 1925 | Verify failed (retries exhausted) | `"Verification failed: {e}\n\n..."` | `VerifyFailed` |
| 15 | 1965 | Verify timeout (retries exhausted) | `"Verification timed out...\n\n..."` | `VerifyFailed` |
| 16 | 2021 | Single-backend fallback (should not reach) | `"Error: {last_error}"` | `BackendError` |

Note: Line numbers reference the codebase at design time (2026-04-03) and may shift during implementation.

**Retry exhaustion mapping**: Non-verify retry paths (#3, #10) exhaust retries and produce `BackendError` (the backend itself failed). Verify/fix loop paths (#14, #15) exhaust retries and produce `VerifyFailed` (the backend produced output but verification rejected it). This distinction matters: `BackendError` suggests the backend is unhealthy, while `VerifyFailed` suggests the backend is working but producing incorrect output.

### `for_each` Loop Aggregate Failure

The `for_each` loop at ~line 1374 constructs `StepResult` directly (not via `StepResult::error()`) with `success: all_success`. When `all_success` is `false`, this produces `StepResult { success: false, failure: None, validation: None }`, violating the contract invariant.

**Resolution**: When `all_success` is false, populate `failure` with `StepFailureKind::BackendError` and a message summarizing which iterations failed. This keeps the aggregate result consistent with the contract. Per-iteration failure details are already printed to stdout during execution; structured per-iteration failures can be added later if needed.

```rust
// In for_each aggregate result construction (~line 1374):
StepResult {
    // ... existing fields ...
    failure: if all_success {
        None
    } else {
        Some(StepFailure {
            kind: StepFailureKind::BackendError,
            message: format!("for_each: {}/{} iterations failed", fail_count, total),
            backend: None,
            exit_code: None,
            elapsed_ms,
        })
    },
}
```

### Validation + Failure Interaction

When both `validation` and `failure` could apply:

| Scenario | `success` | `validation` | `failure` |
|----------|-----------|-------------|-----------|
| Step succeeds, no validate clause | `true` | `None` | `None` |
| Step succeeds, validation passes | `true` | `Some(passed=true)` | `None` |
| Step succeeds execution, validation fails | `false` | `Some(passed=false, FailureType)` | `None` |
| Step fails execution (timeout, error, etc.) | `false` | `None` | `Some(StepFailure)` |
| Step skipped (condition/dependency) | `false` | `None` | `Some(kind=Skipped)` |

Key invariant: `failure.is_some()` and `validation.is_some() && !validation.passed` are **mutually exclusive**. A step either fails at execution (populates `failure`) or fails at validation (populates `validation`). Never both.

### What Does NOT Change

- `ValidationResult` struct - unchanged
- `FailureType` enum - unchanged (stays scoped to validation)
- `StepResult.output` content - unchanged (still contains human-readable error)
- Success path construction - only adds `failure: None`
- `run_validation()` logic - unchanged

---

## Implementation Plan

### Phase 1: Define Types and Update StepResult

- [ ] Add `StepFailureKind` enum with `#[derive(Debug, Clone, Copy, PartialEq, Eq)]` and `#[allow(dead_code)]`
- [ ] Add `impl fmt::Display for StepFailureKind` (e.g., `Timeout` -> `"timeout"`, `BackendError` -> `"backend_error"`)
- [ ] Add `StepFailure` struct with `#[allow(dead_code)]`
- [ ] Add `failure: Option<StepFailure>` field to `StepResult` with `#[allow(dead_code)]`
- [ ] Update `StepResult::error()` to accept `StepFailureKind` parameter
- [ ] Update all success-path `StepResult` construction sites to include `failure: None`

### Phase 2: Classify All Failure Paths

- [ ] Update call sites #1-2 (skip/dependency) with `StepFailureKind::Skipped`
- [ ] Update call sites #3, #5-10, #16 (backend errors) with `StepFailureKind::BackendError`
- [ ] Update call sites #4, #11 (timeouts) with `StepFailureKind::Timeout`
- [ ] Update call sites #12-13 (edit failures) with `StepFailureKind::EditFailed`
- [ ] Update call sites #14-15 (verify failures) with `StepFailureKind::VerifyFailed`
- [ ] Update `for_each` aggregate result (~line 1374) to populate `failure` when `all_success` is false

### Phase 3: Thread Additional Context

- [ ] For timeout paths (#4, #11): Extract duration from error message and set `elapsed_ms` accurately
- [ ] For backend error paths with exit codes (#3): Thread `exit_code` from shell output into `StepFailure`
- [ ] For backend paths (#6-10): Ensure `backend` field is populated

### Phase 4: Testing

- [ ] Update existing test construction sites with `failure: None` (success cases)
- [ ] Unit test: `StepResult::error()` produces correct `StepFailure` for each kind
- [ ] Unit test: `StepFailureKind::Timeout` created for timeout paths
- [ ] Unit test: `StepFailureKind::BackendError` created for backend error paths
- [ ] Unit test: `StepFailureKind::Skipped` created for skip paths
- [ ] Unit test: `StepFailureKind::EditFailed` created for edit failure paths
- [ ] Unit test: `StepFailureKind::VerifyFailed` created for verify exhaustion paths
- [ ] Contract test: every `StepResult` with `success=false` has `failure.is_some()` OR `validation.passed==false`
- [ ] Integration test: workflow with multiple failure modes produces correct `StepFailureKind` per step
- [ ] `cargo test` passes with 0 failures
- [ ] `cargo clippy` passes with 0 warnings

---

## Constraints

**Must**:
- Respect CLO-182 design contract: `FailureType` stays scoped to validation only
- `StepResult.output` content unchanged for all paths (backward compatible)
- Every `StepResult` with `success=false` has either `failure.is_some()` or `validation.passed==false`
- No new external dependencies

**Must-not**:
- Must not modify `ValidationResult` or `FailureType` enums
- Must not populate `validation` field for non-validation failures
- Must not change behavior of success paths beyond adding `failure: None`

**Prefer**:
- Keep `StepFailure` construction co-located with `StepResult::error()` (single construction point)
- Derive `Copy`, `PartialEq`, `Eq` on `StepFailureKind` for ergonomic test assertions and future use in collections
- Keep `message` field content identical to `StepResult.output` (no divergence)

**Escalate when**:
- If any success-path `StepResult` construction needs `failure: Some(...)` (violates invariant)
- If `for_each` loop paths need special treatment beyond the current 17 call sites
- If the test contract (`success=false` implies structured failure) finds violations in existing code

---

## Acceptance Criteria

- [ ] `StepFailureKind` enum has exactly 6 variants: `Timeout`, `BackendError`, `EmptyOutput`, `Skipped`, `EditFailed`, `VerifyFailed` - verified by `grep -A8 "pub enum StepFailureKind" src/workflow.rs`
- [ ] `StepFailure` struct has fields: `kind`, `message`, `backend`, `exit_code`, `elapsed_ms` - verified by `grep -A7 "pub struct StepFailure" src/workflow.rs`
- [ ] `StepResult` has `failure: Option<StepFailure>` field - verified by `grep "pub failure" src/workflow.rs`
- [ ] All 16 `StepResult::error()` call sites pass a `StepFailureKind` - verified by `grep -c "StepResult::error(" src/workflow.rs` returning 16
- [ ] `cargo test` passes with 0 failures
- [ ] `cargo clippy -- -D warnings` passes with 0 warnings

**Verification method**: `cargo test && cargo clippy -- -D warnings`

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | StepFailureKind enum has 6 variants | Timeout, BackendError, EmptyOutput, Skipped, EditFailed, VerifyFailed | `grep -A8 "pub enum StepFailureKind" src/workflow.rs` |
| 2 | StepResult::error() produces StepFailure | `failure.is_some()` with correct kind | `cargo test step_failure` |
| 3 | Timeout failure path produces Timeout kind | `kind == StepFailureKind::Timeout` | `cargo test timeout_failure` |
| 4 | Backend error path produces BackendError kind | `kind == StepFailureKind::BackendError` | `cargo test backend_error_failure` |
| 5 | Skip path produces Skipped kind | `kind == StepFailureKind::Skipped` | `cargo test skipped_failure` |
| 6 | Success path has failure: None | `failure.is_none()` | `cargo test success_no_failure` |
| 7 | Validation failure has failure: None | validation.is_some() and failure.is_none() | `cargo test validation_failure_no_step_failure` |
| 8 | Contract: success=false implies structured data | All failed StepResults have failure or failed validation | `cargo test failure_contract` |
| 9 | Existing tests still pass | 0 regressions | `cargo test` |
| 10 | No clippy warnings | Clean output | `cargo clippy -- -D warnings` |

**Edge cases to cover**:
- Step that times out on final retry (should be `Timeout`, not `BackendError`)
- Step with `continue_on_error` that gets skipped (should be `Skipped`)
- Step that succeeds execution but fails validation (should have `failure: None`, `validation.passed: false`)
- `for_each` loop where some iterations succeed and some fail (partial success - `failure` on the aggregate result)

---

## Testing Strategy

- **Unit Tests**: Test `StepResult::error()` with each `StepFailureKind`. Test the mutual exclusion invariant (failure vs validation). Test `PartialEq` matching on `StepFailureKind`.
- **Integration Tests**: Workflow TOML with a step that times out, a step that hits a missing backend, and a step that gets skipped - verify each produces the correct `StepFailureKind`.
- **Contract Test**: Iterate all `StepResult` values from a multi-step workflow run and assert: if `success == false`, then `failure.is_some() || (validation.is_some() && !validation.as_ref().unwrap().passed)`.

---

## Open Questions

- [ ] Should `StepFailureKind` derive `Serialize`/`Deserialize` for future workflow result persistence? (Deferred - add when a consumer needs it)
- [x] Should `for_each` aggregate results carry a `Vec<StepFailure>` for per-iteration failures? - No, use single `StepFailure` with `BackendError` kind for the aggregate. Per-iteration detail deferred.

---

## References

- [CLO-185 Linear Task](https://linear.app/cloud-ai/issue/CLO-185)
- [CLO-182 Design Doc](clo-182-stepresult-extensions.md) - established the `failure_info` recommendation
- [Discovery Report](../prds/discovery-report-2026-04-03-clo-185.md) - prior art, persona reviews, stress tests
- [Parent PRD](../prds/prd-output-validation-pipeline.md) - O3: structured error data
- [Temporal Failures](https://docs.temporal.io/references/failures) - typed failure hierarchy reference
