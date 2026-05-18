# PRD: Phase 2 — Predictable CLI Execution

| Field | Value |
|-------|-------|
| Author | MK |
| Status | Draft |
| Created | 2026-05-17 |
| Last Updated | 2026-05-17 (rev 2) |
| Stakeholders | Lok core, rs-wisper consumers |
| Source Investigations | [codex-gap-analysis.md](../investigations/codex-gap-analysis.md), [gemini-cli-gap-analysis.md](../investigations/gemini-cli-gap-analysis.md), [ollama-cli-gap-analysis.md](../investigations/ollama-cli-gap-analysis.md), [ollama-api-gap-analysis.md](../investigations/ollama-api-gap-analysis.md) |
| Parent / Prior Work | [prd-llm-mux-port.md](prd-llm-mux-port.md), [prd-output-validation-pipeline.md](prd-output-validation-pipeline.md), [prd-structured-failure-data.md](prd-structured-failure-data.md) |

## 1. Overview

Phase 1 made lok's backends *fail loudly*: typed `BackendError`, retry policies, richer `QueryOutput`, validation pipeline, and structured `FailureType` across every step path. Phase 2 makes lok's backends *succeed predictably*: schema-enforced output instead of regex-scraped text, real health checks instead of optimistic `true`, capability-aware routing instead of "every model is a text chat", and per-step permission/model selection instead of hardcoded defaults.

The four CLI utilities lok orchestrates (Codex, Gemini CLI, Ollama HTTP, Claude) have all shipped major automation surface area since lok's last update — JSON schemas, token usage in events, ephemeral sessions, approval/sandbox profiles, headers/TLS. Lok uses ~15–20% of that surface today. The bet: by adopting the structured paths each tool now offers, lok eliminates its single largest reliability tax (fragile text parsing) and unlocks observability (token cost, real health, model coverage) that workflows already need but cannot currently get.

## 2. Problem & Objectives

### Problem Statement

Lok's job is to send a prompt to one or more CLI utilities and return a result a downstream workflow step can act on. Today that contract is broken in three ways:

1. **Output is text-shaped, not data-shaped.** Codex's JSONL stream is matched by substring (`line.contains("\"agent_message\"")`), Gemini's stdout is parsed by `skip_lines = 1`, Ollama returns single-turn chat text. None of the three uses the structured-output features each tool now offers (`--output-schema`, `--output-format json`, `format: <jsonschema>`). Workflows that need a list of issues, vulnerabilities, or diffs strip markdown fences and parse fragments by hand.
2. **Health is asserted, not verified.** `OllamaBackend::is_available()` returns `true` unconditionally. Gemini's check confirms only that `npx` is on PATH, not that `@google/gemini-cli` is installed or authenticated. There is no CLI version detection, so flags added after lok's last update silently no-op on older installs and silently work on newer ones with no visibility.
3. **Capability is invisible, configuration is half-wired.** Per-step `model` exists in config and is passed as `None` at every call site. `apply_edits` workflows run under hardcoded `-s read-only` and fail to write. Ollama cannot reach remote servers behind auth proxies (no header/TLS support). Backends do not advertise whether they support tools, vision, thinking, or schemas, so the conductor cannot route by capability.

The result: rs-wisper-style review workflows see ~5% silent failures (Phase 1 catches the empty/noise cases; Phase 2 must catch the malformed-structure cases), users have no token-cost visibility, and any workflow that wants Codex to write files must bypass lok's default args.

### Objectives

- **O1 — Structured output by default.** Every backend that supports a schema/JSON output mode uses it, and lok returns parsed `serde_json::Value` in `QueryOutput.structured` for downstream steps to consume without regex.
- **O2 — Real health and version awareness.** `is_available()` reflects an actual ping; `lok doctor` reports each backend's installed version, auth method, and which optional flags are usable.
- **O3 — Capability-aware backends.** Each backend declares supported capabilities (schema, tools, vision, thinking, streaming) and the conductor/consensus engine routes by capability instead of by name.
- **O4 — Per-step configuration is honored end-to-end.** `step.model`, `step.sandbox`/`approval_mode`, and `step.timeout` flow from TOML through `run_query_with_config` into the actual CLI invocation.
- **O5 — Token and usage observability.** `QueryOutput.usage` is populated by every backend that emits the data (Codex `turn.completed`, Gemini `stats`, Ollama `prompt_eval_count`/`eval_count`).
- **O6 — Remote and ephemeral execution.** Ollama supports custom headers and TLS so workflows can target auth-gated remote servers. Codex sessions default to `--ephemeral` so non-interactive runs do not accumulate disk artifacts.

### Success Metrics (KPIs)

| Metric | Current | Target | How Measured |
|--------|---------|--------|--------------|
| Workflows whose downstream steps regex-parse LLM output | majority | 0 (use `structured` field) | Audit `src/workflows/*.toml` and `src/tasks/*.rs` |
| Silent failures from malformed structure | unknown (~5% upper bound from Phase 1 audit) | < 0.5% | Validation step rejection rate on schema-mode runs |
| `QueryOutput.usage` populated per backend | 0 of 4 | 4 of 4 | Integration test asserts `usage.is_some()` |
| `lok doctor` accurate availability calls | 1 of 4 (Codex only) | 4 of 4 | New `lok doctor` snapshot tests |
| Per-step `model` override actually reaches CLI | 0 backends | 4 backends | Snapshot test of constructed argv |
| Codex session files written per `lok run` | N (one per step) | 0 | `~/.codex/sessions/` count after CI run |
| Ollama queries against TLS+auth-gated server succeed | 0 | 100% | Integration test against test server |

## 3. Users & Use Cases

### Personas

| Persona | Role | Need | Pain Point |
|---------|------|------|------------|
| Workflow author (MK) | Writes TOML workflows | Reliable structured handoff between steps | Hand-parses model output, fights markdown fences, debugs silent empty results |
| rs-wisper integrator | Calls `lok run review-pr` from another tool | Predictable JSON output for review issues | Receives free-text reviews, must LLM-parse them again |
| Ops / on-call | Runs lok in CI or cron | Cost and health visibility | No token usage logged; failures look like exit codes only |
| Enterprise user | Hosts Ollama behind auth proxy on TLS | Use private Ollama from lok workflows | Backend has no header/TLS knobs; connection refused |

### Key Use Cases

**UC-1: Hunt returns typed issues**
- Trigger: `lok run hunt <dir>`
- Steps: lok builds the hunt JSON Schema → sends prompt with `--output-schema` (Codex) or `--output-format json` (Gemini) or `format: schema` (Ollama) → backend returns schema-valid JSON → lok parses into `Vec<Issue>` → downstream step gets `{{ steps.hunt.structured.issues }}`
- Outcome: Downstream rendering / issue-creation step receives structured data, no markdown stripping.

**UC-2: Apply-edits step writes files without TOML hacks**
- Trigger: Workflow step has `apply_edits = true`
- Steps: Engine sees the flag → swaps Codex sandbox to `workspace-write` for that step only → other steps keep `read-only`
- Outcome: Edits land on disk; user does not edit defaults globally.

**UC-3: Doctor surfaces a usable diagnosis**
- Trigger: `lok doctor`
- Steps: Each backend reports `version`, `available` (real ping), `auth_method`, `supported_capabilities`, and unusable flags on this version
- Outcome: User immediately sees that Gemini is OAuth-only (will hang headless) or that Ollama is unreachable, instead of waiting for a query to fail.

**UC-4: Cost report after every workflow**
- Trigger: `lok run <workflow>` completes
- Steps: Each `QueryOutput.usage` is summed → printed in verbose summary → exposed in JSON output mode
- Outcome: User knows that the run cost N input + M output tokens across K calls.

**UC-5: Remote Ollama through auth proxy**
- Trigger: `lok ask --backend ollama` with `headers` and `tls` configured
- Steps: Backend attaches headers to reqwest client; uses TLS settings
- Outcome: Query reaches the gated server; returns valid response.

**UC-6: Multi-turn Ollama workflow**
- Trigger: Two-step workflow where step 2's prompt is "for each issue you just listed, suggest a fix" against an Ollama model
- Steps: Conductor records step 1's prompt + response → constructs `StepContext.history` for step 2 → Ollama backend translates history into the `messages: [{role:"user",...},{role:"assistant",...},{role:"user",...}]` array
- Outcome: Step 2 actually sees step 1's output as conversational context; today it sees only the rendered prompt with no model memory.

**UC-7: Multi-line prompt with code blocks**
- Trigger: A workflow step's prompt is a 4 kB markdown document containing nested backticks, embedded quotes, and a code fence
- Steps: Backend writes the prompt to a tempfile / pipes via stdin → CLI reads it cleanly → no shell escaping or argv length issues
- Outcome: The prompt arrives at the model byte-for-byte; today this fails on Gemini with shell-quoting errors.


## 4. Functional Requirements

### FR Group: Schema-Driven Structured Output

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-1 | `BackendConfig` accepts optional `schema` (inline JSON or path) and `output_format` (`text` / `json` / `stream-json`) | Must | Round-trip TOML test; defaults preserve existing behavior |
| FR-2 | `WorkflowStep` accepts a `schema` field overriding backend default | Must | Per-step schema applied; missing step falls back to backend; missing both falls back to text mode |
| FR-3 | Codex backend passes `--output-schema <tmpfile>` when a schema is set; default args add `--ephemeral` | Must | Argv snapshot test; tmpfile cleaned up on drop |
| FR-3a | Codex JSONL stream is parsed as discrete events, not by substring matching. The schema payload is extracted from the `agent_message` item of the **last** `turn.completed` event in the stream. Intermediate reasoning / tool-call items are discarded for the structured payload. `turn.failed` short-circuits to `BackendError::Backend` | Must | Unit test with a recorded JSONL fixture containing reasoning + tool-call + final agent_message; integration test confirms intermediate JSON-shaped reasoning is not mistaken for the result |
| FR-4 | Gemini backend passes `--output-format json` and reads `response`/`stats`/`error` from the JSON envelope | Must | Replace `skip_lines`; integration test against gemini-cli ≥ v0.42 |
| FR-5 | Ollama backend sends `format` field with a JSON Schema when a schema is set | Must | Integration test against ollama ≥ v0.24 |
| FR-6 | Claude backend continues text-mode (no native schema flag); validator step enforces structure | Should | Documented carve-out; capability declares `supports_schema: false` |
| FR-7 | `QueryOutput.structured` is populated whenever schema mode is used; parse failure returns `BackendError::Parse` (non-retryable, surfaced to validator) | Must | Unit test for happy path and malformed-JSON path |
| FR-8 | Built-in schemas for `hunt`, `audit`, `diff`, `fix` tasks ship in repo and are wired by default | Should | `lok run hunt` returns `structured.issues` without user config |

### FR Group: Real Health Checks & Version Detection

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-9 | `Backend` trait gains `async fn health_check(&self) -> Result<HealthStatus, BackendError>` | Must | `HealthStatus { available, version, auth_method, capabilities, unusable_flags, models: Vec<ModelInfo> }` |
| FR-9a | `Engine::warmup_backends()` runs every enabled backend's `health_check` in parallel before workflow execution starts, populating a shared `HealthCache`. `lok run`, `lok ask`, and `lok doctor` all call it. Existing sync `is_available()` reads from the cache (returns `false` until warmup completes; never blocks the runtime). | Must | Integration test confirms ≤1 `--version` / `/api/version` call per backend per `lok` invocation; unit test confirms sync `is_available()` never makes a network/process call itself |
| FR-10 | Sync `is_available()` returns the cached `HealthStatus.available` flag populated by warmup; never spawns processes or makes HTTP calls on its own. TTL only applies to long-lived processes (daemon mode, FR-15a) — for one-shot `lok` invocations the cache is per-run, no TTL refresh | Must | OnceCell + timestamp; assertion in test harness fails if `is_available()` triggers a syscall |
| FR-11 | Ollama health check pings `GET /api/version` **and** `GET /api/tags`, reporting server version plus the list of locally available models | Must | Replaces unconditional `true`; `HealthStatus.models` populated; doctor renders model list |
| FR-11a | Workflow validation rejects steps requesting an Ollama model not present in `HealthStatus.models`, with a remediation hint (`ollama pull <model>`). Other backends apply the same check when their CLI exposes a model list (deferred for Codex/Gemini if unavailable) | Must | Unit test for unknown-model rejection; remediation hint asserted in error message |
| FR-12 | Gemini health check runs `npx @google/gemini-cli --version` and detects auth via env (`GEMINI_API_KEY` / OAuth / Vertex) | Must | Returns `auth_method: ApiKey \| OAuth \| Vertex \| None` |
| FR-13 | Codex health check runs `codex --version`; reports unusable flags (`--output-schema` if < v0.119.0, etc.) | Must | Version parsed; flag matrix consulted |
| FR-14 | `lok doctor` renders the full `HealthStatus` per backend (table or JSON), including model list when available | Must | Snapshot test |
| FR-15 | Version detection result is cached for the lifetime of a `lok` invocation | Should | One `--version` call per backend per run |
| FR-15a | `LOK_HEALTH_TTL` env var (default unset) enables TTL-based refresh for long-running embedders of lok (rs-wisper daemon, etc.). When set, the cache invalidates after the TTL; sync `is_available()` still reads cache only — refresh happens out-of-band on next `warmup_backends()` call | Could | Defer if no concrete consumer needs it |

### FR Group: Capability Registry

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-16 | `BackendCapabilities` struct: `schema`, `tools`, `vision`, `thinking`, `streaming`, `sandbox_levels`, `multi_turn`. `tools` and `multi_turn` are wired to actual behavior in FR-33a and FR-19c respectively — they are not advisory-only flags | Must | Declared per backend; consumed by conductor |
| FR-17 | Consensus / conductor refuses to route a `schema`-, `tools`-, or `multi_turn`-requiring step to a backend that lacks that capability. Error message names the missing capability and suggests an eligible backend from the registry | Must | Returns `BackendError::Config` with actionable message per missing capability |
| FR-18 | `lok backends` command prints capability matrix (including resolved per-backend model list from FR-11) | Should | New subcommand; snapshot test |
| FR-19 | Per-backend capabilities are version-aware (e.g., Codex `schema` only if version ≥ v0.119.0, Ollama `tools` only if server version ≥ v0.24 and the selected model declares tool support) | Must | Capability computed from health check, not a static table |

### FR Group: Backend Trait & Subprocess I/O Hardening

This group exists because several Phase 2 requirements (per-step model, multi-turn Ollama, safe multi-line prompts, async health) all require changing trait signatures and subprocess I/O patterns. Doing these together avoids two churn-passes.

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-19a | `Backend::query` signature accepts a `StepContext` struct instead of bare strings. `StepContext { prompt: &str, history: &[Message], model: Option<&str>, sandbox: Option<Sandbox>, schema: Option<&Schema>, options: &OptionsMap, timeout: Duration }`. Existing callers wrap their args once; downstream FRs (FR-20…FR-24, FR-41) consume fields without further signature churn | Must | All five backends compile against new signature; one `query` call site per backend |
| FR-19b | `Backend::query` becomes `async fn` (or returns `BoxFuture`) so health checks, schema parsing, and tempfile I/O do not need internal `block_on`. The conductor is already async via Phase 1 | Must | No `tokio::runtime::Handle::block_on` inside `src/backend/*.rs` |
| FR-19c | Conversation history is propagated end-to-end: `WorkflowStep` records its rendered prompt and final response into a per-run `History` log, the conductor passes the relevant slice to `StepContext.history`, and backends advertising `multi_turn: true` (currently Ollama, Claude, Codex when targeting Responses-style sessions) translate it into their native messages array. Single-turn backends ignore history with a debug log | Must | Integration test: a 2-step Ollama workflow where step 2 references "the issue you just listed" succeeds (vs. fails today); unit test confirms Gemini single-turn ignores history without error |
| FR-19d | Prompts passed to subprocess-backed CLIs (Codex, Gemini, Claude CLI) are written to a tempfile (or piped via stdin) rather than passed as a shell argument. Multi-line prompts, prompts containing quotes, and prompts > OS argv limit must work | Must | Integration test: 10kB multi-line prompt with embedded backticks and quotes round-trips through each subprocess backend; unit test asserts no prompt content appears in argv snapshot |
| FR-19e | All subprocess invocations use `tokio::process::Command` (already partially true). `stdout` and `stderr` are captured into separate buffers; stderr is preserved in `BackendError` payloads for diagnostics (per Phase 1 stderr-separation work) | Must | Snapshot test for error path includes stderr in the `BackendError::Backend` message |

### FR Group: Per-Step Configuration Threaded Through

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-20 | `step.model` reaches `Backend::query` via `StepContext.model` for every backend | Must | Snapshot test confirms `--model X` (Codex/Gemini), `model: X` (Ollama), profile (Claude) |
| FR-21 | `step.sandbox` (`read-only` / `workspace-write` / `danger-full-access`) controls Codex `-s` and Gemini `--approval-mode` per step | Must | Argv snapshot per sandbox level |
| FR-22 | Steps with `apply_edits = true` and no explicit sandbox default to `workspace-write` | Must | Integration test: edit step writes a file |
| FR-23 | Per-step `timeout` overrides backend default, backend default overrides global | Must | Existing layered-config rule from Phase 1 |
| FR-24 | Per-step `options` map (temperature, top_p, num_ctx, reasoning_effort) passes through where supported | Should | Codex `--reasoning-effort`, Ollama `options`, Gemini ignored with warning |

### FR Group: Token & Usage Observability

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-25 | Codex backend extracts `usage` from `turn.completed` events (input/cached_input/output/reasoning) | Must | `QueryOutput.usage` populated |
| FR-26 | Gemini backend extracts `stats.promptTokenCount` / `candidatesTokenCount` from JSON envelope | Must | `QueryOutput.usage` populated |
| FR-27 | Ollama backend extracts `prompt_eval_count` / `eval_count` from `/api/chat` response | Must | `QueryOutput.usage` populated |
| FR-28 | Run summary aggregates `usage` across all steps and prints in verbose mode | Should | `lok run -v` shows totals |
| FR-29 | JSON output mode (`lok run --output json`) includes per-step `usage` | Should | Snapshot test |
| FR-30 | Optional `token_budget` per workflow aborts when sum exceeds limit | Could | Defer if scope creeps |

### FR Group: Ollama Remote-Ready

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-31 | `BackendConfig.headers: Option<HashMap<String, String>>` attached to reqwest client | Must | TOML round-trip + request-header integration test |
| FR-32 | `BackendConfig.danger_accept_invalid_certs: bool` plus `tls_ca_cert: Option<PathBuf>` for custom CAs | Must | Integration test against self-signed TLS server |
| FR-33 | Ollama backend supports `system` prompt and `options` map (temperature, top_p, num_ctx, keep_alive) | Must | Request body snapshot test |
| FR-33a | Ollama backend supports tool calling for models that advertise it (`/api/chat` with `tools` array). Capability registry declares `tools: true` for Ollama; per-step `tools` definition flows through `StepContext.options` until a typed `tools` field is introduced in Phase 3. The conductor refuses to route a `tools`-using step to a backend whose health-checked model does not list tool support | Should | Request body snapshot includes tools; integration test with a model known to support function calling |
| FR-34 | Ollama `keep_alive` defaults to match the step timeout so models unload on lok timeout | Should | Sent on every request |
| FR-35 | Ollama backend classifies "model not found", "no model loaded", "connection refused" into typed `BackendError` variants. Pre-flight model verification (FR-11a) catches "model not found" before the HTTP call where possible | Must | Unit test per error string |
| FR-36 | `done_reason: "length"` surfaces as truncation indicator (validator can decide to retry) | Should | Field added to `QueryOutput.metadata` |

### FR Group: Session Hygiene

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-37 | Codex default args include `--ephemeral` (when supported by detected version) | Must | Argv snapshot; `~/.codex/sessions/` unchanged after run |
| FR-38 | Gemini supports optional `session_export` / `session_import` per workflow step | Could | Defer if scope creeps; track behind feature flag |
| FR-39 | Failed runs preserve the JSONL/stderr for the failing step under `.lok/debug/<run-id>/` | Should | Postmortem artifact for support |

## 5. Non-Functional Requirements

| Category | Requirement | Target |
|----------|-------------|--------|
| Performance | Schema-mode roundtrip overhead vs text-mode | < 50ms per call (tmpfile + parse) |
| Performance | Health check overhead per `lok` invocation | < 200ms cumulative (one ping per enabled backend, parallel) |
| Reliability | Schema parse failures retried via existing `RetryExecutor` | Parse → `BackendError::Parse` (non-retryable, surfaces to validator) |
| Backward compat | Existing `lok.toml` and workflows continue to work unmodified | All new fields use `#[serde(default)]` **including nested struct fields** (e.g., inside `headers`, `tls`, `options`, `tools`) — `#[serde(deny_unknown_fields)]` on `BackendConfig` stays, but every newly-added nested struct field must be tested for missing-from-toml round-trip |
| Backward compat | Default behavior matches Phase 1 when no schema/sandbox/model/history is set | Snapshot tests on existing workflows; `StepContext::default()` produces Phase-1-equivalent argv per backend |
| Process I/O | Prompts up to 1 MiB pass through subprocess backends without truncation or shell-quoting errors | Tempfile / stdin path (FR-19d); regression test with binary-clean multi-line input |
| Observability | Token usage available in `lok run --output json` | `usage` populated for ≥ 3 of 4 backends |
| Security | New `danger_accept_invalid_certs` defaults false; logs a warning when true | Audit log line on every query when set |
| Disk | No session artifacts written by default for non-interactive runs | Codex `--ephemeral` on; documented for users |

## 6. Scope & Phasing

### In Scope (Phase 2)

- FR Groups 1–7 above (schema output, real health, capability registry, per-step config, usage, Ollama remote, session hygiene).
- New `lok doctor` output, optional `lok backends` capability matrix.
- Built-in schemas for `hunt`, `audit`, `diff`, `fix`.
- Documentation: `docs/backends/*.md` updated per backend, plus a migration note for `skip_lines` deprecation.

### Out of Scope (reason)

- **Streaming output (`stream-json`)** — Phase 3. Requires changes to `Backend::query` signature (`Result<QueryOutput>` → channel-based). Not blocking; current text/JSON mode is enough for predictability.
- **Persistent Codex/Gemini daemon (app-server, ACP, subagent protocol)** — Phase 3+. Architecture is still evolving in gemini-cli; revisit once stable.
- **MCP server inside lok (bidirectional Lok ↔ Gemini)** — Phase 3+.
- **GitHub Action backend type** — separate PRD; not a CLI utility integration.
- **Multimodal / image input for Ollama or Claude** — no workflow demand today.
- **Codex hooks / plugins / goals / memory** — explicitly excluded in [codex-gap-analysis.md §7](../investigations/codex-gap-analysis.md).
- **Model management (pull/delete/push for Ollama)** — not lok's role per [ollama-cli-gap-analysis.md](../investigations/ollama-cli-gap-analysis.md).

### Future Phases

| Phase | Features | Depends On |
|-------|----------|------------|
| Phase 3 | Streaming, persistent daemon, session export/import, MCP server | Phase 2 capability registry |
| Phase 4 | Multimodal, embeddings backend trait, RAG-style workflows | Phase 3 streaming + structured output |

## 7. Dependencies

| Dependency | Owner | Status | Risk if Delayed |
|------------|-------|--------|-----------------|
| Codex CLI ≥ v0.119.0 on dev/CI machines | Lok user | Available | Schema mode falls back to text; capability registry handles it |
| gemini-cli ≥ v0.42.0 | Lok user | Available | Same fallback path |
| Ollama ≥ v0.24.0 | Lok user | Available | `format: schema` falls back to plain JSON mode |
| Phase 1 PRDs landed (CLO-180/181/182/183/184/185) | Lok core | 10/14 tasks done per memory | Blocks FR-9 (uses `BackendError`) |
| `serde_json::Value` plumbed in `QueryOutput.structured` | Phase 1 (llm-mux port) | Field exists, unused | None |

## 8. Risks & Open Questions

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Codex `--output-schema` silently ignored when MCP servers active (Codex issue #15451) | L (lok runs `-s read-only`, no MCP) | M | Validator step double-checks schema conformance; documented |
| Gemini intermediate messages also match schema (Codex issue #19816 analog) | M | M | Consume only the last schema-valid event from JSONL stream |
| `--output-format json` not on older gemini-cli installs | M | M | Version detection + capability registry fall back to text mode |
| Ollama `format: <schema>` not in older servers | L (v0.24 widespread) | M | Same fallback; `BackendError::Config` if schema strictly required |
| Adding `health_check` to `Backend` trait breaks downstream implementations | L (single repo) | L | Default trait impl returns `Available` for backward compat |
| Per-step sandbox switching surprises users who rely on global default | L | M | Documented; `lok run --dry-run` shows resolved sandbox per step |
| `danger_accept_invalid_certs` misused in production | M | H | Logged warning on every use; doc note |
| Capability matrix becomes a maintenance burden as CLIs evolve | M | L | Version-aware computation, not static table |
| Trait signature churn (FR-19a/19b) cascades through every backend at once | M | M | Single coordinated PR for trait + all backends; existing tests pin behavior |
| Conversation history injection blows the context window for long chains | M | M | Per-step `history_window` config; default conservative (last N steps); validator emits a warning when injected history > 50% of model context |
| Tempfile-for-prompt leaks if subprocess hangs past timeout | L | L | RAII guard on tempfile; explicit cleanup in `Drop` and on timeout path |
| `warmup_backends()` adds startup latency when a backend is unreachable | M | L | Per-backend warmup is parallel + has its own timeout (default 2s); failure marks backend `available: false` and continues |

### Open Questions

- [ ] Should `QueryOutput.structured` be `Option<serde_json::Value>` or a typed `enum StructuredOutput { Issues(Vec<Issue>), ... }`? Owner: MK, deadline: design review before FR-1.
- [ ] Where do JSON Schemas live: `schemas/` directory or inline in `tasks/*.rs`? Owner: MK, deadline: before FR-8.
- [ ] Should `Backend::health_check` be on the trait or a free function that takes `&dyn Backend`? Owner: MK, deadline: before FR-9 implementation.
- [ ] How does the conductor pick a fallback backend when the preferred one lacks `schema` capability — fail fast or downgrade to text mode? Owner: MK, deadline: before FR-17.
- [ ] Should `lok doctor` block (refuse to proceed) or warn when a backend is unhealthy? Owner: MK, deadline: before FR-14.
- [ ] What is the default `history_window` for multi-turn injection — all prior steps in the dependency chain, or last N? Owner: MK, deadline: before FR-19c implementation.
- [ ] Where does `StepContext` live in code — `src/backend/context.rs`, or alongside `WorkflowStep` in `src/config.rs`? Owner: MK, deadline: before FR-19a implementation.
- [ ] Should `Backend::query` becoming async use `async fn` (requires `async_trait` or Rust 1.75+ `impl Trait` in traits) or return `Pin<Box<dyn Future>>`? Owner: MK, deadline: before FR-19b implementation.
- [ ] Should tempfile-based prompt delivery use `tempfile::NamedTempFile` (cleaned on drop) or piped stdin? Trade-off: stdin works for all three subprocess backends uniformly; tempfile is required if any CLI does not accept stdin. Owner: MK, deadline: before FR-19d implementation.

## 9. Rollout & Measurement

### Release Plan

- **Sequencing.** Land FRs in the order: **trait & I/O hardening (FR-19a–19e) first** because every later group consumes the new `StepContext` signature and async query → capability registry (FR-16–19) → health checks + warmup (FR-9, FR-9a, FR-10–15a, FR-11a) → per-step config plumbing (FR-20–24) → Ollama remote/options/tools (FR-31–36) → schema output per backend (FR-1–8, FR-3a) → usage extraction (FR-25–30) → session hygiene (FR-37–39). Each group ships as its own Linear task / PR; the order keeps every PR independently mergeable.
- **Feature flags.** None at the backend level — these are additive config fields. A `LOK_FORCE_TEXT_MODE=1` escape hatch covers users hitting unknown schema bugs.
- **Migration.** `skip_lines` stays usable but is deprecated with a single-warning log; remove in Phase 3.
- **Rollback.** Each FR group reverts cleanly; tests pin Phase 1 behavior when new config fields are absent.

### Measurement Plan

- **Per-PR.** Argv snapshot tests, request-body snapshot tests, integration tests against real Codex / Gemini / Ollama instances (already in lok's test setup).
- **First end-to-end check.** After capability registry + health checks land, run the rs-wisper review pipeline against a known PR; compare structured-output rejection rate to Phase 1 baseline.
- **Decision points.**
  - If schema-mode parse failures > 2% after FR-1–8 ship, pause and investigate (likely model-specific or schema-specific).
  - If health checks add > 200ms to `lok ask`, move them behind an explicit `--check` flag.
  - If capability-aware routing causes user confusion (workflows that "worked" now refuse to run), add a `--allow-capability-downgrade` flag and surface in `lok doctor`.

## 10. Appendix: Source-to-Requirement Map

| Investigation finding | Maps to FR |
|-----------------------|-----------|
| Codex `--output-schema` unused (codex §3.1) | FR-1, FR-3, FR-8 |
| Codex `--ephemeral` unused (codex §3.4) | FR-37 |
| Codex usage in `turn.completed` ignored (codex §3.2) | FR-25 |
| Codex sandbox hardcoded `read-only` (codex §5.2) | FR-21, FR-22 |
| Codex `step.model` not threaded (codex §5.6) | FR-20 |
| Codex JSONL substring matching (codex §5.1) | FR-3, **FR-3a**, FR-7 |
| Gemini shell pipe + `skip_lines` (gemini §4.1, §6.7) | FR-4 |
| Gemini multi-line prompt fragility (gemini §6.4) | **FR-19d** |
| Gemini exit-code classification (gemini §4.3) | FR-9 (folded into health), Phase 1 `BackendError` |
| Gemini version detection (gemini §6.1) | FR-12 |
| Gemini auth detection (gemini §6.3) | FR-12 |
| Gemini `--include-directories` (gemini §6.5) | FR-24 (options passthrough) |
| Ollama `is_available()` lies + sync trait method (ollama-api §3.1) | FR-9, **FR-9a**, FR-10, FR-11 |
| Ollama no headers / TLS (ollama-cli §1.1, §1.3) | FR-31, FR-32 |
| Ollama no system prompt / options (ollama-cli §1.1, ollama-api §2) | FR-33 |
| Ollama no tool calling (ollama-api §1, §3) | **FR-33a** (with capability gating via FR-16, FR-17) |
| Ollama single-turn only / drops history (ollama-api §1) | **FR-19c** |
| Ollama hardcoded `llama3.2` fallback / no model verification (ollama-api §3.4, §5) | **FR-11, FR-11a** |
| Ollama no `keep_alive` coordination (ollama-api §3.3) | FR-34 |
| Ollama `format` (schema) unused (ollama-api §1) | FR-5 |
| Ollama usage fields unused (ollama-api §1) | FR-27 |
| Ollama error classification (ollama-api §5.1) | FR-35 |
| `BackendConfig` `deny_unknown_fields` trap (ollama-cli) | NFR row on serde(default) for nested structs |
| All backends: no capability advertising (ollama-api §5.3, gemini §5.5, codex §4.3) | FR-16–19 |
| All backends: `Backend::query` takes bare strings, no `StepContext` or history | **FR-19a, FR-19b, FR-19c** |

## 11. Pre-Implementation Code Review Checklist

Before any Linear task is created or implementation begins, walk the following files and confirm the assumptions encoded in this PRD still hold. Findings that contradict the PRD must be reconciled here first — do **not** silently adjust requirements during implementation.

### 11.1 `src/backend/mod.rs` — Trait Signatures
- [ ] **Confirm current `Backend::query` signature.** Document the exact parameters today. Plan the exact diff to `fn query(&self, ctx: StepContext) -> impl Future<Output = ...>` (FR-19a, FR-19b).
- [ ] **Confirm current `is_available` signature** (sync? async? `bool`? `Result`?). If sync, confirm the no-syscall cache-only behavior in FR-10 is achievable; if any backend currently spawns a process inside `is_available`, surface the refactor cost.
- [ ] **Identify every concrete implementor** (`CodexBackend`, `GeminiBackend`, `OllamaBackend`, `ClaudeBackend`, `BedrockBackend`?) — each one must be migrated in lockstep when the trait changes, so plan the PR boundary accordingly.

### 11.2 `src/config.rs` — Backend / Step / Workflow Config
- [ ] **Confirm `BackendConfig` uses `#[serde(deny_unknown_fields)]`** as noted in ollama-cli gap analysis. If yes, list every existing field — new fields (`schema`, `headers`, `tls_ca_cert`, `danger_accept_invalid_certs`, `options`, `keep_alive`, `tools`) need `#[serde(default)]` and an explicit round-trip test.
- [ ] **Confirm `WorkflowStep` config struct.** Identify whether it currently carries `model`, `sandbox`, `timeout`, `schema`, `tools`, `history_window`. Plan additive fields; do not rename existing ones.
- [ ] **Audit nested structs** (`OptionsMap`, any `TlsConfig`, any `HeadersMap`). Each nested struct needs the same `serde(default)` discipline — verify with a deliberate "minimum TOML" parse test.

### 11.3 `src/workflow.rs` (or wherever the conductor / DAG lives) — History & StepResult
- [ ] **Locate `StepResult` (or equivalent).** Confirm whether it stores: rendered prompt, final response string, structured payload, usage. FR-19c needs the rendered prompt + final response retained per step.
- [ ] **Confirm the conductor's step iteration loop.** Identify where `Backend::query` is called and trace what context is in scope at that call site — this is where `StepContext` must be constructed.
- [ ] **Identify the rule for "which prior steps' history is in scope".** Default proposal: the linear chain of completed steps that feed the current step's `depends_on`. Confirm or adjust before FR-19c lands.

### 11.4 `src/backend/codex.rs` — JSONL Stream Parsing
- [ ] **Locate the existing `line.contains("\"agent_message\"")` substring match.** Document every event type currently observed in production JSONL streams (capture a real stream into a fixture).
- [ ] **Plan the event-driven parser** (FR-3a): a `serde`-deserialized enum over `{thread.started, turn.started, turn.completed, turn.failed, item.started, item.completed, error}`, accumulating items per turn, returning the last turn's `agent_message`. Reasoning items must not feed `QueryOutput.structured`.
- [ ] **Confirm `turn.completed.usage`** field shape on current Codex; populate FR-25.

### 11.5 `src/backend/gemini.rs` — Subprocess I/O
- [ ] **Locate the current `Command::new(...)` invocation.** Confirm whether the prompt is passed via `.arg(prompt)`, piped to stdin, or written to a file. Multi-line / quote-bearing prompts must move to stdin/tempfile per FR-19d.
- [ ] **Locate `skip_lines = 1` usage** and confirm the call site that strips the first line of stdout. Replace with `--output-format json` envelope parsing (FR-4).
- [ ] **Confirm `npx @google/gemini-cli` is the invocation form** today; plan whether health check uses `npx ... --version` directly or shells through a configurable command path.

### 11.6 `src/backend/ollama.rs` — HTTP Client & Body
- [ ] **Locate the `reqwest::Client` construction.** Confirm it currently has no header injection / TLS config; plan FR-31, FR-32.
- [ ] **Locate the `/api/chat` request body.** Confirm it sends only `{model, messages: [{role: "user", content: prompt}]}` today; plan addition of `system`, `options`, `format`, `keep_alive`, `tools` (FR-5, FR-19c, FR-33, FR-33a, FR-34).
- [ ] **Confirm `is_available` body** is `Ok(true)` (or similar). Plan FR-11 / FR-11a swap.

### 11.7 Cross-Cutting
- [ ] **Find every call site of `Backend::query`** (`grep -n 'backend\.query\|\.query(' src/`). Plan the migration: one PR per backend, or one PR for the trait + all backends? Recommendation: trait change + all backends in one PR (FR-19a–19b), then per-backend feature PRs.
- [ ] **Confirm Phase 1 `BackendError` variants** cover the new failure modes (parse, model-not-found, auth-missing, version-mismatch). Extend if necessary; coordinate with rs-wisper consumers.
- [ ] **Locate `main.rs` / engine entry points** where `warmup_backends()` (FR-9a) will be called. Confirm there is exactly one entry per command path (`run`, `ask`, `doctor`, `backends`).

The output of this checklist is a short reconciliation note (per file, one bullet: "matches PRD" / "PRD needs update because X"). That note becomes the gate between this PRD and the Linear-task breakdown.
