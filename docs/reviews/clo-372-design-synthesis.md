# Design Review Synthesis: CLO-372

**Date:** 2026-05-18
**Design:** `docs/designs/clo-372-thread-stepcontext-non-step.md`
**Inputs:** `docs/reviews/clo-372-design-gemini.md`

## Verdict

approve_with_changes

## Summary

Gemini approved the design and found no blockers. Two observations were raised:

1. `Config` cloning in `Team`/`Spawn` is acceptable for this scoped change; `Arc<Config>` remains an available future pattern if clone cost ever matters.
2. Preserving the existing `timeout = 0` convention by mapping to a one-year effective timeout is correct and behavior-preserving.

## Applied suggestions

- Added an explanatory comment to the `effective_timeout_secs` design snippet documenting the `timeout = 0` convention.
- Added the comment requirement to migration step 2.

## Flagged / not applied

None.

## Final assessment

The design is ready for human review and planning. It is scoped to FR-20b, preserves current runtime behavior, and carries prior CLO-371 lessons forward through explicit model/timeout population and public API visibility checks.
