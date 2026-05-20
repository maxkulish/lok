# Design Review: CLO-383

**Reviewer**: Claude (manual, in lieu of broken lok pipeline)  
**Reviewed**: 2026-05-20  
**Pipeline**: Manual review (lok design-review workflow had variable-resolution failures in shell steps)

---

## 1. Completeness Check

| Section | Present | Quality |
|---|---|---|
| Problem | ✅ | Clear with FR-21 context |
| Goals / Non-goals | ✅ | Well-scoped, explicit exclusions |
| Architecture | ✅ | Clean ASCII diagram, 4-file scope |
| Public API | ✅ | Before/after Rust signatures, exact call sites |
| Assumptions | ✅ | 6 assumptions with confidence + verification |
| Test plan | ✅ | Complete 8×2 matrix, unit + integration |
| Migration / Rollout | ✅ | Additive, no feature flag needed |
| Open questions | ✅ | 3 genuine open questions with tradeoffs |

All 8 required sections present. No missing sections.

## 2. Architecture Assessment

**Strengths**:
- Approach A (add `apply_edits` to `StepContext`, resolve in backends) is the right separation of concerns.
- No Backend trait changes means zero blast radius on unrelated backends.
- The resolution rule is simple, backend-local, and testable.
- The ASCII diagram correctly shows data flow across 3 files.

**Concerns**:
- The design correctly notes `StepContext` struct literal breakage in tests (good), but the `test_step_context_default_is_phase1_equivalent` test in `src/backend/context.rs` constructs `StepContext` with a struct literal. This test MUST be updated in the PR.
- Same concern for any other `StepContext {` struct-literal uses outside `from_prompt`. The design mentions `cargo check` will find them, but a grep gate during implementation is safer.

## 3. ADR Compliance

No ADRs in `docs/adrs/` directly constrain sandbox behavior. The design aligns with CLO-371-L1 (carrying struct fields populated at construction time) and CLO-374-L1 (shell command construction escapes all components).

## 4. Code Quality

- Public API signatures are clean and additive.
- The `effective_sandbox` resolution is a simple `match` — no over-engineering.
- The warning emission point is left genuinely open; this is appropriate.

## 5. Security Review

- No new secrets or credentials.
- The sandbox defaulting logic is read-only (no privilege escalation beyond what the step already requested via `apply_edits = true`).
- No new user input parsing.

## 6. Blind Spots

- **Consensus steps with mixed backends**: The design notes this as an open question. This is correct for scope but should be tracked as a follow-up issue.
- **Missing: `apply_edits` + `sandbox=None` on non-Codex/Gemini backends**: The design says Claude/Ollama/Bedrock ignore `apply_edits`. This is correct per the non-goals list. A future ticket could surface a warning when `apply_edits=true` is used with a backend that can't honor it.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is solid and ready for implementation. Suggestions are minor and do not block implementation.

## 8. Actionable Feedback

| Priority | Finding | Action |
|---|---|---|
| HIGH | `StepContext` struct literal in `src/backend/context.rs:108` will break compilation | Update test + grep for all literals |
| MEDIUM | Warning emission point is unresolved | Decide during implementation; either approach is fine |
| LOW | Helper extraction (match duplication) | Defer per design; revisit when 3rd subprocess backend needs it |
| LOW | Mixed-backend consensus validation | File follow-up issue after FR-22 lands |
