# Spec Review: clo-202

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-04-04
**Pipeline**: lok spec-review

---

The text is a **valid** structured specification review. It contains all required sections, a clear verdict, and actionable feedback. The content is already clean with no noise to remove.

## 1. Problem Statement Assessment
The problem statement is clear, complete, and highly accurate. It perfectly identifies the fragility of the current string-based `anyhow::Error` classification and accurately flags the affected call sites (`workflow.rs`, `run_query_with_config`, `implement.rs`). It sets a strong foundation for why typed errors are critical for the upcoming retry policy work.

## 2. Acceptance Criteria Review
**Strong**: The criteria are highly specific, testable, and cover the necessary bases (the enum, the backends, the trait, the tests, and backward compatibility for non-backend errors).
**Gaps**: The criteria fail to mention the `QueryResult` struct (`src/backend/mod.rs:57`). Currently, `QueryResult` stores only `success: bool` and `output: String`. If this is not updated, errors will be stringified when returned from `run_query_with_config`, forcing callers like `implement.rs` to continue parsing strings, which defeats the purpose.

## 3. Constraints Check
**Aligned**: Using `thiserror`, preserving string context for humans, keeping `BackendErrorKind` as a fallback for shell errors, and allowing CLI backends to map process failures are all well-aligned with the existing architecture.
**Concerns**: There is no constraint explicitly addressing how external `tokio::time::timeout` errors (e.g. `Elapsed`) in `workflow.rs` and `run_query_with_config` relate to the new `BackendError::Timeout`.

## 4. Decomposition Quality
**Well-scoped**: The steps are logically ordered and correctly identify that the trait change acts as a linchpin before backends can be updated in parallel.
**Issues**:
1. Missing sub-task for updating `QueryResult` to retain the typed error (e.g., adding `error: Option<BackendError>`).
2. Step 8 states `summarize_backend_error` "works with both `BackendError` and raw strings." Since Rust does not support function overloading, the decomposition needs to explain how this signature changes (e.g., using a trait bound `Into<BackendErrorKind>`, or splitting into two functions).

## 5. Evaluation Coverage
**Covered**: Great unit test coverage is planned for `is_retryable()`, `Display` formatting, and conversions.
**Gaps**: Missing an integration test to verify that a typed error returned by a backend propagates cleanly through `run_query_with_config` into `implement.rs` without being string-parsed.

## 6. Codebase Alignment
**Violations**: In `workflow.rs`, the spec says to "keep existing mapping: timeout -> Timeout, everything else -> BackendError". However, if an internal timeout occurs inside a backend (e.g. Ollama's reqwest timeout mapped to `BackendError::Timeout`), the "everything else" rule would map it to `StepFailureKind::BackendError`. This loses the internal timeout classification.
**Alignment**: Moving to a domain-specific `Result<T, BackendError>` rather than leaning entirely on `anyhow::Result` is idiomatic Rust and aligns well with improving the robustness of the orchestrator.

## 7. Blind Spots
- **`QueryResult` Type Erasure:** As noted, returning `Vec<QueryResult>` from `run_query_with_config` will erase the typed error unless `QueryResult` is modified to carry the `BackendError`.
- **`summarize_backend_error` Signature:** Assuming one function can natively take both an Enum and a String without explicitly defining the trait mechanics or splitting the function.
- **Nested Timeouts:** Blurring the line between internal backend timeouts (`BackendError::Timeout`) and external execution timeouts (`tokio::time::timeout`). If both exist, they need to be mapped to the same failure kind in `workflow.rs`.

## 8. Verdict
APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Update `QueryResult`:** Add a sub-task to update the `QueryResult` struct in `src/backend/mod.rs` to include the typed error (e.g., `pub error: Option<BackendError>`). Update `run_query_with_config` to populate this field so `src/tasks/implement.rs` can use `From<&BackendError>`.
2. **Clarify `summarize_backend_error`:** Update Step 8 to explicitly split the function into `summarize_backend_error(err: &BackendError)` and `summarize_shell_error(err: &str)` (or use a trait bound). This makes the Rust implementation straightforward and avoids type gymnastics.
3. **Fix Timeout Mapping:** In the `workflow.rs` update step, specify that if a query returns `Err(BackendError::Timeout)`, it should map to `StepFailureKind::Timeout`, rather than falling into the catch-all `BackendError` mapping.
