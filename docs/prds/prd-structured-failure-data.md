# PRD: Structured Failure Data for Step Errors

| Field | Value |
|-------|-------|
| Author | MK |
| Status | Draft |
| Created | 2026-04-03 |
| Last Updated | 2026-04-03 |
| Parent PRD | [Output Validation Pipeline](prd-output-validation-pipeline.md) (O3) |
| Linear | [CLO-185](https://linear.app/cloud-ai/issue/CLO-185) |

## 1. Overview

Complete the structured failure classification across all step execution failure paths in lok's workflow engine. The validation infrastructure (CLO-182/183/184) added `FailureType` and `ValidationResult` but only populates them for validation-specific failures. The ~15 other failure paths (timeouts, backend errors, empty output from non-validation contexts) still use `success=false` with an unstructured error string in `output`, making it impossible for downstream steps to programmatically distinguish failure modes.

## 2. Problem & Objectives

### Problem Statement

Downstream steps and the degradation engine need to make decisions based on *how* a step failed - retry on timeout, skip on validation failure, alert on auth error. Currently, all non-validation failures funnel through `StepResult::error()` which sets `validation: None` and puts an error message string in `output`. The only way to distinguish failure types is string parsing, which is fragile and incomplete.

### Objectives

- **O1**: Every step failure produces a `ValidationResult` with appropriate `FailureType`
- **O2**: `FailureType` enum covers all observed failure categories (timeout, backend error, empty output, validation failed)
- **O3**: Structured failure data includes: step name, backend, duration, exit code, failure type, failure reason

### Success Metrics

| Metric | Current | Target | How Measured |
|--------|---------|--------|--------------|
| Failure paths with structured FailureType | ~3 (validation only) | All (~18) | Audit `StepResult::error()` call sites |
| Failure types distinguishable programmatically | 3 enum variants | 5+ variants | Count `FailureType` variants |

## 3. Users & Use Cases

### Personas

| Persona | Role | Need | Pain Point |
|---------|------|------|------------|
| Workflow author | Writes TOML workflows | Conditional logic based on failure type | Can't distinguish timeout from validation failure |
| Degradation engine | Future system component | Targeted retry/skip/fallback decisions | All failures look identical (`success=false`) |

### Key Use Cases

**UC-1: Retry on timeout, skip on validation failure**
- Trigger: Step fails
- Steps: 1. Check `failure_type` 2. If Timeout, retry with longer duration 3. If ValidationFailed, skip to fallback
- Outcome: Targeted recovery without string parsing

**UC-2: Alert on auth/infrastructure errors**
- Trigger: Step fails with backend error
- Steps: 1. Check `failure_type=BackendError` 2. Check `failure_reason` for auth/network keywords
- Outcome: Ops-relevant errors surfaced distinctly from content failures

## 4. Functional Requirements

### FR Group: FailureType Enum Extension

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-1 | Add `FailureType::Timeout` variant | Must | Timeout failures produce `failure_type: Some(Timeout)` |
| FR-2 | Add `FailureType::BackendError` variant | Must | Backend errors produce `failure_type: Some(BackendError)` |
| FR-3 | Existing `EmptyOutput`, `ValidationFailed`, `ValidatorError` unchanged | Must | Existing validation tests still pass |

### FR Group: Failure Path Classification

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-4 | Timeout in shell steps produces `FailureType::Timeout` with duration | Must | Test: shell step timeout -> `failure_type == Timeout` |
| FR-5 | Timeout in LLM backend produces `FailureType::Timeout` with duration | Must | Test: LLM step timeout -> `failure_type == Timeout` |
| FR-6 | Non-zero exit code produces `FailureType::BackendError` with exit code | Must | Test: shell error -> `failure_type == BackendError` |
| FR-7 | Backend creation failure produces `FailureType::BackendError` | Must | Test: missing backend -> `failure_type == BackendError` |
| FR-8 | All-backends-failed in consensus produces `FailureType::BackendError` | Must | Test: all consensus backends fail -> structured failure |
| FR-9 | Empty output from non-validation context produces `FailureType::EmptyOutput` | Should | Test: backend returns empty string -> `EmptyOutput` |
| FR-10 | Edit parse/apply failures produce `FailureType::BackendError` | Should | Test: edit failure -> structured failure |
| FR-11 | Verify/fix loop exhaustion produces `FailureType::BackendError` | Should | Test: verify fails all retries -> structured failure |

### FR Group: StepResult Enhancement

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-12 | `StepResult::error()` accepts optional `FailureType` and populates `validation` field | Must | All `StepResult::error()` calls produce `validation: Some(...)` |
| FR-13 | `ValidationResult` populated with: validator="step_execution", elapsed_ms, failure_type, failure_reason | Must | Structured data available for every failure |

## 5. Non-Functional Requirements

| Category | Requirement | Target |
|----------|-------------|--------|
| Compatibility | Existing tests pass without modification | 0 test regressions |
| Performance | No measurable overhead on success path | < 1ms additional per step |

## 6. Scope & Phasing

### In Scope (This Phase)
- Extend `FailureType` enum with `Timeout` and `BackendError`
- Modify `StepResult::error()` to accept and populate structured failure data
- Audit and update all ~15 failure paths in `workflow.rs`
- Unit tests for each failure type classification
- Integration test with multiple failure modes

### Out of Scope
- Downstream degradation logic (consuming failure types) - future task
- Workflow TOML syntax for conditional-on-failure-type - future task
- Retry policy configuration per failure type - future task
- `BackendErrorKind` (from utils.rs) integration into FailureType - could be future refinement

### Future Phases

| Phase | Features | Depends On |
|-------|----------|------------|
| Degradation engine | Consume FailureType for retry/skip/fallback | CLO-185 |
| Conditional execution on failure type | TOML syntax for failure-aware branching | CLO-185 + degradation |

## 7. Dependencies

| Dependency | Owner | Status | Risk if Delayed |
|------------|-------|--------|-----------------|
| CLO-184: LLM validation | MK | Done (merged) | None |
| CLO-183: Heuristic validators | MK | Done (merged) | None |
| CLO-182: StepResult extension | MK | Done (merged) | None |

## 8. Risks & Open Questions

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Changing `StepResult::error()` signature breaks call sites | M | L | Provide default/backward-compatible overload |
| Missing a failure path in audit | L | M | Grep for all `StepResult::error` and `success: false` |

### Open Questions
- [x] Should `FailureType` live at StepResult level or remain only in ValidationResult? - Keep in ValidationResult, populate it for all failures
- [x] Should `BackendErrorKind` from utils.rs be folded into `FailureType`? - No, keep separate; FailureType is step-level, BackendErrorKind is for retry logic
