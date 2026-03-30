# CLO-180 Implementation Plan: Extend Backend::query() to return QueryOutput struct

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-180
**Design Document**: docs/design-docs/clo-180-query-output-struct.md
**Created**: 2026-03-30
**Overall Progress**: 100% (21/21 tasks completed)

---

## Architecture Context

The `Backend` trait is the central abstraction for all LLM query execution. This plan changes the trait's `query()` return type from `Result<String>` to `Result<QueryOutput>` to capture stderr and exit code separately. 5 backend implementations, `run_query_with_config()`, and 13 direct call sites across 6 files must be updated. All `QueryResult` consumers (main.rs, tasks/*.rs) are unaffected because `QueryResult.output` remains a `String`.

---

## Tasks

### Phase 1: Define QueryOutput and Change Trait (src/backend/mod.rs)

- [x] Task 1: Add `QueryOutput` struct with `#[derive(Debug, Clone)]`
  - [x] Define `pub struct QueryOutput { pub stdout: String, pub stderr: Option<String>, pub exit_code: Option<i32> }`
  - [x] Add `pub fn from_text(text: String) -> Self` constructor (stderr=None, exit_code=None)
  - [x] Add `pub fn from_process(stdout: String, stderr: String, exit_code: i32) -> Self` constructor (normalizes empty stderr to None)

- [x] Task 2: Change `Backend::query()` trait signature
  - [x] Change return type from `Result<String>` to `Result<QueryOutput>`
  - [x] Update `run_query_with_config()` to extract `.stdout` from `QueryOutput` into `QueryResult.output`

### Phase 2: Update Backend Implementations

- [x] Task 3: Update Claude backend (`src/backend/claude.rs`)
  - [x] API mode: wrap text response with `QueryOutput::from_text()`
  - [x] CLI mode: capture stderr and exit_code into `QueryOutput::from_process()` (stderr already piped)
  - [x] Internal `query_with_system()` stays `Result<String>` - wrapping at `Backend::query()` level

- [x] Task 4: Update Gemini backend (`src/backend/gemini.rs`)
  - [x] Capture stderr (already piped at line 62-63, read at line 71 but discarded)
  - [x] Capture exit code from `output.status.code()`
  - [x] Run `parse_output()` on stdout before `QueryOutput` construction
  - [x] Return `QueryOutput::from_process()` with parsed stdout, stderr, exit_code

- [x] Task 5: Update Codex backend (`src/backend/codex.rs`)
  - [x] Capture stderr (already piped at line 69-70)
  - [x] Capture exit code from `output.status.code()`
  - [x] Run `parse_output()` on stdout before `QueryOutput` construction
  - [x] Return `QueryOutput::from_process()` with parsed stdout, stderr, exit_code

- [x] Task 6: Update Ollama backend (`src/backend/ollama.rs`)
  - [x] Wrap response text with `QueryOutput::from_text()`

- [x] Task 7: Update Bedrock backend (`src/backend/bedrock.rs`)
  - [x] Wrap response text with `QueryOutput::from_text()`

### Phase 3: Update All Callers (13 call sites across 6 files)

- [x] Task 8: Update `workflow.rs` (6 call sites)
  - [x] Single-backend query with retries (~line 1182): `Ok(Ok(t))` -> `Ok(Ok(qo))`, `text = qo.stdout`
  - [x] For-each iteration query (~line 797): extract `.stdout`
  - [x] Multi-backend fan-out (~line 968): extract `.stdout`
  - [x] Synthesis query (~line 1072): extract `.stdout`
  - [x] Fix-retry re-query #1 (~line 1389): extract `.stdout`
  - [x] Fix-retry re-query #2 (~line 1437): extract `.stdout`

- [x] Task 9: Update `conductor.rs` (1 call site)
  - [x] Update query call to extract `.stdout`
  - [x] Fix `.len()` call at line 191: change to `.stdout.len()`

- [x] Task 10: Update `spawn.rs` (2 call sites)
  - [x] Update both query calls to extract `.stdout`

- [x] Task 11: Update `debate.rs` (1 call site)
  - [x] Update query call to extract `.stdout`

- [x] Task 12: Update `team.rs` (2 call sites)
  - [x] Update both query calls to extract `.stdout`

### Phase 4: Testing and Validation

- [x] Task 13: Add unit tests for `QueryOutput` constructors
  - [x] Test `from_text()`: verify stderr=None, exit_code=None
  - [x] Test `from_process()` with non-empty stderr: verify stderr=Some(...)
  - [x] Test `from_process()` with empty stderr: verify stderr=None (normalization)
  - [x] Test `from_process()` with empty stdout + exit code 0: verify no error

- [x] Task 14: Run full test suite
  - [x] `cargo test` - all existing tests pass (0 failures)
  - [x] `cargo clippy -- -D warnings` - no warnings
  - [x] `cargo build --features bedrock` - feature gate compiles
  - [x] `cargo test --features bedrock` - feature-gated tests pass

### Phase 5: Finalization

- [x] Task 15: Commit and create PR
  - [x] Commit with message: `feat(CLO-180): extend Backend::query() to return QueryOutput struct`
  - [x] Push branch: `git push -u origin feat/clo-180-query-output-struct`
  - [x] Create PR via `/pr:create CLO-180`

---

## Module Structure

Modified modules:
- `src/backend/mod.rs` - New `QueryOutput` struct, trait change, `run_query_with_config()` update
- `src/backend/claude.rs` - API + CLI mode return `QueryOutput`
- `src/backend/gemini.rs` - Return captured stderr/exit_code
- `src/backend/codex.rs` - Return captured stderr/exit_code
- `src/backend/ollama.rs` - Wrap with `QueryOutput::from_text()`
- `src/backend/bedrock.rs` - Wrap with `QueryOutput::from_text()`
- `src/workflow.rs` - 6 call sites extract `.stdout`
- `src/conductor.rs` - 1 call site + `.len()` fix
- `src/spawn.rs` - 2 call sites extract `.stdout`
- `src/debate.rs` - 1 call site extract `.stdout`
- `src/team.rs` - 2 call sites extract `.stdout`

Unchanged modules (use `QueryResult.output` via `run_query`):
- `src/main.rs`
- `src/tasks/fix.rs`, `src/tasks/ci.rs`, `src/tasks/implement.rs`, `src/tasks/spec.rs`, `src/tasks/hunt.rs`
- `src/output.rs`

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage will be recalculated based on completed tasks.

---

## Notes

- Phase 1 and 2 cannot be split - the trait change and all implementations must compile together
- Phase 3 can be done incrementally file by file, but all must be done before `cargo test` passes
- The error path (non-zero exit code -> `bail!`) is unchanged in all backends
- `query_with_system()` in claude.rs stays as `Result<String>` internally
- Empty stderr is normalized to `None` via `.filter(|s| !s.is_empty())`
