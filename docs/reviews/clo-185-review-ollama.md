# Design Review: clo-185

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-03
**Pipeline**: lok design-review

---

## Design Document Review: CLO-185 Structured Failure Data

---

### **VERDICT: APPROVE_WITH_SUGGESTIONS**

---

### **Key Findings**

**Strengths:**
- Clear separation of execution-domain (`StepFailure`) vs validation-domain (`ValidationResult`) failures, respecting CLO-182 contract
- Well-researched prior art (Temporal/Dagster patterns) with documented rationale
- Comprehensive testing strategy including contract enforcement tests
- Good observability hooks (`elapsed_ms`, `backend`, `exit_code`)
- Implementation plan is granular and testable

**Issues Found:**

1. **Call site count mismatch** - Analysis says 17 call sites, Acceptance Criteria says 16. Needs reconciliation before implementation.

2. **`for_each` partial success undefined** - Design mentions deferring `Vec<StepFailure>` but doesn't specify what `failure` contains for aggregate results when some iterations succeed and some fail. The edge case list mentions this but the spec doesn't define expected behavior.

3. **Retry exhaustion ambiguity** - `VerifyFailed` covers verify/fix loop exhaustion, but the Implementation Plan mentions "backend retry" paths (item #8). Should retries without a verify clause produce `BackendError`? This distinction isn't explicit.

4. **`EmptyOutput` variant scope** - Currently limited to "no validate clause present." What happens when a validate clause exists but backend returns empty? That flows through validation, but `EmptyOutput` won't be set. The variant's scope could be clearer.

---

### **Prioritized Action Items**

| Priority | Item |
|----------|------|
| P1 | Reconcile call site count: verify exact number and update either Analysis (line ~50) or Acceptance Criteria to match |
| P2 | Add explicit spec for `for_each` aggregate `failure` field behavior in partial-success scenarios (single `StepFailure` representing aggregate? `None` with individual iteration failures tracked separately?) |
| P2 | Clarify retry exhaustion mapping: non-verify retries → `BackendError` vs verify loop exhaustion → `VerifyFailed` |
| P3 | Add a note to `EmptyOutput` variant documenting its mutual exclusion with validation path |
| P3 | Consider whether `StepFailureKind` should derive `Copy` (currently only `Clone`) - all variants are simple and `Copy` would simplify test assertions |

---

### **Architecture Quality: Strong**
The two-domain separation is sound. The 6 specific variants avoid the overloading problem identified in discovery.

### **Operational Readiness: Good**
Contract test ensures drift detection. Consider adding a debug log at `StepResult::error()` construction site for troubleshooting.

### **Security Posture: No Concerns**
No secrets, no new dependencies, internal data modeling only.
