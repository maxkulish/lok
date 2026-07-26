# Review Synthesis: CLO-593 extract-lok-backend-lib

**Synthesized:** 2026-07-26
**Pipeline:** manual opencode Gemini review (lok workflow template failed due to variable interpolation bug)
**Reviewer:** Gemini 3.5 Flash
**Verdict:** APPROVE_WITH_SUGGESTIONS

---

## Reviewer Status

| Reviewer | Status | Detail |
|---|---|---|
| Gemini 3.5 Flash | OK | Completed via opencode; 51-line structured review written |
| Codex / Ollama | SKIPPED | Workflow step failed on `steps.health_check.output` template variable; no local Ollama model available |
| Claude fallback | SKIPPED | External reviewer succeeded, so fallback not invoked |

## Agreement (High Confidence)

No multi-reviewer agreement possible because only Gemini produced output. Findings below are taken from the single reviewer.

## Key Findings

| # | Finding | Severity |
|---|---|---|
| 1 | **Process-global `BACKEND_CACHE`** may leak configuration between downstream consumers that use the same backend name with different `BackendConfig` values. | critical |
| 2 | **Tokio `MutexGuard` leaks** through `acquire_test_lock` public test-support surface, coupling consumers to lok's exact tokio version. | high |
| 3 | **Library side-effects**: `retry.rs` writes directly to stderr via `colored` `eprintln!`, which is poor practice for a reusable library. | high |
| 4 | **ADR location (Q1)** and `test-support` feature visibility (Q7) remain unresolved. | medium |
| 5 | **Dependency bloat from Approach A**: consumers compile `clap`, `minijinja`, `chrono`, etc. even though backend code does not need them. | medium |
| 6 | No explicit check for config drift / mutation when reusing cached backends. | medium |
| 7 | MSRV and downstream dependency alignment not verified. | medium |
| 8 | `dead_code` / clippy warnings may appear after splitting binary and library compilation units. | low |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** — The design is solid and ready to implement, but hygiene issues around global state, public test surface, and library logging should be addressed during rollout.

## Priority Actions

1. **Critical (Q3 global state):** Decide on `BACKEND_CACHE` handling. Options: (a) implement a `BackendRegistry` instance that owns the cache, (b) key the static map by a hash of `BackendConfig`, or (c) accept the global and document the constraint. If not resolved in this ticket, add a follow-up before CLO-594 locks the boundary.
2. **High (Q7 test surface):** Wrap `acquire_test_lock`'s return in an opaque `TestLockGuard` to avoid leaking `tokio::sync::MutexGuard` across the library boundary.
3. **High (Q4 logging):** Replace direct `eprintln!` / `colored` output in `retry.rs` with a logging facade (e.g., `log::warn!` or `tracing::warn!`) so consumers control formatting and destination. If that adds a new dependency, evaluate `tracing` vs keeping the print and documenting it as a known limitation.
4. **Medium (Q1 ADR location):** Create `lok/docs/adrs/` and record the `[lib]` vs workspace decision there, mirroring gcm's convention.
5. **Medium (Q5 workspace trigger):** State the trigger criteria for migrating to a workspace split in the ADR (e.g., measured downstream build-time regression threshold).
6. **Medium:** Verify MSRV and dependency alignment with gcm/remem before opening the lok PR.
7. **Low:** Run `cargo clippy --all-targets` after each rollout step to catch new `dead_code` warnings.
