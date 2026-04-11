# Spec: Wire Apply-Verify Pipeline into Workflow Step Execution

**Task**: [CLO-211](https://linear.app/cloud-ai/issue/CLO-211)
**Created**: 2026-04-11
**Estimated scope**: M (1 main file + 1 new adapter module, ~6 sub-tasks)
**Depends on**: CLO-205 (done), CLO-210 (done), CLO-202 (done)

---

## 1. Problem Statement

`src/workflow.rs` currently implements its own inline edit-apply-verify-retry loop (lines 1928-2170) that is structurally inferior to the `src/apply_verify/` primitives shipped in CLO-210. The legacy implementation:

- **Only parses JSON** via `parse_edits()` at `src/workflow.rs:2906-2917` — ignores unified diff and full-file formats that `EditParser` supports.
- **Uses opaque `anyhow::Error`** throughout `apply_edits()` at `src/workflow.rs:2920-2962` and the fix loop — no structured error classification, no partial-apply state.
- **Depends on `git-agent` for rollback** (`git_agent::undo()` at lines 1980, 2047, 2087) — fails silently when `git-agent` is unavailable, leaving the workspace in a partial state.
- **Duplicates retry logic** between verify-fail (lines 2038-2081) and verify-timeout (lines 2082-2170) branches — two nearly-identical re-query blocks.
- **Lacks verification bounds** — `run_shell()` at `src/workflow.rs:2706-2750` has no output cap and no explicit process-group cleanup.
- **No real rollback on apply failure** — if the second of three edits fails, the first edit is left written to disk unless `git-agent` is present.

CLO-210 shipped a complete replacement for every one of these concerns in `src/apply_verify/`:

| Concern | Legacy | CLO-210 primitive |
|---|---|---|
| Parse | `parse_edits()` (JSON only) | `EditParser::parse()` (3-format auto-detect + markdown extraction) |
| Apply | `apply_edits()` (bails on error) | `DiffApplier::apply()` (returns `ApplyError { kind, partial }`) |
| Rollback | `git_agent::undo()` (optional, whole-repo) | `Rollback::rollback()` (backup-driven, per-file, best-effort) |
| Verify | `run_shell()` (unbounded) | `Verification::run()` (bounded output, timeout, process-group kill) |
| Retry loop | Inline duplicated branches | `RetryLoop::execute()` (parse → apply → verify → rollback → re-query) |

Until CLO-211 lands, the apply-verify module is dead code: compiled and tested, but never called from workflow execution. The user-visible Phase 8 feature (apply-verify pipeline) is not actually available.

**Who's affected**: Every workflow that sets `apply_edits = true`, `verify = "..."`, or `fix_retries > 0`. Without CLO-211, these workflows continue to run the legacy path.

**Why it matters**: This is the last task in Phase 8. Without the wire-up, Phase 8 is 67% complete (2/3 tasks: CLO-205 parser, CLO-210 primitives) but 0% functionally delivered to users. CLO-210's deferred decision — env var inheritance for the verification subprocess — also needs to be resolved here.

**Key files and line spans** (as of 2026-04-11):
- `src/workflow.rs:1350` — `apply_edits_flag` capture from `step.apply_edits`
- `src/workflow.rs:1928-2170` — inline `'fix_loop:` (apply → verify → rollback → re-query)
- `src/workflow.rs:2706-2750` — `run_shell()` (verify command executor)
- `src/workflow.rs:2906-2917` — `parse_edits()` (JSON-only legacy parser)
- `src/workflow.rs:2920-2962` — `apply_edits()` (anyhow-based edit application)
- `src/workflow.rs:89-99` — `AgenticOutput` legacy edit struct
- `src/workflow.rs:995-1039` — `StepFailureKind` (has `EditFailed`, `VerifyFailed`) and `StepFailure`
- `src/apply_verify/retry_loop.rs:15-298` — `RetryLoop` target API
- `src/apply_verify/verification.rs:20-162` — `Verification` (no env handling, cwd as param)
- `src/apply_verify/diff_applier.rs:23-196` — `DiffApplier` with `ApplyError { kind, partial }`
- `src/apply_verify/edit_parser.rs:65-102` — `EditParser::parse()`
- `src/apply_verify/rollback.rs:48-62` — `Rollback::rollback()`

---

## 2. Acceptance Criteria

### Functional

- [ ] **AC-1**: `src/workflow.rs` step execution invokes `RetryLoop::execute()` for any step with `apply_edits = true`; `parse_edits()` is no longer called from the step execution path.
- [ ] **AC-2**: `apply_edits()` is no longer called from the step execution path; `DiffApplier::apply()` is called by `RetryLoop`.
- [ ] **AC-3**: When `verify` is set, verification runs via `Verification::run()` (not `run_shell()` for the verify command) with the step timeout as the verification timeout.
- [ ] **AC-4**: When `fix_retries > 0` and verify fails, `RetryLoop` re-queries the LLM through an `EditRequester` adapter that wraps `backend.query()`.
- [ ] **AC-5**: On apply failure OR verify failure, `Rollback::rollback()` is called with the partial `ApplyResult` before any re-query.
- [ ] **AC-6**: All three edit formats work end-to-end through the workflow: JSON old/new (legacy), unified diff, full file.
- [ ] **AC-7**: `git-agent` checkpoint is still called **once per step execution** (before entering `RetryLoop`) when `git-agent` is available, but only for event-log/audit purposes — rollback correctness no longer depends on it. **Granularity change**: the legacy code called `git_agent::checkpoint()` on every attempt of the fix loop; the new code calls it exactly once, before `RetryLoop::execute()`. This is a deliberate simplification — the checkpoint was always intended for event logging, and per-attempt granularity was an accidental side effect of the inline loop structure. Documented in AC, commit message, and Linear comment.
- [ ] **AC-7b**: Even when `git-agent` is available, rollback on apply/verify failure uses `Rollback::rollback()` (from `src/apply_verify/rollback.rs`) — **not** `git_agent::undo()`. Verified by test 14b.
- [ ] **AC-8**: The `Verification` subprocess inherits the parent process environment by default. **No code change needed**: `tokio::process::Command::new("sh")` inherits parent env automatically unless `env_clear()` is called, and CLO-210's `Verification::run()` does not call `env_clear()`. This AC is documentation only — it records the decision for CLO-210's deferred question (C-5). Verified by test 13 (checks `PATH` is visible) and test 13b (checks custom env var is inherited).
- [ ] **AC-9**: Failed-edit step results populate `StepFailureKind::EditFailed` or `StepFailureKind::VerifyFailed` with a human-readable message following exact format strings (ST-3 table). Messages must distinguish verify timeout (`"Verification timed out after Xms"`) from non-zero exit (`"Verification failed with exit code N"`). `Option::None` exit codes in the timeout case must never produce `{None}` or panic — the formatter must branch on `timed_out` before formatting `exit_code`.
- [ ] **AC-9b**: `StepResult.elapsed_ms` is the outer-loop wall-clock time (via `start.elapsed().as_millis()` at the existing `src/workflow.rs:2163` pattern), preserved unchanged from the legacy implementation. This includes the initial backend query, all retries, and all rollbacks. `RetryLoopOutcome` has no timing field; timing is owned by the outer step execution.
- [ ] **AC-10**: Deletion policy for legacy helpers, **based on confirmed grep results from 2026-04-11**:
  - **Delete**: `parse_edits()` (`src/workflow.rs:2906-2917`), `apply_edits()` (`src/workflow.rs:2920-2962`), `AgenticOutput` (`src/workflow.rs:92-99`) — all confirmed no callers outside `workflow.rs` step execution and their unit tests.
  - **Keep**: `FileEdit` (`src/workflow.rs:83-87`) — used by `src/apply_verify/edit_parser.rs` and `src/apply_verify/diff_applier.rs`. Retained as-is.
  - **Keep**: `extract_json_from_text` (`src/workflow.rs:2851`) and `sanitize_json_strings` (`src/workflow.rs:2809`) — both `pub(crate)` and used by `src/template/context.rs` and `src/apply_verify/edit_parser.rs`. Retained as-is.
  - **Delete**: unit tests that exclusively test `parse_edits()` (e.g., `test_parse_edits_with_literal_newlines`, `test_parse_edits_with_backticks_in_content` at `src/workflow.rs:4700-4754` — actual lines to be confirmed during implementation).

### Behavioral Preservation

- [ ] **AC-11**: Workflows with `apply_edits = true` and JSON-format LLM output continue to work (backward compatibility via `EditParser`'s JSON path).
- [ ] **AC-11b**: Workflows with `apply_edits = true` and `verify = None` apply edits via `apply_once` (ST-4) without entering `RetryLoop`. Apply failures still trigger `Rollback::rollback()`. No retries happen (they have no verify to drive them). This is explicitly covered by tests 11 and 12.
- [ ] **AC-11c**: Parse errors trigger re-query by default (not immediate failure). This is a deliberate behavior change from the legacy code, which immediately returned `StepFailureKind::EditFailed` on parse error. Resolved in C-9; verified by test 5 (parse error retry succeeds) and test 6 (parse error exhausts retries). Documented in commit message and Linear comment.
- [ ] **AC-12**: Empty-edits case (LLM returns JSON with `edits: []` or equivalent) is treated as a no-op that proceeds to verify (same as current behavior).
- [ ] **AC-13**: Workflows with `format = "..."` still run the format command before verification. Preserved via **shell command composition**: when both `format` and `verify` are set, the `Verification::command` is built as `(format_cmd) || true && (verify_cmd)` — the `|| true` on the format segment preserves the current non-fatal semantics (format failure does not short-circuit verify), and the whole composed command runs inside `RetryLoop`, so format failures still trigger rollback+retry only when they cause verify to fail. See also the updated decision in C-6.
- [ ] **AC-13b**: When `apply_edits = true` and `verify = None` and `format = Some(_)`, the format command runs once after apply (before returning from the step). This uses the `apply_once` path (ST-4) plus an inline format invocation via `run_shell()` — `RetryLoop` is not used because there is no verify to drive retries.
- [ ] **AC-14**: The `checkpointed` log line and git-agent error-path warnings remain in the user-visible CLI output (regression risk: CLI output is tested by integration fixtures).

### Quality

- [ ] **AC-15**: `cargo test` passes with zero regressions in existing workflow tests.
- [ ] **AC-16**: `cargo clippy -- -D warnings` clean.
- [ ] **AC-17**: Net test count increases by at least 6 new integration tests (see Section 5) covering the new wiring.
- [ ] **AC-18**: No new `unwrap()` or `expect()` calls on non-infallible paths inside step execution.

**Verification method**:
- AC-1 through AC-12: New integration tests under `src/workflow.rs` `#[cfg(test)]` module exercising each format and each failure mode through the full step-execution path.
- AC-13, AC-14: Existing workflow tests that touch `format` and git-agent output paths must pass unchanged.
- AC-15, AC-16: CI commands.
- AC-17: `rg "#\[tokio::test\]|#\[test\]" src/workflow.rs | wc -l` before vs after.
- AC-18: Manual diff review.

---

## 3. Constraints

### Must

- **C-1**: Preserve the existing `Step` config surface — no new required fields. `apply_edits`, `verify`, `fix_retries`, `timeout` remain the sole knobs. (New *optional* fields are acceptable but not required for v1.)
- **C-2**: Preserve backward compatibility for the three current usage patterns: (a) `apply_edits = true` alone, (b) `apply_edits = true` + `verify`, (c) `apply_edits = true` + `verify` + `fix_retries`. Existing workflow TOML files must not need changes.
- **C-3**: Preserve backward compatibility for the JSON edit format — `EditParser` already handles this via its `JsonOldNew` variant.
- **C-4**: Single source of truth for `cwd`: the workflow passes one `cwd: &Path` into `RetryLoop::execute()`, which forwards it to both `DiffApplier::apply()` and `Verification::run()`. Do not store `cwd` in any new struct field.
- **C-5**: The `Verification` subprocess must inherit the parent process environment by default. Rationale: `run_shell()` inherits today via `tokio::process::Command::new("sh")` without `env_clear()`; the integration must preserve this so workflows that rely on `PATH`, `HOME`, and custom env vars keep working. This resolves CLO-210's explicitly deferred question.
- **C-6** (revised after spec review): The `format` command (when present) runs *between* apply and verify, preserving current ordering and non-fatal semantics. **Resolution of C-6 vs MN-7 conflict**: `RetryLoop::execute()` couples apply and verify tightly, so there is no literal "between" window in the Rust API. Instead, `format` is composed into the `Verification::command` as a shell pipeline: when both `format` and `verify` are set, `Verification.command = "(<format_cmd>) || true && (<verify_cmd>)"`. The `|| true` preserves the legacy non-fatal semantics for format. The `&&` ensures verify only runs if format's shell short-circuit allows it (which `|| true` always does). When `format` alone is set without `verify`, it runs via `run_shell()` in the `apply_once` path (AC-13b). This resolves the conflict without touching `src/apply_verify/` (MN-7) and without adding a `pre_verify` hook to `RetryLoop`. **Caveat**: the composed command's stderr now includes format stderr, potentially polluting verify error messages. Acceptable tradeoff for v1; a dedicated `format_and_verify` field on `Verification` is a possible follow-up.
- **C-7**: The `git-agent` checkpoint call must remain in place when git-agent is available, but only as a pre-apply event notification. Rollback correctness must not depend on its success. If the checkpoint fails, log a warning and proceed.
- **C-8**: Error messages surfaced to the user must include the specific failure variant. `StepFailureKind::EditFailed` for parse/apply, `StepFailureKind::VerifyFailed` for verification exhaustion. The message string must include enough context for the user to understand what went wrong (e.g., file path and `AmbiguousMatch` count, or verify exit code).
- **C-9**: The `stop_on_parse_error` flag on `RetryLoop` must be set to **`false`** so that parse errors trigger re-query. This is a deliberate **behavior change** from the literal legacy behavior (which immediately returned `StepFailureKind::EditFailed` on parse error without re-querying). Rationale: the conceptual intent of `fix_retries` is "retry any failure in the apply-verify cycle"; parse failures were excluded by accident, not design, in the legacy code. The new behavior uses the LLM's retry budget to guide the LLM back to a parseable format, which is consistent with how apply and verify failures are handled. The behavior change must be noted in the commit message and in the CLO-211 completion comment on Linear so future spec reads can find it. Escalation: if any reviewer flags this as risky, escalate per E-5.

- **C-10** (new, from review): `Step.fix_retries` maps **1:1** to `RetryLoop::max_retries`. A value of `fix_retries = N` means N *retries* after an initial failure, so total attempts = 1 + N. Examples: `fix_retries = 0` → 1 attempt only (apply once, verify once, fail). `fix_retries = 3` → up to 4 attempts. This matches the legacy `fix_attempt < fix_retries` loop condition at `src/workflow.rs:2052`.

- **C-11** (new, from review): `step.timeout = 0` is a sentinel meaning "no timeout" in the legacy code, implemented as a large duration (≈1 year). The `Verification` field expects a real `Duration`, so the mapping is: `timeout_duration = if step.timeout == 0 { Duration::from_secs(31_536_000) } else { Duration::from_millis(step.timeout) }`. This exact mapping already exists in the legacy step execution code (search for `timeout_ms` construction near `src/workflow.rs:1350-1400`) and must be reused unchanged.

- **C-12** (new, from review): `DEFAULT_VERIFY_MAX_OUTPUT_BYTES = 1_048_576` (1 MiB). This matches `EditParser::MAX_INPUT_SIZE` in `src/apply_verify/edit_parser.rs`, which is a deliberate symmetry — the parser caps LLM-produced output at 1 MiB, so the verify command's output cap matches. A shared constant or a doc-comment cross-reference is sufficient; creating a public `const` in `apply_verify::mod` is preferred if trivially possible without touching the module's API surface (a `pub const` is not an API-breaking change and does not violate MN-7).

### Must-not

- **MN-1**: Must not introduce a new dependency on `git-agent` for rollback. `Rollback::rollback()` is the rollback mechanism; git-agent remains optional event logging only.
- **MN-2**: Must not wrap `RetryLoop::execute()` in another retry loop. The legacy code had *two* retry layers: `fix_attempt` loop around `backend.query()` + the inline verify retry. Collapse both into `RetryLoop`.
- **MN-3**: Must not duplicate the verify-fail and verify-timeout branches. `VerifyResult` distinguishes `timed_out` as a struct field; `RetryLoop` handles both in one path.
- **MN-4**: Must not change the `StepFailureKind` enum. The existing `EditFailed` and `VerifyFailed` variants are sufficient. Adding new variants is out of scope.
- **MN-5**: Must not delete `parse_edits()`, `apply_edits()`, `AgenticOutput`, `extract_json_from_text`, or `sanitize_json_strings` without first confirming (via `rg`) that no other module references them. If they have other callers, leave them and file a follow-up cleanup task.
- **MN-6**: Must not add TOML-parse-level backward incompatibility. Any new optional `Step` fields must have `#[serde(default)]`.
- **MN-7**: Must not apply changes to `src/apply_verify/` as part of this task. That module is complete; touching it is scope creep. If a genuine API gap emerges (e.g., `Verification` needs env var injection), stop and escalate.

### Prefer

- **P-1**: Prefer extracting the `EditRequester` adapter into a small nested module (e.g., `mod apply_verify_adapter` inside `src/workflow.rs`, or a new `src/workflow/apply_verify_adapter.rs` if the `workflow.rs` file is getting unwieldy) rather than adding free-floating closures. Testability is the driver.
- **P-2**: Prefer building `Verification` once per step (outside any loop) and passing it into `RetryLoop` by value/move — it's a small struct (command, timeout, max_output_bytes).
- **P-3**: Prefer a sensible default for `max_output_bytes` (e.g., 1 MiB = `1_048_576`). This is not configurable in v1; add a TODO note for CLO-follow-up if observability demands control.
- **P-4**: Prefer keeping the CLI output lines as close to current wording as possible to minimize test churn. "Applying edits..." → still `→ Applying edits...`. "Verification passed" → still `✓ Verification passed`. Adapt only where `RetryLoop` changes the semantics (e.g., attempt numbering).
- **P-5**: Prefer writing new tests with real `tempfile::TempDir` fixtures over mocking `DiffApplier` / `Verification`. This follows the user's feedback memory: real filesystem > mocks.

### Escalate when

- **E-1**: If the `EditRequester` trait shape does not fit the workflow's re-query semantics (e.g., the prompt-building strategy conflicts with `RetryContext`), stop and review the trait in `src/apply_verify/retry_loop.rs`. Do not modify the trait without approval.
- **E-2**: If multi-hunk diffs become a production issue (observed in real LLM outputs >5% of apply attempts), stop and file a follow-up task. Do not extend `DiffApplier` speculatively — per CLO-210 spec line 179.
- **E-3**: If any existing workflow test produces a semantically different result under the new wiring (beyond the documented parse-error retry change in C-9), stop and reconcile. Do not silently "fix" tests to match the new behavior without documenting the divergence in the CLO-211 completion comment.
- **E-4**: If removing `parse_edits()`/`apply_edits()` breaks anything outside the step execution path (e.g., another module imports them), stop, leave the legacy helpers, and file a cleanup follow-up.
- **E-5**: If any reviewer flags the `stop_on_parse_error = false` decision (C-9) as risky for existing workflows, escalate to user for explicit approval before merging.

---

## 4. Decomposition

All sub-tasks are in `src/workflow.rs` unless noted. The order below is the recommended dependency order; some steps can run in parallel if a reviewer wants to split PR review across people.

### ST-1: Add `EditRequester` adapter

**Files**: `src/workflow.rs` (new nested module or new file)
**Depends on**: None
**Estimated effort**: 1-2 hours

Create a struct that implements `apply_verify::EditRequester`:
- Fields: a reference to the `Backend` trait object, the original prompt, the step timeout duration, model override (Option<String>), the step's backend name, the step's cwd, and the step name (for CLI output). Holds a `&Mutex<StepRuntime>` or similar for mutating `step_stderr`, `step_exit_code`, `last_error` observed during retries so the outer step execution can record them in `StepResult`.
- `request_edits(&self, context: &RetryContext<'_>) -> Result<String, String>` does:
  1. **Print a progress line** matching legacy CLI output: `"  ↻ Fix attempt {N}/{fix_retries}..."` where N is `context.attempt` and `fix_retries` is the adapter's stored total retries. This replaces the legacy `println!` at `src/workflow.rs:2054` and 2094. Required for P-4 (minimize CLI churn).
  2. Build a remediation prompt from the original prompt + `context.reason` (**must** include `context.previous_raw` so the LLM sees its failing output — closing the blind spot Gemini flagged).
  3. Call `backend.query(&fix_prompt, &cwd, model_override.as_deref())` under `tokio::time::timeout(self.timeout_duration, ...)`.
  4. On timeout or error, print the matching legacy line (`"    ✗ Re-query failed: {e}"` or `"    ✗ Re-query timed out"`) and return `Err(message)`.
  5. On success, capture `qo.stderr` and `qo.exit_code` into the adapter's mutable state (so outer step execution can surface them in `StepResult`), and return `Ok(qo.stdout)`.

**Re-query prompt templates** (exact format strings, preserving legacy structure and including `previous_raw`):

```text
# ParseError variant
{prompt}

## Previous Attempt Failed

Parse error:
```
{display}
```

The output you generated was:
```
{previous_raw}
```

Please provide a corrected output. Use JSON old/new format, unified diff, or full file content. Extract edits from markdown code blocks if helpful.
```

```text
# ApplyError variant
{prompt}

## Previous Attempt Failed

Apply error:
```
{message}
```

Files that failed to apply: {partial_paths_comma_separated}

The output you generated was:
```
{previous_raw}
```

Please provide corrected edits.
```

```text
# VerifyError variant
{prompt}

## Previous Attempt Failed

Verification error:
```
{stderr_truncated_to_4kb}
```

Exit code: {exit_code_or_TIMEOUT}
Elapsed: {elapsed_ms}ms

The output you generated was:
```
{previous_raw}
```

Please provide a corrected fix.
```

For `VerifyError`, the `exit_code_or_TIMEOUT` field is the string `"TIMEOUT"` when `VerifyResult.timed_out == true`, otherwise `format!("{}", exit_code.unwrap_or(-1))`. Never produce `None` or a panic.

`stderr_truncated_to_4kb` caps stderr at 4096 bytes (UTF-8 safe truncation) to keep prompts below backend context limits.

**Acceptance**:
- Unit test for each `RetryReason` variant that asserts the built prompt contains the original prompt + the remediation context + `context.previous_raw`. Test uses a mock `Backend` that records the prompt.
- Unit test that captures stdout to verify the `"↻ Fix attempt N/M..."` line is emitted.
- Unit test for VerifyError with `timed_out = true` asserts the prompt contains `"Exit code: TIMEOUT"` and does not contain `"None"`.

### ST-2: Replace fix_loop with `RetryLoop::execute()`

**Files**: `src/workflow.rs` (lines 1928-2170 replaced)
**Depends on**: ST-1
**Estimated effort**: 3-4 hours

In the step execution path, replace the inline `'fix_loop:` with:

```rust
if apply_edits_flag {
    // 1. git-agent checkpoint ONCE per step (event logging only, per AC-7)
    println!("  → Applying edits...");
    let checkpoint_msg = format!("pre-edit: {}", step_name);
    match git_agent::checkpoint(&cwd, &checkpoint_msg).await {
        Ok(true)  => println!("    ✓ git-agent checkpoint created"),
        Ok(false) => {} // git-agent not available, silent
        Err(e)    => println!("    ⚠ git-agent checkpoint failed: {}", e),
    }

    // 2. Build Verification command — compose format into verify if both set (C-6)
    let verify_command_opt = match (format.as_deref(), verify.as_deref()) {
        (Some(f), Some(v)) => Some(format!("({}) || true && ({})", f, v)),
        (None,    Some(v)) => Some(v.to_string()),
        (Some(_), None)    => None, // format alone handled in apply_once (AC-13b)
        (None,    None)    => None,
    };

    // 3. Map step.timeout (ms, 0=no-timeout) → Duration (C-11)
    let timeout_duration = if step.timeout == 0 {
        Duration::from_secs(31_536_000) // ~1 year sentinel, matches legacy
    } else {
        Duration::from_millis(step.timeout)
    };

    // 4. Branch on whether we have anything to verify
    let step_outcome: StepOutcome = match verify_command_opt {
        Some(cmd) => {
            // Full RetryLoop path (C-6 composed command)
            let verification = Verification {
                command: cmd,
                timeout: timeout_duration,
                max_output_bytes: DEFAULT_VERIFY_MAX_OUTPUT_BYTES, // 1 MiB, C-12
            };
            let retry_loop = RetryLoop {
                max_retries: fix_retries,
                verify: verification,
                stop_on_parse_error: false, // C-9
            };
            let applier = DiffApplier;
            let requester = WorkflowEditRequester {
                backend: backend.as_ref(),
                original_prompt: prompt.clone(),
                timeout_duration,
                model_override: model_override.clone(),
                cwd: cwd.clone(),
                step_name: step_name.clone(),
                fix_retries,
                // &Mutex<StepRuntime> for recording step_stderr/exit_code across retries
                runtime: &step_runtime,
            };
            let outcome = retry_loop.execute(
                text.clone(), &cwd, &applier, &requester
            ).await;
            StepOutcome::from_retry(outcome)
        }
        None => {
            // Apply-only path (ST-4): parse + apply + optional format, no retries
            apply_once(&text, &cwd, format.as_deref()).await
        }
    };

    // 5. Map outcome → StepResult (ST-3)
    match step_outcome { /* see ST-3 */ }
}
// The `else if let Some(verify_cmd)` branch (verify without apply_edits)
// remains unchanged and continues to use run_shell() with timeout.
```

**Key deltas from legacy**:
- **No nested `fix_loop` label**: `RetryLoop::execute()` is the loop.
- **No `checkpointed` local variable**: rollback no longer depends on it (AC-7).
- **Single git-agent checkpoint call**: per AC-7 granularity change.
- **Single `timeout_duration` calculation**: reused for both apply timing and verify timeout.
- **Format compiles into verify shell command**: per C-6 resolution.
- **`WorkflowEditRequester` handles all re-query progress output**: per ST-1.

**Acceptance**: Step execution compiles; happy-path workflow test (apply + verify, single JSON edit) passes; `cargo clippy -- -D warnings` clean.

**Implementer discretion on ST-2 scope**: Ollama suggested splitting ST-2 into ST-2a (adapter), ST-2b (verify-fail branch), ST-2c (verify-timeout branch), ST-2d (cleanup of local variables). Gemini said the current scope is acceptable. **Decision**: the implementer may split ST-2 at commit granularity if the 250-line diff is unwieldy for review, but the spec treats it as a single sub-task because the verify-fail and verify-timeout branches collapse into one `RetryLoop::execute()` call — there is no natural seam between them in the new code. If splitting, group commits as: `(1) adapter + scaffolding`, `(2) replace fix_loop with RetryLoop`, `(3) remove dead locals + format composition`.

### ST-3: Map `RetryLoopOutcome` / `ApplyError` / `VerifyResult` to `StepResult`

**Files**: `src/workflow.rs` (adjacent to ST-2)
**Depends on**: ST-2
**Estimated effort**: 2-3 hours

For each failure mode, produce a `StepFailureKind` + message. **Exact format strings** (preserves legacy diagnostic style where applicable; resolves AC-9 and the Ollama/Gemini timeout-formatting concern):

| RetryLoop terminal state | StepFailureKind | Message format string |
|---|---|---|
| `success: true` | (none) | Normal `StepResult` with `success: true`, `failure: None` |
| Last attempt failed on parse | `EditFailed` | `format!("Parse failed after {} attempts: {}\n\nLast output:\n{}", attempt_count, last_parse_error, last_raw_truncated_4kb)` |
| Last attempt failed on apply | `EditFailed` | `format!("Apply failed after {} attempts: {}\n\nFailed files: {}\n\nLast output:\n{}", attempt_count, last_apply_error, partial_paths_joined, last_raw_truncated_4kb)` |
| Last attempt verify exit non-zero | `VerifyFailed` | `format!("Verification failed after {} attempts with exit code {}.\n\nStderr:\n{}", attempt_count, exit_code, stderr_truncated_1kb)` |
| Last attempt verify timed out | `VerifyFailed` | `format!("Verification timed out after {} attempts ({}ms limit).\n\nPartial stderr:\n{}", attempt_count, timeout_ms, stderr_truncated_1kb)` |
| `stop_on_parse_error = true` + parse failed (not used in v1 per C-9) | `EditFailed` | N/A |

**Critical**: the timeout case must branch on `VerifyResult.timed_out` **before** formatting `exit_code` to avoid producing `None` or panicking. Code pattern:
```rust
let msg = if verify_result.timed_out {
    format!("Verification timed out after {} attempts ({}ms limit).\n\nPartial stderr:\n{}", ..., timeout_ms, stderr_trunc)
} else {
    format!("Verification failed after {} attempts with exit code {}.\n\nStderr:\n{}", ..., verify_result.exit_code.unwrap_or(-1), stderr_trunc)
};
```

`attempt_count` is `outcome.attempts.len()` (total attempts including the initial one, so 1 for no retries, up to `fix_retries + 1`).

`last_raw_truncated_4kb` and `stderr_truncated_1kb` are UTF-8-safe truncations.

`partial_paths_joined` is `partial_paths.iter().map(|p| p.display()).join(", ")` or equivalent.

The `StepFailure` struct is populated:
- `kind`: as per table
- `message`: as per format string above
- `backend`: `Some(backend_name.clone())`
- `exit_code`: for verify-exit case, `verify_result.exit_code`; for timeout/parse/apply, `None`
- `elapsed_ms`: `start.elapsed().as_millis() as u64` (AC-9b, matches legacy line 2163)

**Acceptance**:
- Unit tests for each failure mode assert the resulting `StepResult` has the expected `failure.kind` AND that the message matches a regex for the expected format string keywords.
- Unit test for the timeout case asserts the message contains `"timed out"` and does NOT contain `"None"` or `"-1"`.
- Unit test for the exit-code case asserts the message contains `"exit code"` and a digit.

### ST-4: Implement `apply_once` helper for verify-less apply path

**Files**: `src/workflow.rs` (new private helper)
**Depends on**: ST-2
**Estimated effort**: 1-1.5 hours

When `apply_edits = true` but `verify` is `None`, `RetryLoop` is overkill (it requires a `Verification`). Implement a private async helper in `src/workflow.rs`:

```rust
async fn apply_once(
    text: &str,
    cwd: &Path,
    format: Option<&str>,
) -> StepOutcome {
    // 1. Parse — no retries, no re-query (matches legacy single-shot behavior)
    let parsed = match EditParser::new().parse(text) {
        Ok(p) => p,
        Err(e) => return StepOutcome::parse_failed(e.to_string(), text.to_string()),
    };

    // 2. Apply — rollback on failure, but do NOT re-query (no retry budget in this path)
    let applier = DiffApplier;
    let apply_result = match applier.apply(&parsed, cwd).await {
        Ok(r) => r,
        Err(ae) => {
            // Best-effort rollback; surfacing rollback errors is non-fatal
            let _rollback_report = Rollback::rollback(&ae.partial, cwd).await;
            return StepOutcome::apply_failed(ae, text.to_string());
        }
    };

    // 3. Format (AC-13b) — non-fatal, inline run_shell invocation, no RetryLoop involvement
    if let Some(fmt_cmd) = format {
        println!("  → Running format command: {}", fmt_cmd);
        match run_shell(fmt_cmd, cwd, /* timeout */ Duration::from_secs(120)).await {
            Ok(_) => {}
            Err(e) => {
                println!("    ⚠ format command failed (non-fatal): {}", e);
                // Intentionally continue — format failures are non-fatal (matches C-6 legacy semantics)
            }
        }
    }

    StepOutcome::apply_succeeded(apply_result)
}
```

**Notes**:
- The helper lives **in `src/workflow.rs`** (not in `src/apply_verify/`) because it is workflow-specific glue that composes multiple primitives and threads CLI output. Keeping it here respects MN-7.
- The helper does **not** call `RetryLoop::execute()` because there is no verify to drive retries. Apply-only semantics match the legacy path for this configuration.
- Format handling uses `run_shell()` rather than shell composition because there is no `Verification` to compose into (AC-13b).
- The helper does **not** call `git_agent::checkpoint()` — that is done once in the caller (ST-2) before either `apply_once` or `RetryLoop::execute()` is invoked.

**Acceptance**:
- Workflow test with `apply_edits = true, verify = None` successfully applies a JSON edit and completes the step (test 11).
- Workflow test with `apply_edits = true, verify = None` and apply failure rolls back the workspace (test 12).
- Workflow test with `apply_edits = true, verify = None, format = Some(...)` runs format after apply and proceeds (test 11b — new, added in ST-5 below).

### ST-5: Integration tests for the new wiring

**Files**: `src/workflow.rs` `#[cfg(test)]` module (or new `tests/workflow_apply_verify.rs` integration test file if the existing module is too large)
**Depends on**: ST-1 through ST-4
**Estimated effort**: 5-7 hours

Write the tests listed in Section 5's table. Use `tempfile::TempDir` for real filesystem fixtures. Use a mock `Backend` that returns scripted outputs (one `stdout` per scripted call; exhaust the script and fail if asked for more). Run each test with `#[tokio::test]`.

**Test placement**: if `src/workflow.rs` `#[cfg(test)]` is already > 2000 lines, create a new `tests/workflow_apply_verify.rs` integration test file and put all new tests there. Prefer in-file for test discoverability if the file isn't overwhelming.

**Test coverage checklist** (must cover every acceptance criterion that the spec calls out as "verified by test N"):

- [ ] AC-1/AC-2/AC-4: Happy path JSON (test 1)
- [ ] AC-6: Happy path for all three edit formats (tests 1, 2, 3)
- [ ] AC-7: git-agent checkpoint called once per step, not per attempt (test 14, 14b)
- [ ] AC-7b: Rollback uses `Rollback::rollback()` even when git-agent is available (test 14b — **new**)
- [ ] AC-8/C-5: Env inheritance (tests 13, 13b)
- [ ] AC-9 / ST-3: All five failure-mode message formats (tests 6, 10, plus new format-string assertions in tests 6 and 10)
- [ ] AC-11b: Apply-only path (tests 11, 12)
- [ ] AC-11c / C-9: `stop_on_parse_error = false` triggers retry (tests 5 and 6 exercise this; add explicit assertion in test 5 that `RetryLoop.stop_on_parse_error` is false)
- [ ] AC-12: Empty edits proceeds to verify (test 4)
- [ ] AC-13: Format composed into verify command (test 15 — **new**)
- [ ] AC-13b: Format-without-verify single-shot path (test 11b — **new**)

**New tests added in response to spec review**:
- **Test 11b** `test_apply_without_verify_runs_format`: `apply_edits = true, verify = None, format = Some("echo formatted")`. Verifies `apply_once` runs the format command after apply, continues even on format failure.
- **Test 13b** `test_apply_verify_env_custom_var`: Sets a custom env var `LOK_TEST_VAR=abc123` before running lok, uses verify command `sh -c 'test "$LOK_TEST_VAR" = abc123'`. Proves not just `PATH` but arbitrary parent env is inherited (closes C-5 gap flagged by Ollama).
- **Test 14b** `test_apply_verify_git_agent_available_rollback_path`: git-agent **is** available and `checkpoint` succeeds, but apply fails mid-way. Asserts `Rollback::rollback()` was called (workspace matches pre-step state) and `git_agent::undo()` was **not** called (via a mock git-agent that records calls).
- **Test 15** `test_apply_verify_format_composes_into_verify`: `apply_edits = true, verify = Some("echo ok"), format = Some("echo formatted")`. Asserts the verify command observed was `"(echo formatted) || true && (echo ok)"` (via a mock `Verification` or by capturing the shell command string).

**Acceptance**: All 17 tests (1 through 15, including 11b, 13b, 14b) pass. Minimum 6 are net-new to satisfy AC-17.

### ST-6: Delete legacy helpers (conditional, per AC-10 retention policy)

**Files**: `src/workflow.rs`
**Depends on**: ST-2, ST-5 (must confirm no regressions)
**Estimated effort**: 30-45 minutes

**Retention decisions (locked in during spec review; do not re-check during implementation unless the retention policy itself changed)**:

| Symbol | Line (approx) | Decision | Reason |
|---|---|---|---|
| `parse_edits` (fn) | 2906-2917 | **DELETE** | Only caller is the legacy step execution path, which is being replaced by `EditParser::parse()` in ST-2. |
| `apply_edits` (fn) | 2920-2962 | **DELETE** | Only caller is the legacy step execution path, which is being replaced by `DiffApplier::apply()` in ST-2. |
| `AgenticOutput` (struct) | 92-99 | **DELETE** | Only used as the return type of `parse_edits`. Deletion falls out of deleting `parse_edits`. |
| `FileEdit` (struct) | 83-87 | **KEEP** | Used by `src/apply_verify/edit_parser.rs` and `src/apply_verify/diff_applier.rs`. **Do not delete.** |
| `extract_json_from_text` (fn) | 2851 | **KEEP** | `pub(crate)`, used by `src/template/context.rs` AND `src/apply_verify/edit_parser.rs`. **Do not delete.** |
| `sanitize_json_strings` (fn) | 2809 | **KEEP** | `pub(crate)`, used by `src/template/context.rs` AND `src/apply_verify/edit_parser.rs`. **Do not delete.** |

**Verification steps** (do these grep checks at the start of ST-6 to confirm the retention decisions are still valid — line numbers may have shifted, but caller modules should be unchanged):

```bash
# Confirm these are safe to delete (should return ONLY workflow.rs references in the step execution path or its tests):
rg 'parse_edits\(' --type rust
rg 'apply_edits\(' --type rust  # watch out: the Step.apply_edits FIELD is also named this
rg 'AgenticOutput' --type rust

# Confirm these are NOT safe to delete (should return references in template/context.rs and apply_verify/edit_parser.rs):
rg 'extract_json_from_text' --type rust
rg 'sanitize_json_strings' --type rust
rg 'FileEdit' --type rust
```

**Deletion list** (after verification):
1. Delete function body of `parse_edits()` (approx lines 2906-2917).
2. Delete function body of `apply_edits()` (approx lines 2920-2962).
3. Delete struct definition of `AgenticOutput` (approx lines 92-99).
4. Delete unit tests that exclusively test `parse_edits()` — search for `#[test]` or `#[tokio::test]` test fns whose bodies call `parse_edits(` directly. Line range to inspect: ~4700-4754.
5. Update imports: remove any `use ... {parse_edits, apply_edits, AgenticOutput}` references that no longer resolve.

**Escalation**: If any of the "DELETE" items has an unexpected external caller (per E-4), leave it, note in the PR description, and file a cleanup follow-up task. Do not force the deletion.

**Acceptance**:
- `cargo check` passes.
- `cargo test` passes (tests that exclusively exercised deleted helpers are expected to be deleted alongside the helpers).
- `cargo clippy -- -D warnings` clean.
- `rg 'fn parse_edits\(' src/workflow.rs` returns 0 results.
- `rg 'fn apply_edits\(' src/workflow.rs` returns 0 results (Step.apply_edits field still exists; grep for `fn` prefix).
- `rg 'struct AgenticOutput' src/workflow.rs` returns 0 results.
- `rg 'FileEdit' src/workflow.rs` still returns results (KEEP confirmation).

### Dependency order

```
ST-1 (adapter) ──┐
                 ├─→ ST-2 (replace fix_loop) ──┬─→ ST-3 (error mapping) ──┬─→ ST-5 (tests) ──→ ST-6 (delete legacy)
                 │                              └─→ ST-4 (apply_once)  ───┘
                 └─ (independent; can be written as stub first and filled in during ST-2)
```

ST-1 and ST-4 are small enough to be combined in a single commit with ST-2 if desired; ST-3 deserves its own commit for review clarity; ST-5 is a final commit; ST-6 is a cleanup commit at the end.

---

## 5. Evaluation

### Integration Test Table

All tests live in `src/workflow.rs` `#[cfg(test)]` (or `tests/workflow_apply_verify.rs`) and use `tempfile::TempDir` + a mock `Backend`.

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `test_apply_verify_happy_path_json` — single JSON old/new edit, verify passes on first attempt | Step succeeds; file content updated; verify ran once; zero retries | `cargo test test_apply_verify_happy_path_json` |
| 2 | `test_apply_verify_happy_path_unified_diff` — LLM emits unified diff format, verify passes | Step succeeds; file updated via diff; EditParser auto-detected format | `cargo test test_apply_verify_happy_path_unified_diff` |
| 3 | `test_apply_verify_happy_path_full_file` — LLM emits full file content, verify passes | Step succeeds; file overwritten; format detected | `cargo test test_apply_verify_happy_path_full_file` |
| 4 | `test_apply_verify_empty_edits_proceeds_to_verify` — LLM returns `{"edits": []}`, verify passes | Step succeeds; no files modified; verify still runs | `cargo test test_apply_verify_empty_edits_proceeds_to_verify` |
| 5 | `test_apply_verify_parse_error_retries_then_succeeds` — attempt 1 is unparseable garbage, attempt 2 is valid JSON, verify passes | Step succeeds on attempt 2; both attempts recorded in debug output | `cargo test test_apply_verify_parse_error_retries_then_succeeds` |
| 6 | `test_apply_verify_parse_error_exhausts_retries` — all attempts are unparseable, `fix_retries = 2` | StepResult has `StepFailureKind::EditFailed`; message mentions parse error | `cargo test test_apply_verify_parse_error_exhausts_retries` |
| 7 | `test_apply_verify_apply_error_rolls_back_and_retries` — attempt 1 has old-text-not-found (rollback triggered), attempt 2 is valid, verify passes | Step succeeds; workspace matches attempt 2 (no residue from attempt 1) | `cargo test test_apply_verify_apply_error_rolls_back_and_retries` |
| 8 | `test_apply_verify_verify_fail_rolls_back_and_retries` — attempt 1 applies but verify exits non-zero, attempt 2 applies and verify passes | Step succeeds; workspace matches attempt 2 | `cargo test test_apply_verify_verify_fail_rolls_back_and_retries` |
| 9 | `test_apply_verify_verify_timeout_rolls_back_and_retries` — verify command hangs > timeout on attempt 1, attempt 2 finishes fast | Step succeeds; timeout was detected; process group killed; workspace matches attempt 2 | `cargo test test_apply_verify_verify_timeout_rolls_back_and_retries` |
| 10 | `test_apply_verify_exhausts_retries_on_verify` — all 3 verify attempts fail, `fix_retries = 2` | StepResult has `StepFailureKind::VerifyFailed`; workspace matches pre-step state (final rollback applied); message mentions exit code or timeout | `cargo test test_apply_verify_exhausts_retries_on_verify` |
| 11 | `test_apply_without_verify_single_shot` — `apply_edits = true`, `verify = None`, valid JSON edit | Step succeeds; file updated; `RetryLoop` not used (ST-4 path) | `cargo test test_apply_without_verify_single_shot` |
| 11b | `test_apply_without_verify_runs_format` — `apply_edits = true`, `verify = None`, `format = Some("echo formatted")` | Step succeeds; `apply_once` path runs format after apply via inline `run_shell`; `RetryLoop` not used (AC-13b) | `cargo test test_apply_without_verify_runs_format` |
| 12 | `test_apply_without_verify_rolls_back_on_apply_error` — `apply_edits = true`, `verify = None`, apply fails mid-way | StepResult has `EditFailed`; workspace matches pre-step state | `cargo test test_apply_without_verify_rolls_back_on_apply_error` |
| 13 | `test_apply_verify_env_inheritance_path` — verify command is `sh -c 'test -n "$PATH"'` | Verify succeeds because `PATH` was inherited (C-5, coarse check) | `cargo test test_apply_verify_env_inheritance_path` |
| 13b | `test_apply_verify_env_inheritance_custom_var` — test sets `LOK_TEST_VAR=abc123` in the test process env, verify command is `sh -c 'test "$LOK_TEST_VAR" = abc123'` | Verify succeeds because **arbitrary parent env** is inherited (C-5, closes Ollama gap) | `cargo test test_apply_verify_env_inheritance_custom_var` |
| 14 | `test_apply_verify_git_agent_optional` — git-agent unavailable; full cycle runs without it | Step succeeds; no git-agent warnings; rollback still works via `Rollback::rollback()` | `cargo test test_apply_verify_git_agent_optional` |
| 14b | `test_apply_verify_git_agent_available_rollback_uses_rollback` — git-agent **is** available and checkpoint succeeds, apply fails mid-way | `Rollback::rollback()` is called (workspace restored) AND `git_agent::undo()` is **not** called (AC-7b, closes Ollama/Gemini gap) | `cargo test test_apply_verify_git_agent_available_rollback_uses_rollback` |
| 15 | `test_apply_verify_format_composes_into_verify_command` — `apply_edits = true`, `verify = Some("echo ok")`, `format = Some("echo formatted")` | Verify command observed is `"(echo formatted) || true && (echo ok)"` (AC-13, C-6 resolution) | `cargo test test_apply_verify_format_composes_into_verify_command` |

Minimum required: **6 of these tests** must be new to satisfy AC-17 (tests 1, 5, 7, 8, 10, 13b are the minimum set if time is constrained). All 17 are preferred.

**Additional regression assertion (not a standalone test)**:
- **Parse error retry regression**: In test 5 (`test_apply_verify_parse_error_retries_then_succeeds`), add an explicit assertion that the constructed `RetryLoop` has `stop_on_parse_error: false`. This locks in the behavior change from C-9 and prevents silent regression if someone flips the default.

### Build & Lint

| # | Command | Expected |
|---|---|---|
| 15 | `cargo test` | All tests pass including the full existing workflow test suite |
| 16 | `cargo clippy -- -D warnings` | Clean; no new warnings |
| 17 | `cargo build --features bedrock` | Builds (bedrock feature still works) |
| 18 | `rg "fn parse_edits\(" src/workflow.rs` | Returns 0 results (or 1 result + note explaining why it stayed — ST-6 decision) |
| 19 | `rg "fn apply_edits\(" src/workflow.rs` | Returns 0 results (or 1 result + note) |
| 20 | `rg "git_agent::undo" src/workflow.rs` | Returns 0 results (rollback no longer depends on git-agent) |

### Edge cases to verify manually

- **Large LLM output**: LLM returns 5 MB of text. `EditParser::parse()` rejects with `InputTooLarge` (1 MB cap). `RetryLoop` records the parse error and re-queries with a message that guides the LLM to produce smaller output.
- **Verify command produces massive stdout/stderr**: 10 MB of log output. `Verification::run()` caps at `max_output_bytes`. `VerifyResult.truncated = true`. Step message includes "(output truncated)" note.
- **Nested `fix_retries`**: Step has `fix_retries = 3`. After 4 total attempts (1 initial + 3 retries), `RetryLoopOutcome::success = false`. StepResult has `VerifyFailed` with "after 4 attempts".
- **Unicode in file paths and edit content**: JSON edit with `old = "héllo"`, file at `tests/файл.rs`. Apply succeeds; verify runs against the correct path.
- **Rollback failure**: File is made read-only between apply and rollback. `RollbackReport.failed` has an entry; StepResult surfaces this as a warning but the primary failure (whatever triggered the rollback) is the `StepFailureKind`.
- **Format command between apply and verify fails**: `format = "cargo fmt --check"`, format exits non-zero. Current behavior: continue to verify. New behavior: same (format failures stay non-fatal per C-6).

### Non-goals (explicitly out of scope for CLO-211)

- Adding `max_output_bytes`, `verify_timeout`, or `apply_parser` as Step config fields. Defaults are hardcoded; if needed, a follow-up task.
- Adding env var injection to `Verification::run()`. Parent env is inherited by default (C-5). Custom env vars are a follow-up.
- Optimizing the multi-hunk diff path. Current behavior (fail with `MultiHunkDiffNotContiguous`) is preserved.
- Migrating the `format` command to `Verification::run()`. Format stays on `run_shell()` for now — no spec constraint requires migrating it.
- Changing the `StepResult` schema. No new fields on `StepResult` or `StepFailure`.
- Updating the CLI output format beyond what `RetryLoop` demands. Minimizing CLI churn is a P-4 preference.
- Migrating tests that only touch `parse_edits()`/`apply_edits()` as unit tests — they'll be deleted along with the helpers in ST-6.

---

## Open questions (resolved in this spec)

### Resolved before initial draft

1. **Env var inheritance for `Verification` subprocess** → **RESOLVED** in C-5 / AC-8: parent env is inherited by default. Rationale: matches current `run_shell()` behavior; zero backward-compat risk. No code change needed — `tokio::process::Command::new("sh")` inherits by default when `env_clear()` is not called.
2. **`stop_on_parse_error` default** → **RESOLVED** in C-9 / AC-11c: set to `false`. Rationale: matches the conceptual intent of `fix_retries` (retry any failure mode); the current literal behavior of immediately failing on parse errors was an accident of the inline loop, not an intentional policy. This is a deliberate **behavior change** documented in commit message, PR description, and Linear completion comment. E-5 applies: if any reviewer flags this as risky, escalate for explicit approval.
3. **`RetryLoop` vs simple apply path when `verify = None`** → **RESOLVED** in ST-4 / AC-11b: use a simpler `apply_once` helper when verify is absent. `RetryLoop` requires a `Verification`; forcing a no-op verify would be ugly.
4. **`checkpointed` flag semantics post-rollback-redesign** → **RESOLVED** in AC-7 / AC-7b: `git-agent` checkpoint is event logging only; the `checkpointed` flag is removed from rollback decision logic. Rollback always uses `Rollback::rollback()` with the partial `ApplyResult`, regardless of whether git-agent is available.
5. **Default for `max_output_bytes`** → **RESOLVED** in P-3 / C-12: 1 MiB (`1_048_576` bytes). Hardcoded constant; matches `EditParser::MAX_INPUT_SIZE` symmetry; configurable in a follow-up.
6. **Message format for exhausted-retry failures** → **RESOLVED** in ST-3 table: exact format strings for all five failure modes with timeout branching to avoid `None` formatting bugs.

### Resolved during spec review (NEEDS_REVISION → addressed)

7. **C-6 vs MN-7 contradiction** (Gemini): the format command must run between apply and verify, but `RetryLoop::execute()` couples them and MN-7 forbids touching `src/apply_verify/`. → **RESOLVED** via **shell composition**: `Verification.command = "(format_cmd) || true && (verify_cmd)"`. Preserves non-fatal format semantics, keeps format inside `RetryLoop`, no module API changes. Verified by test 15.
8. **Missing `previous_raw` in re-query templates** (Gemini): legacy prompt templates don't pass the LLM's failing output back to it, degrading retry effectiveness. → **RESOLVED** in ST-1 updated prompt templates: all three (ParseError, ApplyError, VerifyError) now include `context.previous_raw` verbatim with clear labeling.
9. **Apply-only path not in acceptance criteria** (Ollama): AC-3 covers verification but the `apply_edits = true, verify = None` path has no AC. → **RESOLVED** in AC-11b and ST-4: explicit `apply_once` helper, explicit AC, explicit tests 11 and 12.
10. **git-agent granularity change** (Gemini): moving `checkpoint` outside the retry loop changes audit granularity from per-attempt to per-step. → **RESOLVED** in AC-7 with explicit documentation: this is a deliberate simplification. Legacy per-attempt behavior was accidental. Must be noted in commit/PR/Linear.
11. **git-agent available + rollback path** (Ollama): no test verifies that `Rollback::rollback()` is used even when `git_agent::undo()` is available. → **RESOLVED** with new test 14b.
12. **Timeout formatting in verify failure message** (Gemini): `VerifyResult.exit_code` is `None` on timeout; the spec must not produce `"None"` or panic. → **RESOLVED** in ST-3 table and AC-9 with explicit code pattern: branch on `timed_out` before formatting `exit_code`.
13. **`elapsed_ms` instrumentation** (Ollama): `RetryLoopOutcome` has no timing field; unclear if `StepResult.elapsed_ms` is preserved. → **RESOLVED** in AC-9b: outer `start` instant is preserved unchanged from legacy line 2163.
14. **CLI output parity** (Ollama): `RetryLoop` has no logging hooks; legacy code has per-attempt `println!` lines. → **RESOLVED** in ST-1: progress lines are emitted from `WorkflowEditRequester::request_edits()` (called once per retry attempt) and from the outer step execution for pre-apply messages. No new hook needed in `RetryLoop`.
15. **`stop_on_parse_error` regression risk** (Ollama): no test locks in the `false` default. → **RESOLVED** with explicit assertion in test 5 (see "Additional regression assertion" in Section 5).
16. **`fix_retries` 1:1 mapping** (Ollama): not explicitly stated that `fix_retries = N` → `N + 1` total attempts. → **RESOLVED** in C-10 with concrete examples.
17. **`step.timeout = 0` sentinel handling** (Ollama): legacy uses `0` as "no timeout"; unclear how the new code maps this. → **RESOLVED** in C-11 with explicit `if step.timeout == 0 { Duration::from_secs(31_536_000) } else { ... }` mapping.
18. **`MAX_INPUT_SIZE` / `max_output_bytes` cross-reference** (Ollama): risk of drift between parser cap and verify output cap. → **RESOLVED** in C-12: both hardcoded at 1 MiB with explicit doc-comment cross-reference; optional `pub const` in `apply_verify::mod` if trivially possible.
19. **Format-without-verify path** (Ollama — derived from apply-only path gap): if `apply_edits = true, verify = None, format = Some(_)`, where does format run? → **RESOLVED** in AC-13b: inline `run_shell()` call in `apply_once`.
20. **ST-6 retention rules** (Ollama): `FileEdit`, `extract_json_from_text`, `sanitize_json_strings` are shared with other modules and must not be deleted. → **RESOLVED** in AC-10 and ST-6 with an explicit keep/delete decision table locked in during spec review.

### Unresolved / deferred to follow-up tasks

- **Stderr pollution from composed format+verify**: the `(format) || true && (verify)` composition sends format stderr into the same pipe as verify stderr. Acceptable for v1 (rare edge case; error messages are truncated anyway). If observability issues arise, a dedicated `format_and_verify` field on `Verification` is the follow-up shape.
- **Configurable `max_output_bytes`**: not in v1 per P-3. Follow-up if users hit the 1 MiB cap.
- **Multi-hunk unified diff support**: CLO-210 marked this as "fail fast, escalate if > 5% of apply attempts". Preserved here via E-2.

## Glossary

- **EditRequester**: Trait in `src/apply_verify/retry_loop.rs` that `RetryLoop` calls to re-query the LLM with remediation context. CLO-211 implements a workflow-backed adapter.
- **RetryContext / RetryReason**: The context passed to `request_edits()`; carries attempt number, previous raw output, and the structured failure reason.
- **ApplyError { kind, partial }**: The typed error from `DiffApplier::apply()`. `partial` is the `ApplyResult` of files that were written before the failure — passed directly to `Rollback::rollback()` to restore them.
- **VerifyResult**: The never-erring result from `Verification::run()`; carries `success`, `stdout`, `stderr`, `exit_code`, `elapsed_ms`, `timed_out`, `truncated`.
- **fix_loop / fix_attempt**: The legacy inline retry loop in `src/workflow.rs:1928-2170`; being replaced by `RetryLoop::execute()`.
- **Fix retries**: The user-facing retry budget (`Step.fix_retries`); maps 1:1 to `RetryLoop::max_retries`.
