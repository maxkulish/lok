# Review Synthesis: CLO-185 - Structured Failure Data for Step Errors

**Synthesized**: 2026-04-03
**Pipeline**: lok design-review + Claude direct review
**Reviewers**: Gemini 3.1 Pro (FAILED), Codex/Ollama (glm-5:cloud), Claude Opus 4.6

---

> **Note:** Gemini review timed out. Synthesis based on Ollama and Claude reviews.

## Agreement (High Confidence)

Items where 2+ reviewers independently identified the same concern:

| # | Finding | Severity | Reviewers |
|---|---------|----------|-----------|
| 1 | `for_each` loop produces `StepResult { success: false, failure: None, validation: None }` - violates the contract invariant. Design defers this but the contract test will fail. | P1 | Claude, Ollama |
| 2 | `EmptyOutput` variant has no current call site in the execution path - either add a code path or defer the variant | P2 | Claude, Ollama |
| 3 | Architecture is sound - two-domain separation (execution vs validation) properly follows CLO-182 contract | Positive | Claude, Ollama |
| 4 | No security concerns - internal data model change only | Positive | Claude, Ollama |
| 5 | Contract enforcement test is a strong design decision that prevents drift | Positive | Claude, Ollama |

## Disagreement (Needs Human Decision)

| # | Topic | Claude Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | Call site count | Verified 16 via `grep -c` matching the acceptance criteria table | Says "17 vs 16 mismatch" - the Background section says "~17 non-validation failure paths" (approximate), acceptance criteria says 16 (exact). The 16 count is correct; "~17" in Background is a rough estimate. |

**Resolution**: No actual mismatch. Background uses approximate language ("~17"), the Failure Path Mapping table enumerates exactly 16, and `grep -c 'StepResult::error('` confirms 16. No action needed.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `output.clone()` in error() creates redundant `message` field - if always identical to `StepResult.output`, consider whether field is justified | Claude | P3 |
| 2 | Missing `#[allow(dead_code)]` on new `failure` field - needed for clippy, consistent with existing pattern | Claude | P2 |
| 3 | Should derive `Eq` alongside `PartialEq` on `StepFailureKind` - all variants are fieldless | Claude | P3 |
| 4 | Consider deriving `Copy` on `StepFailureKind` - all variants are simple | Ollama | P3 |
| 5 | Multi-backend consensus failure (#6) classified as single `BackendError` loses per-backend failure detail | Claude | P3 |
| 6 | `elapsed_ms: 0` for skip paths is technically correct but may confuse reporting | Claude | P3 |
| 7 | Retry exhaustion mapping could be more explicit: non-verify retries -> `BackendError`, verify loop -> `VerifyFailed` | Ollama | P2 |
| 8 | Consider adding debug log at `StepResult::error()` construction site | Ollama | P3 |

## Consolidated Verdict

**Overall: APPROVE_WITH_SUGGESTIONS**

- Gemini: REVIEW_FAILED (timeout)
- Ollama: APPROVE_WITH_SUGGESTIONS
- Claude: APPROVE_WITH_SUGGESTIONS

Consensus rule: No NEEDS_REVISION, not all APPROVE, therefore APPROVE_WITH_SUGGESTIONS.

## Priority Actions

| # | Action | Severity | Source |
|---|--------|----------|--------|
| 1 | **Resolve `for_each` contract violation**: Either add `StepFailureKind::PartialFailure`, classify with existing variant, or explicitly exclude for_each in contract test with documented rationale | P1 | Claude + Ollama |
| 2 | **Justify or remove `EmptyOutput` variant**: No current call site produces it. Either add execution-level empty-output detection or defer the variant | P2 | Claude + Ollama |
| 3 | **Add `#[allow(dead_code)]` annotations**: On `failure` field, `StepFailure` struct, `StepFailureKind` enum - consistent with existing pattern | P2 | Claude |
| 4 | **Clarify retry exhaustion mapping**: Make explicit that non-verify retries -> `BackendError`, verify loop exhaustion -> `VerifyFailed` | P2 | Ollama |
| 5 | **Derive `Eq` and `Copy` on `StepFailureKind`**: All variants are fieldless, both traits are trivially correct | P3 | Claude + Ollama |
| 6 | **Consider `Display` impl for `StepFailureKind`**: Human-readable variant names for logging | P3 | Claude |
