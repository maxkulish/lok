# Pre-PR validation: clo-379

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-19
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned three findings (1 HIGH, 1 MEDIUM, 1 LOW) |
| Gemini | REVIEW_FAILED | CLI returned empty output due to untrusted-directory state |
| Claude fallback | SKIPPED | Codex succeeded; fallback not required |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR

- **Parse JSONL even on non-zero Codex exits.** `src/backend/codex.rs:123` short-circuits on `!output.status.success()` and discards stdout, returning only `Codex failed: {stderr}`. The design's manual verification step 3 explicitly requires "the error message contains the upstream `model is not supported` text rather than the raw JSONL", and the `turn-failed.jsonl` fixture corresponds to a Codex run that exits non-zero — so the JSONL error path is unreachable in production. Fix: in the non-zero-exit branch, run `parse_jsonl_stream(&stdout)` first; if it returns `Err(ExecutionFailed { .. })` or `Err(Parse { .. })`, propagate that error (annotating `exit_code: Some(exit_code)` on `ExecutionFailed`); fall back to the current stderr message only when stdout has no usable JSONL error event. This is the single change that makes the design's stated behavior actually fire on real Codex failures.

- **Create `tests/codex_parse_output.rs` per plan ST3.** Plan acceptance is `cargo test --test codex_parse_output` and the target does not exist (`tests/` shows only `codex_fixtures.rs`, `integration.rs`, `workflows/`). The four fixture-driven assertions currently live inside `src/backend/codex_event.rs:420-479`. Move them to a new `tests/codex_parse_output.rs` and expose `parse_jsonl_stream` + `ParsedTurn` via a `#[cfg(test)] pub use` (or `pub(crate)`) re-export through `src/backend/mod.rs` so integration tests can call them. The coverage already exists; this is purely a relocation to satisfy the plan's named test target.

## Out of Scope / Deferred

- **Debug logging for skipped unknown events and unparseable JSONL lines.** Real gap (design §Architecture and plan ST1 both call for `debug!`). However, the crate has no logging facade today (`tracing` / `log` are not in `Cargo.toml`, no `debug!` macro is used anywhere under `src/`), and adding one is a new dependency decision outside the FR-3a parser scope. Defer to whichever ticket introduces a logging crate (likely an observability ticket alongside FR-25); track as a TODO comment on the two `// Skipping ...` sites if desired, but do not block this PR.

## False Positives / Tooling Artifacts

- **Gemini REVIEW_FAILED.** Tooling artifact (Gemini CLI refused to read the untrusted directory). Not a finding against the change. Codex provided clean independent coverage so synthesis is well-grounded with one external reviewer.

## Recommendation

PROCEED_WITH_FIXES. Two bounded changes are required before opening the PR: (1) in `src/backend/codex.rs:123` parse stdout via `parse_jsonl_stream` before falling back to stderr on non-zero exits, so JSONL `turn.failed` / `error` messages surface as intended by the design's verification step 3; (2) move the four fixture-driven tests from `src/backend/codex_event.rs` into a new `tests/codex_parse_output.rs` (with the necessary `pub(crate)` re-export) to satisfy plan ST3's `cargo test --test codex_parse_output` acceptance gate. Both are mechanical and well-scoped to a single fix iteration. The `debug!` logging gap is real but should ride with the future logging-introduction ticket since this crate has no logging facade today.
