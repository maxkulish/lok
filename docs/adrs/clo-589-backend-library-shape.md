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

The table below is the durable item-by-item contract for CLO-590. "Library" means public from the future `[lib]` target; "Binary" means retained behind the CLI module tree; "Deferred" means CLO-590 must resolve the named seam before exposing it.

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
| `effective_timeout` | `src/backend/mod.rs` | Deferred to CLO-590 | Backend-runtime concern currently coupled to binary-owned `Config`; keep binary-side or re-type around leaf config. |
| `step_context_for_backend` | `src/backend/mod.rs` | Deferred to CLO-590 | Backend-runtime concern currently coupled to binary-owned `Config`; keep binary-side or re-type around leaf config. |

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
