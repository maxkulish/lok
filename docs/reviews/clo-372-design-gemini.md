# Gemini Design Review: CLO-372

**Reviewer:** gemini-2.5-pro via `gemini` CLI
**Date:** 2026-05-18
**Design:** `docs/designs/clo-372-thread-stepcontext-non-step.md`

## Verdict

approve

---

## Findings

This is an excellent design document. It is thorough, well-scoped, and demonstrates a clear understanding of the problem and its context within the broader project. The proposed solution is pragmatic, minimally invasive, and effectively addresses the stated goals. The inclusion of non-goals, a detailed test plan, and a proactive analysis of assumptions and open questions significantly de-risks the implementation.

I have no findings that require changes before implementation. The following are minor observations for consideration.

### 1. (Observation, Low) `Config` Ownership Strategy

**Severity:** Low

The design correctly identifies that `Team` and `Spawn`'s async closures will require owned `Config` data (Assumptions A4) and proposes cloning it. This is a sound and simple approach, especially since `Config` is already `Clone` and not expected to be excessively large. The alternative, using `Arc<Config>` more pervasively as suggested for `run_query_with_config`, is also viable. The choice to prefer cloning is a reasonable trade-off between performance and implementation complexity for this specific change. This is not a required change, but an awareness point for the future: if `Config`'s clone cost ever becomes a concern, the project has a clear pattern (`Arc`) to follow.

### 2. (Observation, Info) Timeout Convention

**Severity:** Info

The design correctly preserves the existing "0 means no timeout" convention by mapping it to a one-year duration in `effective_timeout_secs`. This is the right call to avoid changing behavior. The proactive discussion in Q1 is noted and appreciated. This ensures that the `StepContext.timeout` field accurately reflects the duration used by the outer `tokio::time::timeout` wrapper, which is the primary goal.

---

## Actionable Suggestions

The design is approved and ready for implementation.

1. **Proceed with Implementation:** The plan laid out in Section 7 is clear and logical. No changes are needed.
2. **Add Explanatory Comment for Timeout:** During implementation of `effective_timeout_secs`, consider adding a small code comment clarifying why `0` is mapped to a large value, for the benefit of future maintainers. E.g., `// To preserve the existing convention where a 0-second timeout disables the timeout, we map it to a near-infinite duration for the tokio::time::timeout wrapper.`
3. **Execute the `grep` Guard:** The `grep` command proposed in the test plan is a simple and effective way to ensure complete migration. Be sure to run it before concluding the work.
