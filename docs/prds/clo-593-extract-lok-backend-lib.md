# PRD: Extract lok's Backend abstraction into a consumable library target

| Field | Value |
|---|---|
| Author | Max Kulish |
| Status | Draft |
| Created | 2026-07-26 |
| Linear | [CLO-593](https://linear.app/cloud-ai/issue/CLO-593/extract-loks-backend-abstraction-into-a-consumable-library-target) |
| Branch | `feat/clo-593-extract` |
| Labels | HITL |
| Blocks | [CLO-594](https://linear.app/cloud-ai/issue/CLO-594/lock-the-gcm-library-boundary-the-syncasync-seam-and-the-config-shape-adr) |

## 1. Overview

`lok` owns a hardened, multi-backend LLM abstraction (`claude`, `codex`, `gemini`, `ollama`, and optional `bedrock`) that today is only usable by `lok`'s own binaries because the `lokomotiv` package declares two `[[bin]]` targets and no `src/lib.rs`. The same abstraction already exists in `gcm` and is about to be needed by a `remem` fork. Exposing lok's backend layer as a library lets downstream crates consume one tested implementation instead of writing a third copy.

This change adds a library target to the `lokomotiv` package, moves `src/backend/` behind the new public boundary, decouples it from lok's orchestration-specific `Config`, and verifies the boundary with an external-consumer integration test.

## 2. Problem & Objectives

### Problem

- `lokomotiv` has no library target; the public `Backend` trait and its implementations are private to the binary crate.
- `gcm` and `remem` each need the same provider abstraction. Copying it again guarantees divergent behaviour per provider.
- `src/backend/` is lightly coupled to the rest of lok (`crate::config`, one `crate::utils` helper), but those references currently prevent extraction.
- `StepContext` carries orchestration concepts (`apply_edits`, `sandbox`) that may not belong in a library consumed by a memory tool.

### Objectives

- **O1:** Add a library target that exposes the `Backend` trait and the concrete backends.
- **O2:** Decouple the backend module from lok's orchestration `Config`; keep only backend-relevant config inside the library.
- **O3:** Keep the existing 1,356-test suite green after every move.
- **O4:** Provide an integration test that consumes the library as an external crate would, including an Ollama query.
- **O5:** Record the crate-shape decision and the async trait shape in an ADR.

## 3. Scope

| # | Requirement |
|---|---|
| S1 | Classify every production `crate::config` reference in `src/backend/` as: move into library, become a caller-populated field, or become a trait the caller implements. |
| S2 | Add a `[lib]` target to the existing `lokomotiv` package and re-export the backend public API from `src/lib.rs`. |
| S3 | Move `BackendConfig` and retry defaults into the library; leave the orchestration `Config` in the binary crate. |
| S4 | Resolve the single `crate::utils::canonicalize_async` reference by duplicating or relocating the helper inside the library. |
| S5 | Move `src/backend/` behind the library boundary and update both binaries to import from the new public API. |
| S6 | Audit the public surface, especially whether `StepContext` leaks orchestration concepts; rename or narrow it if necessary. |
| S7 | Add an integration test in `tests/` that constructs an Ollama backend via the public library API and runs a query. |
| S8 | Record the crate-shape decision and async trait contract in `architecture/`. |

## 4. Functional Requirements

- **FR-1:** The package builds both binaries and the library with `cargo build --all-targets`.
- **FR-2:** The library exposes `Backend`, `BackendError`, `QueryOutput`, `TokenUsage`, `HealthStatus`, `StepContext` (or its successor), `Message`, `Role`, `SandboxMode`, and the concrete backend constructors.
- **FR-3:** Binary code continues to compile against the new public API without behaviour changes.
- **FR-4:** `BackendConfig` and retry defaults live in the library and are deserializable without lok's orchestration types.
- **FR-5:** The integration test exercises the public API from outside the crate root, using only items reachable through the library.
- **FR-6:** All 1,356 tests pass after each significant move, not only at the end.

## 5. Acceptance Criteria

- [ ] 1,356 tests pass after each extraction step.
- [ ] `cargo build --all-targets` succeeds.
- [ ] An integration test in `tests/` consumes the library like an external crate and includes an Ollama query.
- [ ] The ADR in `architecture/` records the `[lib]` vs workspace decision and the async trait shape.
- [ ] Every production `crate::config` reference is classified and resolved.

## 6. Out of Scope

- Vendoring the backend module into `gcm` or `remem` (explicitly rejected in the Linear issue).
- Writing a fresh backend layer in `remem` (explicitly rejected).
- Converting `async_trait` to native async traits; the current shape is preserved.
- Changing provider behaviour, models, or CLI invocations beyond what the extraction requires.
