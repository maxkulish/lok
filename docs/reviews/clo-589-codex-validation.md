# Pre-PR validation: clo-589

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-07-26
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- HIGH - The ADR does not meet the required item-by-item boundary contract. The design and PRD require every public item in `src/backend/mod.rs` / `src/backend/context.rs` to be allocated to library or binary ownership, but the ADR uses broad buckets like "helper types" and omits explicit decisions for `DEFAULT_TIMEOUT`, `NO_TIMEOUT`, `get_retry_policy`, `effective_timeout`, `step_context_for_backend`, `BACKEND_CACHE`, `get_backend_cache`, `CachedBackend`, and `get_cached_health`. See [design goal](/Users/mk/Code/orchestrator/lok--feat-clo-589-record/docs/designs/clo-589-backend-library-shape.md:12), [PRD AC](/Users/mk/Code/orchestrator/lok--feat-clo-589-record/docs/prds/clo-589-backend-library-shape.md:38), [ADR list](/Users/mk/Code/orchestrator/lok--feat-clo-589-record/docs/adrs/clo-589-backend-library-shape.md:40), and current exports in [src/backend/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-589-record/src/backend/mod.rs:284).

## Missing Items

- Explicit ADR allocation for all public backend/context symbols, especially the timeout helpers, cache/global-state items, and test-only/public helper surface.
- A stronger ST3 check: the current plan only checks "principal" symbols, so it passes even when the itemized boundary requirement is incomplete.

## Recommendations

- Expand `docs/adrs/clo-589-backend-library-shape.md` with a concrete table: symbol, current source path, library/binary decision, rationale.
- Update ST3 to compare the ADR table against `rg -n 'pub (fn|struct|enum|trait|const|static|use)' src/backend/mod.rs src/backend/context.rs`.
- I verified `git diff main...HEAD`, `git diff --check`, ADR/index acceptance checks, and `cargo fmt --check`. `cargo clippy --all-targets -- -D warnings` and `cargo test` could not run in this read-only sandbox because Cargo could not open `target/debug/.cargo-lock`.
