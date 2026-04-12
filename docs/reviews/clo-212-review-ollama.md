# Design Review: CLO-212 - Role Routing Config

**Reviewed**: 2026-04-12
**Reviewer**: Codex via Ollama (glm-5:cloud)
**Design Document**: docs/design-docs/clo-212-role-routing-config.md
**Pipeline**: lok design-review

---

I've reviewed the design document. Here's my assessment:

---

## Verdict: **APPROVE_WITH_SUGGESTIONS**

The document is well-structured and demonstrates solid architectural thinking. The separation of `RoutingStrategy` from `ConsensusStrategy` is a key insight that will prevent future confusion. The two-tier resolution pattern (team → global) is clean and matches industry patterns from the discovery research.

---

## Strengths

- **Clear conceptual separation**: Distinguishing routing (backend selection) from consensus (response combination) is correct
- **Backward compatibility is explicit**: `#[serde(default)]` and `Delegator` fallback preserve existing behavior
- **Validation at config load time**: Catches `min_success` bounds errors early, not at runtime
- **Good acceptance criteria**: Specific, testable conditions with clear verification method
- **Discovery research documented**: Prior art analysis shows due diligence

---

## Key Findings

### 1. Missing Error Handling Specification
**Severity: Medium**

The document mentions "unknown backends produce warnings, not errors" but doesn't specify:
- What error types will `RoleResolver::resolve()` return?
- How should callers distinguish between "role not found" vs "all backends disabled" vs "validation failed"?
- The `Resolution::empty()` method is truncated in the code block

**Suggestion**: Define a `RoleResolutionError` enum with variants for each failure mode.

### 2. No Timeout Handling for Parallel Strategy
**Severity: Medium**

`RoutingStrategy::Parallel { min_success }` fires all backends but has no explicit timeout. If 3 of 5 backends respond and the other 2 hang, what happens?

**Suggestion**: Add `timeout: Option<Duration>` to `RoutingStrategy::Parallel` or document that callers must wrap with timeout.

### 3. Cancellation Not Addressed
**Severity: Medium**

Parallel execution needs cancellation semantics. If `min_success = 2` and 2 backends succeed, should the remaining futures be cancelled?

**Suggestion**: Document cancellation behavior explicitly. Consider adding `cancel_remaining_on_success: bool` to `Parallel` strategy.

### 4. Phase Plan Disconnected from Sections
**Severity: Low**

Implementation phases reference "Phase 2" but architecture section doesn't label phases. Makes it harder to validate completeness.

**Suggestion**: Either remove phase numbering or add phase labels to architecture diagram.

### 5. Open Questions Should Be Resolved Before Implementation
**Severity: Medium**

Three open questions remain, including `--team` flag semantics which affects CLI contract:

> Should `--team` flag be additive with `[defaults.team]` config (flag overrides config) or exclusive?

This is core behavior that should be decided in design, not implementation.

**Suggestion**: Resolve open questions before approving implementation.

---

## Minor Issues

| Issue | Location | Fix |
|-------|----------|-----|
| Code block truncated | `Resolution::empty()` | Complete the implementation example |
| No error type hierarchy | Detailed Design | Define `RoleResolutionError` enum |
| Missing rollback plan | Testing Strategy | Add rollback steps if migration fails |
| No performance bounds | N/A | Consider adding latency SLAs for resolution |

---

## Prioritized Action Items

1. **Resolve open questions** (pre-implementation blocker) - Decide `--team` flag semantics and "no backends available" responsibility
2. **Define error types** - Create `RoleResolutionError` enum with clear variants
3. **Add timeout semantics** - Document or implement timeout handling for `Parallel` strategy
4. **Document cancellation behavior** - Specify what happens to pending futures after `min_success`
5. **Complete truncated code** - Finish `Resolution::empty()` and any other incomplete code blocks

---

## Summary

The design is fundamentally sound and demonstrates good architectural judgment. The main gaps are around error handling specification and timeout/cancellation semantics for parallel execution. These are addressable during implementation but specifying them upfront would reduce churn. Recommend resolving the open questions and adding the error type definition before implementation begins.
