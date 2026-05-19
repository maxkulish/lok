# Design Review: CLO-379

**Reviewer**: Self-review (design-review workflow broken: unknown variable syntax `{{ steps.health_check.output }}`)
**Reviewed**: 2026-05-19
**Pipeline**: Manual review due to lok workflow template bug

---

## Context
- Branch: feat/clo-379-codex-json
- Design: docs/designs/clo-379-event-driven-codex-parser.md
- PRD: docs/prds/prd-phase-2-predictable-cli-execution-v5.md §4 FR-3a

## Findings

### F1 [minor] `ThreadStarted {}` discards `thread_id`
**Where:** design doc § Public API
**What:** The `ThreadStarted` variant has no fields, so `thread_id` from fixtures is silently ignored. This is harmless for FR-3a since we never need thread_id.
**Suggested fix:** No action needed. If FR-25 or FR-3b needs `thread_id`, add the field then.

### F2 [minor] Integration test visibility needs `pub(crate)` or explicit re-export
**Where:** design doc § Test plan
**What:** `CodexEvent`/`CodexItem` use `pub(super)`, which integration tests in `tests/` cannot see.
**Suggested fix:** Make `parse_jsonl_stream` `pub(crate)` in `codex_event.rs`, or add `#[cfg(test)] pub use codex_event::*` in `src/backend/mod.rs`. Either works.

### F3 [nit] `TurnStarted` and `ThreadStarted` could use unit variants instead of empty structs
**Where:** design doc § Public API
**What:** `ThreadStarted {}` and `TurnStarted {}` are unit structs syntactically. Using `ThreadStarted,` (unit variant) is more idiomatic Rust.
**Suggested fix:** Cosmetic; no functional impact.

### F4 [minor] `CodexUsage` uses `#[serde(default)]` on `u32` fields
**Where:** design doc § Public API
**What:** `#[serde(default)]` on `u32` means missing fields default to `0`. If a `turn.completed` emits `usage: {}`, all counts will be zero. This is defensible but might hide genuine empty-usage events.
**Suggested fix:** Keep as-is. Codex always emits populated usage on completion. Zero is a reasonable default.

## Strengths
- Type-safe serde event model with forward-compat hatch (`#[serde(other)]`).
- Clear state machine separates per-turn accumulation from end-of-stream selection.
- Error propagation aligns with existing `BackendError` variants - no trait churn.
- Test plan covers happy path, error path, edge cases, and forward-compat.
- Explicitly out-of-scopes FR-3b and FR-25, preventing scope creep.

## Verdict

PASS_WITH_NOTES

The design is sound and matches the PRD. The four findings above are all minor/nit level and do not materially affect the implementation. Fix F2 (integration test visibility) during implementation; F1/F3/F4 are acceptable as-is.
