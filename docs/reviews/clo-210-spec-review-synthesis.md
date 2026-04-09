# CLO-210 Spec Review - Synthesis

**Reviewed**: 2026-04-08
**Spec**: specs/2026-04-08-clo-210-apply-verify-pipeline.md
**Reviewers**: Gemini 2.5 Pro (direct invocation after workflow failure), Perplexity sonar-reasoning
**Combined verdict**: APPROVE_AFTER_REVISIONS (both reviewers flagged specific, fixable issues)

The standard lok spec-review workflow (`.lok/workflows/spec-review.toml`) crashed due to two unrelated bugs (MiniJinja parsing `steps.gemini-review.success` as subtraction via hyphen; validation step expecting JSON/REVIEW_FAILED prefix that the review prompt doesn't emit). Reviews were obtained by running Gemini directly in the background and Perplexity via the `perplexity_reason` tool in parallel.

## Per-Reviewer Verdicts

| Reviewer | Verdict | Severity of Issues |
|---|---|---|
| Gemini 2.5 Pro | NEEDS_REVISION | 1 critical logic gap, 2 design flaws |
| Perplexity sonar-reasoning | APPROVE_WITH_SUGGESTIONS | 3 HIGH, 2 MEDIUM, 1 LOW |

Gemini's NEEDS_REVISION was driven by a single hard bug: the `VerifyFailureContext` type was unconstructable for parse/apply failures because it required a `&VerifyResult` that doesn't exist when the pipeline short-circuits before verify runs. That alone forced restructure.

Perplexity did not flag the context-construction bug (it was focused on the API layer and error types) but caught a different set of issues Gemini missed: dual-API code smell, timeout `elapsed_ms` lie, unbounded output capture.

Taken together, the two reviews are complementary: Gemini caught type-system bugs and state-divergence risk; Perplexity caught API smell and runtime resource bugs.

## Issues by Severity

### CRITICAL (must fix before implementation)

1. **`VerifyFailureContext` unconstructable for parse/apply errors** (Gemini)
   - Old design: `VerifyFailureContext<'a> { previous_raw, verify_result: &'a VerifyResult, apply_error: Option<&'a str> }`
   - Problem: required `verify_result` has nothing to point to when parse or apply fails
   - Fix: rename to `RetryContext` and use `RetryReason<'a>` enum with `ParseError(&str) | ApplyError { message, partial_paths } | VerifyError(&VerifyResult)` variants

2. **Dual `apply` / `apply_with_partial` API** (Perplexity)
   - Old design: two methods on `DiffApplier` returning different result shapes
   - Problem: "Alternative Classes with Different Interfaces" smell; callers must choose which to use
   - Fix: single `apply()` returning `Result<ApplyResult, ApplyError>` where `ApplyError` struct carries `partial: ApplyResult`

### HIGH (design correctness)

3. **`elapsed_ms` lies on timeout** (Perplexity)
   - Old: `elapsed_ms = self.timeout.as_millis() as u64` on timeout path
   - Fix: capture `Instant::now()` before spawn; report `start.elapsed().as_millis()` in all paths

4. **Unbounded stdout/stderr capture** (Perplexity)
   - Old: no cap on verify command output
   - Fix: `Verification::max_output_bytes: usize` + `VerifyResult::truncated: bool`; stream into bounded buffers

5. **Zombie processes via `kill_on_drop`** (Gemini)
   - Old: relied on `kill_on_drop(true)` to clean up on timeout
   - Problem: only kills the direct `sh` child, not grandchildren (`npm test`, `node`, etc.)
   - Fix: `CommandExt::process_group(0)` before spawn (Unix); `libc::kill(-pid, SIGKILL)` on timeout to reap the whole group

6. **`cwd` duplication between Verification struct and RetryLoop** (Gemini)
   - Old: `Verification { cwd: PathBuf }` AND `RetryLoop::execute(cwd: &Path)`
   - Problem: two sources of truth can diverge
   - Fix: remove `cwd` from `Verification`; make it a `run(&self, cwd: &Path)` parameter

### MEDIUM (clarity / sharper constraints)

7. **`stop_on_parse_error` semantics underspecified** (Perplexity)
   - Fix: explicit truth table in acceptance criteria

8. **Single retry budget rationale not documented** (Perplexity)
   - Fix: state explicitly that a single `max_retries` budget is intentional (bounded total effort) and document the tradeoff

### LOW (acknowledgements / test additions)

9. **Directory rollback leaves empty parent dirs** (Gemini)
   - Fix: document as accepted non-bug in Must-not constraints

10. **Shell-pass-through not tested** (Perplexity)
    - Fix: note in edge cases that shell metacharacters are expected behavior, not a bug

11. **Compile test for RetryReason variants** (Gemini)
    - Fix: add tests #30b and #30c for `RetryReason::ParseError` and `RetryReason::ApplyError` context types

## Applied in Spec

All 11 issues were applied to the spec in-place. Key structural changes:

- `VerifyFailureContext` → `RetryContext` with `RetryReason` enum
- `DiffApplier::apply` + `apply_with_partial` → single `apply()` with `ApplyError { kind, partial }`
- `Verification { cwd: PathBuf, .. }` → `Verification { max_output_bytes, .. }` + `run(&self, cwd: &Path)`
- `elapsed_ms = self.timeout.as_millis()` → `start.elapsed().as_millis()` using `Instant::now()` captured pre-spawn
- Added process-group handling requirement in Must constraints
- Added `VerifyResult::truncated` field
- Added directory-rollback non-bug acknowledgment in Must-not
- Test count target raised from 35+ to 40+
- New tests: #9b (empty edits), #22 (rewritten - real elapsed_ms), #22b (output truncation), #22c (process-group kill), #30b (parse retry context), #30c (apply retry context)

## Residual Concerns / Deferred

- **Env var inheritance in verify subprocess** (Perplexity blind spot): not addressed in CLO-210 - will be surfaced in CLO-211 where the workflow executor owns the environment story
- **FD exhaustion on spawn failures** (Perplexity blind spot): not addressed - acceptable operational risk for now

## Final Verdict

**APPROVED for implementation** after applying the 11 items above. The spec is now self-contained, internally consistent, and ready for autonomous execution.
