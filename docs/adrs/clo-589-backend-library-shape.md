# ADR: Backend abstraction exposure shape for `lokomotiv`

- **Title:** Extract backend abstraction as library API from `lok`
- **Status:** Accepted
- **Date:** 2026-07-26
- **Decision owner:** CLO-589 ticket
- **Related issue:** <https://linear.app/cloud-ai/issue/CLO-589/record-the-crate-shape-adr-for-extracting-the-backend-abstraction-as-a>

## Context

`src/backend/*` is currently embedded in the binary crate and is now needed by multiple external consumers (e.g. the remem-ai fork and future tools).

We need a crate-shape decision before moving code:

- keep everything in this package as a `[lib]` target, or
- split backend into a separate Cargo crate.

The current backend module references only a small part of `Config`-shaped data at call time, but `Config` itself is tightly coupled to binary-only modules.

## Decision

This ADR records shape only (no code extraction performed in this ticket).

We will expose `src/backend` as part of a **[lib] target inside the existing `lokomotiv` package** (i.e., one crate, two products: binary + library).

### Why this shape

1. **Low coupling risk:** `Config` cannot be moved cleanly into a standalone backend-only crate because:
   - `Config` includes `cache::CacheConfig` (`src/config.rs:18`) and `role` types (`src/config.rs:24`),
   - which pulls in binary-side dependencies and creates a cycle risk if moved into a generic backend crate.
2. **Boundary is small and deliberate now:** only a narrow ABI surface must move into the library (`Backend`/types + serde helpers + select config leaves), which is a better fit as an in-package extraction.
3. **Operational simplicity:** no extra cargo workspace/workload for initial publish, and no second release cadence.

## Boundary allocation

The table below is the durable item-by-item contract for the extraction, carried out under CLO-593. "Library" means public from the future `[lib]` target; "Binary" means retained behind the CLI module tree; "Deferred" means the extraction must resolve the named seam before exposing it.

| Public item | Current source | Owner | Rationale |
| --- | --- | --- | --- |
| `Backend` and `Backend::{name, query, is_available, health_check}` | `src/backend/mod.rs` | Library | Core consumer abstraction and its complete trait contract. |
| `BackendError` | `src/backend/mod.rs` | Library | Typed failure contract for `Backend::query`. |
| `BackendError::is_retryable` | `src/backend/mod.rs` | Library | Error-classification behavior follows `BackendError`. |
| `TokenUsage` | `src/backend/mod.rs` | Library | Consumer-visible token accounting. |
| `TokenUsage::new` | `src/backend/mod.rs` | Library | Constructor follows `TokenUsage`. |
| `TokenUsage::with_cached` | `src/backend/mod.rs` | Library | Builder follows `TokenUsage`. |
| `TokenUsage::with_reasoning` | `src/backend/mod.rs` | Library | Builder follows `TokenUsage`. |
| `TokenUsage::saturating_add` | `src/backend/mod.rs` | Library | Aggregation follows `TokenUsage`. |
| `QueryOutput` | `src/backend/mod.rs` | Library | Successful backend result contract. |
| `QueryOutput::from_text` | `src/backend/mod.rs` | Library | Constructor follows `QueryOutput`. |
| `QueryOutput::from_process` | `src/backend/mod.rs` | Library | Constructor follows `QueryOutput`. |
| `QueryOutput::with_model` | `src/backend/mod.rs` | Library | Builder follows `QueryOutput`. |
| `QueryOutput::with_usage` | `src/backend/mod.rs` | Library | Builder follows `QueryOutput`. |
| `QueryOutput::with_structured` | `src/backend/mod.rs` | Library | Builder follows `QueryOutput`. |
| `QueryResult` | `src/backend/mod.rs` | Library | Public result type already used at the backend boundary. |
| `DEFAULT_TIMEOUT` | `src/backend/mod.rs` | Library | Shared backend timeout default. |
| `NO_TIMEOUT` | `src/backend/mod.rs` | Library | Shared sentinel used by backend execution. |
| `get_retry_policy` | `src/backend/mod.rs` | Library | Converts library-owned leaf config into `RetryPolicy`. |
| `StepContext` | `src/backend/context.rs` | Library | Complete per-query input contract. |
| `StepContext::from_prompt` | `src/backend/context.rs` | Library | Minimal constructor follows `StepContext`. |
| `StepOptions` | `src/backend/context.rs` | Library | Public field type of `StepContext`. |
| `Message` | `src/backend/context.rs` | Library | Public conversation-history element. |
| `Role` | `src/backend/context.rs` | Library | Public field type of `Message`. |
| `SandboxMode` | `src/backend/context.rs` | Library | Public field type of `StepContext`. |
| `HealthStatus` | `src/backend/context.rs` | Library | Backend health result contract. |
| `HealthStatus::new_available` | `src/backend/context.rs` | Library | Constructor follows `HealthStatus`. |
| `HealthStatus::new_unavailable` | `src/backend/context.rs` | Library | Constructor follows `HealthStatus`. |
| `ModelInfo` | `src/backend/context.rs` | Library | Public field type of `HealthStatus`. |
| `RetryExecutor`, `RetryPolicy` | `src/backend/retry.rs` | Library | Reusable backend retry behavior and policy. |
| `ClaudeBackend`, `CodexBackend`, `GeminiBackend`, `OllamaBackend` and their constructors | `src/backend/{claude,codex,gemini,ollama}.rs` | Library | Allow consumers to instantiate a concrete backend from leaf config. |
| `FLAG_MATRIX` | `src/backend/codex.rs`, re-exported by `src/backend/mod.rs` | Library | Public Codex capability metadata. |
| `BedrockBackend` and its constructor | `src/backend/bedrock.rs` | Library, feature-gated | Optional concrete backend; remains under `cfg(feature = "bedrock")`. |
| `BackendConfig`, `Defaults` | `src/config.rs` | Library | Leaf configuration required by backend constructors and retry policy. |
| `deser_duration_seconds`, `deser_duration_millis` | `src/config.rs` | Library | Serde dependencies of the leaf configuration. |
| `serialize_duration_seconds`, `serialize_duration_millis` | `src/config.rs` | Library | Serde dependencies of the leaf configuration. |
| `create_backend` | `src/backend/mod.rs` | Binary | Mixes dispatch with process-global cache behavior. |
| `create_claude_backend` | `src/backend/mod.rs` | Binary | Accepts binary-owned `Config`. |
| `get_backends` | `src/backend/mod.rs` | Binary | Performs CLI selection, warnings, and orchestration. |
| `list_backends` | `src/backend/mod.rs` | Binary | CLI presentation behavior. |
| `is_backend_available` | `src/backend/mod.rs` | Binary | Binary-side availability/selection helper. |
| `run_query` | `src/backend/mod.rs` | Binary | Multi-backend CLI orchestration. |
| `run_query_with_config` | `src/backend/mod.rs` | Binary | Accepts `Config` and owns progress/terminal behavior. |
| `Engine` | `src/backend/mod.rs` | Binary | CLI lifecycle namespace. |
| `Engine::warmup_backends` | `src/backend/mod.rs` | Binary | Mutates process-global health state from full config. |
| `print_verbose_header` | `src/backend/mod.rs` | Binary | Terminal output policy. |
| `print_verbose_timing` | `src/backend/mod.rs` | Binary | Terminal output policy. |
| `BACKEND_CACHE` | `src/backend/mod.rs` | Binary | Process-global orchestration state. |
| `get_backend_cache` | `src/backend/mod.rs` | Binary | Accessor for process-global orchestration state. |
| `CachedBackend` | `src/backend/mod.rs` | Binary | Storage record for the process-global cache. |
| `get_cached_health` | `src/backend/mod.rs` | Binary | Reads process-global health state. |
| `Config` | `src/config.rs` | Binary | Owns conductor, tasks, roles, teams, and cache configuration. |
| `effective_timeout` | `src/backend/mod.rs` | Deferred to the extraction | Backend-runtime concern currently coupled to binary-owned `Config`; keep binary-side or re-type around leaf config. Resolved binary-side in `src/engine.rs`. |
| `step_context_for_backend` | `src/backend/mod.rs` | Deferred to the extraction | Backend-runtime concern currently coupled to binary-owned `Config`; keep binary-side or re-type around leaf config. Resolved binary-side in `src/engine.rs`. |

The `#[cfg(test)]` public helpers are not downstream API: `StubBackend`, `clear_health_cache`, `set_mock_health`, `TEST_MUTEX`, and `acquire_test_lock` remain test-only implementation support.

This split avoids re-import cycles while still allowing an external crate to instantiate query backends.

## Config-cycle rule (non-negotiable)

`Config` remains binary-owned because moving it into library territory causes a hard dependency cycle and forces inclusion of binary-only config domains. This prevents the wrong layer from owning orchestration policy.

## Versioning and publication decision

- **Versioning:** the library and binary share the existing `lokomotiv` crate version.
- **Publish location:** no separate crates.io package at this stage; consumers use the existing `lokomotiv` crate artifact and consume library items directly from that package.

## Bedrock feature exposure

`bedrock` remains an optional crate feature on `lokomotiv` (`feature = "bedrock"` in `Cargo.toml`) with feature-gated exports for `BedrockBackend` and any bedrock-specific types/dependencies.
This preserves current behavior while allowing binary or library consumers to opt into Bedrock support explicitly.

## Rejected option: separate `lok-backend` workspace crate

- Adds one extra release artifact and independent versioning immediately.
- Requires a larger boundary design up front for `Config`, `role`, and cache ownership.
- Introduces publish/build coordination overhead before the extraction’s primary value is demonstrated.

Given CLO-589 is a recording/entry-point decision, these are deferred to future work if/when version decoupling is explicitly required.

## Consequences

Recorded during the CLO-593 implementation, which carried out this decision.

### Positive

- **Least disruption.** One package, one version, one `Cargo.toml`, one publish step. Existing CI, `shell.nix` and docs.rs setup keep working.
- **Preserves git history.** The backend files were not relocated on disk, so `git blame` remains useful.
- **Unblocks consumers immediately.** gcm and remem can depend on `lokomotiv` as a path or git dependency today.
- **Keeps the workspace split as a future option.** The backend module tree is self-contained, so moving it to a separate crate later is a mechanical refactor once the public API is stable.

### Negative

- **Dependency bloat for consumers.** Library users compile the full `lokomotiv` dependency set (`clap`, `indicatif`, `colored`, `minijinja`, `chrono`, `dirs`, `humantime` and the rest) even though the backend code does not need most of them after the orchestration split.
- **Global backend cache.** `BACKEND_CACHE` remains a process-global `OnceLock` keyed by backend name. Two consumers in the same process using the same name with different configs silently share a cached instance. Acceptable for a CLI, questionable for a library.
- **Library stderr output.** `retry.rs` still writes retry warnings straight to stderr through `colored`. Consumers may want a logging facade or a callback instead.
- **Test surface leaks Tokio.** The `test-support` feature exposes `acquire_test_lock`, which returns `tokio::sync::MutexGuard<'static, ()>`, tying consumers to lok's Tokio version.

## Follow-up work before the boundary is locked

Tracked by CLO-591, to be resolved before the public API is treated as stable:

1. **Global cache semantics.** Either key the cache by config hash, introduce an instance-scoped `BackendRegistry`, or document the process-global constraint.
2. **Retry logging.** Replace the direct `eprintln!` with `log::warn!` / `tracing::warn!`, or an optional callback.
3. **Test lock guard.** Wrap `acquire_test_lock` in an opaque `TestLockGuard` so no Tokio type crosses the boundary.

## Trigger for revisiting the workspace split

Reopen the `[lib]`-versus-workspace question when any of the following is measured:

- A downstream cold-build delta exceeds 15% attributable to lokomotiv's non-backend dependencies.
- `lokomotiv` is published to crates.io and consumer feedback asks for a lighter dependency tree.
- The global cache or the stderr-output constraint becomes blocking for a consumer.

Until then the single-package library target remains the approved shape.

## Divergences between this contract and the CLO-593 implementation

Recorded at `b6bf2eb` so the drift is reviewable rather than silent. None of these is obviously wrong, but each departs from the table above and none was renegotiated here first.

| Item | This ADR says | Implementation does |
| --- | --- | --- |
| `create_backend` | Binary, because it mixes dispatch with process-global cache behavior | Library, re-typed as `create_backend(name, &BackendConfig, RetryPolicy)` and re-exported at the crate root |
| `QueryResult` | Library | Binary: moved to `src/engine.rs` |
| `get_retry_policy` | Library | Removed, replaced by `RetryPolicy::from_backend_config` (`src/backend/retry.rs:49`) |
| `Defaults` | Library | Replaced by a new `RetryDefaults` type in `src/backend/config.rs` |
| `#[cfg(test)]` helpers | Not downstream API | Reachable through a new `test-support` cargo feature, enabled by a self path dev-dependency on `lokomotiv` |
| `ClaudeBackend`, `FLAG_MATRIX` | Library | Library, but only under `lokomotiv::backend::`; unlike their siblings they are not re-exported at the crate root |

## References

- Discovery: `docs/discovery/clo-593.md`
- PRD: `docs/prds/clo-593-extract-lok-backend-lib.md`
- Design: `docs/designs/clo-593-extract-lok-backend-lib.md`
- Plan: `docs/plans/clo-593-extract-lok-backend-lib.md`
- Lessons: `.pi/lessons/clo-589-backend-shape-lessons.md`
