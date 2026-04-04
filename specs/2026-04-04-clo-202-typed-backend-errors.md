# Spec: Add BackendError Enum with Typed Variants and is_retryable()

**Created**: 2026-04-04
**Linear**: CLO-202
**Estimated scope**: M (8 files, ~10 sub-tasks)

## 1. Problem Statement

lok's `Backend::query()` trait method returns `Result<QueryOutput>` using `anyhow::Result` (`src/backend/mod.rs:53`). All 5 backend implementations (claude, codex, gemini, ollama, bedrock) return opaque `anyhow::Error` values via `.context()` and `bail!()`. Callers cannot programmatically distinguish a timeout from an auth failure from a rate limit without parsing error strings.

A post-hoc string classifier exists in `src/utils.rs:50` (`classify_backend_error()` returning `BackendErrorKind` with 6 variants), but it's inherently fragile - it pattern-matches on lowercase substrings like "429", "econnrefused", "command not found". If an LLM provider changes their error message format, classification breaks silently.

**What needs to change**: Replace `anyhow::Result<QueryOutput>` with `Result<QueryOutput, BackendError>` on the `Backend` trait. Each backend classifies its own errors at the source (HTTP status codes, IO errors, etc.) instead of the caller guessing from strings. The existing `BackendErrorKind` in `utils.rs` becomes a thin wrapper over `BackendError` for backward compatibility.

**Who's affected**: All callers of `Backend::query()` - primarily `src/workflow.rs` (10 call sites wrapped in `tokio::time::timeout`) and `src/backend/mod.rs:162` (`run_query_with_config`). Also `src/tasks/implement.rs:354` which calls `classify_backend_error`.

**Why it matters**: This is the foundation for CLO-206 (RetryPolicy), CLO-207 (QueryOutput enrichment), CLO-208 (RetryExecutor), and CLO-211 (apply-verify pipeline). Without typed errors, retry logic can't distinguish retryable from fatal errors.

## 2. Acceptance Criteria

- [ ] `BackendError` enum exists in `src/backend/mod.rs` with 8 variants: `Timeout`, `RateLimit`, `Auth`, `Network`, `Parse`, `ExecutionFailed`, `Unavailable`, `Config`
- [ ] `BackendError` implements `std::error::Error` + `Display` via `thiserror::Error`
- [ ] `BackendError::is_retryable()` returns `true` only for `Timeout`, `RateLimit`, `Network`
- [ ] `Backend::query()` signature is `async fn query(...) -> Result<QueryOutput, BackendError>`
- [ ] All 5 backends (claude, codex, gemini, ollama, bedrock) return typed `BackendError` variants
- [ ] `run_query_with_config` in `src/backend/mod.rs` handles `BackendError` in 3-branch match (`Ok(Ok)`, `Ok(Err)`, `Err(timeout)`)
- [ ] `QueryResult` carries `error: Option<BackendError>` so callers get typed errors without string parsing
- [ ] `run_query_with_config` constructs `BackendError::Timeout` for `tokio::time::timeout` expiry (external timeout ownership)
- [ ] `src/workflow.rs` compiles with updated error handling at all 10 `backend.query()` call sites
- [ ] `workflow.rs` maps `BackendError::Timeout` to `StepFailureKind::Timeout` (not catch-all `BackendError`)
- [ ] `BackendErrorKind` in `src/utils.rs` updated: `From<&BackendError>` impl replaces string classification for typed errors
- [ ] `classify_backend_error()` retained as legacy fallback for non-backend error strings (shell commands)
- [ ] `summarize_backend_error` split into `summarize_backend_error(err: &BackendError)` + `summarize_shell_error(err: &str)`
- [ ] `cargo test` passes
- [ ] `cargo clippy -- -D warnings` clean

**Verification method**: `cargo test && cargo clippy -- -D warnings`

## 3. Constraints

**Must**:
- Use `thiserror::Error` derive (already in Cargo.toml as `thiserror = "2"`)
- Each variant carries a `String` message for human-readable context (preserves current error detail level)
- `Timeout` variant carries `elapsed_ms: u64` for diagnostics. Populated by: backends for internal timeouts (e.g., ollama reqwest), `run_query_with_config` for external `tokio::time::timeout` wraps
- `RateLimit` variant carries optional `retry_after_ms: Option<u64>` for server-provided hints
- `ExecutionFailed` variant carries optional `exit_code: Option<i32>` for CLI backends
- Keep `BackendErrorKind` and `classify_backend_error()` working for non-backend error strings (shell commands in workflow.rs). Mark `classify_backend_error()` as legacy fallback via doc comment
- Implement `std::error::Error::source()` via `thiserror`'s `#[source]` attribute where an underlying error exists (e.g., `Network` wrapping `reqwest::Error`)
- `From<anyhow::Error> for BackendError` is transitional scaffolding for migration only - maps to `ExecutionFailed`. Add `// TODO: remove once all backends return typed errors` comment

**Must-not**:
- Do not change `QueryOutput` struct (that's CLO-207)
- Do not add retry logic (that's CLO-206/208)
- Do not change `StepFailureKind` or `StepFailure` mapping logic (CLO-185 domain), but DO map `BackendError::Timeout` to `StepFailureKind::Timeout` explicitly in workflow.rs
- Do not remove `anyhow` from Cargo.toml (still used elsewhere)

**Prefer**:
- Classify at the source: backends should use HTTP status codes, IO error kinds, and process exit codes directly rather than parsing their own error strings
- For CLI backends (codex, gemini CLI, claude CLI) that return opaque stderr, fall back to string classification using existing `classify_backend_error()` logic, then map to `BackendError`
- Keep variant data minimal - just enough for retry decisions and diagnostics
- Use pattern matching (not `PartialEq`) in tests to assert variant kind without comparing message strings

**Escalate when**:
- A backend's error paths can't be reliably classified (e.g., gemini CLI returns no structured errors)
- Changing `Backend::query()` return type causes cascade into `conductor.rs` or `team.rs`

### BackendError -> BackendErrorKind Mapping Table

| BackendError | BackendErrorKind | Notes |
|---|---|---|
| `Timeout` | `NetworkError` | Timeout is network-adjacent for UX |
| `RateLimit` | `RateLimited` | Direct map |
| `Auth` | `AuthError` | Direct map |
| `Network` | `NetworkError` | Direct map |
| `Parse` | `Unknown` | No parsing category in BackendErrorKind |
| `ExecutionFailed` | `Unknown` | Generic failure |
| `Unavailable` | `CapacityExhausted` | Server overloaded or CLI not installed |
| `Config` | `Unknown` | No config category in BackendErrorKind |

## 4. Decomposition

1. **Define `BackendError` enum and `is_retryable()`** - files: `src/backend/mod.rs`
   - Add enum with 8 variants, thiserror derive, is_retryable() method
   - Add transitional `From<anyhow::Error>` impl mapping to `ExecutionFailed` (with TODO comment for removal)
   - Add `#[source]` attribute on variants that wrap underlying errors (e.g., Network wrapping reqwest/IO)

2. **Update `Backend` trait signature and `run_query_with_config`** - files: `src/backend/mod.rs`
   - Change `query()` return type to `Result<QueryOutput, BackendError>`
   - Add `error: Option<BackendError>` field to `QueryResult` struct
   - Update `run_query_with_config()` 3-branch match:
     - `Ok(Ok(qo))` -> success, error: None
     - `Ok(Err(e))` -> error: Some(e), output: e.to_string()
     - `Err(_)` -> construct `BackendError::Timeout { elapsed_ms, message }`, error: Some(timeout)
   - Keep `create_backend()` and `create_claude_backend()` returning `anyhow::Result` (init errors, not query)
   - NOTE: This breaks all 5 backends until sub-tasks 3-7 complete. Accept temporary breakage within a single branch - all backend updates are in one commit.

3. **Update claude backend** - files: `src/backend/claude.rs`
   - API mode: map HTTP status codes (401/403 -> Auth, 429 -> RateLimit, 529 -> RateLimit, 5xx -> ExecutionFailed, reqwest send errors -> Network, JSON parse errors -> Parse)
   - CLI mode: map process spawn failure -> Unavailable, non-zero exit -> classify stderr via `classify_backend_error()` then map to BackendError variant

4. **Update codex backend** - files: `src/backend/codex.rs`
   - Map process spawn failure -> Unavailable, non-zero exit -> classify stderr, parse failure -> Parse

5. **Update gemini backend** - files: `src/backend/gemini.rs`
   - Map process spawn failure -> Unavailable, non-zero exit -> classify stderr (429 patterns -> RateLimit, capacity -> RateLimit)

6. **Update ollama backend** - files: `src/backend/ollama.rs`
   - Map reqwest errors -> Network (connection) or Timeout (built-in client timeout), HTTP status -> RateLimit/ExecutionFailed, JSON parse errors -> Parse

7. **Update bedrock backend** - files: `src/backend/bedrock.rs`
   - Map AWS SDK errors -> Auth/Network/ExecutionFailed, parse errors -> Parse
   - Feature-gated: `#[cfg(feature = "bedrock")]`

8. **Update utils.rs** - files: `src/utils.rs`
   - Add `impl From<&BackendError> for BackendErrorKind` using mapping table from Constraints section
   - Mark `classify_backend_error()` with doc comment: `/// Legacy: classifies errors from string content. Prefer From<&BackendError> for typed errors.`
   - Split `summarize_backend_error` into:
     - `summarize_backend_error(err: &BackendError) -> String` (typed path)
     - `summarize_shell_error(backend_name: &str, err: &str) -> String` (string path for shell commands)

9. **Update workflow.rs callers** - files: `src/workflow.rs`
   - Update 10 `backend.query()` error handling sites
   - Map `BackendError::Timeout` explicitly to `StepFailureKind::Timeout` (not catch-all)
   - Map all other `BackendError` variants to `StepFailureKind::BackendError`
   - Update `summarize_backend_error` calls: use typed version for backend errors, `summarize_shell_error` for shell command errors

10. **Update tasks/implement.rs** - files: `src/tasks/implement.rs`
    - Replace `classify_backend_error(&r.output)` at line 354 with `From<&BackendError>` conversion using `QueryResult.error` field

**Dependency order**: 1 -> 2 -> (3, 4, 5, 6, 7 in parallel) -> (8, 9, 10 in parallel)

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `BackendError::RateLimit` is retryable | `is_retryable()` returns `true` | `cargo test test_backend_error_retryable` |
| 2 | `BackendError::Auth` is NOT retryable | `is_retryable()` returns `false` | `cargo test test_backend_error_not_retryable` |
| 3 | `BackendError` implements `Display` | Each variant formats with message | `cargo test test_backend_error_display` |
| 4 | `BackendErrorKind::from(&BackendError::RateLimit{..})` | Returns `BackendErrorKind::RateLimited` | `cargo test test_backend_error_kind_from` |
| 5 | All 8 `BackendError` -> `BackendErrorKind` mappings correct | Matches mapping table in constraints | `cargo test test_backend_error_kind_mapping_all` |
| 6 | String-based classification still works | `classify_backend_error("429")` returns `RateLimited` | `cargo test test_classify_rate_limit_429` (existing) |
| 7 | `QueryResult` carries typed error | `result.error` is `Some(BackendError::Network{..})` | `cargo test test_query_result_carries_error` |
| 8 | Timeout in `run_query_with_config` produces `BackendError::Timeout` | `result.error` matches `Timeout` variant | `cargo test test_timeout_produces_backend_error` |
| 9 | Full compilation | All backends compile with new return type | `cargo build` |
| 10 | Full test suite | All existing + new tests pass | `cargo test` |
| 11 | Clippy clean | No warnings | `cargo clippy -- -D warnings` |

**Edge cases to verify**:
- CLI backend (codex) returns stderr with no recognizable error pattern -> maps to `ExecutionFailed`
- Ollama reqwest timeout (built-in client timeout, not tokio wrapper) -> maps to `BackendError::Timeout`
- External `tokio::time::timeout` in `run_query_with_config` -> maps to `BackendError::Timeout` (constructed by caller, not backend)
- Claude API returns 529 (overloaded, Anthropic-specific) -> maps to `RateLimit`
- Bedrock feature not enabled -> `create_backend("bedrock")` still returns `anyhow::Error` (init, not query)
- `anyhow::Error` from non-backend sources (e.g., file IO in workflow) -> unaffected, still uses anyhow
- `BackendError::Timeout` from backend propagates through workflow.rs to `StepFailureKind::Timeout` (not catch-all)
