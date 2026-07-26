# Pre-PR validation: clo-589

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-07-26
**Pipeline**: lok pre-pr-validation
---

Verified everything I need. The two reports disagree on one factual question, and I can now settle it: the ADR genuinely does not name the cache cluster or the timeout constants, but Codex overreached on two sub-claims, and Gemini's PASS rests on a misattribution.

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned FAIL. Core finding partially confirmed; two sub-claims refuted. Could not run clippy/test (sandbox lock) — I ran them instead. |
| Gemini | OK | Returned PASS. Scope and gate claims confirmed; boundary-completeness claim rests on attributing the design doc's table to the ADR. |
| Claude fallback | SKIPPED | At least one external reviewer succeeded. |

**Independent verification I ran** (settles the Codex/Gemini split):
- `cargo fmt --check` → clean; `git diff --check` → clean
- `cargo clippy --all-targets -- -D warnings` → exit 0
- `cargo test` → 664 + 8 + 8 + 5 + 7 passed, 0 failed
- `git diff --name-status main...HEAD` → 9 files, all under `docs/`, nothing in `src/`, `Cargo.toml`, or `Cargo.lock`
- `rg -in 'cache|health|timeout|retry' docs/adrs/clo-589-backend-library-shape.md` → the only `cache` hits are `cache::CacheConfig` *inside* `Config`; the ADR never mentions `BACKEND_CACHE`, `get_cached_health`, `DEFAULT_TIMEOUT`, or `NO_TIMEOUT`
- `#[cfg(test)]` gating confirmed at `src/backend/mod.rs:423`, `:444`, `:453`, `:890`

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR

- **The ADR's boundary allocation is bucketed, not item-by-item, and omits the global-state cluster entirely.** PRD AC at `docs/prds/clo-589-backend-library-shape.md:38` requires item-by-item recording, and the design at `docs/designs/clo-589-backend-library-shape.md:291` explicitly promises CLO-590 will be "reviewed against the boundary table in this ADR" — but `docs/adrs/clo-589-backend-library-shape.md:36-61` ships bullet buckets ending in "etc.". Concretely missing by name:
  - **Binary-side, absent entirely:** `BACKEND_CACHE`, `get_backend_cache`, `CachedBackend`, `get_cached_health` (design allocates these at `:74-76`; `src/backend/mod.rs:407,409,415,639`)
  - **Library-side, absent entirely:** `DEFAULT_TIMEOUT`, `NO_TIMEOUT` (design `:51-52`; `src/backend/mod.rs:293,296`), `FLAG_MATRIX` (`src/backend/mod.rs:15`)
  - **Library-side, only implied:** `get_retry_policy` folded into "retry helpers"; the seven context types (`StepContext`, `StepOptions`, `Message`, `Role`, `SandboxMode`, `HealthStatus`, `ModelInfo`) folded into "context/message models"
  
  This is the ticket's entire deliverable — a citable contract. CLO-590 will cite the durable, indexed ADR, not a per-ticket design doc, so the gap pushes it back to re-deriving the split from a document the ADR index doesn't point at.

- **The two seam helpers are silently absent rather than explicitly deferred.** `effective_timeout` and `step_context_for_backend` (`src/backend/mod.rs:303,321`) are *intentionally* unassigned per design `:217` and `:297`. The ADR says nothing at all, which reads as an oversight rather than a decision. One sentence naming them as deliberately deferred to CLO-590 converts a silent hole into a recorded deferral.

Both are edits to a single markdown file — no code, no re-review of the decision itself.

## Out of Scope / Deferred

- **ST3 is weaker than the PRD AC** (Codex). `docs/plans/clo-589-backend-library-shape.md:77-82` checks 11 principal symbols, so it passes even when item-by-item recording is incomplete — which is exactly what happened here. Tightening it to diff the ADR table against `rg -n 'pub (fn|struct|enum|trait|const|static|use)'` is a sound idea and cheap enough to ride along in the same commit, but it is plan-document polish and does not block the PR.
- **Re-type the seam helpers to `&BackendConfig` + `&Defaults`** (Gemini rec 1). Already tracked verbatim as a design open question at `:297`. A signature change CLO-589 explicitly forbids (`:24`). CLO-590's call.
- **Relocate `BackendConfig`/`Defaults` into `src/backend/config.rs`** (Gemini rec 2). Already tracked at design `:296`, which states CLO-590 must pick and record. Deferring is the design's stated position, not an oversight.
- **Test-double exposure** (`StubBackend`, `clear_health_cache`, `set_mock_health`) — tracked at design `:300`; needs a `testing` feature decision in CLO-590.

## False Positives / Tooling Artifacts

- **Codex: "no explicit decision for `effective_timeout` / `step_context_for_backend`"** — refuted as a defect. The design deliberately withholds allocation (`:217`: "the ADR does not assign them. See Open questions"). Codex counted a sanctioned deferral as an omission. The residual issue is only that the ADR doesn't *say* it's deferred, which I folded into Must Fix above.
- **Codex: "test-only/public helper surface" needs allocation** — refuted. `StubBackend` (`:425`), `clear_health_cache` (`:445`), `set_mock_health` (`:454`), `TEST_MUTEX` (`:891`), and `acquire_test_lock` (`:894`) all sit behind `#[cfg(test)]`, so they never appear in a downstream build's surface and need no boundary allocation.
- **Codex: clippy/test unverified** — tooling artifact from a read-only sandbox that couldn't open `target/debug/.cargo-lock`. Both pass on my run.
- **Gemini: the ADR keeps the "global health cache" binary-owned** — factually wrong. That phrase describes the design doc's boundary diagram (`:74-76`), not the ADR, which never mentions the cache. Gemini's "All acceptance criteria have been fully covered" and its PASS both rest on this misattribution, which is why the PASS doesn't survive.

## Recommendation

**PROCEED_WITH_FIXES.** The architectural decision is sound and neither reviewer challenged it: the `[lib]`-in-`lokomotiv` shape, the `Config`-cycle rule, shared versioning, and the feature-gated `bedrock` exposure are all recorded with rationale, the rejected `lok-backend` alternative is stated, the index entry exists, and the diff is documentation-only with a green gate (clippy exit 0, 692 tests passing). What's missing is recording fidelity, not judgment — the ADR summarizes a boundary the design specifies precisely, and the design itself promised that table would live in the ADR. Two bounded edits to `docs/adrs/clo-589-backend-library-shape.md` close it: (1) replace the two bullet buckets with a table of `symbol | current source path | library/binary | rationale` covering every non-`cfg(test)` public item in `src/backend/mod.rs` and `src/backend/context.rs` — the design's diagram at `:43-80` and signature block at `:186-215` already contain every row, so this is transcription, not new analysis; (2) add an explicit "deferred to CLO-590" line for `effective_timeout` and `step_context_for_backend`. Optionally tighten ST3 in the same commit so the plan's own gate enforces the AC that this review had to catch by hand. No user decision is needed and no re-review of the decision is warranted — re-verify only the ADR text, then transition to PR.

## Re-validation

The single permitted fix iteration was applied:

- Replaced the ADR's bucketed lists with an item-by-item table containing public item, current source, owner, and rationale.
- Explicitly assigned the process-global health cluster and timeout/retry constants.
- Explicitly recorded `effective_timeout` and `step_context_for_backend` as deferred to CLO-590.
- Explicitly identified `#[cfg(test)]` helpers as test-only rather than downstream API.

A local coverage check confirmed all 32 symbols and deferrals named by the Must Fix section are present. The post-fix gate passed: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (664 unit tests plus all integration and doc-test suites; 0 failures).

**Re-validation result: PASS.** All Must Fix Before PR items are addressed; no second validation iteration is required.
