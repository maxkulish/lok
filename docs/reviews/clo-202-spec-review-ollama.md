# Spec Review: clo-202

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-04
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Clear and well-defined.** The problem statement accurately captures:
- Current state: `anyhow::Result` everywhere, string-based `classify_backend_error()` as post-hoc classifier
- Root cause: Callers parse error strings to determine error type
- Impact: Fragile classification that breaks when providers change error messages
- Dependency context: Foundation for CLO-206/207/208/211

**Minor gap:** The problem statement doesn't mention that `BackendErrorKind` already has 6 variants while `BackendError` proposes 8. The relationship between these two types needs clarification.

## 2. Acceptance Criteria Review

**Strong**:
- Clear enumeration of 8 variants
- Specific method signature for `is_retryable()`
- Test verification method specified
- Compiler-level verification (`cargo clippy`)

**Gaps**:
- **Variant mapping undefined**: `BackendErrorKind` has `CapacityExhausted` and `NotInstalled`, but `BackendError` proposes `Unavailable` and `Config`. The mapping between old and new enums isn't specified.
- **Missing criterion**: What happens to the `elapsed_ms` field in `QueryResult` when `BackendError::Timeout` is returned? Currently `run_query_with_config` computes `elapsed_ms` separately from error handling.
- **Missing criterion**: The `From<&BackendError> for BackendErrorKind` impl is mentioned in sub-task 8 but not as an acceptance criterion.
- **Vague criterion**: "All 5 backends return typed `BackendError` variants" - doesn't specify how each backend maps its error sources. For example, Ollama uses reqwest which has `reqwest::Error` - how does that map to `Network` vs `Timeout`?

## 3. Constraints Check

**Aligned**:
- `thiserror` already in Cargo.toml (version 2)
- `QueryOutput` unchanged (correct scope boundary)
- `StepFailureKind` unchanged (correct scope boundary)

**Concerns**:
- **Constraint conflict**: "Keep `BackendErrorKind` and `classify_backend_error()` working" conflicts with the proposed `From<&BackendError> for BackendErrorKind` impl. If `BackendError` is now the source of truth, `BackendErrorKind` becomes a compatibility shim. The spec should explicitly state that `classify_backend_error()` becomes a legacy fallback.
- **Missing constraint**: The spec says "Classify at the source" but doesn't constrain whether backends should use `.map_err()` or pattern-match on underlying error types. The current backends use `.context()` which wraps the error but doesn't convert it.
- **Implicit constraint not captured**: `StepFailureKind::BackendError` already exists and is used in workflow.rs. The spec should explicitly state that `BackendError::is_retryable()` informs but doesn't replace the `StepFailureKind` mapping at the workflow layer.

## 4. Decomposition Quality

**Well-scoped**:
- Sub-task 1-2 are appropriately small (enum definition, trait change)
- Sub-task 3-7 are parallelizable by backend

**Issues**:
- **Sub-task 1 has hidden complexity**: Adding `From<anyhow::Error> for BackendError` is problematic. If backends return `BackendError` directly, the `From` impl is only needed for transitional code. The spec should clarify whether this is temporary scaffolding or permanent API surface.
- **Missing sub-task**: No sub-task for updating `run_query_with_config`'s `QueryResult` handling. Currently `QueryResult.success` is a boolean - but if `backend.query()` returns `Result<QueryOutput, BackendError>`, the error handling in `run_query_with_config` needs to change (currently it uses `anyhow::Result` internally).
- **Missing sub-task**: No mention of updating tests in `src/backend/mod.rs` (there are existing tests for `QueryOutput`).
- **Dependency ordering unclear**: Sub-task 8 (callers) depends on sub-tasks 3-7 (backends), but the dependency order shows 2 -> (3,4,5,6,7 in parallel) -> 8. This is correct, but sub-task 2 (trait signature change) will break compilation for all backends simultaneously - the spec should note that this is a "flag day" change.

## 5. Evaluation Coverage

**Covered**:
- `is_retryable()` behavior
- `Display` impl
- `BackendErrorKind` conversion
- String-based classification fallback

**Gaps**:
- **Missing test**: What happens when `BackendError::Timeout` is created by `tokio::time::timeout` wrapper (in `run_query_with_config`), not by the backend itself? The timeout error originates outside the backend.
- **Missing test**: Ollama's built-in client timeout (`timeout_secs`) vs workflow timeout - which `BackendError::Timeout` takes precedence?
- **Missing test**: Claude API's HTTP 529 (overloaded) - spec says "RateLimit or Unavailable" but no test confirms which.
- **Missing test**: `From<anyhow::Error>` mapping - test that unknown `anyhow::Error` maps to `ExecutionFailed`.

## 6. Codebase Alignment

**Violations**:
- **`BackendErrorKind` vs `BackendError` variant mismatch**: 
  - `BackendErrorKind::CapacityExhausted` has no `BackendError` equivalent
  - `BackendErrorKind::NotInstalled` maps to `BackendError::Unavailable`?
  - `BackendError` has `Config` variant not in `BackendErrorKind`
  
  The spec should include a mapping table.

- **`StepFailureKind` relationship undefined**: Current code at `workflow.rs:1448-1449` uses `StepFailureKind::BackendError` for all backend errors. The spec doesn't explain whether fine-grained `BackendError` variants should produce `StepFailureKind::BackendError` or if there's a different mapping.

**Alignment**:
- Correctly identifies `thiserror` already in use
- Correctly identifies `classify_backend_error` location at `src/utils.rs:50`
- Correctly identifies the 10 call sites pattern (verified: grep shows 6 unique locations but multiple uses)

## 7. Blind Spots

1. **Timeout ownership**: `run_query_with_config` wraps `backend.query()` with `tokio::time::timeout`. When timeout occurs, it's the wrapper that fails, not the backend. The spec doesn't address who creates `BackendError::Timeout` - the backend can't, because it doesn't know about the wrapper timeout.

2. **`QueryResult` struct impact**: `QueryResult` stores `success: bool` and `output: String`. Currently the error message is formatted as `format!("Error: {}", e)`. With typed `BackendError`, should `QueryResult` change to store `Option<BackendError>`? The spec says "Do not change `QueryOutput`" but `QueryResult` is different.

3. **Error chain preservation**: `BackendError` should implement `std::error::Error` and `source()` to preserve the underlying error. The spec doesn't mention this.

4. **`PartialEq` on `BackendError`**: Tests will need to compare `BackendError` variants. Should it derive `PartialEq`? If variants contain `String` messages, equality comparison may not be useful.

5. **CLI backend stderr parsing**: The spec says "fall back to string classification using existing `classify_backend_error()` logic" for CLI backends. But if the backend returns `BackendError`, the caller shouldn't need to parse strings. This suggests backends should do their own stderr parsing and map to typed variants.

6. **`run_query_with_config` error handling**: Current code at `mod.rs:199-216` has three branches:
   - `Ok(Ok(query_output))` - success
   - `Ok(Err(e))` - backend error (currently `anyhow::Error`)
   - `Err(_)` - timeout from tokio wrapper
   
   With `BackendError`, the `Ok(Err(e))` branch now has `e: BackendError`. But the timeout branch still needs to create `BackendError::Timeout` - where does that happen?

## 8. Verdict

**NEEDS_REVISION**

The specification is well-structured and addresses the core problem, but has critical gaps that will cause implementation confusion:

1. **Variant mapping undefined** - `BackendErrorKind` and `BackendError` have mismatched variants
2. **Timeout ownership unclear** - wrapper timeout vs backend timeout
3. **Missing sub-tasks** - `run_query_with_config` error handling, tests in `mod.rs`
4. **`From<anyhow::Error>` rationale unclear** - transitional or permanent?

## 9. Actionable Feedback

**Priority 1 - Must Fix Before Implementation**:

1. **Add variant mapping table**: Show how each `BackendError` variant maps to `BackendErrorKind`. For example:
   ```
   BackendError::RateLimit    -> BackendErrorKind::RateLimited
   BackendError::Unavailable  -> BackendErrorKind::CapacityExhausted (?)
   BackendError::Config       -> BackendErrorKind::Unknown (?)
   ```

2. **Clarify timeout ownership**: Add a sub-task or constraint specifying that `run_query_with_config` creates `BackendError::Timeout` when `tokio::time::timeout` elapses, since backends don't control wrapper timeouts.

3. **Fix `run_query_with_config` gap**: Add sub-task for updating `QueryResult` handling in `run_query_with_config` to work with `BackendError`. Currently:
   ```rust
   Ok(Err(e)) => QueryResult { ..., output: format!("Error: {}", e), success: false }
   ```
   With `BackendError`, this becomes:
   ```rust
   Ok(Err(e)) => QueryResult { ..., output: e.to_string(), success: false }
   ```

4. **Clarify `From<anyhow::Error>` scope**: Is this for backward compatibility with `create_backend()` which returns `anyhow::Result`? If so, explicitly scope it to init errors, not query errors.

**Priority 2 - Should Clarify**:

5. **Add `Config` variant mapping**: `BackendError::Config` has no `BackendErrorKind` equivalent. Should it map to `Unknown` or should `BackendErrorKind` gain a `Config` variant?

6. **Add `elapsed_ms` handling for `BackendError::Timeout`**: The constraint says `Timeout` variant "carries `elapsed_ms: u64`", but `run_query_with_config` computes `elapsed_ms` separately. Should `BackendError::Timeout` be constructed with the elapsed time?

7. **Specify `source()` implementation**: `BackendError` should implement `std::error::Error::source()` to preserve error chains. Add this to constraints.

**Priority 3 - Nice to Have**:

8. **Add edge case test for Ollama timeout**: Ollama has both a built-in client timeout and the wrapper timeout. Which `BackendError::Timeout` takes precedence?

9. **Add note about test scaffolding**: Sub-task 1 will break all backend compilation. Recommend adding a compile-fix branch that implements a stub `From<anyhow::Error>` temporarily.
