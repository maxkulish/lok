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
| `QueryResult` | `src/engine.rs` | **Binary** *(amended CLO-591)* | Carries `elapsed_ms` and a backend name: a CLI-shaped aggregate. `QueryOutput` is the library's result type. |
| `DEFAULT_TIMEOUT` | `src/backend/mod.rs` | Library | Shared backend timeout default. |
| `NO_TIMEOUT` | `src/backend/mod.rs` | Library | Shared sentinel used by backend execution. |
| `get_retry_policy` | `src/engine.rs` | **Binary** *(amended CLO-591)* | Takes the binary's `Defaults`; the library equivalent is `RetryPolicy::from_backend_config`. |
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
| `FLAG_MATRIX` | `src/backend/codex.rs` | Library, **not** root re-exported *(amended CLO-591)* | Codex-specific flag table. Public because the binary reads it, but not part of the provider-agnostic abstraction. |
| `BedrockBackend` and its constructor | `src/backend/bedrock.rs` | Library, feature-gated | Optional concrete backend; remains under `cfg(feature = "bedrock")`. |
| `BackendConfig` | `src/backend/config.rs` | Library | Leaf configuration required by backend constructors and retry policy. |
| `Defaults` | `src/config.rs` | **Binary** *(amended CLO-591)* | The binary's config-shaped defaults. The library takes the narrow `RetryDefaults` instead. |
| `deser_duration_seconds`, `deser_duration_millis` | `src/config.rs` | Library | Serde dependencies of the leaf configuration. |
| `serialize_duration_seconds`, `serialize_duration_millis` | `src/config.rs` | Library | Serde dependencies of the leaf configuration. |
| `create_backend` | `src/backend/mod.rs` | **Library** *(amended CLO-591)* | The consumer's entry point. It does still mix dispatch with the process-global cache; that constraint is documented on `BACKEND_CACHE` and deferred, not resolved. |
| `create_claude_backend` | `src/backend/mod.rs` | Binary | Accepts binary-owned `Config`. |
| `get_backends` | `src/backend/mod.rs` | Binary | Performs CLI selection, warnings, and orchestration. |
| `list_backends` | `src/backend/mod.rs` | Binary | CLI presentation behavior. |
| `is_backend_available` | `src/backend/mod.rs` | **Library, binary-support** *(amended CLO-591)* | Called from each provider's own `is_available()`, so it cannot leave the library. This is precisely what blocks re-keying or relocating `BACKEND_CACHE`. |
| `run_query` | `src/backend/mod.rs` | Binary | Multi-backend CLI orchestration. |
| `run_query_with_config` | `src/backend/mod.rs` | Binary | Accepts `Config` and owns progress/terminal behavior. |
| `Engine` | `src/backend/mod.rs` | Binary | CLI lifecycle namespace. |
| `Engine::warmup_backends` | `src/backend/mod.rs` | Binary | Mutates process-global health state from full config. |
| `print_verbose_header` | `src/backend/mod.rs` | Binary | Terminal output policy. |
| `print_verbose_timing` | `src/backend/mod.rs` | Binary | Terminal output policy. |
| `BACKEND_CACHE` | `src/backend/mod.rs` | **Library, binary-support** *(amended CLO-591)* | Stays library-side because `is_backend_available` reads it. Its shared-instance constraint is documented on the item and deferred. |
| `get_backend_cache` | `src/backend/mod.rs` | **Library, binary-support** *(amended CLO-591)* | Consumed by `src/engine.rs` across the target boundary, where `pub(crate)` does not reach. |
| `CachedBackend` | `src/backend/mod.rs` | **Library, binary-support** *(amended CLO-591)* | Consumed by `src/engine.rs` and `src/workflow.rs`. Fuses instance and health deliberately. |
| `get_cached_health` | `src/backend/mod.rs` | Binary | Reads process-global health state. |
| `Config` | `src/config.rs` | Binary | Owns conductor, tasks, roles, teams, and cache configuration. |
| `effective_timeout` | `src/backend/mod.rs` | Deferred to the extraction | Backend-runtime concern currently coupled to binary-owned `Config`; keep binary-side or re-type around leaf config. Resolved binary-side in `src/engine.rs`. |
| `step_context_for_backend` | `src/backend/mod.rs` | Deferred to the extraction | Backend-runtime concern currently coupled to binary-owned `Config`; keep binary-side or re-type around leaf config. Resolved binary-side in `src/engine.rs`. |

The `#[cfg(test)]` public helpers are not downstream API in spirit, but CLO-593 made them reachable through the `test-support` feature and CLO-591 kept that deliberately; see the divergence table. `StubBackend`, `clear_health_cache`, `set_mock_health`, `write_exec_script` and `acquire_test_lock` are test-only implementation support. `TEST_MUTEX` was made **private** in CLO-591: as a `pub static tokio::sync::Mutex<()>` it pinned consumers to lok's Tokio version, and `acquire_test_lock` now returns an opaque `TestLockGuard`.

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

Recorded at `b6bf2eb` so the drift is reviewable rather than silent. **Closed in CLO-591**; each row below states its resolution. Two rows were themselves factually wrong and are corrected here.

| Item | This ADR said | Actual | Resolution |
| --- | --- | --- | --- |
| `create_backend` | Binary, because it mixes dispatch with process-global cache behavior | Library, re-typed as `create_backend(name, &BackendConfig, RetryPolicy)` and re-exported at the crate root | **ADR amended.** It stays in the library: it is the consumer's entry point and `tests/backend_public_api.rs` depends on it. The cache concern that motivated "Binary" is recorded below as deferred. |
| `QueryResult` | Library | Binary: moved to `src/engine.rs:27` | **ADR amended.** It carries `elapsed_ms` and a `backend` name, a CLI-shaped aggregate. `QueryOutput` is the library type. Binary placement is correct. |
| `get_retry_policy` | Library | ~~"Removed, replaced by `RetryPolicy::from_backend_config`"~~ **This was wrong.** It still exists, at `src/engine.rs:35`. It moved to the binary rather than disappearing. | **Row corrected, then closed** as "moved to binary". |
| `Defaults` | Library | ~~"Replaced by a new `RetryDefaults`"~~ **This was wrong.** Both exist: `Defaults` binary-side, `RetryDefaults` in `src/backend/config.rs`. `src/engine.rs:35` takes a `&Defaults` and constructs a `RetryDefaults` from it. | **Row corrected, then closed** as a deliberate split: the binary keeps its config-shaped defaults, the library takes a narrow struct. |
| `#[cfg(test)]` helpers | Not downstream API | Reachable through a `test-support` cargo feature | **ADR amended.** The feature is deliberate and useful. CLO-591 removed the Tokio leak that made it risky (see follow-up 3). |
| `ClaudeBackend`, `FLAG_MATRIX` | Library, re-exported at the crate root | Only under `lokomotiv::backend::` | **Split.** `ClaudeBackend` is now re-exported at the crate root, matching its siblings (code moved). `FLAG_MATRIX` stays at `backend::codex::FLAG_MATRIX` (**ADR amended**): the binary uses it, but a Codex-specific flag table is not part of the provider-agnostic abstraction. |

## Public API classification (CLO-591)

Every `pub` item reachable from `lokomotiv::` falls into one of four categories. The fourth, *binary-support*, is new; the original three could not describe the health-cache items.

| Category | Meaning | Members |
| --- | --- | --- |
| **Abstraction** | The provider-agnostic API a consumer is meant to use | `Backend`, `create_backend`, `BackendConfig`, `BackendError`, `QueryOutput`, `TokenUsage`, `StepContext`, `StepOptions`, `HealthStatus`, `ModelInfo`, `Message`, `Role`, `SandboxMode`, `RetryPolicy`, `RetryExecutor`, `RetryDefaults`, the four provider types, `DEFAULT_TIMEOUT`, `NO_TIMEOUT`, `resolve_timeout` |
| **Binary-support** | Public only because `src/engine.rs` and `src/workflow.rs` consume them across the lib/bin target boundary, where `pub(crate)` does not reach | `BACKEND_CACHE`, `get_backend_cache`, `CachedBackend`, `is_cache_entry_fresh`, `health_cache_ttl`, `parse_health_cache_ttl`, `HEALTH_TTL_LOGGED`, `DEFAULT_HEALTH_CACHE_TTL`, `HEALTH_TTL_ENV`, `is_backend_available`, `FLAG_MATRIX`, `FlagRequirement`, `ClaudeBackend::api_details`, the `config` serde helpers |
| **Test-only** | Behind `#[cfg(any(test, feature = "test-support"))]` | `acquire_test_lock`, `TestLockGuard`, `StubBackend`, `clear_health_cache`, `set_mock_health`, `write_exec_script` |
| **Removed** | Demoted to `pub(crate)` in CLO-591, after checking each against the binary | `ClaudeMode`, `resolve_health_cache_ttl`, the Bedrock wire DTOs (`Message`, `MessageContent`, `ContentBlock`, `BedrockResponse`, `BedrockUsage`, `ResponseBlock`), `BedrockBackend::invoke_with_messages` |

Eliminating the binary-support category means moving the health cache into the binary, which the deferral below rules out for now.

## CLO-593 follow-ups: resolutions

**1. Global cache semantics — deferred, with reason.**

`BACKEND_CACHE` remains keyed by backend name alone, so two consumers in one process using the same name with different configurations still share an instance. Both fixes this ADR floated are blocked by a constraint neither anticipated.

`CachedBackend` deliberately fuses the backend instance with its health status ("Replaces separate CONSTRUCTED_BACKENDS and HEALTH_CACHE maps, ensuring consistency"), and `is_backend_available(name)` looks entries up **by name alone** from inside each provider's own `is_available()` implementation. So keying by `name + hash(config)` breaks that lookup, which has no configuration to hash; and moving the cache to the binary breaks the library's own `is_available()`. Either fix requires reworking `is_available`, which is a larger change than stripping presentation.

The constraint is now documented on `BACKEND_CACHE` and `create_backend` so a consumer meets it in rustdoc rather than in production. **Revisit before CLO-592 publishes.**

**2. Retry logging — resolved.** All library warnings go through the `log` facade: `log::warn!` for the retry notice, the `LOK_HEALTH_TTL` parse warning and the read-only-sandbox downgrade, `log::debug!` for the Claude probe diagnostics. `log` with no logger installed is a no-op, so an embedding consumer sees nothing. Messages carry facts rather than terminal chrome; the binary installs `env_logger` with a custom formatter and renders them.

Two sites the CLO-591 ticket did not know about were included: `codex.rs` and `gemini.rs` were writing the sandbox warning to **stdout**, which corrupts a consumer's piped output.

**3. Test lock guard — resolved.** `acquire_test_lock` returns an opaque `TestLockGuard` instead of `tokio::sync::MutexGuard`. The leak existed in two places, not the one the ticket named: `TEST_MUTEX` was itself a `pub static tokio::sync::Mutex<()>`, so wrapping only the guard would have left it open. `TEST_MUTEX` is now private.

## StepContext is call-shaped (CLO-591)

Settled, so it is not asked a third time. Every field on `src/backend/context.rs` is a per-call concern: `prompt`, `history`, `model`, `cwd`, `sandbox`, `apply_edits`, `schema`, `options`, `timeout`. None names a step, workflow, run or orchestration state. The struct is `Copy` with all borrows tied to the caller's frame, and `StepContext::from_prompt` is already the narrow entry point the CLO-593 handoff asked someone to add.

The only workflow residue is the name and the `Step`-prefixed FR comments. Renaming is **not** recommended: it is a breaking change to the library's main type for cosmetic gain. CLO-592 can decide if it matters.

## Boundary enforcement (CLO-591)

The six divergences above happened because a single package with one shared dependency list cannot enforce a boundary; only discipline can, and discipline drifted. CLO-591 kept the single-package shape (a workspace split was considered and declined) and added the `library-boundary` CI job as the compensating control: a lean build, an exact-name dependency-tree assertion, a source-level ban on terminal writes in `src/backend/`, lean tests and clippy, the bedrock check, and a `-D missing_docs` rustdoc gate.

These gates are load-bearing rather than decorative. Without them, `cli` being a default feature means a new `colored` import on a library path compiles green.

## References

- Discovery: `docs/discovery/clo-593.md`
- PRD: `docs/prds/clo-593-extract-lok-backend-lib.md`
- Design: `docs/designs/clo-593-extract-lok-backend-lib.md`
- Plan: `docs/plans/clo-593-extract-lok-backend-lib.md`
- Lessons: `.pi/lessons/clo-589-backend-shape-lessons.md`
