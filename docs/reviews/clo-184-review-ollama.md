# Design Review: clo-184

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-03
**Pipeline**: lok design-review

---

# Design Document Review: CLO-184 LLM-Based Step Validation

## Verdict: **APPROVE_WITH_SUGGESTIONS**

---

## Key Findings

### Strengths

**Architecture Quality**
- Layered validation (heuristic → LLM) minimizes cost by skipping LLM when heuristics fail
- Single `run_step_validation()` function from all wiring points follows DRY principle
- Uses existing `Backend` trait without new abstractions
- Clear separation: interpolation, execution, parsing, and result propagation

**Codebase Alignment**
- References specific files (`src/workflow.rs`, lines 250-260) and existing structs
- Extends `ValidateConfig` with sensible defaults
- Integrates with existing `FailureType` enum rather than creating parallel error types

**Security Posture**
- `max_input_length` protects against context window overflow
- `on_error = "fail"` as safe default when validation infrastructure fails
- JSON structured output avoids prompt injection from REVIEW_FAILED text parsing
- No new security surface - reuses existing Backend abstraction

**Operational Readiness**
- Clear JSON response schema with `status`, `reason`, `cleaned` fields
- Backward compatibility with `REVIEW_FAILED:` prefix fallback
- `on_error` policy for infrastructure failures
- `replace_output` as opt-in (safer default)

---

## Prioritized Actionable Items

### P1 - Address Before Implementation

1. **Clarify validation timeout behavior**
   - Document states "inherited from backend's configured timeout" but doesn't specify if this is the step's backend or the validator's backend
   - Recommendation: Add explicit `validate.timeout` field or clarify that validation uses the validator backend's timeout

2. **Resolve the open question about `{{ steps.X.raw_output }}`**
   - This is marked as "open" but has implications for the implementation scope
   - Recommendation: Decide now or explicitly defer to a follow-up with tracked issue number

### P2 - Consider During Implementation

3. **Document concurrent validation safety**
   - Multiple steps may validate simultaneously; confirm no shared mutable state
   - Add a brief note in the Implementation Details section

4. **Clarify `cleaned` field security implications**
   - LLM-generated `cleaned` output replaces original output - what if the LLM injects malicious content?
   - Recommendation: Add a note that cleaned output inherits the same trust model as the step output itself

5. **Add edge case for empty `cleaned` field**
   - Current spec says empty validation response = pass with no cleaning
   - Clarify: what if JSON returns `{"status": "pass", "cleaned": ""}`? Should that be treated as a failure?

### P3 - Nice to Have

6. **Consider cost protection mechanism**
   - If validation runs on every step in large workflows, costs could accumulate
   - Recommendation: Add a note in Constraints about considering per-workflow validation call limits (deferred scope is fine)

7. **Add context window accounting note**
   - `max_input_length` truncates output, but prompt template also consumes tokens
   - Recommendation: Add note that `max_input_length` should account for prompt template overhead

---

## Summary

This is a well-structured, thorough design document. The architecture is sound, the implementation plan is actionable, and the acceptance criteria are testable. The layered validation approach (heuristic → LLM) with cost optimization, combined with JSON structured output for reliability, follows best practices from the discovery research.

The two P1 items should be resolved before implementation to avoid scope creep during coding. Otherwise, this is ready for implementation.
