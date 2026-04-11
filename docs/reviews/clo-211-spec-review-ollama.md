# Spec Review: clo-211

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-11
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Strong**: The problem statement is thorough and well-structured. It clearly identifies the six deficiencies in the legacy implementation and maps each to the CLO-210 replacement primitives. The "Who's affected" and "Why it matters" sections provide excellent context for prioritization.

**Minor gap**: The problem statement references `AgenticOutput` at `src/workflow.rs:89-99` as legacy, but doesn't explicitly note that `FileEdit` (also defined nearby) is reused by `EditParser`. This creates a minor ambiguity about whether `FileEdit` should be deleted or kept. The spec later says in ST-6 to delete it "only if they have no other callers" - a grep check would reveal `EditParser` uses `FileEdit`.

**Assessment**: Clear, complete, and accurate. Matches Linear task description precisely.

---

## 2. Acceptance Criteria Review

**Strong**:
- **AC-1 through AC-6**: Well-defined, testable criteria with clear before/after states
- **AC-7**: Git-agent audit-only semantics are clearly specified
- **AC-9**: Failure classification maps cleanly to existing `StepFailureKind` enum
- **AC-10**: Cleanup criterion with explicit "grep before deletion" safety

**Gaps**:
- **AC-3** specifies "step timeout as the verification timeout" but doesn't address what happens when `step.timeout = 0` (no timeout). The `DEFAULT_STEP_TIMEOUT_MS` is 120s, but `timeout_ms = 0` means "effectively no timeout" (1 year in the code). Should `Verification` receive `None` or use a sensible default?

- **AC-8** resolves the env var inheritance question correctly but doesn't specify *how* this is achieved. The current `Verification::run()` already inherits parent env by default via `Command::spawn()`. The spec should note this is a non-change (no code modification needed for C-5).

- **Missing criterion**: What happens when `apply_edits = true` but `verify = None`? AC-3 covers the verify path, but the "apply-only" path isn't explicitly tested. ST-4 mentions an `apply_once` helper but no AC validates this path.

- **Missing criterion**: The spec mentions `stop_on_parse_error` default is `false` (C-9) but no AC verifies parse errors are retried (not immediately failed).

---

## 3. Constraints Check

**Aligned**:
- **C-1 through C-4**: Correctly use Must/Must-not/Prefer/Escalate categories
- **C-5** (env inheritance) matches `run_shell()` current behavior
- **C-7** (no StepResult schema changes) aligns with CLO-182's existing structure

**Concerns**:
- **C-6** (format failures stay non-fatal): The spec says "format failures stay non-fatal" but doesn't define what happens if format fails while `apply_edits = true`. Does the step fail? Does it continue without applying? The legacy code would fail the step with `EditFailed`. The constraint should clarify this behavior is preserved.

- **P-1** (preserve git-agent checkpoint call): The spec says to preserve the checkpoint call "for event-log/audit purposes." This is good, but there's no AC or test verifying that git-agent is *not* used for rollback. Test #14 covers "git-agent unavailable" but not "git-agent available but rollback uses Rollback::rollback instead."

- **P-3** (`max_output_bytes` default): Hardcoded at 1 MiB. This matches `EditParser`'s `MAX_INPUT_SIZE` but there's no cross-reference to ensure they stay in sync.

**Missing constraints**:
- What happens to the `'fix_loop` label in workflow.rs? The spec implies removing it but doesn't explicitly state this.
- The `Step.fix_retries` field mapping to `RetryLoop::max_retries` - should this be 1:1 (where `fix_retries = 3` means 3 retries = 4 total attempts)? The spec test table says "after 4 total attempts (1 initial + 3 retries)" which confirms 1:1, but the constraint section doesn't state this explicitly.

---

## 4. Decomposition Quality

**Well-scoped**:
- **ST-1** (WorkflowEditRequester): Cleanly isolated trait implementation
- **ST-2** (replace inline loop): Clear surgical change
- **ST-5** (git-agent repurpose): Small, focused change
- **ST-6** (delete legacy helpers): Clear cleanup task

**Issues**:
- **ST-4** (apply-only path) mentions creating an `apply_once` helper but doesn't specify where this lives. Is it a private function in workflow.rs? A separate module? This impacts test placement.

- **ST-2 scope**: The sub-task says "Replace the inline fix loop" but doesn't mention that the `'fix_loop` label spans approximately 250 lines (1928-2170). This is a large diff. Consider splitting:
  - ST-2a: Add `WorkflowEditRequester` adapter
  - ST-2b: Replace verify-fail branch
  - ST-2c: Replace verify-timeout branch
  - ST-2d: Remove now-unused local variables (checkpointed, etc.)

- **Missing sub-task**: Update the `Step::fix_retries` documentation/comment to clarify it maps to `RetryLoop::max_retries`.

- **Missing sub-task**: The spec mentions `StepResult.elapsed_ms` recalculation after retries (line ~2180 in legacy code). Is this preserved in the new implementation? No sub-task addresses timing calculation.

---

## 5. Evaluation Coverage

**Covered**:
- Tests #1-14 cover all major acceptance criteria
- Edge case table covers large output, nested retries, Unicode, rollback failure
- Build/lint commands are comprehensive

**Gaps**:

- **No test for apply-only path (no verify)**: AC-3 covers verification but test #4 is "apply + verify success." Missing a test for `apply_edits = true, verify = None`.

- **No test for `stop_on_parse_error = false`**: Test #7 covers parse error triggering retry, but doesn't verify the default is `false`. A regression test is needed.

- **No test for git-agent *available* but not used for rollback**: Test #14 covers "git-agent unavailable" but not "git-agent available + rollback still uses Rollback::rollback." This is critical for AC-7 verification.

- **Missing test table entry for C-9 decision**: The spec resolves `stop_on_parse_error` default in C-9 but test #7 doesn't explicitly verify parse errors are retried (not immediately failed).

- **Test #13 verification**: Uses `sh -c 'test -n "$PATH"'` to verify env inheritance. This is correct for PATH but doesn't verify *all* parent env is inherited (e.g., custom env vars). A more thorough test would set a custom env var and verify it's visible.

---

## 6. Codebase Alignment

**Violations**: None found. The spec correctly:
- Uses existing `StepFailureKind::EditFailed` and `StepFailureKind::VerifyFailed`
- Maps `Step.fix_retries` to `RetryLoop::max_retries`
- Preserves `StepResult` schema without modifications
- Follows the `Backend::query()` async pattern for `EditRequester::request_edits()`

**Alignment observations**:

- The `EditParser` already uses `FileEdit` from `workflow.rs` - this is correct reuse, not a violation.

- The `Verification::run()` signature `run(&self, cwd: &Path)` matches the pattern of `DiffApplier::apply(&self, parsed: &ParsedEdits, cwd: &Path)` - both take `cwd` as a parameter rather than storing it.

- The `BackendError` enum in `src/backend/mod.rs` provides `is_retryable()` but `RetryLoop` uses a simpler success/failure model. This is acceptable because `EditRequester::request_edits()` returns `Result<String, String>` - the workflow adapter can map `BackendError` to a string message.

- The spec's test commands use `cargo test test_name` but the codebase convention (based on the test modules seen) uses `#[tokio::test]` async tests. Test commands should work correctly.

---

## 7. Blind Spots

**What the specification misses**:

1. **Timing instrumentation**: The legacy code recalculates `elapsed_ms` after the fix loop completes (`start.elapsed().as_millis()` at line ~2180). The spec doesn't explicitly state whether `RetryLoop::execute()` returns timing information, or whether `AttemptRecord.elapsed_ms` is populated. If `RetryLoop` doesn't track timing, the `StepResult.elapsed_ms` field may be incorrect.

2. **Concurrent step execution**: The codebase uses `join_all` for parallel step execution. If a workflow has `apply_edits = true` on parallel steps, do all steps share the same `DiffApplier` instance, or is it per-step? The spec doesn't address thread safety, though `DiffApplier` is a unit struct with no state, so it's probably fine.

3. **Error message formatting**: The spec says "human-readable message mentioning the specific failure mode" but doesn't specify the format. The legacy code uses `"Parse failed: {}\n\nOriginal output:\n{}"` and `"Edit failed: {}"`. Should the new implementation preserve these formats for backward compatibility?

4. **CLI output parity**: P-4 mentions "minimizing CLI churn" but the legacy code has specific `println!` statements for:
   - `"  {} Applying edits..."`
   - `"    {} Applied {} edit(s)"`
   - `"  {} Fix attempt {}/{}..."`
   - `"    {} Re-querying LLM with error..."`

   The spec doesn't specify whether these outputs are preserved. `RetryLoop` has `AttemptRecord` but no logging hooks. A `Reporter` trait (similar to `EditRequester`) might be needed for CLI output parity.

5. **Validation integration**: The legacy code runs validation after the fix loop (lines ~2100-2150). The spec focuses on the fix loop but doesn't explicitly state that validation runs *after* `RetryLoop::execute()` succeeds. Is validation still in scope for CLO-211?

6. **Partial apply state**: The `ApplyError { kind, partial }` design captures partial state, but the spec doesn't explain how `partial` is passed back to the caller for diagnostics. `RetryLoopOutcome::final_apply` is only populated on success - the failed attempt's partial state is only in `AttemptRecord`.

---

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, thorough, and correctly maps the CLO-210 primitives to the legacy codebase. The acceptance criteria are testable, the constraints are well-reasoned, and the decomposition is appropriately sized. However, there are several gaps that should be addressed before implementation:

1. **Apply-only path** (verify = None) needs explicit test coverage
2. **CLI output parity** should be addressed - either a logging hook or explicit non-goal
3. **Elapsed timing** in `StepResult` needs clarification
4. **Git-agent available + rollback path** needs test verification
5. **stop_on_parse_error default** needs regression test

---

## 9. Actionable Feedback

**Priority 1 (Address before implementation)**:

1. **Add AC-11**: "When `apply_edits = true` and `verify = None`, apply succeeds without entering RetryLoop, using a simpler apply_once helper. Rollback still occurs on apply failure."

2. **Add test for apply-only path**: `test_apply_without_verify_succeeds` - applies edits successfully, no verification step, workspace reflects changes.

3. **Clarify timing**: Either add `elapsed_ms` to `RetryLoopOutcome`, or explicitly state that `StepResult.elapsed_ms` is calculated from the outer `start` instant (preserved from legacy).

4. **Add CLI output consideration**: Either:
   - Add a `Reporter` trait (analogous to `EditRequester`) to `RetryLoop` for progress notifications, or
   - Mark CLI output as a non-goal for CLO-211 with a follow-up task.

**Priority 2 (Address during implementation)**:

5. **Add test for git-agent available + rollback**: Verify that even when `git-agent` is available and checkpoint succeeds, `Rollback::rollback()` is still called (not `git_agent::undo()`).

6. **Add `stop_on_parse_error` default test**: Verify that a parse error triggers retry (not immediate failure) by default.

7. **Document `FileEdit` ownership**: Explicitly state in ST-6 that `FileEdit` stays in `workflow.rs` because `EditParser` uses it. Only `AgenticOutput`, `parse_edits()`, `apply_edits()`, `extract_json_from_text`, and `sanitize_json_strings` are candidates for deletion.

**Priority 3 (Minor improvements)**:

8. **Consider splitting ST-2**: The inline loop replacement is a 250-line diff. Splitting into smaller sub-tasks reduces risk and enables incremental review.

9. **Add cross-reference for `MAX_INPUT_SIZE`**: Either hardcode `1_024_1_024` in both places with a comment, or create a shared constant.

10. **Specify error message formats**: If backward compatibility is desired, include the exact format strings in the spec.
