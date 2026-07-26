# ADR 001: Add a `[lib]` target to `lokomotiv` instead of splitting into a workspace

**Status:** Accepted  
**Date:** 2026-07-26  
**Author:** CLO-593 implementation  
**Context:** [CLO-593](https://linear.app/cloud-ai/issue/CLO-593/extract-loks-backend-abstraction-into-a-consumable-library-target)  

## Context

`lok` started as a binary-only CLI package. Its `src/backend/` layer (~7,350 lines) implements a clean multi-provider LLM abstraction (`Backend` trait, concrete backends, retry/timeout/token logic) but was unreachable from outside the crate because `Cargo.toml` declared only `[[bin]]` targets.

Two downstream consumers need the same abstraction:
- `gcm` currently maintains a parallel provider layer in `src/provider/`.
- A `remem` fork is about to need the same thing.

The question was whether to extract `src/backend/` into a standalone `lok-backend` crate in a Cargo workspace, or to add a `[lib]` target to the existing `lokomotiv` package.

## Decision

Add a `[lib]` target named `lokomotiv` to the existing package. The library root (`src/lib.rs`) declares `pub mod backend;` and curated `pub use` re-exports. Binary-side orchestration (`Engine`, `get_backends`, `run_query*`, progress reporting, config-driven timeout resolution) moves to a binary-only `src/engine.rs` module. The `src/backend/` files stay in place and become the library module tree.

## Consequences

### Positive

- **Least disruption.** One package, one version, one `Cargo.toml`, one publish step. Existing CI, `shell.nix`, and docs.rs setup keep working.
- **Preserves git history.** Provider files were not relocated on disk, so `git blame` remains useful.
- **Unblocks consumers immediately.** gcm and remem can depend on `lokomotiv` as a path or git dependency today.
- **Keeps workspace split as a future option.** The backend module tree is self-contained; moving it to a separate crate later is a mechanical refactor once the public API is stable.

### Negative

- **Dependency bloat for consumers.** Library users compile the full `lokomotiv` dependency set (`clap`, `indicatif`, `colored`, `minijinja`, `chrono`, `dirs`, `humantime`, etc.) even though the backend code does not need most of them after the orchestration split.
- **Global backend cache.** `BACKEND_CACHE` remains a process-global `OnceLock` keyed by backend name. Multiple consumers in the same process that use the same name with different configs will share a cached instance. This is acceptable for CLI tools but questionable for a library.
- **Library stderr output.** `retry.rs` still writes retry warnings directly to stderr via `colored`. Consumers may want a logging facade or callback instead.
- **Test surface leaks Tokio.** The `test-support` feature exposes `acquire_test_lock`, which returns `tokio::sync::MutexGuard<'static, ()>`. This ties consumers to lok's Tokio version.

## Follow-up work before the boundary is locked

The following items should be resolved before CLO-594 locks the public API:

1. **Global cache semantics:** either key the cache by config hash, introduce an instance-scoped `BackendRegistry`, or document the process-global constraint.
2. **Retry logging:** replace the direct `eprintln!` with `log::warn!` / `tracing::warn!` or an optional callback.
3. **Test lock guard:** wrap `acquire_test_lock` in an opaque `TestLockGuard` to avoid leaking Tokio types across the boundary.

## Trigger for workspace split

Revisit the `[lib]`-vs-workspace decision when any of the following is measured:

- A downstream cold-build delta exceeds 15% attributable to lokomotiv's non-backend dependencies.
- `lokomotiv` is published to crates.io and consumer feedback requests a lighter dependency tree.
- The global cache or stderr-output constraints become blocking for a consumer.

Until then, the single-package library target remains the approved shape.

## References

- Design: `docs/designs/clo-593-extract-lok-backend-lib.md`
- Plan: `docs/plans/clo-593-extract-lok-backend-lib.md`
- Discovery: `docs/discovery/clo-593.md`
- PRD: `docs/prds/clo-593-extract-lok-backend-lib.md`
- Prior ADR on the same decision: `docs/adrs/clo-589-backend-library-shape.md`
