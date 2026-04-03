# Spec Review: clo-183

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-04-03
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and accurately matches the Linear task description. It correctly bounds the scope to string-based checks only, cleanly deferring LLM-based verification to a future task (CLO-184), while establishing the unified configuration structures needed for both.

## 2. Acceptance Criteria Review
**Strong**: The criteria are highly specific, providing exact enum variants to use (`EmptyOutput`, `ValidationFailed`) and clear testable edge cases for each of the three check types (e.g., whitespace-only output).
**Gaps**:
- It doesn't specify how to populate the `validator: String` field which is required by `ValidationResult` (e.g., `"heuristic:not_empty"`).
- It doesn't specify how to populate `failure_reason` on the `ValidationResult`. A clear error string here is needed for debug logs (e.g., "Output is missing expected string '## Summary'").
- It dictates that `raw_output` should store the original output on failure, but doesn't clarify what happens to the `output` field.

## 3. Constraints Check
**Aligned**: The constraints are excellent. They enforce zero-allocation string operations, strictly forbid accidental LLM networking, and cleanly match the problem statement. The `Escalate` constraint correctly anticipates a known issue with TOML parsing nested tables via Serde default derives (`[steps.validate]` vs inline tables).
**Concerns**: No constraints contradict the codebase.

## 4. Decomposition Quality
**Well-scoped**: The sub-tasks are logical, independent, and broken down nicely into components that should each take < 2 hours. Implementation -> Step wiring -> Tests is an optimal execution sequence.
**Issues**: None. The dependency mapping (4 and 5 are independent after 3) is accurate.

## 5. Evaluation Coverage
**Covered**: Thorough unit tests cover both the parser logic and execution of the validation checks. Includes integration tests showing behavior within a TOML-defined workflow and tests for `continue_on_error`.
**Gaps**:
- Needs a test for multi-field scenarios (e.g., if a user erroneously provides both `check = "not_empty"` AND `backend = "claude"` before CLO-184 is implemented) to ensure the backend field is cleanly ignored.
- Needs a test for what happens when `validation` fails, but `fix_retries > 0`.

## 6. Codebase Alignment
**Violations**:
- **`StepResult.raw_output` mutation rule**: The AC says "`StepResult.raw_output` stores original output". However, the struct definition in `src/workflow.rs` states: `/// Original output before validation mutations. Populated only when validation changes output; None if validation ran but made no changes`. Since heuristic validators do not mutate the output, populating `raw_output` duplicates data and violates the existing struct contract.
**Alignment**: The proposal to integrate with the existing `continue_on_error` correctly leverages the fail-fast mechanics inside `src/workflow.rs` (lines 555-690) simply by checking `success = false`.

## 7. Blind Spots
- **Integration with `fix_retries`**: The spec mandates wiring the validation *after* line 1443 (`break 'fix_loop;`). This explicitly places validation outside the `fix_retries` retry loop (which normally catches `verify` script failures and asks the LLM to fix them). If validation fails here, the step fails instantly (or triggers a blind top-level retry), completely bypassing the LLM's self-healing loop. If this is intentional for heuristics, it should be explicitly stated. If not, validation should be moved *inside* the `'fix_loop`.

## 8. Verdict
APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Remove `raw_output` assignment**: Do not populate `StepResult.raw_output` on validation failure. The struct doc clearly states it should only be populated if the validator *mutates* the output. String checks don't mutate, so `output` holds the original text and `raw_output` should remain `None`.
2. **Clarify `ValidationResult` fields**: Specify what string should be passed to the `validator` field (e.g., `"heuristic:not_empty"` or `"heuristic:min_length"`) and provide guidance on formatting `failure_reason` (e.g., `"Output length 50 is less than minimum 200"`).
3. **Address `fix_retries` interaction**: Explicitly decide if heuristic validation failures should trigger the LLM `fix_retries` loop. If yes, move the validation logic *inside* the `'fix_loop` before line 1443. If no, keep it as-is but add a comment to the spec acknowledging that heuristic failures deliberately bypass self-healing.
4. **Clarify `StepResult.output` on failure**: Specify that on validation failure, `StepResult.output` must remain the original LLM/shell output (unlike `StepResult::error()` which replaces `output` with the error message).
