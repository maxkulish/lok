# Spec Review: clo-211

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-04-11
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, accurate, and provides excellent context. The side-by-side mapping of legacy concerns to the new CLO-210 primitives perfectly illustrates the necessity of this task. However, there is an unstated assumption that the new primitives (specifically `RetryLoop`) support all existing workflow semantics—such as the execution of the `format` command between apply and verify—which they do not.

## 2. Acceptance Criteria Review
**Strong**: The criteria are specific, measurable, and explicitly mention expected backward compatibility (AC-11, AC-12) and error handling improvements (AC-9).
**Gaps**: AC-13 ("Workflows with `format = "..."` still run the format command between apply and verify (this lives outside RetryLoop...)") specifies an implementation detail that is technically impossible given the current `RetryLoop` API (see Blind Spots).

## 3. Constraints Check
**Aligned**: C-5 (Environment inheritance), C-4 (Single source of truth for `cwd`), and C-9 (`stop_on_parse_error = false`) show a deep understanding of the current system and elegantly resolve previous deferred decisions from Phase 8.
**Concerns**:
- **C-6 vs MN-7 Contradiction**: C-6 requires running the `format` command *between* apply and verify without moving it inside `RetryLoop`. MN-7 forbids modifying `RetryLoop` in `src/apply_verify/`. Because `RetryLoop::execute()` tightly couples `applier.apply()` and `self.verify.run()`, there is no way to interject a command between them. One of these constraints must give.

## 4. Decomposition Quality
**Well-scoped**: The sub-tasks are logical, independently testable, and appropriately sized (1-4 hours each). Extracting the `EditRequester` to an adapter module (ST-1) is a great pattern to maintain separation of concerns.
**Issues**:
- ST-2's code snippet does not account for where the `format` command execution fits, as `RetryLoop` encapsulates both apply and verify.
- ST-1's prompt templates mirror the legacy implementation but fail to utilize the newly available `previous_raw` context provided by `RetryContext`.

## 5. Evaluation Coverage
**Covered**: The test table is exceptionally thorough. Tests 1-14 cover success paths, format detection, retry exhaustion across all phases (parse, apply, verify), timeouts, and environment inheritance.
**Gaps**: There is no test verifying that the `format` command runs and correctly formats files *before* verification. Given the architectural conflict identified above, this test scenario is missing but crucial.

## 6. Codebase Alignment
**Violations**:
- The `git_agent::checkpoint` call is moved outside the retry loop in ST-2. Legacy code created a checkpoint *per attempt* inside `fix_loop`. This changes the audit trail granularity, which is not acknowledged in the spec.
**Alignment**:
- ST-1 correctly identifies that `backend.query()` needs a `tokio::time::timeout` wrapper (consistent with `run_query` in `src/backend/mod.rs`).
- ST-3 correctly maps `RetryLoopOutcome` to the existing `StepFailureKind` variants without modifying the enum structure.

## 7. Blind Spots
1. **Format Execution Impossible in `RetryLoop`**: Because `RetryLoop::execute` immediately calls `self.verify.run()` after `applier.apply()`, there is no execution window for the `format` command. If the verify command depends on formatting (e.g., a linter that expects strict formatting), it will fail.
2. **Loss of LLM Context (`previous_raw`)**: ST-1 requires building a remediation prompt but explicitly copies the legacy templates, leaving out `context.previous_raw`. Since the LLM backend is stateless across calls, not including the LLM's failed output will force it to guess what caused the failure, severely degrading retry effectiveness.
3. **git-agent Checkpoint Granularity**: ST-2 moves the `git_agent::checkpoint` call outside the retry loop. Only the initial attempt is logged, whereas the legacy code logged every attempt. The spec should explicitly note this change or provide a callback to inject it into `RetryLoop`.
4. **Timeout Formatting**: In AC-9 / ST-3, if a timeout occurs, `VerifyResult.exit_code` is `None` but `timed_out` is `true`. The ST-3 template uses `{exit_or_timeout}`, but doesn't explicitly mention constructing a distinct string like "Timeout" vs "Exit code X".

## 8. Verdict
NEEDS_REVISION

## 9. Actionable Feedback
1. **Resolve the Format/RetryLoop Conflict**: Amend MN-7 to allow adding an optional `format: Option<Verification>` field or a `pre_verify` hook to `RetryLoop`. Alternatively, clarify that `format` will now run *after* `RetryLoop` (which may break linters used as verifiers) or before it (which is useless).
2. **Include `previous_raw` in Re-query Prompts**: Update ST-1's prompt templates to inject `context.previous_raw` so the LLM can see its failing output. For example: `Parse error:\n```\n{display}\n```\n\nYou generated:\n```\n{previous_raw}\n```\n\nPlease provide...`
3. **Document git-agent Behavioral Change**: Update AC-7 and C-7 to explicitly state that checkpoints will now only occur once per step execution (pre-initial apply), rather than per retry attempt. If per-attempt logging is required, `EditRequester::request_edits` is a potential place to hook it without modifying `RetryLoop`.
4. **Clarify `{exit_or_timeout}` in ST-3**: Specify exactly how to format the timeout case to avoid `Option::None` formatting panics in the step result message.
