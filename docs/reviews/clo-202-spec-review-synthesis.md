# Spec Review Synthesis: clo-202

**Synthesized**: 2026-04-04
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

| # | Finding | Severity |
|---|---------|----------|
| 1 | **`QueryResult` must carry typed error** - Both flag that `QueryResult` (success bool + output string) erases `BackendError` when returned from `run_query_with_config`, forcing callers back to string parsing. Missing sub-task to add `Option<BackendError>` field. | Critical |
| 2 | **Timeout ownership undefined** - `tokio::time::timeout` wraps `backend.query()` externally. Backends can't create `BackendError::Timeout` for wrapper timeouts. Spec doesn't say who constructs this variant or how internal backend timeouts (e.g. Ollama reqwest) relate to external ones. | Critical |
| 3 | **Missing sub-task for `run_query_with_config`** - The three-branch match (`Ok(Ok(...))`, `Ok(Err(...))`, `Err(...)`) needs updating to work with `BackendError` instead of `anyhow::Error`. Neither the decomposition nor acceptance criteria cover this. | High |
| 4 | **Integration test gap** - No test verifies typed error propagation end-to-end: backend -> `run_query_with_config` -> `implement.rs`. Unit tests for `is_retryable()` and `Display` are planned, but the critical path through the orchestrator is untested. | Medium |
| 5 | **`summarize_backend_error` signature unclear** - Spec says it handles both `BackendError` and raw strings, but Rust doesn't support overloading. Implementation mechanics (trait bound, two functions, or enum wrapper) not specified. | Medium |
| 6 | **Problem statement and decomposition are strong** - Both reviewers confirm the problem is well-defined, call sites correctly identified, and sub-tasks logically ordered with appropriate parallelism. | (Positive) |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | **Overall readiness** | APPROVE_WITH_SUGGESTIONS - gaps are addressable during implementation | NEEDS_REVISION - variant mapping, timeout ownership, and `From<anyhow::Error>` rationale must be resolved before starting |
| 2 | **`BackendErrorKind` mapping severity** | Notes the mismatch briefly; suggests `From<&BackendError>` covers it | Flags as critical blocker - demands explicit mapping table since variants don't align (`CapacityExhausted` vs `Unavailable`, missing `Config`) |
| 3 | **`classify_backend_error()` fate** | Implicit: superseded by typed errors | Explicit concern: spec says "keep working" but `From<&BackendError>` makes it a compatibility shim. Should be stated as legacy fallback. |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **`From<anyhow::Error>` scope unclear** - Is this transitional scaffolding for migration or permanent API surface? If only for `create_backend()` init errors, scope it explicitly. | Ollama | High |
| 2 | **Flag day compilation breakage** - Sub-task 2 (trait signature change) breaks all 5 backends simultaneously. Spec should note this and recommend a stub `From<anyhow::Error>` to keep things compiling during migration. | Ollama | Medium |
| 3 | **Error chain preservation** - `BackendError` should implement `std::error::Error::source()` to preserve underlying error chains. Not mentioned in constraints. | Ollama | Medium |
| 4 | **`elapsed_ms` on Timeout variant** - Spec says Timeout carries `elapsed_ms: u64`, but `run_query_with_config` computes elapsed time separately. Who populates the field? | Ollama | Medium |
| 5 | **Timeout mapping fix for workflow.rs** - If a backend returns `Err(BackendError::Timeout)`, the current "everything else -> StepFailureKind::BackendError" catch-all loses the timeout classification. Must explicitly map to `StepFailureKind::Timeout`. | Gemini | Medium |
| 6 | **Split `summarize_backend_error`** - Concrete suggestion: `summarize_backend_error(err: &BackendError)` + `summarize_shell_error(err: &str)` instead of type gymnastics. | Gemini | Low |
| 7 | **`PartialEq` derivation for testing** - Tests need to compare `BackendError` variants, but String fields make equality fragile. Spec should address whether to derive it or use pattern matching in tests. | Ollama | Low |

## Consolidated Verdict

**NEEDS_REVISION**

Ollama's NEEDS_REVISION controls. The spec has a solid foundation but three structural gaps will cause implementation confusion if not resolved upfront: `QueryResult` type erasure, timeout ownership, and the `BackendErrorKind`-to-`BackendError` variant mapping.

## Priority Actions

Ordered by severity, agreements first.

1. **Add `QueryResult` update sub-task** (Agreement #1, Critical) - Add `error: Option<BackendError>` to `QueryResult`. Update `run_query_with_config` to populate it. Without this, callers still parse strings.

2. **Define timeout ownership** (Agreement #2, Critical) - Specify that `run_query_with_config` creates `BackendError::Timeout` for wrapper timeouts. Specify that backend-internal timeouts (Ollama reqwest, Claude API) also produce `BackendError::Timeout`. Map both to `StepFailureKind::Timeout` in workflow.rs (Novel #5).

3. **Add `run_query_with_config` sub-task** (Agreement #3, High) - Cover the three-branch match update, `QueryResult` population, and `elapsed_ms` handling for timeout cases (Novel #4).

4. **Add variant mapping table** (Disagreement #2, High) - Explicit `BackendError` -> `BackendErrorKind` mapping. Decide: does `Config` map to `Unknown`? Does `Unavailable` map to `CapacityExhausted`?

5. **Scope `From<anyhow::Error>`** (Novel #1, High) - State whether this is transitional (migration only) or permanent. If transitional, add a removal sub-task.

6. **Add integration test** (Agreement #4, Medium) - Test typed error propagation: backend -> `run_query_with_config` -> caller, without string parsing.

7. **Clarify `summarize_backend_error` signature** (Agreement #5, Medium) - Either split into two functions (Novel #6) or specify the trait bound approach.

8. **Note flag-day compilation strategy** (Novel #2, Medium) - Acknowledge sub-task 2 breaks all backends. Recommend approach (stub impl, feature flag, or accept temporary breakage in a single commit).
