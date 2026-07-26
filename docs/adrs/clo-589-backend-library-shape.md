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

### In-`lokomotiv` library (`src/backend` + re-exports)

These APIs are intended for downstream consumers:

- `Backend` trait
- `BackendError`
- `TokenUsage`
- `QueryOutput`
- `QueryResult`
- `BackendConfig` and `Defaults` (including serde helpers used by them: `deser_duration_seconds`, `serialize_duration_seconds`, and related duration helpers)
- backend implementations and helper types in `src/backend` (`CodexBackend`, `GeminiBackend`, `ClaudeBackend`, `OllamaBackend`, optional `BedrockBackend`, retry helpers, context/message models, etc.)
- feature-gated `bedrock` surface (`cfg(feature = "bedrock")`)

### Remain in binary module (`src/main.rs`-owned orchestration path)

These items explicitly remain on the CLI side and are _not_ part of the public library API in this extraction step:

- `create_backend`
- `create_claude_backend`
- `get_backends`
- `run_query`
- `run_query_with_config`
- `list_backends`
- `Engine`
- `print_verbose_*` (`print_verbose_header`, `print_verbose_timing`)
- `Config` and all other full-CLI config ownership (`conductor`, `tasks`, `roles`, `cache`, etc.)

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
