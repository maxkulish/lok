# Spec: Implement DiffApplier, Rollback, Verification, and RetryLoop

**Created**: 2026-04-08
**Estimated scope**: L (4 new files + 1 mod edit, ~7 sub-tasks, target: 35+ tests)

## 1. Problem Statement

lok's current edit pipeline is built around a minimal `apply_edits()` function in `src/workflow.rs:2919-2961` that does simple string replacement on JSON old/new pairs. It has multiple gaps that prevent it from handling realistic LLM edit flows:

1. **No rollback on failure.** If verification after `apply_edits` fails, there is no mechanism to restore original file contents - the repo is left in a broken state and the step executor relies on `git_agent::checkpoint()` for recovery (`src/workflow.rs:1939-1941`). That couples the apply pipeline to an external git checkpoint tool.
2. **Verification is ad-hoc.** The `step.verify: Option<String>` field (`src/workflow.rs:236`) shells out via `run_shell()` (`src/workflow.rs:2706-2750`), but `run_shell()` bails on non-zero exit code (`anyhow::bail!` at line 2742) instead of returning a structured verification result. Callers cannot distinguish "verify ran and returned failure" from "verify could not run at all".
3. **Retry is half-wired.** `step.fix_retries: u32` exists (`src/workflow.rs:239`) and the executor has a `fix_attempt` loop (`src/workflow.rs:1933-1935`), but there is no component that orchestrates the full parse -> apply -> verify -> rollback -> re-query cycle as a unit. The retry logic is inlined in `WorkflowRunner::run` and tied to the execution path.
4. **Three-format support is unused.** CLO-205 added `EditParser` at `src/apply_verify/edit_parser.rs` that normalizes unified diff, JSON old/new pairs, and full file content into `Vec<FileEdit>`. But nothing in `src/apply_verify/mod.rs` can actually *apply* those normalized edits - the module only parses.

This task creates the four apply-verify components that bridge the gap between `EditParser` (CLO-205) and the workflow executor (CLO-211). It is pure infrastructure: four new modules under `src/apply_verify/` with unit tests and no changes to `src/workflow.rs` execution path. CLO-211 is the wire-up task that will replace the legacy `apply_edits()` + `run_shell()` + inline retry logic with the new pipeline.

**Key existing types involved** (do not modify):
- `FileEdit` (`src/workflow.rs:83-87`): `file: String`, `old: String`, `new: String`
- `ParsedEdits` (`src/apply_verify/edit_parser.rs:20-28`): `edits: Vec<FileEdit>`, `format: EditFormat`, `summary: Option<String>`
- `EditFormat` (`src/apply_verify/edit_parser.rs:7-16`): `UnifiedDiff | JsonOldNew | FullFile`

**Reference implementation**: llm-mux (https://github.com/ducks/llm-mux) - see `docs/prds/prd-llm-mux-port.md:120-144` for the mapped port.

## 2. Acceptance Criteria

**Module structure**
- [ ] `src/apply_verify/diff_applier.rs` created
- [ ] `src/apply_verify/rollback.rs` created
- [ ] `src/apply_verify/verification.rs` created
- [ ] `src/apply_verify/retry_loop.rs` created
- [ ] `src/apply_verify/mod.rs` declares the four new modules and re-exports the public types listed below
- [ ] No changes to `src/workflow.rs` except adding `#[allow(dead_code)]` or equivalent suppression only if the compiler would otherwise complain about pre-existing items (should not be needed)

**DiffApplier (`diff_applier.rs`)**
- [ ] `pub struct DiffApplier;` with an async `apply()` method
- [ ] `pub async fn apply(&self, parsed: &ParsedEdits, cwd: &Path) -> Result<ApplyResult, ApplyError>` - takes a reference to the parsed edits from `EditParser`
- [ ] `pub struct ApplyResult { pub modified_files: Vec<FileBackup>, pub format_applied: EditFormat }` - non-exhaustive so additional metadata fields can be added later
- [ ] `pub struct FileBackup { pub path: PathBuf, pub original_content: Option<String>, pub new_content: String }` - `original_content: None` when the file did not exist before apply (needed for rollback of create-new-file)
- [ ] **Single API design**: `apply()` is the only entry point. On partial failure, the `ApplyError` struct carries the partial `ApplyResult` directly so callers that need rollback can access successful edits through the error payload. This avoids a dual `apply`/`apply_with_partial` API (code smell flagged in review).
- [ ] `pub struct ApplyError { pub kind: ApplyErrorKind, pub partial: ApplyResult }` - the struct always carries a partial `ApplyResult` (possibly empty if the failure happened on the first file). Uses `thiserror` `#[error]` attribute that delegates to `kind` for `Display`.
- [ ] `pub enum ApplyErrorKind` with variants: `FileNotFound { path: PathBuf }`, `ReadFailed { path: PathBuf, source: Arc<io::Error> }`, `WriteFailed { path: PathBuf, source: Arc<io::Error> }`, `OldTextNotFound { path: PathBuf, snippet: String }`, `AmbiguousMatch { path: PathBuf, count: usize }`, `MultiHunkDiffNotContiguous { path: PathBuf }`, `InvalidEdit { reason: String }` - uses `thiserror`. `Arc<io::Error>` is used so the error can derive `Clone` (required because `ApplyError` is stored in `AttemptRecord` which derives `Clone`).
- [ ] Applies `JsonOldNew` format as single-occurrence string replacement: reads file, fails with `OldTextNotFound` if `old` is absent, fails with `AmbiguousMatch` if `old` appears more than once, writes result (mirrors legacy `apply_edits` behavior at `src/workflow.rs:2935-2954`)
- [ ] Applies `FullFile` format by writing `new` as the entire file content. If the file already exists, its content goes into `FileBackup::original_content`. If the file does not exist, `FileBackup::original_content` is `None` and parent directories are created via `tokio::fs::create_dir_all()`
- [ ] Applies `UnifiedDiff` format by reusing the `JsonOldNew` find-and-replace path on the normalized `FileEdit` vector - no separate patch engine needed. **Known limitation (inherited from CLO-205)**: `EditParser::parse_unified_diff()` concatenates all hunks for a file into a single merged `old`/`new` pair (`src/apply_verify/edit_parser.rs:321-377`). For single-hunk diffs this yields a contiguous substring that matches the file; for **multi-hunk diffs where hunks are non-contiguous** (separated by unchanged lines) the merged `old` string does NOT appear in the file. In this case the applier returns `ApplyErrorKind::MultiHunkDiffNotContiguous { path }` (distinct from generic `OldTextNotFound` so CLO-211 can surface a clear remediation message to the LLM: "emit JSON old/new pairs or a single-hunk diff"). DiffApplier does NOT attempt to split hunks or walk `@@` headers - that would require a new `ParsedEdits` schema and is out of scope here.
- [ ] Each successful file modification is recorded in `ApplyResult::modified_files` **before** the next file is processed, so that a mid-run failure still produces a rollback-capable `ApplyResult` inside the error
- [ ] `ApplyResult`, `FileBackup`, `ApplyError`, `ApplyErrorKind` all derive `Debug, Clone`
- [ ] All `pub` items have doc comments

**Rollback (`rollback.rs`)**
- [ ] `pub struct Rollback;` with an async `rollback()` method
- [ ] `pub async fn rollback(apply_result: &ApplyResult, cwd: &Path) -> RollbackReport` - **does not return `Result`**: rollback is best-effort and reports per-file success/failure
- [ ] `pub struct RollbackReport { pub restored: Vec<PathBuf>, pub failed: Vec<RollbackFailure>, pub deleted: Vec<PathBuf> }` - `deleted` tracks files that were created during apply and removed during rollback
- [ ] `pub struct RollbackFailure { pub path: PathBuf, pub reason: String }`
- [ ] For each `FileBackup` in `apply_result.modified_files` in **reverse order**:
  - If `original_content.is_some()`: write original content back to path; on I/O error, record in `failed` and continue
  - If `original_content.is_none()`: delete the file at `path`; on I/O error, record in `failed` and continue
- [ ] Rollback continues processing remaining files even if one rollback step fails (atomic per-file guarantee from task description)
- [ ] `RollbackReport::is_fully_restored(&self) -> bool` helper that returns `self.failed.is_empty()`
- [ ] `RollbackReport` derives `Debug, Clone`

**Verification (`verification.rs`)**
- [ ] `pub struct Verification { pub command: String, pub timeout: Duration, pub max_output_bytes: usize }` - `cwd` is NOT held on the struct (see review note below); `max_output_bytes` caps captured stdout+stderr to prevent unbounded memory use when a verify command spews output
- [ ] **Rationale for removing `cwd` from the struct**: `RetryLoop::execute` accepts `cwd: &Path` for the applier. Having `Verification` carry its own `cwd` creates two sources of truth that can silently diverge (Gemini review flag). The only legitimate reason to hold `cwd` on the struct would be if Verification were reused across different working directories in a single loop, which it is not. So `cwd` becomes a `run()` parameter.
- [ ] `pub async fn run(&self, cwd: &Path) -> VerifyResult` - **does not return `Result`**: verify failure is a normal outcome, not an error. `cwd` is passed in so `RetryLoop` can enforce a single source of truth for the working directory
- [ ] `pub struct VerifyResult { pub success: bool, pub stdout: String, pub stderr: String, pub exit_code: Option<i32>, pub elapsed_ms: u64, pub timed_out: bool, pub truncated: bool }`
- [ ] `truncated: bool` is set to `true` when the captured stdout+stderr reached `max_output_bytes` and further output was dropped. Callers can distinguish "clean short output" from "output was cut off"
- [ ] Executes the command via `tokio::process::Command::new("sh").arg("-c").arg(&self.command)` with `current_dir(cwd)`, piped stdio, `kill_on_drop(true)` - mirrors the process setup in `run_shell()` at `src/workflow.rs:2722-2731`
- [ ] **Process group handling (Unix)**: `kill_on_drop(true)` only kills the direct child (the `sh` invocation); grandchildren spawned by `sh` (e.g., `npm test` -> `node`) are orphaned on timeout. To kill the entire process tree, use `std::os::unix::process::CommandExt::process_group(0)` on the `Command` before spawn (puts `sh` into its own process group), and on timeout send `SIGKILL` to the negative PID (`libc::kill(-pid as i32, SIGKILL)`) to reap the whole group. Wrap with `#[cfg(unix)]`; on non-Unix, fall back to `kill_on_drop(true)` alone. This is REQUIRED behavior, not optional
- [ ] **Elapsed time measurement**: capture `let start = std::time::Instant::now();` *before* `Command::spawn()`, and set `elapsed_ms = start.elapsed().as_millis() as u64` when producing the `VerifyResult` - regardless of whether the command completed normally, failed, or timed out. Do NOT report `elapsed_ms = self.timeout.as_millis()` on timeout (that would be a lie if the process was killed slightly before or after the deadline). Real wall-clock time is the only source of truth
- [ ] Enforces `self.timeout` via `tokio::time::timeout`. On timeout: sets `timed_out: true`, `success: false`, `exit_code: None`, populates `stdout`/`stderr` with whatever was captured before the timeout (best effort), `elapsed_ms` = actual elapsed from `Instant::now()`
- [ ] `success = output.status.success()` (i.e., exit code 0)
- [ ] Does NOT apply command wrapping (the `command_wrapper` config at `src/workflow.rs:1398`) - verification runs raw shell commands. Wrapping is a workflow concern, not a verification concern
- [ ] Handles spawn failures (e.g., `sh` not found): returns `VerifyResult { success: false, stdout: "", stderr: "<spawn error>", exit_code: None, elapsed_ms: <actual>, timed_out: false, truncated: false }`
- [ ] Output capture uses a streamed reader that stops appending once `max_output_bytes` is reached (do not let `Child::wait_with_output` accumulate unbounded; read `stdout`/`stderr` pipes incrementally and cap at the limit)
- [ ] `VerifyResult` derives `Debug, Clone`

**RetryLoop (`retry_loop.rs`)**
- [ ] `pub struct RetryLoop { pub max_retries: u32, pub verify: Verification, pub stop_on_parse_error: bool }`
- [ ] **`stop_on_parse_error` semantics (explicit truth table)**:
  - `true` + parse error: record `parse_error`, append `AttemptRecord`, return `RetryLoopOutcome { success: false, .. }` immediately. Do NOT call `requester.request_edits`. Use case: caller wants fail-fast on malformed LLM output
  - `false` + parse error: record `parse_error`, append `AttemptRecord`, treat as a retry-eligible failure (proceed to step 6 of control flow below). Use case: caller wants the loop to re-query the LLM to get cleaner output
  - Neither setting affects apply/verify error handling - both always retry (within the budget)
- [ ] **Single retry budget design (`max_retries`)**: `max_retries` applies to the combined count of parse/apply/verify failures. A single budget is intentional: separating budgets (e.g., `max_parse_retries`, `max_apply_retries`, `max_verify_retries`) would allow pathological cases where a flaky LLM burns the parse budget and then still gets a fresh verify budget, ballooning total effort. Single budget = bounded total work. If a caller wants different behavior they can wrap two `RetryLoop` instances or set `stop_on_parse_error = true` to short-circuit parse-heavy flakiness
- [ ] `#[async_trait] pub trait EditRequester: Send + Sync` with method `async fn request_edits(&self, context: &RetryContext<'_>) -> Result<String, String>` - callers implement this to wire in a Backend query. Using a trait (not a closure) keeps `apply_verify` decoupled from the `Backend` trait and the async-closure complexity
- [ ] `pub struct RetryContext<'a> { pub attempt: u32, pub previous_raw: &'a str, pub reason: RetryReason<'a> }` - passed to `EditRequester::request_edits` so the requester can build a remediation prompt. **Renamed from `VerifyFailureContext`** because it is used for parse and apply failures too (Gemini review flagged the old name as misleading and the struct as unconstructable for non-verify failures)
- [ ] `pub enum RetryReason<'a>` with variants:
  - `ParseError(&'a str)` - parse step failed; the `&str` is the `EditParseError` display string
  - `ApplyError { message: &'a str, partial_paths: &'a [PathBuf] }` - apply step failed; `message` is the `ApplyErrorKind` display, `partial_paths` lists files successfully modified before the failure (drawn from `ApplyError::partial.modified_files`)
  - `VerifyError(&'a VerifyResult)` - apply succeeded but verify returned `success: false` (including timeout). Carries a borrow of the actual `VerifyResult` so the requester can format stderr/stdout into the remediation prompt
- [ ] Rationale: using an enum instead of an `Option<&VerifyResult> + Option<&str>` tuple avoids the unconstructable-context bug Gemini flagged (the old design required a `VerifyResult` field even when there wasn't one)
- [ ] `pub async fn execute(&self, initial_raw: String, cwd: &Path, applier: &DiffApplier, requester: &dyn EditRequester) -> RetryLoopOutcome` - takes `cwd` as a parameter so the loop has one source of truth to pass to both `applier.apply` and `self.verify.run(cwd)`
- [ ] `pub struct RetryLoopOutcome { pub success: bool, pub attempts: Vec<AttemptRecord>, pub final_verify: Option<VerifyResult>, pub final_apply: Option<ApplyResult> }`
- [ ] `pub struct AttemptRecord { pub attempt_num: u32, pub raw_output: String, pub parse_error: Option<String>, pub apply_error: Option<String>, pub verify_result: Option<VerifyResult>, pub rolled_back: bool }` - `raw_output` is owned (`String`, not `&str`) so the record can outlive the loop iteration and provide a stable backing store for the next attempt's `RetryContext::previous_raw` borrow
- [ ] Control flow per attempt (attempt 0 is the initial call, attempts 1..=max_retries are retries):
  1. Parse `raw_output` via `EditParser::parse`. If `Err(parse_err)`: record `parse_error: Some(parse_err.to_string())`. If `self.stop_on_parse_error`: append the attempt record, exit loop with `success: false`. Otherwise construct `RetryReason::ParseError(&parse_err_string)` and go to step 6 (re-query)
  2. Apply via `applier.apply(&parsed, cwd)`. If `Err(ApplyError { kind, partial })`: record `apply_error: Some(kind.to_string())`, rollback `partial.modified_files` via `Rollback::rollback`, mark `rolled_back: true`, construct `RetryReason::ApplyError { message: &kind_str, partial_paths: &paths }`, go to step 6
  3. Run `self.verify.run(cwd)` - note `cwd` is the parameter, not a struct field. Record `verify_result: Some(result)`
  4. If `verify.success`: set `RetryLoopOutcome::success = true`, `final_verify = Some(result)`, `final_apply = Some(apply_result)`, append attempt record, and **return** (do not rollback on success)
  5. If `!verify.success`: rollback all modified files via `Rollback::rollback(&apply_result, cwd)`, mark `rolled_back: true`, construct `RetryReason::VerifyError(&result)`, go to step 6
  6. If `attempt_num < max_retries`: append the current attempt record to `outcome.attempts`, call `requester.request_edits(&RetryContext { attempt: attempt_num + 1, previous_raw: &current_raw, reason })` to get new raw output, increment `attempt_num`, continue loop. If `attempt_num == max_retries`: append the current attempt record and exit loop with `success: false`
- [ ] On requester failure (`request_edits` returns `Err(msg)`): record `msg` in the current `AttemptRecord::apply_error` (last-written field, since parse/apply/verify already completed for this attempt), exit loop with `success: false`
- [ ] `max_retries = 0` means "run once, no retries" (attempt 0 only) - the condition `attempt_num < max_retries` is `0 < 0 = false`, so the loop exits after the first attempt regardless of outcome
- [ ] **Lifetime note**: `RetryContext::previous_raw` and `RetryReason::*` borrows all point into locals (the current attempt's `raw_output` string, parse error string, apply error string, verify result). These borrows must not outlive the `requester.request_edits` call. Because `request_edits` is `async`, the borrows must be valid across await points but dropped before the next iteration mutates the locals. The test suite must exercise this (see test #30 variants)
- [ ] All records and context types derive `Debug` (context types do not derive `Clone` because they hold borrows)
- [ ] `RetryLoopOutcome` and `AttemptRecord` derive `Debug, Clone`

**Module exports (`mod.rs`)**
- [ ] `pub mod diff_applier;`
- [ ] `pub mod rollback;`
- [ ] `pub mod verification;`
- [ ] `pub mod retry_loop;`
- [ ] Re-export: `pub use diff_applier::{DiffApplier, ApplyResult, ApplyError, ApplyErrorKind, FileBackup};`
- [ ] Re-export: `pub use rollback::{Rollback, RollbackReport, RollbackFailure};`
- [ ] Re-export: `pub use verification::{Verification, VerifyResult};`
- [ ] Re-export: `pub use retry_loop::{RetryLoop, RetryLoopOutcome, AttemptRecord, EditRequester, RetryContext, RetryReason};`
- [ ] Existing `edit_parser` re-exports remain unchanged

**Test coverage**
- [ ] Unit tests live alongside their modules in `#[cfg(test)] mod tests` blocks
- [ ] Target: 35+ total tests across the 4 modules
- [ ] Tests use `tempfile::tempdir()` for filesystem isolation (pattern from `src/workflow.rs:4850` test block)
- [ ] Tests use `#[tokio::test]` for async tests

**Build gates**
- [ ] `cargo test` passes (all existing 367 unit + 6 integration + new CLO-210 tests)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo fmt --check` clean

**Verification method**:
```bash
cargo test -- apply_verify::diff_applier apply_verify::rollback apply_verify::verification apply_verify::retry_loop
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## 3. Constraints

**Must**:
- Use the existing `FileEdit` struct from `src/workflow.rs:83-87` - do not define a new edit type. Import via `use crate::workflow::FileEdit;` (same pattern as `edit_parser.rs:1`)
- Use the existing `EditParser`, `ParsedEdits`, `EditFormat`, `EditParseError` from `crate::apply_verify::edit_parser`
- Use `thiserror` for error enums (consistent with `EditParseError`, `BackendError`, `TemplateError`)
- Use `async-trait` for the `EditRequester` trait (already in Cargo.toml, pattern from `src/backend/mod.rs`)
- Use `tokio::fs` for all file I/O (consistent with `run_shell` and existing `apply_edits`)
- Use `tokio::process::Command` for shell execution (consistent with `run_shell` at `src/workflow.rs:2722`)
- Use `tokio::time::timeout` for verification timeout enforcement (consistent with `src/workflow.rs:1398, 1534`)
- Use `std::time::Instant` to measure actual wall-clock elapsed time for `VerifyResult::elapsed_ms` - never derive `elapsed_ms` from the timeout value
- On Unix, use `std::os::unix::process::CommandExt::process_group(0)` before spawning `sh -c` so the verify command and its descendants can be killed together on timeout. On timeout, send `SIGKILL` to the negated PID (process group) via `libc::kill(-pid as i32, libc::SIGKILL)` or equivalent. On non-Unix, `kill_on_drop(true)` alone is acceptable (lok currently targets macOS/Linux, so non-Unix is a compile-time fallback, not a tested path)
- Cap stdout+stderr capture at `Verification::max_output_bytes` to prevent unbounded memory use; set `VerifyResult::truncated = true` when the cap is reached
- `ApplyResult::modified_files` records must be in application order so rollback can reverse them safely
- `Rollback` processes files in **reverse order** of application
- All `pub` items have doc comments that explain purpose and invariants
- Tests must use `tempfile::tempdir()` for any filesystem operations - no touching `/tmp` directly
- Follow the existing test style: `#[tokio::test] async fn test_name()` with `let dir = tempdir().unwrap();` setup

**Must-not**:
- Do NOT modify `src/workflow.rs` except if you must remove a stale `#[allow(dead_code)]` from `src/apply_verify/mod.rs` re-exports (the new re-exports will be used by tests, so they should no longer need `#[allow(dead_code)]`). Touching execution paths is CLO-211 scope.
- Do NOT call `crate::backend::*` from `apply_verify` - the `EditRequester` trait keeps the dependency inverted
- Do NOT call `git_agent::checkpoint` or any git operations - rollback is pure filesystem restore
- Do NOT add external crates (no `diff`, `patch`, `similar`, etc.) - CLO-205's `EditParser` already normalizes diffs to `Vec<FileEdit>`
- Do NOT build a separate hunk-aware diff applier. The multi-hunk limitation is documented and accepted; solving it requires changes to CLO-205 `ParsedEdits` which is explicitly out of scope
- Do NOT use `unsafe` code **except for the `libc::kill` call in the Unix process-group cleanup path** - that specific `unsafe` block is required and acceptable; document the invariant (pid was just obtained from `child.id()` and is valid at the moment of the kill)
- Do NOT use `panic!` or `unwrap()` outside tests
- Do NOT use `std::fs` (blocking) - use `tokio::fs` everywhere
- Do NOT introduce a `command_wrapper` option on `Verification` - verification runs raw commands, wrapping is a workflow-level concern
- Do NOT swallow `EditRequester::request_edits` errors silently - surface them in the `AttemptRecord`
- Do NOT attempt to clean up empty parent directories left behind when `Rollback` deletes a newly-created file. If `DiffApplier` created `a/b/c/` to write `a/b/c/new_file.rs`, rolling back deletes `new_file.rs` but leaves `a/b/c/` in place. This is a known, accepted non-bug (Gemini flagged it) - tracking per-directory creation state adds complexity for no meaningful benefit
- Do NOT hold `cwd` on the `Verification` struct - it must be a `run()` parameter to keep a single source of truth across apply and verify stages

**Prefer**:
- Keep the four modules self-contained and test them in isolation. `RetryLoop` tests should use a mock `EditRequester` implemented inline in the test module - no real `Backend` calls
- Use `PathBuf::join` for path composition, never string concatenation
- Use `io::ErrorKind::NotFound` checks rather than stringifying errors (pattern from `src/workflow.rs:2927`)
- Keep `Verification::run()` signature minimal - no streaming output, no live logging (caller can wrap if needed)
- Name tests descriptively: `test_diff_applier_full_file_creates_missing_file`, `test_rollback_continues_after_single_failure`, etc.

**Escalate when**:
- The `EditRequester` trait shape conflicts with how CLO-211 needs to wire it into the workflow executor (if this emerges during implementation, stop and review the CLO-211 task description)
- The multi-hunk unified-diff limitation becomes a production blocker (observed in real LLM outputs via CLO-211 integration). If so, file a follow-up task to extend `ParsedEdits` with per-hunk data and add a hunk-walking path to `DiffApplier`. Do NOT solve it speculatively in CLO-210
- `CommandExt::process_group` is not available on the minimum supported Rust version (should be 1.64+; stop and confirm lok's MSRV before implementing)
- The Unix process-group + `libc::kill` approach still leaves zombies under load (verify with a stress test that spawns `sh -c 'sleep 100 & sleep 100 & wait'` and kills the group; if zombies remain, escalate to discuss a broader cleanup strategy)
- `libc` is not already a dependency of lok and adding it is objected to (in that case, fall back to `nix` crate or a raw `unsafe extern "C" fn kill(...)` declaration - do not silently drop the process-group cleanup)

## 4. Decomposition

1. **Module skeleton + types**: Create `src/apply_verify/diff_applier.rs`, `rollback.rs`, `verification.rs`, `retry_loop.rs` with empty type definitions (`ApplyResult`, `FileBackup`, `ApplyError`, `ApplyErrorKind`, `RollbackReport`, `RollbackFailure`, `VerifyResult`, `RetryLoopOutcome`, `AttemptRecord`, `RetryContext`, `RetryReason`, `EditRequester` trait). Update `src/apply_verify/mod.rs` to declare and re-export the new modules. Run `cargo check` to confirm the module tree compiles. - files: `src/apply_verify/{mod.rs, diff_applier.rs, rollback.rs, verification.rs, retry_loop.rs}`

2. **DiffApplier implementation + tests**: Implement `DiffApplier::apply` (single-entry-point API, no separate `apply_with_partial`) for all three `EditFormat` variants. Cover `JsonOldNew` (find/replace), `FullFile` (write whole content, create parent dirs, handle missing file), `UnifiedDiff` (delegate to `JsonOldNew` path via normalized `FileEdit` vector, surface `MultiHunkDiffNotContiguous` for non-contiguous multi-hunk). Add unit tests for: single edit success, multi-file success, `OldTextNotFound`, `AmbiguousMatch`, `FileNotFound`, full-file create-new, full-file overwrite-existing, partial failure where `ApplyError.partial` carries the successfully-modified files, unified-diff single-hunk applied via find/replace, unified-diff multi-hunk non-contiguous fails with `MultiHunkDiffNotContiguous`, empty `Vec<FileEdit>` is a no-op (returns `Ok`). Target: 12+ tests. - files: `src/apply_verify/diff_applier.rs`

3. **Rollback implementation + tests**: Implement `Rollback::rollback` that processes `FileBackup` entries in reverse order, distinguishing "restore original content" from "delete newly-created file". Return `RollbackReport` with restored/failed/deleted lists. Add unit tests for: single file restored, multi-file restored in reverse order, newly-created file deleted, rollback continues after a single I/O failure, fully restored report, partial failure report, `is_fully_restored()` helper. Target: 8+ tests. - files: `src/apply_verify/rollback.rs`

4. **Verification implementation + tests**: Implement `Verification::run(cwd: &Path)` that spawns `sh -c <command>` with timeout, uses a Unix process-group setup via `CommandExt::process_group(0)` under `#[cfg(unix)]`, streams stdout/stderr into bounded buffers capped at `max_output_bytes`, and returns structured `VerifyResult` with a real `elapsed_ms` measured via `Instant::now()`. Handle spawn failure (sets `success: false` with stderr populated), timeout (sets `timed_out: true`, kills the whole process group), normal success, normal failure, output truncation (sets `truncated: true`). Add unit tests for: successful command (`exit 0`), failing command (`exit 1`), command with stdout output, command with stderr output, command that exceeds timeout with `sleep 10` (assert `timed_out: true` and `elapsed_ms` roughly equals the timeout within tolerance), command that writes to both streams, invalid shell command (exit 127 from `sh`), output-truncation test (command emits >`max_output_bytes` and `truncated` is set), elapsed_ms greater than zero on quick commands. Target: 9+ tests. - files: `src/apply_verify/verification.rs`

5. **RetryLoop implementation + tests**: Implement `RetryLoop::execute` orchestrating parse -> apply -> verify -> rollback -> re-query, using the `RetryReason` enum so parse and apply failures produce a valid `RetryContext`. Define a test-only `MockEditRequester` that returns canned raw outputs per attempt and records the `RetryReason` variant it was called with. Add unit tests for: success on first attempt (no retries), success on second attempt after rollback, max retries exhausted, parse error abort with `stop_on_parse_error: true`, parse error retry with `stop_on_parse_error: false` (requester sees `RetryReason::ParseError`), apply error triggers rollback and requester sees `RetryReason::ApplyError` with partial paths, verify failure triggers rollback and requester sees `RetryReason::VerifyError`, `AttemptRecord` list captures each attempt, `requester` error surfaced in outcome, `max_retries = 0` runs exactly once. Target: 11+ tests. - files: `src/apply_verify/retry_loop.rs`

6. **Integration sanity test**: Add one end-to-end test in `retry_loop.rs` that: writes a real temp file, parses a real JsonOldNew edit, applies it, runs a real `echo ok` verify command, asserts the file was modified and `success: true`. This catches wiring mistakes between the four components. Target: 1 test. - files: `src/apply_verify/retry_loop.rs`

7. **Test count + clippy sweep**: Run `cargo test` to confirm 35+ new tests pass and total count increases by at least 35. Run `cargo clippy -- -D warnings` and fix any lints. Run `cargo fmt`. Update `src/apply_verify/mod.rs` to remove any stale `#[allow(dead_code)]` attributes - the new re-exports must be live because at minimum their tests use them. - files: any of the above as needed

**Dependency order**: 1 -> (2, 3, 4 in parallel) -> 5 -> 6 -> 7

**Parallelism note**: Sub-tasks 2, 3, and 4 touch disjoint files and can be written in parallel by a subagent pool. Sub-task 5 depends on all three because `RetryLoop` imports `DiffApplier`, `Rollback`, and `Verification`.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `JsonOldNew` single-file edit applies successfully | `ApplyResult` with one `FileBackup`, original content preserved, file on disk contains `new` | `cargo test -- apply_verify::diff_applier::tests::test_apply_json_single_file` |
| 2 | `JsonOldNew` edit with non-existent `old` string | Returns `ApplyError::OldTextNotFound` with path and snippet | `cargo test -- apply_verify::diff_applier::tests::test_apply_old_text_not_found` |
| 3 | `JsonOldNew` edit with ambiguous `old` (2+ matches) | Returns `ApplyError::AmbiguousMatch` with count | `cargo test -- apply_verify::diff_applier::tests::test_apply_ambiguous_match` |
| 4 | `JsonOldNew` edit targeting missing file | Returns `ApplyError::FileNotFound` | `cargo test -- apply_verify::diff_applier::tests::test_apply_file_not_found` |
| 5 | `FullFile` edit on existing file | Original content captured in `FileBackup.original_content`, file content replaced with `new` | `cargo test -- apply_verify::diff_applier::tests::test_apply_full_file_overwrite` |
| 6 | `FullFile` edit creates new file | `FileBackup.original_content = None`, parent dirs created | `cargo test -- apply_verify::diff_applier::tests::test_apply_full_file_create_new` |
| 7 | `UnifiedDiff` **single-hunk** format applies via find/replace path | Treated identically to `JsonOldNew`, file modified, no separate parser invoked | `cargo test -- apply_verify::diff_applier::tests::test_apply_unified_diff_single_hunk` |
| 7b | `UnifiedDiff` **multi-hunk non-contiguous** format fails with clear error | Returns `ApplyError { kind: ApplyErrorKind::MultiHunkDiffNotContiguous { path }, partial }` where `partial.modified_files` is empty | `cargo test -- apply_verify::diff_applier::tests::test_apply_unified_diff_multi_hunk_fails` |
| 8 | Multi-file `JsonOldNew` succeeds on all files | `ApplyResult::modified_files` has entries in application order | `cargo test -- apply_verify::diff_applier::tests::test_apply_multi_file_success` |
| 9 | Multi-file `JsonOldNew` fails on second file | `apply` returns `Err(ApplyError { kind, partial })` where `partial.modified_files` has exactly 1 entry (the first file) | `cargo test -- apply_verify::diff_applier::tests::test_apply_partial_failure` |
| 9b | `DiffApplier` on empty `Vec<FileEdit>` | Returns `Ok(ApplyResult { modified_files: vec![], .. })` - no-op | `cargo test -- apply_verify::diff_applier::tests::test_apply_empty_edits` |
| 10 | Rollback restores single file from `FileBackup` | File content matches `original_content`, `RollbackReport::restored` contains path, `failed` empty | `cargo test -- apply_verify::rollback::tests::test_rollback_single_file` |
| 11 | Rollback processes files in reverse order | Assertion via test-internal logging/counter that last-modified file is first-restored | `cargo test -- apply_verify::rollback::tests::test_rollback_reverse_order` |
| 12 | Rollback deletes newly-created file | File no longer exists on disk, `RollbackReport::deleted` contains path | `cargo test -- apply_verify::rollback::tests::test_rollback_deletes_new_file` |
| 13 | Rollback continues after single I/O failure | One file in `failed`, remaining files in `restored`, no panic | `cargo test -- apply_verify::rollback::tests::test_rollback_continues_on_failure` |
| 14 | `RollbackReport::is_fully_restored` true when `failed` empty | Assertion | `cargo test -- apply_verify::rollback::tests::test_is_fully_restored_true` |
| 15 | `RollbackReport::is_fully_restored` false when `failed` non-empty | Assertion | `cargo test -- apply_verify::rollback::tests::test_is_fully_restored_false` |
| 16 | `Verification` with `exit 0` returns `success: true` | `VerifyResult { success: true, exit_code: Some(0), timed_out: false }` | `cargo test -- apply_verify::verification::tests::test_verify_success` |
| 17 | `Verification` with `exit 1` returns `success: false` | `VerifyResult { success: false, exit_code: Some(1), timed_out: false }` | `cargo test -- apply_verify::verification::tests::test_verify_failure` |
| 18 | `Verification` captures stdout and stderr separately | `stdout` and `stderr` fields contain expected content | `cargo test -- apply_verify::verification::tests::test_verify_captures_both_streams` |
| 19 | `Verification` exceeds timeout | `VerifyResult { success: false, timed_out: true, exit_code: None }` | `cargo test -- apply_verify::verification::tests::test_verify_timeout` |
| 20 | `Verification` with invalid command (non-existent binary) | `VerifyResult { success: false }` with non-empty `stderr` (the shell's error) - `sh -c` handles this gracefully with exit 127 | `cargo test -- apply_verify::verification::tests::test_verify_invalid_command` |
| 21 | `Verification` measures elapsed_ms | `elapsed_ms` is greater than zero and roughly matches sleep duration | `cargo test -- apply_verify::verification::tests::test_verify_elapsed_ms` |
| 22 | `Verification::elapsed_ms` on timeout reflects actual wall-clock time | On `sleep 10` with 100ms timeout, `elapsed_ms` is in `[100, 300]` (timeout + reasonable reap slack), NOT hardcoded to `timeout.as_millis()` | `cargo test -- apply_verify::verification::tests::test_verify_timeout_real_elapsed` |
| 22b | `Verification` truncates output at `max_output_bytes` | Command `yes | head -c 1000000` with `max_output_bytes = 4096`: `VerifyResult.truncated = true`, `stdout.len() <= 4096` | `cargo test -- apply_verify::verification::tests::test_verify_output_truncated` |
| 22c | `Verification` kills descendant processes on timeout (Unix) | `sh -c 'sleep 30 & wait'` with 200ms timeout: process returns within ~500ms (not 30s), i.e. the child process group was reaped | `cargo test -- apply_verify::verification::tests::test_verify_timeout_kills_process_group` |
| 23 | `RetryLoop` succeeds on first attempt with no retries | `attempts.len() == 1`, `success: true`, no rollback recorded | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_first_attempt_success` |
| 24 | `RetryLoop` retries after verify failure and succeeds on attempt 2 | `attempts.len() == 2`, first attempt has `rolled_back: true`, second has `verify_result.success: true` | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_succeeds_after_rollback` |
| 25 | `RetryLoop` exhausts max_retries and fails | `attempts.len() == max_retries + 1`, `success: false`, all attempts rolled back | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_max_retries_exhausted` |
| 26 | `RetryLoop` with `max_retries = 0` runs exactly once | `attempts.len() == 1` regardless of verify outcome | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_zero_retries` |
| 27 | `RetryLoop` aborts on parse error when `stop_on_parse_error: true` | `attempts.len() == 1`, `attempts[0].parse_error.is_some()`, `success: false` | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_stop_on_parse_error` |
| 28 | `RetryLoop` retries on parse error when `stop_on_parse_error: false` | `attempts.len() > 1` if max_retries > 0 | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_retry_on_parse_error` |
| 29 | `RetryLoop` rolls back on apply error before re-querying | `attempts[0].apply_error.is_some()`, `rolled_back: true`, file on disk unchanged from pre-apply | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_rollback_on_apply_error` |
| 30 | `RetryLoop` passes `RetryContext` with `RetryReason::VerifyError` on verify failure | Mock requester asserts `context.attempt == 1`, matches `RetryReason::VerifyError(vr)` where `!vr.success` | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_requester_context_verify` |
| 30b | `RetryLoop` passes `RetryContext` with `RetryReason::ParseError` when `stop_on_parse_error: false` | Mock requester asserts `matches!(context.reason, RetryReason::ParseError(_))` on the first retry | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_requester_context_parse` |
| 30c | `RetryLoop` passes `RetryContext` with `RetryReason::ApplyError` including partial paths | Mock requester asserts `matches!(context.reason, RetryReason::ApplyError { partial_paths, .. } if partial_paths.len() == 1)` after a 2-file apply failing on the 2nd file | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_requester_context_apply` |
| 31 | `RetryLoop` surfaces `EditRequester` error in outcome | `success: false`, last `AttemptRecord::apply_error` contains the requester error message | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_requester_error` |
| 32 | `RetryLoop` does NOT rollback on success | File on disk contains the applied edit after successful verify | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_no_rollback_on_success` |
| 33 | End-to-end: parse + apply + verify (`echo ok`) on real tempdir | File modified, `RetryLoopOutcome::success = true`, 1 attempt | `cargo test -- apply_verify::retry_loop::tests::test_retry_loop_end_to_end` |
| 34 | `cargo test` total count increases by at least 40 | Baseline 367 + 40 = 407+ unit tests (added 5 extra tests for output truncation, process-group kill, empty edits, parse/apply retry contexts, multi-hunk failure) | `cargo test 2>&1 \| grep "test result"` |
| 35 | `cargo clippy -- -D warnings` clean | No warnings | `cargo clippy -- -D warnings` |
| 36 | `cargo fmt --check` clean | No formatting drift | `cargo fmt --check` |

**Edge cases to verify**:
- Multi-hunk unified diff where hunks are non-contiguous: `DiffApplier` returns `ApplyError { kind: ApplyErrorKind::MultiHunkDiffNotContiguous, .. }` (test #7b above)
- `FullFile` edit where the target file does not exist: `FileBackup.original_content = None`, parent dirs created via `create_dir_all`, file written
- Rollback of a newly-created file leaves the created parent directories in place - accepted, not a bug
- Rollback of a file whose original content was empty string `""`: restore writes empty content, no error
- Rollback where the restored file was deleted by an external process between apply and rollback: record in `failed` with a clear reason, do not panic
- `Verification` command that exits with a signal (e.g., killed by SIGKILL): `exit_code: None`, `success: false`, `timed_out: false` if the signal came from outside our timeout path
- `Verification` with a command that spawns grandchildren via `sh -c 'sleep 30 & wait'`: timeout must kill the whole process group (test #22c)
- `Verification` with a command that emits more than `max_output_bytes` of stdout: `truncated: true`, `stdout.len() <= max_output_bytes` (test #22b)
- `RetryLoop` where apply succeeds but verify fails: the full `ApplyResult.modified_files` must be rolled back before the next attempt
- `RetryLoop` where the new raw output from `requester` is identical to the previous one: still counts as an attempt, no deduplication logic
- `RetryLoop` where apply fails on the *first* file: `ApplyError.partial.modified_files` is empty; rollback is still called (no-op); `RetryReason::ApplyError.partial_paths` is an empty slice
- `DiffApplier` on an empty `Vec<FileEdit>`: returns `Ok(ApplyResult { modified_files: vec![], format_applied: parsed.format })` - no-op, not an error (test #9b)
- File paths with non-UTF-8 bytes: tests use ASCII only; non-UTF-8 handling inherits from `tokio::fs` behavior and is not in scope
- Unicode normalization in file contents: not in scope; `EditParser` already handles CRLF normalization, byte-level match is sufficient
- Shell metacharacters in the verify command string (e.g., `echo $HOME`): passed through `sh -c` verbatim; the LLM authored the command, so substitution is expected. Not a security concern in the apply_verify layer
