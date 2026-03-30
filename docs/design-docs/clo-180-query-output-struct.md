# CLO-180: Extend Backend::query() to return QueryOutput struct

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-180
**Status**: Finalized
**Finalized**: 2026-03-30
**Approved By**: Mk Km
**Author**: Mk Km
**Created**: 2026-03-30

---

## Summary

Change the `Backend` trait's `query()` return type from `Result<String>` to `Result<QueryOutput>` to capture stderr and exit code separately from stdout. This is the foundational internal change for the output validation pipeline (PRD: `docs/prds/prd-output-validation-pipeline.md`), unblocking CLO-181 (per-step model override) and CLO-182 (StepResult extensions).

---

## Background

Lok's `Backend::query()` returns `Result<String>` - just the stdout/response text. Three CLI backends (Claude CLI, Gemini, Codex) already pipe stderr separately via `Stdio::piped()` and capture exit codes, but **discard both on success**. Two API backends (Ollama, Bedrock) plus Claude's API mode use HTTP/SDK calls with no process I/O.

The output validation pipeline needs structured process output to distinguish noise from content. MCP initialization noise from Gemini CLI and empty output from Ollama are currently invisible to the workflow engine because stderr and exit codes are lost at the `query()` boundary.

### Prior Research

**Discovery report**: `docs/prds/discovery-report-2026-03-30.md`

Key findings relevant to this task:

1. **Tokio subprocess patterns**: The canonical Rust pattern for concurrent stdout/stderr capture uses `ChildStdout`/`ChildStderr` with `take()` + `tokio::join!`. The current CLI backends already use `Stdio::piped()` for both streams - the fix is returning existing captured data, not re-architecting capture. (gemini.rs:62-63 already pipes stderr but discards it at line 71).

2. **Competitive analysis**: No existing LLM orchestration tool captures CLI subprocess stderr/exit_code in a structured return type. All validation frameworks (Guardrails AI, Instructor, NeMo) wrap API calls only. This trait change fills a genuine gap.

3. **Blast radius** (validated by AI review): The trait change touches 5 backend implementations, `run_query_with_config()`, and **13 direct `backend.query()` call sites** across 6 files: workflow.rs (6), conductor.rs (1), spawn.rs (2), debate.rs (1), team.rs (2), backend/mod.rs (1). All callers currently use the `String` result as-is - they need `.stdout` instead. Additionally, `conductor.rs:191` calls `.len()` on the query result, which will fail to compile on `QueryOutput`.

4. **Exit code insufficiency** (Azure CLI case study): Exit code 0 can mask complete failures (empty output). The combination of exit_code + stdout content is needed, not exit_code alone.

---

## Architecture

### Component Overview

The `Backend` trait is the central abstraction for all LLM query execution. Every workflow step, consensus query, retry, and synthesis call goes through this trait.

```
                    Backend trait
                   query() -> Result<QueryOutput>
                        |
        ┌───────────────┼───────────────────────────┐
        |               |               |            |
   CLI backends    API backends    HTTP backend   SDK backend
   (claude-cli,   (claude-api)    (ollama)       (bedrock)
    gemini, codex)
        |               |               |            |
   stdout + stderr  text only      text only     text only
   + exit_code      (None/None)    (None/None)   (None/None)
        |               |               |            |
        └───────────────┴───────────────┴────────────┘
                        |
              run_query_with_config()
                        |
              QueryResult { output, ... }
                        |
         ┌──────────────┼──────────────┐
         |              |              |
    workflow.rs     conductor.rs   spawn.rs      debate.rs    team.rs     main.rs
    (6 sites)       (1 site)       (2 sites)     (1 site)     (2 sites)   (via QueryResult)
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/backend/mod.rs` | Modified | New `QueryOutput` struct, trait signature change, `run_query_with_config()` updated |
| `src/backend/claude.rs` | Modified | Return `QueryOutput` from both API and CLI modes |
| `src/backend/gemini.rs` | Modified | Return captured stderr and exit code (already piped) |
| `src/backend/codex.rs` | Modified | Return captured stderr and exit code (already piped) |
| `src/backend/ollama.rs` | Modified | Return `QueryOutput` with `stderr: None, exit_code: None` |
| `src/backend/bedrock.rs` | Modified | Return `QueryOutput` with `stderr: None, exit_code: None` |
| `src/workflow.rs` | Modified | 6 `backend.query()` call sites use `.stdout` |
| `src/conductor.rs` | Modified | 1 `backend.query()` call site; line 191 `.len()` call needs `.stdout.len()` |
| `src/spawn.rs` | Modified | 2 `backend.query()` call sites use `.stdout` |
| `src/debate.rs` | Modified | 1 direct `backend.query()` call site uses `.stdout` |
| `src/team.rs` | Modified | 2 `backend.query()` call sites use `.stdout` |
| `src/main.rs` | No change | Uses `QueryResult.output` (unchanged via `run_query`) |
| `src/tasks/*.rs` | No change | Uses `QueryResult.output` (unchanged via `run_query`) |

### Dependencies

- **Internal**: `workflow.rs` (consumer), `main.rs` (consumer via `run_query`)
- **External**: `async_trait` (trait definition), `tokio` (subprocess I/O), `anyhow` (error handling)

---

## Detailed Design

### New QueryOutput Struct

```rust
// src/backend/mod.rs

/// Structured output from a backend query, capturing stdout, stderr, and exit code.
#[derive(Debug, Clone)]
pub struct QueryOutput {
    pub stdout: String,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}

impl QueryOutput {
    /// Create output for API backends (no process I/O).
    pub fn from_text(text: String) -> Self {
        Self {
            stdout: text,
            stderr: None,
            exit_code: None,
        }
    }

    /// Create output for CLI backends with full process data.
    pub fn from_process(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self {
            stdout,
            stderr: Some(stderr),
            exit_code: Some(exit_code),
        }
    }
}
```

### Trait Signature Change

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput>;  // changed
    fn is_available(&self) -> bool;
}
```

### Backend Implementation Changes

**API backends** (claude API mode, ollama, bedrock) - minimal change:
```rust
// Wrap existing text return with QueryOutput::from_text()
Ok(QueryOutput::from_text(text))
```

**CLI backends** (claude CLI, gemini, codex) - return existing captured data:
```rust
// Already have: let output = child.wait_with_output().await?;
let exit_code = output.status.code().unwrap_or(-1);
let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();

if !output.status.success() {
    anyhow::bail!("... exit code {}: {}", exit_code, stderr_str);
}

// For Gemini/Codex: parse_output() runs BEFORE QueryOutput construction
let parsed_stdout = parse_output(&stdout_str, config);

// Normalize empty stderr to None for cleaner downstream matching
let stderr_opt = Some(stderr_str).filter(|s| !s.is_empty());

Ok(QueryOutput {
    stdout: parsed_stdout,
    stderr: stderr_opt,
    exit_code: Some(exit_code),
})
```

**Note on error path**: When a command fails (non-zero exit code), the function continues to `bail!` - only successful queries return `QueryOutput`. Stderr from failed processes is included in the error message as before. This is a deliberate design choice: failed queries are errors, not structured output.

**Note on internal methods**: Claude backend's `query_with_system()` continues to return `Result<String>` internally. The wrapping to `QueryOutput` happens at the `Backend::query()` impl level, not in internal helper methods.

### run_query_with_config() Change

The wrapping layer converts `Result<QueryOutput>` to `QueryResult`. Only `stdout` propagates to `QueryResult.output`:

```rust
// Line 148-155 in mod.rs, currently:
Ok(Ok(output)) => QueryResult {
    backend: backend.name().to_string(),
    output,           // was String, now QueryOutput
    success: true,
    elapsed_ms,
}

// Becomes:
Ok(Ok(query_output)) => QueryResult {
    backend: backend.name().to_string(),
    output: query_output.stdout,  // extract stdout only
    success: true,
    elapsed_ms,
}
```

This means **all consumers of `QueryResult` are unchanged** - they continue to see `output: String` containing stdout.

### workflow.rs Call Site Changes

All 13 direct `backend.query()` call sites across 6 files follow the same pattern:

```rust
// Currently (e.g., line 1182):
Ok(Ok(t)) => { text = t; ... }

// Becomes:
Ok(Ok(qo)) => { text = qo.stdout; ... }
```

**Special case - conductor.rs:191**: Currently calls `.len()` on the query result (a `String`). Must change to `.stdout.len()`:
```rust
// Before: result.len()
// After:  result.stdout.len()
```

The `QueryOutput` is destructured at each call site - only `stdout` is used in this task. Future tasks (CLO-182) will propagate `stderr` and `exit_code` to `StepResult`.

### API/Interface Design

| Function/Method | Parameters | Returns | Description |
|-----------------|------------|---------|-------------|
| `Backend::query()` | `prompt: &str, cwd: &Path` | `Result<QueryOutput>` | Core query - changed return type |
| `QueryOutput::from_text()` | `text: String` | `QueryOutput` | Constructor for API backends |
| `QueryOutput::from_process()` | `stdout: String, stderr: String, exit_code: i32` | `QueryOutput` | Constructor for CLI backends |

---

## Implementation Plan

### Phase 1: Define QueryOutput and Change Trait

- [ ] Add `QueryOutput` struct to `src/backend/mod.rs`
- [ ] Add `from_text()` and `from_process()` constructors
- [ ] Change `Backend::query()` signature to return `Result<QueryOutput>`

### Phase 2: Update All Backend Implementations

- [ ] Update Claude backend - API mode: `QueryOutput::from_text()`
- [ ] Update Claude backend - CLI mode: `QueryOutput::from_process()` with captured stderr/exit_code
- [ ] Update Gemini backend - CLI: `QueryOutput::from_process()` (stderr already captured at line 71)
- [ ] Update Codex backend - CLI: `QueryOutput::from_process()` with captured stderr/exit_code
- [ ] Update Ollama backend - HTTP: `QueryOutput::from_text()`
- [ ] Update Bedrock backend - SDK: `QueryOutput::from_text()`

### Phase 3: Update All Callers (13 call sites across 6 files)

- [ ] Update `run_query_with_config()` in `backend/mod.rs` to extract `.stdout`
- [ ] Update 6 `backend.query()` call sites in `workflow.rs` to use `.stdout`
- [ ] Update 1 call site in `conductor.rs` - fix `.len()` -> `.stdout.len()`
- [ ] Update 2 call sites in `spawn.rs` to use `.stdout`
- [ ] Update 1 call site in `debate.rs` to use `.stdout`
- [ ] Update 2 call sites in `team.rs` to use `.stdout`

### Phase 4: Testing & Validation

- [ ] All existing tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Manual test: run a workflow with Gemini backend, verify stdout output unchanged

---

## Constraints

**Must**:
- All existing tests pass without modification (behavioral compatibility)
- API backends return `stderr: None, exit_code: None` (no fabricated values)
- CLI backends return actual stderr content and exit code on success
- Error path (non-zero exit code) continues to `bail!` as before - `QueryOutput` is only for successful queries
- `QueryResult.output` continues to contain stdout only (consumer compatibility)

**Must-not**:
- Must not change the `QueryResult` struct (that's a separate concern for `run_query` consumers)
- Must not merge stderr into stdout (the whole point is separation)
- Must not change workflow behavior - downstream steps see identical output
- Must not add `model: Option<&str>` parameter to `query()` (that's CLO-181)

**Prefer**:
- Use `QueryOutput::from_text()` / `from_process()` constructors over raw struct construction
- Keep the error path unchanged - only wrap the success path in `QueryOutput`

**Escalate when**:
- Any test in `workflow.rs` fails due to the change (may indicate hidden coupling)
- The Gemini `parse_output()` or Codex `parse_output()` functions need signature changes

---

## Acceptance Criteria

- [ ] `QueryOutput` struct defined in `src/backend/mod.rs` with `stdout: String`, `stderr: Option<String>`, `exit_code: Option<i32>`
- [ ] `Backend::query()` signature returns `Result<QueryOutput>` across all 5 backends (6 with Bedrock feature)
- [ ] CLI backends (Claude CLI, Gemini, Codex) populate `stderr` and `exit_code` from process output
- [ ] API backends (Claude API, Ollama, Bedrock) return `stderr: None, exit_code: None`
- [ ] `cargo test` passes with 0 failures
- [ ] `cargo clippy` passes with 0 warnings
- [ ] Existing workflow behavior unchanged - stdout is still passed as step output

**Verification method**: `cargo test && cargo clippy -- -D warnings`

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | All existing unit tests pass | 0 failures | `cargo test` |
| 2 | Clippy passes cleanly | 0 warnings | `cargo clippy -- -D warnings` |
| 3 | Project compiles with bedrock feature | Success | `cargo build --features bedrock` |
| 4 | QueryOutput::from_text creates correct struct | stderr=None, exit_code=None | Unit test |
| 5 | QueryOutput::from_process captures stderr | stderr=Some("..."), exit_code=Some(0) | Unit test |
| 6 | run_query_with_config extracts stdout | QueryResult.output == stdout content | Existing integration tests |
| 7 | Empty stdout with exit code 0 | QueryOutput.stdout == "", no error | Unit test |
| 8 | Empty stderr normalized to None | from_process("out", "", 0).stderr == None | Unit test |
| 9 | Bedrock feature gate compiles | Success | `cargo test --features bedrock` |

**Edge cases to cover**:
- CLI backend returns exit code 0 with non-empty stderr (warnings) - stderr should still be captured
- CLI backend returns empty stdout with exit code 0 - `QueryOutput.stdout` is empty string, not an error
- Bedrock feature-gated compilation - ensure both `--features bedrock` and default compile

---

## Testing Strategy

- **Unit Tests**: Add tests for `QueryOutput::from_text()` and `QueryOutput::from_process()` constructors
- **Integration Tests**: Existing workflow tests verify behavioral compatibility (stdout pass-through)
- **Manual Testing**: Run `lok query "hello"` with a configured backend, verify output is unchanged

---

## Open Questions

*Resolved during AI review:*
- ~~Should `QueryOutput` implement `Debug`, `Clone`?~~ **Yes** - `#[derive(Debug, Clone)]` added.
- ~~Should `parse_output()` run before or after `QueryOutput` construction?~~ **Before** - parsed stdout goes into `QueryOutput.stdout`.

*Remaining:*
- [ ] None - all open questions resolved.

---

## References

- [Linear Task](https://linear.app/cloud-ai/issue/CLO-180)
- [Output Validation Pipeline PRD](../prds/prd-output-validation-pipeline.md)
- [Discovery Report](../prds/discovery-report-2026-03-30.md)
- [Blocks CLO-181](https://linear.app/cloud-ai/issue/CLO-181) - Per-step model override
- [Blocks CLO-182](https://linear.app/cloud-ai/issue/CLO-182) - StepResult extensions
