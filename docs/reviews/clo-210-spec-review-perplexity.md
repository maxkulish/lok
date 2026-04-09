# CLO-210 Spec Review - Perplexity (sonar-reasoning)

**Reviewed**: 2026-04-08
**Spec**: specs/2026-04-08-clo-210-apply-verify-pipeline.md
**Verdict**: APPROVE_WITH_SUGGESTIONS

## Summary

The specification is well-structured, self-contained, and deeply aligned with the existing `apply_verify` module. It shows strong understanding of both the CLO-205 upstream (EditParser) and the CLO-211 downstream (workflow executor wire-up) such that the boundary is clean. The decomposition into four modules is idiomatic and the 36 enumerated tests give good coverage. However, several design-level issues warrant revision before implementation.

## 1. Problem Statement Assessment

The problem statement is thorough. It cites concrete file/line references (`src/workflow.rs:2919-2961`, `src/workflow.rs:1398`, etc.) and explicitly lists what each new module is replacing. It correctly identifies that this task is *pure infrastructure* with CLO-211 doing the wire-up.

## 2. Acceptance Criteria Review

**Strengths**:
- Every type is fully specified with exact field lists and derive attributes
- Error enum variants are enumerated with payload structures
- Test scope for each module has a concrete target (12+, 8+, 7+, 10+)

**Issues**:

- **HIGH — Dual API code smell**: the spec introduces both `apply()` and `apply_with_partial()` methods on `DiffApplier`. This is a textbook code smell (Fowler: "Alternative Classes with Different Interfaces"). Prefer a single `apply()` that returns `Result<ApplyResult, ApplyError>` where `ApplyError` carries any partial `ApplyResult` via a struct field. This collapses two APIs into one and lets rollback-aware callers access the partial state through the error payload.

- **HIGH — `VerifyResult.elapsed_ms` is a lie on timeout**: the spec says "On timeout: sets ... `elapsed_ms = self.timeout.as_millis() as u64`". This reports the timeout threshold, not the actual elapsed time. If the child process is killed 50ms before or after the timeout fires, the reported number is wrong. Fix: capture `Instant::now()` before spawn and compute `start.elapsed()` in all exit paths (success, failure, timeout, spawn error).

- **HIGH — Unbounded output capture**: the spec does not place a cap on stdout/stderr capture. A verify command that emits gigabytes (e.g., `find /` or a runaway test suite log) will pin memory until the process exits. Add `Verification::max_output_bytes: usize` and a `VerifyResult::truncated: bool` flag.

- **MEDIUM — `stop_on_parse_error` is underspecified**: the single-sentence description ("true aborts, false counts as retry") leaves two questions open: (1) does `stop_on_parse_error: true` consume a retry attempt or short-circuit immediately? (2) does it affect the parse of re-queried output or only the initial parse? Write an explicit truth table.

- **MEDIUM — Single retry budget conflates failure modes**: `max_retries` counts parse + apply + verify failures together. A flaky LLM that keeps emitting unparseable output can burn the entire budget on parse errors, leaving zero attempts for the verify loop. Document why this is intentional (bounded total effort) or split into per-stage budgets.

- **LOW — Shell injection not tested**: the LLM authors the verify command, so shell injection is technically in-scope. A test that confirms the command is passed through verbatim (no double-escaping) would document the threat model.

## 3. Constraints Check

The Must / Must-not / Prefer / Escalate-when structure is clear and action-oriented. The "no `crate::backend::*` imports" constraint is well-justified (keeps the trait inverted). One gap: the spec says "tests must use `tempfile::tempdir()`" but doesn't explicitly forbid using the real network for verify tests - add a note that verify tests must use pure local commands.

## 4. Decomposition Quality

Seven sub-tasks with a clear dependency graph (1 -> 2,3,4 in parallel -> 5 -> 6 -> 7) is exactly right. Sub-tasks 2-4 are genuinely independent because they touch disjoint files. The parallelism note is accurate.

## 5. Evaluation Coverage

36 tests is rigorous for 4 new modules. Happy-path, error, and edge cases are all represented. Gaps:

- No test for the output-truncation edge case (need one once `max_output_bytes` is added)
- No test for the real elapsed_ms measurement (need one once `Instant::now()` is used)
- No test that the process group cleanup works (once process-group handling is added)

## 6. Codebase Alignment

The spec references the existing `run_shell()` pattern, `tokio::fs` usage, `thiserror` for error types, and `async-trait` for the `EditRequester` - all idiomatic for the codebase. The choice to use a unit struct (`pub struct DiffApplier;`) with methods is consistent with `EditParser` (the direct upstream dependency).

## 7. Blind Spots

- **Verify command inherits environment**: the spec doesn't say what env vars the verify command sees. By default, `tokio::process::Command` inherits the parent's env, which may leak secrets into the verify subprocess. Consider `env_clear()` + explicit passthrough or document inheritance as intentional.
- **File descriptor exhaustion**: if `max_retries` is high and each attempt spawns a verify command, FDs could leak on spawn failures. Not a blocker but worth noting.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The core design is sound and the decomposition is execution-ready. The HIGH issues (dual API, elapsed_ms lie, unbounded output) are straightforward to fix in-place without restructuring.

## 9. Actionable Feedback

1. Collapse `apply` + `apply_with_partial` into a single `apply()` returning `Result<ApplyResult, ApplyError>` where `ApplyError` has a `partial: ApplyResult` field.
2. Replace `elapsed_ms = self.timeout.as_millis()` with real wall-clock measurement via `Instant::now()` captured before spawn.
3. Add `Verification::max_output_bytes: usize` and `VerifyResult::truncated: bool`; stream output into bounded buffers.
4. Document `stop_on_parse_error` as a truth table in the spec.
5. Document the single-retry-budget decision explicitly (or split it).
6. Add a shell-pass-through test for the verify command.
