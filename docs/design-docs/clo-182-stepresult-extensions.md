# CLO-182: Extend StepResult with stderr, exit_code, validation fields

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-182
**Status**: Finalized
**Finalized**: 2026-03-31
**Approved By**: Mk Km
**Author**: Mk Km
**Created**: 2026-03-31

---

## Summary

Add `raw_output`, `stderr`, `exit_code`, and `validation` fields to `StepResult`. Define `ValidationResult` and `FailureType` types. Thread `QueryOutput` data (stderr, exit_code) through the single-backend and shell execution paths to populate the new fields. This unblocks CLO-183 (heuristic validators) and the rest of the output validation pipeline.

---

## Background

CLO-180 added `QueryOutput { stdout, stderr, exit_code }` to the `Backend::query()` return type. CLI backends (Claude CLI, Gemini, Codex) now capture stderr and exit codes, but this data is immediately discarded - only `stdout` survives to `StepResult`. The workflow engine has no way to carry validation metadata, structured failure information, or raw (pre-validation) output.

CLO-182 bridges the gap: `StepResult` gains the fields, `QueryOutput` data flows through to them, and new types (`ValidationResult`, `FailureType`) are ready for CLO-183+ to populate.

### Prior Research

**Discovery report**: `docs/prds/discovery-report-2026-03-31-clo-182.md`

Key findings that shaped this design:

1. **`raw_output` should be `Option<String>`, not `String`**: A non-optional `raw_output` forces a clone of `output` at all 35 construction sites even when no validation exists (90%+ of steps). Using `Option<String>` and only populating it when validation mutates output avoids this overhead.

2. **`FailureType` must be scoped to validation failures only**: The PRD's original `FailureType` mixed execution failures (`Timeout`, `BackendError`) with validation failures (`ValidationFailed`, `EmptyOutput`). Execution failures already have `success: false` and an error message in `output` - putting them inside `ValidationResult` would require populating the `validation` field even without a `validate` clause, contradicting the design intent.

3. **QueryOutput threading varies by execution path**: The single-backend path has `qo` in scope - straightforward. The shell path needs `run_shell()` to return structured output. The consensus and for_each paths extract `qo.stdout` into intermediate structures early - threading stderr/exit_code there is complex and deferred.

4. **Prior art**: Temporal SDK's typed error enums with variant-level metadata is the closest pattern. Guardrails AI's inline wrap-validate-retry architecture validates lok's `validate` clause design.

---

## Architecture

### Component Overview

```
Backend::query() -> QueryOutput { stdout, stderr, exit_code }
       |
       |--- Single-backend path (workflow.rs:1189)
       |      qo.stdout -> text, qo.stderr -> step_stderr, qo.exit_code -> step_exit_code
       |      -> StepResult { output: text, stderr: step_stderr, exit_code: step_exit_code, ... }
       |
       |--- Shell path (workflow.rs:871)
       |      run_shell() -> ShellOutput { stdout, stderr, exit_code }
       |      -> StepResult { output: stdout, stderr, exit_code, ... }
       |
       |--- Consensus path (workflow.rs:975)
       |      qo.stdout extracted into (String, Result<String>) tuple
       |      stderr/exit_code NOT available -> StepResult { stderr: None, exit_code: None, ... }
       |
       |--- For_each path (workflow.rs:804)
       |      qo.stdout extracted per iteration
       |      stderr/exit_code NOT available -> StepResult { stderr: None, exit_code: None, ... }

StepResult { ..., raw_output: Option<String>, stderr: Option<String>,
             exit_code: Option<i32>, validation: Option<ValidationResult> }
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/workflow.rs` | Modified | StepResult struct + ValidationResult + FailureType definitions. 20 production construction sites updated. Single-backend path threads stderr/exit_code. Fix-loop re-query paths thread stderr/exit_code. `run_shell()` returns `ShellOutput`. |
| `src/backend/mod.rs` | Modified | Remove `#[allow(dead_code)]` from QueryOutput.stderr and .exit_code |
| Tests in workflow.rs | Modified | 13 test construction sites gain new fields |

### Dependencies

- **Internal**: `QueryOutput` from `src/backend/mod.rs` (CLO-180, done)
- **External**: `serde` (for Serialize/Deserialize on new types - future use), `serde_json` (existing)

---

## Detailed Design

### New Types

```rust
// src/workflow.rs

/// Result of executing a step
#[derive(Debug, Clone)]
pub struct StepResult {
    pub name: String,
    pub output: String,
    pub parsed_output: Option<serde_json::Value>,
    pub success: bool,
    pub elapsed_ms: u64,
    pub backend: Option<String>,
    // --- New fields (CLO-182) ---
    /// Original output before validation cleaning. None when no validation ran.
    pub raw_output: Option<String>,
    /// Captured stderr from CLI backends. None for API backends and error-path results.
    pub stderr: Option<String>,
    /// Process exit code from CLI backends. None for API backends, error-path results,
    /// and processes killed by signal (Unix: status.code() returns None for signal kills).
    pub exit_code: Option<i32>,
    /// Validation result. None when step has no `validate` clause.
    pub validation: Option<ValidationResult>,
}

/// Result of validating a step's output.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub failure_type: Option<FailureType>,
    pub failure_reason: Option<String>,
    /// Identifier for which validator ran: "heuristic:not_empty", "heuristic:min_length", "llm:haiku"
    pub validator: String,
    pub elapsed_ms: u64,
}

/// Why a validation check failed. Scoped to validation-domain failures only.
/// Execution failures (timeout, backend error) are represented by StepResult.success = false.
#[derive(Debug, Clone)]
pub enum FailureType {
    /// Output failed a heuristic or LLM validation check
    ValidationFailed,
    /// Output was empty or whitespace-only
    EmptyOutput,
}
```

**Design decisions**:
- `raw_output: Option<String>` - Only populated by future validation tasks (CLO-183+) when the validator modifies the output. For now, always `None`.
- `FailureType` has 2 variants, not 5. Execution failures (`Timeout`, `BackendError`, `HealthCheckFailed`) are already handled by `success: false` + error in `output`. If structured execution failure metadata is needed later, a separate `failure_info: Option<FailureInfo>` field can be added.
- `validation: Option<ValidationResult>` - Always `None` in CLO-182. CLO-183+ will populate this when a step has a `validate` clause.

### ShellOutput struct

```rust
// src/workflow.rs (private, near run_shell function)

/// Structured output from a shell command.
struct ShellOutput {
    pub stdout: String,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}
```

### run_shell() Change

```rust
// Current: returns Result<String> (merges stdout+stderr)
// Proposed: returns Result<ShellOutput> (separates them)

async fn run_shell(cmd: &str, cwd: &Path, wrapper: Option<&str>) -> Result<ShellOutput> {
    // ... existing spawn logic unchanged ...

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();

    if !output.status.success() {
        anyhow::bail!("Shell command failed: {}\n{}", final_cmd, stderr_str);
    }

    Ok(ShellOutput {
        stdout,
        stderr: Some(stderr_str).filter(|s| !s.is_empty()),
        exit_code,
    })
}
```

**Breaking change from current behavior**: Currently `run_shell` returns `format!("{}{}", stdout, stderr).trim()` - it concatenates stdout and stderr. After this change, only `stdout` goes into `StepResult.output` and `stderr` goes into `StepResult.stderr`. This is the correct behavior (separate the streams) but may change output for shell steps that previously included stderr in their output. This matches the PRD's intent (FR-1: "Stderr written to a sidecar string, not merged into the query result").

### Single-Backend Path Threading

```rust
// workflow.rs, around line 1168-1192
// Current:
let mut text = String::new();
// ...
Ok(Ok(qo)) => {
    text = qo.stdout;
    query_success = true;
    break;
}

// Proposed: add variables to carry stderr/exit_code
let mut text = String::new();
let mut step_stderr: Option<String> = None;
let mut step_exit_code: Option<i32> = None;
// ...
Ok(Ok(qo)) => {
    text = qo.stdout;
    step_stderr = qo.stderr;
    step_exit_code = qo.exit_code;
    query_success = true;
    break;
}
```

Then at the StepResult construction (line 1485):
```rust
StepResult {
    name: step_name,
    output: current_text,
    parsed_output: parsed,
    success: true,
    elapsed_ms,
    backend: Some(backend_name),
    stderr: step_stderr,
    exit_code: step_exit_code,
    raw_output: None,
    validation: None,
}
```

### Construction Site Strategy

All 33 construction sites need the 4 new fields. The approach differs by site type:

| Site Type | Count | stderr | exit_code | raw_output | validation |
|-----------|-------|--------|-----------|------------|------------|
| Single-backend success | 1 | From QueryOutput | From QueryOutput | None | None |
| Shell success | 1 | From ShellOutput | From ShellOutput | None | None |
| Consensus success | 1 | None (multi-backend) | None | None | None |
| For_each success | 1 | None (per-iteration) | None | None | None |
| Error paths (production) | 16 | None | None | None | None |
| Test fixtures | 13 | None | None | None | None |

For the ~16 error-path construction sites, consider a helper constructor to reduce boilerplate:

```rust
impl StepResult {
    /// Create an error result with all extension fields set to None.
    fn error(name: String, output: String, elapsed_ms: u64, backend: Option<String>) -> Self {
        Self {
            name,
            output,
            parsed_output: None,
            success: false,
            elapsed_ms,
            backend,
            raw_output: None,
            stderr: None,
            exit_code: None,
            validation: None,
        }
    }
}
```

### Fix-Loop Re-Query Paths

Lines 1396-1398 and 1444-1446 extract `qo.stdout` during the apply/verify fix-retry cycle. When the fix loop re-queries the backend, `step_stderr` and `step_exit_code` should be updated with the latest query's values (not retained from the initial query):

```rust
// Lines 1396-1398 (re-query after verify failure)
Ok(Ok(qo)) => {
    current_text = qo.stdout;
    step_stderr = qo.stderr;      // update with latest
    step_exit_code = qo.exit_code; // update with latest
}
```

This ensures StepResult reflects the final successful query's process metadata, not the initial attempt's.

### Synthesis Path

The synthesis backend query at line 1080 also extracts `qo.stdout`, discarding stderr/exit_code. This is part of the consensus path and inherits its `stderr: None, exit_code: None` behavior. Documented here for completeness.

### backend/mod.rs Change

Remove the `#[allow(dead_code)]` attributes since the fields are now consumed:

```rust
pub struct QueryOutput {
    pub stdout: String,
    pub stderr: Option<String>,     // remove #[allow(dead_code)]
    pub exit_code: Option<i32>,     // remove #[allow(dead_code)]
}
```

---

## Implementation Plan

### Phase 1: Define Types

- [ ] Add `ValidationResult` struct to `workflow.rs`
- [ ] Add `FailureType` enum to `workflow.rs`
- [ ] Add `ShellOutput` struct to `workflow.rs` (private)
- [ ] Add 4 new fields to `StepResult`: `raw_output`, `stderr`, `exit_code`, `validation`

### Phase 2: Update run_shell()

- [ ] Change `run_shell()` return type from `Result<String>` to `Result<ShellOutput>`
- [ ] Separate stdout and stderr in return value (stop concatenating)
- [ ] Update shell step construction sites to use `ShellOutput` fields

### Phase 3: Thread QueryOutput in Single-Backend Path

- [ ] Add `step_stderr` and `step_exit_code` variables in retry loop scope
- [ ] Capture `qo.stderr` and `qo.exit_code` from successful query
- [ ] Update fix-loop re-query paths (lines ~1396-1398, ~1444-1446) to refresh stderr/exit_code
- [ ] Pass to StepResult construction at success site

### Phase 4: Update All Remaining Construction Sites

- [ ] Update consensus path StepResult (stderr: None, exit_code: None)
- [ ] Update for_each path StepResult
- [ ] Update all error-path StepResults (~16 sites, use `StepResult::error()` helper)
- [ ] Update all test fixture StepResults (13 sites)

### Phase 5: Clean Up backend/mod.rs

- [ ] Remove `#[allow(dead_code)]` from `QueryOutput.stderr` and `QueryOutput.exit_code`

### Phase 6: Testing & Validation

- [ ] `cargo test` passes with 0 failures
- [ ] `cargo clippy` passes with 0 warnings
- [ ] `cargo build --features bedrock` compiles

---

## Constraints

**Must**:
- All existing tests pass without modification to test assertions (only construction sites change)
- New StepResult fields are `Option<T>` - no breaking changes to consumers
- `stderr` and `exit_code` populated for single-backend and shell step successes
- `stderr` and `exit_code` are `None` for consensus/for_each paths and all error paths
- `validation` is `None` for all steps (populated by CLO-183+)
- `raw_output` is `None` for all steps (populated when validation modifies output)

**Must-not**:
- Must not change the semantics of `StepResult.success` (that's CLO-183+ when validation affects success)
- Must not change `StepResult.output` content for LLM backend paths (shell path changes are intentional per FR-1)
- Must not populate `validation` field in this task
- Must not add execution-level failure variants to `FailureType`

**Prefer**:
- Use explicit `None` at construction sites over a Default impl (keeps each site's intent clear)
- Keep `ShellOutput` private to workflow.rs (it's an implementation detail)

**Escalate when**:
- Any existing test assertion fails (not just construction-site compilation)
- `run_shell()` change causes behavioral differences in shell step output content
- Consensus path needs stderr threading sooner than expected

---

## Acceptance Criteria

- [ ] `StepResult` has 4 new fields: `raw_output: Option<String>`, `stderr: Option<String>`, `exit_code: Option<i32>`, `validation: Option<ValidationResult>` - verified by `grep "pub raw_output\|pub stderr\|pub exit_code\|pub validation" src/workflow.rs`
- [ ] `ValidationResult` struct defined with `passed`, `failure_type`, `failure_reason`, `validator`, `elapsed_ms` fields - verified by `grep "pub struct ValidationResult" src/workflow.rs`
- [ ] `FailureType` enum has exactly 2 variants: `ValidationFailed`, `EmptyOutput` - verified by `grep -A5 "pub enum FailureType" src/workflow.rs`
- [ ] `#[allow(dead_code)]` removed from `QueryOutput.stderr` and `.exit_code` - verified by `grep -c "allow(dead_code)" src/backend/mod.rs` returning 0
- [ ] `run_shell()` returns `ShellOutput` with separated stdout/stderr - verified by `grep "fn run_shell" src/workflow.rs`
- [ ] `cargo test` passes with 0 failures
- [ ] `cargo clippy` passes with 0 warnings

**Verification method**: `cargo test && cargo clippy -- -D warnings`

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | All existing unit tests pass | 0 failures | `cargo test` |
| 2 | Clippy passes cleanly | 0 warnings | `cargo clippy -- -D warnings` |
| 3 | StepResult has new fields | 4 new pub fields present | `grep -c "pub raw_output\|pub stderr\|pub exit_code\|pub validation" src/workflow.rs` returns 4 |
| 4 | ValidationResult struct exists | Struct with 5 fields | `grep "pub struct ValidationResult" src/workflow.rs` |
| 5 | FailureType has 2 variants | Only ValidationFailed, EmptyOutput | `grep -A5 "pub enum FailureType" src/workflow.rs` |
| 6 | dead_code annotations removed | 0 occurrences in backend/mod.rs | `grep -c "allow(dead_code)" src/backend/mod.rs` returns 0 |
| 7 | run_shell returns ShellOutput | Signature updated | `grep "ShellOutput" src/workflow.rs` |
| 8 | Bedrock feature compiles | Success | `cargo build --features bedrock` |

**Edge cases to cover**:
- Shell step with non-empty stderr on success (stderr should populate `StepResult.stderr`)
- Shell step where stderr was previously part of output (behavior change - only stdout now)
- Shell step with empty stdout but non-empty stderr (exit 0) - `StepResult.output` will be empty, stderr in `.stderr` field. This is expected: empty stdout means no content, even if the process emitted warnings to stderr.
- Consensus step result has `stderr: None` (no single QueryOutput to draw from)
- Error-path StepResult has all new fields as `None`
- Process killed by signal (SIGKILL/SIGTERM) - `exit_code` is `None` (not just for API backends)

---

## Testing Strategy

- **Unit Tests**: No new tests needed for CLO-182 - the new fields are all `None` or passthrough. New tests will be added in CLO-183 when validation logic is implemented.
- **Integration Tests**: Existing workflow tests verify behavioral compatibility - same outputs, same success/failure semantics.
- **Manual Testing**: Not required - this is a pure data structure extension with no behavioral changes except `run_shell()` stdout/stderr separation.

---

## Follow-Up Notes (not in scope for CLO-182)

- `print_results()` and `format_results()` (workflow.rs ~lines 2620-2640) display step results. They should show stderr/exit_code when present in verbose/debug mode. Tracked as a follow-up, not part of this task.
- Template interpolation for `{{ steps.X.stderr }}` and `{{ steps.X.exit_code }}` requires changes to the interpolation engine (reads `parsed_output` JSON, not struct fields). Deferred per discovery report.

## Open Questions

- [x] ~~Should `run_shell()` separating stdout from stderr be a breaking behavior change in CLO-182, or should it be deferred?~~ **Resolved: Do it now.** The PRD requires it (FR-1), and delaying creates a harder migration later. Shell steps that relied on seeing stderr in `{{ steps.X.output }}` will need to use `{{ steps.X.stderr }}` instead (once template interpolation is extended in a follow-up task).

---

## References

- [Linear Task](https://linear.app/cloud-ai/issue/CLO-182)
- [Output Validation Pipeline PRD](../prds/prd-output-validation-pipeline.md)
- [Discovery Report](../prds/discovery-report-2026-03-31-clo-182.md)
- [CLO-180 Design Doc](clo-180-query-output-struct.md) - QueryOutput struct (dependency, done)
- [Blocks CLO-183](https://linear.app/cloud-ai/issue/CLO-183) - Heuristic validators
