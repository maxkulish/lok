# PRD: Phase 2 — Predictable CLI Execution (v3)

| Field | Value |
|-------|-------|
| Author | MK |
| Status | Draft |
| Created | 2026-05-17 |
| Last Updated | 2026-05-17 (rev 3) |
| Stakeholders | Lok core, rs-wisper consumers |
| Source Investigations | [codex-gap-analysis.md](../investigations/codex-gap-analysis.md), [codex-quick-ref.md](../investigations/codex-quick-ref.md), [gemini-cli-gap-analysis.md](../investigations/gemini-cli-gap-analysis.md), [ollama-cli-gap-analysis.md](../investigations/ollama-cli-gap-analysis.md), [ollama-api-gap-analysis.md](../investigations/ollama-api-gap-analysis.md) |
| Parent / Prior Work | [prd-llm-mux-port.md](prd-llm-mux-port.md), [prd-output-validation-pipeline.md](prd-output-validation-pipeline.md), [prd-structured-failure-data.md](prd-structured-failure-data.md) |
| Supersedes | rev 2 (in-place edits up to 2026-05-17 mid-day) |

## Revision History

- **rev 1 (initial)** — established schema/health/capability/per-step/usage/remote/session FR groups against the four CLI gap analyses.
- **rev 2** — added FR-3a (event-driven Codex JSONL parsing), FR-9a (warmup), FR-11/11a (Ollama model verification), FR-19a–e (trait & I/O hardening for `StepContext`, async, multi-turn, stdin/tempfile prompts), FR-33a (Ollama tools), §11 pre-implementation code-review checklist.
- **rev 3** — reconciled PRD against actual codebase: corrected backend count (5, not 4 — `BedrockBackend` is real), corrected `QueryOutput.usage` baseline (3/5 already populate, not 0/4), added `StepResult.usage` FR (architectural gap that blocks every Token & Usage FR), extended `TokenUsage` with `cached_tokens`/`reasoning_tokens`, split Claude capabilities per `ClaudeMode::{Api, Cli}`, fixed FR-20 to cover `run_query_with_config` and every non-workflow caller, reconciled `Step` vs `WorkflowStep` naming, added Codex CI-cleanliness flags (`--output-last-message`, `--ignore-user-config`, `--ignore-rules`, `--skip-git-repo-check`), promoted Gemini `--include-directories`, extended Ollama `done_reason` handling and `think` plumbing, added integration-test-fixture FR group, added Phase-1 verification gate.

## 1. Overview

Phase 1 made lok's backends *fail loudly*: typed `BackendError`, retry policies, richer `QueryOutput`, validation pipeline, and structured `FailureType` across every step path. Phase 2 makes lok's backends *succeed predictably*: schema-enforced output instead of regex-scraped text, real health checks instead of optimistic `true`, capability-aware routing instead of "every model is a text chat", and per-step permission/model selection instead of hardcoded defaults.

The five backends lok orchestrates (`CodexBackend`, `GeminiBackend`, `OllamaBackend`, `ClaudeBackend` in dual `Api`/`Cli` modes, and the feature-gated `BedrockBackend`) wrap CLIs and APIs that have shipped major automation surface area since lok's last update — JSON schemas, token usage in events, ephemeral sessions, approval/sandbox profiles, headers/TLS. Lok uses ~15–20% of that surface today. The bet: by adopting the structured paths each tool now offers, lok eliminates its single largest reliability tax (fragile text parsing) and unlocks observability (token cost, real health, model coverage) that workflows already need but cannot currently get.

## 2. Problem & Objectives

### Problem Statement

Lok's job is to send a prompt to one or more backends and return a result a downstream workflow step can act on. Today that contract is broken in four ways:

1. **Output is text-shaped, not data-shaped.** Codex's JSONL stream is matched by substring (`line.contains("\"agent_message\"")`), Gemini's stdout is parsed by `skip_lines = 1`, Ollama returns single-turn chat text, Claude CLI hardcodes `--output-format text`. None of the five backends uses the structured-output features each tool now offers (`--output-schema`, `--output-format json`, `format: <jsonschema>`). Workflows that need a list of issues, vulnerabilities, or diffs strip markdown fences and parse fragments by hand.
2. **Health is asserted, not verified.** `OllamaBackend::is_available()` returns `true` unconditionally. Gemini's check confirms only that `npx` is on PATH, not that `@google/gemini-cli` is installed or authenticated. Claude CLI mode only confirms the binary exists. There is no CLI version detection, so flags added after lok's last update silently no-op on older installs.
3. **Capability is invisible, configuration is half-wired.** Per-step `model` exists in config but is passed as `None` at every `Backend::query` call site outside the workflow engine — `run_query_with_config` (src/backend/mod.rs:381), `conductor.rs:186`, `spawn.rs:173,282`, `team.rs:82,121`, and `debate.rs:230` all hardcode `None`. `apply_edits` workflows run under hardcoded `-s read-only` and fail to write. Ollama cannot reach remote servers behind auth proxies (no header/TLS support). Backends do not advertise whether they support tools, vision, thinking, or schemas, so the conductor cannot route by capability.
4. **Usage and history do not flow.** `QueryOutput.usage` is populated by 3 of 5 backends today (`OllamaBackend` from `prompt_eval_count`/`eval_count`, `ClaudeBackend::Api` from `ClaudeUsage`, `BedrockBackend` from Bedrock metering). But `StepResult` has no `usage` field, so even when a backend reports usage it is **dropped on the conductor floor**. Multi-turn history is not retained between steps; the second Ollama step in a chain has no model memory of the first.

The result: rs-wisper-style review workflows see ~5% silent failures (Phase 1 catches the empty/noise cases; Phase 2 must catch the malformed-structure cases), users have no token-cost visibility even on backends that already meter it, and any workflow that wants Codex to write files must bypass lok's default args.

### Objectives

- **O1 — Structured output by default.** Every backend that supports a schema/JSON output mode uses it, and lok returns parsed `serde_json::Value` in `QueryOutput.structured` for downstream steps to consume without regex.
- **O2 — Real health and version awareness.** `is_available()` reflects an actual ping; `lok doctor` reports each backend's installed version, auth method, and which optional flags are usable.
- **O3 — Capability-aware backends.** Each backend declares supported capabilities (schema, tools, vision, thinking, streaming, multi-turn, sandbox levels). For backends with multiple execution modes (`ClaudeBackend::Api` vs `Cli`), capabilities are computed per mode. The conductor/consensus engine routes by capability instead of by name.
- **O4 — Per-step configuration is honored end-to-end.** `step.model`, `step.sandbox` / `approval_mode`, `step.timeout`, `step.schema`, and `step.options` flow from TOML through `run_query_with_config` **and every other `Backend::query` call site** into the actual CLI/API invocation.
- **O5 — Token and usage observability is end-to-end.** `QueryOutput.usage` is populated by every backend that emits the data, **and `StepResult.usage` carries it through the conductor** into run summaries and JSON output. `TokenUsage` is extended to carry `cached_tokens` (Codex/Anthropic) and `reasoning_tokens` (Codex/o-series) so the existing cost-cache savings are not flattened.
- **O6 — Remote and ephemeral execution.** Ollama supports custom headers and TLS so workflows can target auth-gated remote servers. Codex sessions default to `--ephemeral` and CI-clean flags (`--ignore-user-config`, `--ignore-rules`, `--skip-git-repo-check`) so non-interactive runs do not depend on the runner's `~/.codex` or git layout.

### Success Metrics (KPIs)

| Metric | Current | Target | How Measured |
|--------|---------|--------|--------------|
| Workflows whose downstream steps regex-parse LLM output | majority | 0 (use `structured` field) | Audit `src/workflows/*.toml` and `src/tasks/*.rs` |
| Silent failures from malformed structure | unknown (~5% upper bound from Phase 1 audit) | < 0.5% | Validation step rejection rate on schema-mode runs |
| `QueryOutput.usage` populated per backend | 3 of 5 (Ollama, Claude::Api, Bedrock) | 5 of 5 | Integration test asserts `usage.is_some()` for every backend |
| `StepResult.usage` populated when `QueryOutput.usage` is `Some` | 0 of 5 (field does not exist on `StepResult`) | 5 of 5 | Conductor test: `step.usage == query.usage` |
| `lok doctor` accurate availability calls | 1 of 5 (Codex only) | 5 of 5 | New `lok doctor` snapshot tests |
| Per-step `model` override actually reaches CLI/API | 0 backends (every non-workflow caller passes `None`) | 5 backends across all call sites | Snapshot test of constructed argv / API body; grep proves no `query(..., None)` in `src/` outside test fixtures |
| Codex session files written per `lok run` | N (one per step) | 0 | `~/.codex/sessions/` count after CI run |
| Ollama queries against TLS+auth-gated server succeed | 0 | 100% | Integration test against test server |

## 3. Users & Use Cases

### Personas

| Persona | Role | Need | Pain Point |
|---------|------|------|------------|
| Workflow author (MK) | Writes TOML workflows | Reliable structured handoff between steps | Hand-parses model output, fights markdown fences, debugs silent empty results |
| rs-wisper integrator | Calls `lok run review-pr` from another tool | Predictable JSON output for review issues | Receives free-text reviews, must LLM-parse them again |
| Ops / on-call | Runs lok in CI or cron | Cost and health visibility | No token usage logged even though 3/5 backends already meter; failures look like exit codes only |
| Enterprise user | Hosts Ollama behind auth proxy on TLS | Use private Ollama from lok workflows | Backend has no header/TLS knobs; connection refused |
| CI maintainer | Runs `lok` in GitHub Actions / GitLab CI | Reproducible runs that do not depend on the runner's home dir or git state | Codex reads user config from `~/.codex`; missing `.git` aborts; session files accumulate in cache mounts |

### Key Use Cases

**UC-1: Hunt returns typed issues**
- Trigger: `lok run hunt <dir>`
- Steps: lok builds the hunt JSON Schema → sends prompt with `--output-schema` (Codex) or `--output-format json` (Gemini) or `format: schema` (Ollama) or `--output-format json` (Claude CLI if version supports it; see FR-6) → backend returns schema-valid JSON → lok parses into `Vec<Issue>` → downstream step gets `{{ steps.hunt.structured.issues }}`
- Outcome: Downstream rendering / issue-creation step receives structured data, no markdown stripping.

**UC-2: Apply-edits step writes files without TOML hacks**
- Trigger: Workflow step has `apply_edits = true`
- Steps: Engine sees the flag → swaps Codex sandbox to `workspace-write` for that step only → other steps keep `read-only`
- Outcome: Edits land on disk; user does not edit defaults globally.

**UC-3: Doctor surfaces a usable diagnosis**
- Trigger: `lok doctor`
- Steps: Each backend reports `version`, `available` (real ping), `auth_method`, `supported_capabilities`, and unusable flags on this version. `ClaudeBackend` reports two rows: `claude (api)` and `claude (cli)` with distinct capability sets.
- Outcome: User immediately sees that Gemini is OAuth-only (will hang headless), that Ollama is unreachable, or that Claude CLI is at a version that does not support `--output-format json` — instead of waiting for a query to fail.

**UC-4: Cost report after every workflow**
- Trigger: `lok run <workflow>` completes
- Steps: Each `QueryOutput.usage` (including `cached_tokens` for Anthropic prompt-cache savings and `reasoning_tokens` for o-series) is recorded into `StepResult.usage` → summed across steps → printed in verbose summary → exposed in JSON output mode
- Outcome: User knows that the run cost N input + M output + R reasoning tokens across K calls, and how much was served from cache.

**UC-5: Remote Ollama through auth proxy**
- Trigger: `lok ask --backend ollama` with `headers` and `tls` configured
- Steps: Backend attaches headers to reqwest client; uses TLS settings
- Outcome: Query reaches the gated server; returns valid response.

**UC-6: Multi-turn Ollama workflow**
- Trigger: Two-step workflow where step 2's prompt is "for each issue you just listed, suggest a fix" against an Ollama model
- Steps: Conductor records step 1's prompt + response into `StepResult` → constructs `StepContext.history` for step 2 → Ollama backend translates history into the `messages: [{role:"user",...},{role:"assistant",...},{role:"user",...}]` array
- Outcome: Step 2 actually sees step 1's output as conversational context; today it sees only the rendered prompt with no model memory.

**UC-7: Multi-line prompt with code blocks**
- Trigger: A workflow step's prompt is a 4 kB markdown document containing nested backticks, embedded quotes, and a code fence
- Steps: Backend writes the prompt to a tempfile / pipes via stdin → CLI reads it cleanly → no shell escaping or argv length issues
- Outcome: The prompt arrives at the model byte-for-byte; today this fails on Gemini with shell-quoting errors.

**UC-8: CI run that does not depend on the runner's home dir**
- Trigger: `lok run` invoked from GitHub Actions on a fresh runner
- Steps: Codex backend passes `--ignore-user-config`, `--ignore-rules`, `--skip-git-repo-check` (when the version supports each), `--ephemeral`, and `--output-last-message <tmp>`. No state written to `~/.codex`; no dependency on the runner having `.git` initialised; final message read from the tmp file even if JSONL parsing breaks on a new event type.
- Outcome: Reproducible CI runs; new Codex event types do not regress the result extraction because the final message has a separate non-JSONL path.

## 4. Functional Requirements

### FR Group: Backend Trait & Subprocess I/O Hardening (lands first)

This group exists because several Phase 2 requirements (per-step model, multi-turn Ollama, safe multi-line prompts, async health) all require changing trait signatures and subprocess I/O patterns. Doing these together avoids two churn-passes. **Note:** the codebase struct is named `Step` (defined in `src/workflow.rs:200`) — there is no `WorkflowStep`. All references in this PRD use `Step`.

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-19a | `Backend::query` signature accepts a `StepContext` struct instead of bare strings. `StepContext { prompt: &str, history: &[Message], model: Option<&str>, sandbox: Option<Sandbox>, schema: Option<&Schema>, options: &OptionsMap, timeout: Duration }`. Existing callers wrap their args once; downstream FRs (FR-20…FR-24, FR-41) consume fields without further signature churn | Must | All five backends compile against new signature; one `query` call site per backend |
| FR-19b | `Backend::query` becomes `async fn` (via `async_trait`, already a dep). Health checks, schema parsing, and tempfile I/O do not need internal `block_on`. The conductor is already async via Phase 1 | Must | No `tokio::runtime::Handle::block_on` inside `src/backend/*.rs` |
| FR-19c | Conversation history is propagated end-to-end: `Step` records its rendered prompt and final response into a per-run `History` log (via the new `StepResult.usage`/`StepResult.rendered_prompt` fields added by FR-25a/FR-19f), the conductor passes the relevant slice to `StepContext.history`, and backends advertising `multi_turn: true` (Ollama, Claude API+CLI, Codex when targeting Responses-style sessions) translate it into their native messages array. Single-turn backends (Gemini, Bedrock if applicable) ignore history with a debug log | Must | Integration test: a 2-step Ollama workflow where step 2 references "the issue you just listed" succeeds (vs. fails today); unit test confirms Gemini single-turn ignores history without error |
| FR-19d | Prompts passed to subprocess-backed CLIs (Codex, Gemini, Claude CLI) are written to a tempfile (or piped via stdin) rather than passed as a shell argument. Multi-line prompts, prompts containing quotes, and prompts > OS argv limit must work | Must | Integration test: 10kB multi-line prompt with embedded backticks and quotes round-trips through each subprocess backend; unit test asserts no prompt content appears in argv snapshot |
| FR-19e | All subprocess invocations use `tokio::process::Command` (already partially true). `stdout` and `stderr` are captured into separate buffers; stderr is preserved in `BackendError` payloads for diagnostics (per Phase 1 stderr-separation work) | Must | Snapshot test for error path includes stderr in the `BackendError::Backend` message |
| FR-19f | `StepResult` gains `rendered_prompt: Option<String>` (the post-template prompt actually sent) so FR-19c history reconstruction can use the exact text the backend saw. Field is `Option` so legacy callers keep working | Must | Round-trip test: `StepResult.rendered_prompt == StepContext.prompt` |

### FR Group: Schema-Driven Structured Output

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-1 | `BackendConfig` accepts optional `schema` (inline JSON or path) and `output_format` (`text` / `json` / `stream-json`). New fields use `#[serde(default)]` to preserve the existing `#[serde(deny_unknown_fields)]` contract on `BackendConfig` | Must | Round-trip TOML test; defaults preserve existing behavior |
| FR-2 | `Step` accepts a `schema` field overriding backend default | Must | Per-step schema applied; missing step falls back to backend; missing both falls back to text mode |
| FR-3 | Codex backend passes `--output-schema <tmpfile>` when a schema is set (Codex ≥ v0.119.0); default args add `--ephemeral` | Must | Argv snapshot test; tmpfile cleaned up on drop |
| FR-3a | Codex JSONL stream is parsed as discrete events, not by substring matching. The schema payload is extracted from the `agent_message` item of the **last** `turn.completed` event in the stream. Intermediate reasoning / tool-call items are discarded for the structured payload. `turn.failed` short-circuits to `BackendError::Backend` | Must | Unit test with a recorded JSONL fixture containing reasoning + tool-call + final agent_message; integration test confirms intermediate JSON-shaped reasoning is not mistaken for the result |
| FR-3b | Codex backend also passes `-o <tmpfile>` (`--output-last-message`, Codex ≥ v0.119.0) and reads the final message from that file as the **authoritative** result when present. JSONL events still drive token usage and intermediate failure detection; the `-o` file is the result-extraction path. Rationale: JSONL event schemas evolve; the dedicated last-message file is a stable contract | Must | Unit test: when `-o` file is non-empty, JSONL parsing failures do not fail the step; when `-o` file is absent (older Codex), FR-3a's JSONL extraction is used |
| FR-4 | Gemini backend passes `--output-format json` and reads `response`/`stats`/`error` from the JSON envelope | Must | Replace `skip_lines`; integration test against gemini-cli ≥ v0.42 |
| FR-5 | Ollama backend sends `format` field with a JSON Schema when a schema is set | Must | Integration test against ollama ≥ v0.24 |
| FR-6 | Claude CLI mode passes `--output-format json` (Claude CLI supports it; current `claude.rs:218` hardcodes `text`) and parses the JSON envelope for response text + `usage`. Claude API mode already returns structured JSON; no change beyond schema validation. Capability declares `schema: false` for both modes until Anthropic ships a native schema-enforcement flag — the JSON envelope carries unconstrained text in the `content` field. Validator step enforces structure | Must | Unit test: Claude CLI argv now contains `--output-format json`; JSON parse path populates `QueryOutput.usage` and `QueryOutput.text` |
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
| FR-13 | Codex health check runs `codex --version`; reports unusable flags (`--output-schema`, `-o`, `--ephemeral` if < v0.119.0; `--ignore-user-config`, `--ignore-rules` if < v0.122.0) | Must | Version parsed; flag matrix consulted |
| FR-13a | Claude health check is mode-aware. `ClaudeBackend::Api` checks for `ANTHROPIC_API_KEY` and (cheaply) the configured model is non-empty. `ClaudeBackend::Cli` runs `claude --version` and reports whether `--output-format json` is supported. `HealthStatus` includes a `mode: "api" \| "cli"` discriminator | Must | Snapshot test of both modes |
| FR-14 | `lok doctor` renders the full `HealthStatus` per backend (table or JSON), including model list when available. Claude is rendered as two rows when both modes are configured | Must | Snapshot test |
| FR-15 | Version detection result is cached for the lifetime of a `lok` invocation | Should | One `--version` call per backend per run |
| FR-15a | `LOK_HEALTH_TTL` env var (default unset) enables TTL-based refresh for long-running embedders of lok (rs-wisper daemon, etc.). When set, the cache invalidates after the TTL; sync `is_available()` still reads cache only — refresh happens out-of-band on next `warmup_backends()` call | Could | Defer if no concrete consumer needs it |

### FR Group: Capability Registry

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-16 | `BackendCapabilities` struct: `schema`, `tools`, `vision`, `thinking`, `streaming`, `sandbox_levels`, `multi_turn`. `tools` and `multi_turn` are wired to actual behavior in FR-33a and FR-19c respectively — they are not advisory-only flags. **For backends with multiple execution modes, capabilities are computed per mode** (e.g., `ClaudeBackend::Api` has `schema: false, thinking: true (via extended-thinking)` while `ClaudeBackend::Cli` has `schema: false, thinking: false`) | Must | Declared per backend; consumed by conductor; Claude declares two capability sets keyed by mode |
| FR-17 | Consensus / conductor refuses to route a `schema`-, `tools`-, or `multi_turn`-requiring step to a backend that lacks that capability. Error message names the missing capability and suggests an eligible backend from the registry | Must | Returns `BackendError::Config` with actionable message per missing capability |
| FR-18 | `lok backends` command prints capability matrix (including resolved per-backend model list from FR-11). Claude appears as two rows (api / cli) | Should | New subcommand; snapshot test |
| FR-19 | Per-backend capabilities are version-aware (e.g., Codex `schema` only if version ≥ v0.119.0, Ollama `tools` only if server version ≥ v0.24 and the selected model declares tool support, Claude CLI `json output` only if version supports `--output-format json`) | Must | Capability computed from health check, not a static table |

### FR Group: Per-Step Configuration Threaded Through

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-20 | `step.model` reaches `Backend::query` via `StepContext.model` for every backend **and every call site**. Today the following call sites pass `None` unconditionally and must be updated: `src/backend/mod.rs:381` (`run_query_with_config`), `src/conductor.rs:186`, `src/spawn.rs:173`, `src/spawn.rs:282`, `src/team.rs:82`, `src/team.rs:121`, `src/debate.rs:230`. Workflow paths (`src/workflow.rs:767, 778, 1737, 1943, 2041, 2171`) already thread `model_override`; verify they construct `StepContext.model` correctly under the new signature | Must | Snapshot test confirms `--model X` (Codex/Gemini), `model: X` (Ollama), profile (Claude), `modelId` (Bedrock); grep test asserts no `backend.query(..., None)` literal remains in `src/` outside test fixtures |
| FR-21 | `step.sandbox` (`read-only` / `workspace-write` / `danger-full-access`) controls Codex `-s` and Gemini `--approval-mode` per step | Must | Argv snapshot per sandbox level |
| FR-22 | Steps with `apply_edits = true` and no explicit sandbox default to `workspace-write` | Must | Integration test: edit step writes a file |
| FR-23 | Per-step `timeout` overrides backend default, backend default overrides global | Must | Existing layered-config rule from Phase 1 |
| FR-24 | Per-step `options` map (temperature, top_p, num_ctx, reasoning_effort, `think` for thinking-capable Ollama models) passes through where supported | Should | Codex `--reasoning-effort`, Ollama `options` + `think`, Gemini ignored with warning |
| FR-24a | Gemini per-step `include_directories: Vec<PathBuf>` is promoted from an `options` passthrough to a typed field on `Step`. Gemini backend converts to `--include-directories <comma-list>`. Rationale: this is the primary mechanism for multi-repo review workflows and was previously buried in the options bag | Must | Argv snapshot for Gemini step with `include_directories = ["../shared", "../docs"]` produces `--include-directories ../shared,../docs` |

### FR Group: Token & Usage Observability

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-25 | Codex backend extracts `usage` from `turn.completed` events (input_tokens, cached_input_tokens, output_tokens, reasoning_output_tokens) into `QueryOutput.usage` | Must | `QueryOutput.usage` populated; cached + reasoning fields routed to extended `TokenUsage` (FR-25b) |
| FR-25a | **`StepResult` gains `usage: Option<TokenUsage>` field.** The conductor copies `QueryOutput.usage` into `StepResult.usage` after every `Backend::query` call. Without this field, FR-25 / FR-26 / FR-27 / FR-28 / FR-29 are architecturally impossible — the usage data exists at the backend boundary but is discarded before reaching the run summary. (Field is `Option` to preserve serde back-compat under `deny_unknown_fields`; defaults to `None` for legacy paths.) | Must | Field added at `src/workflow.rs:897`; conductor test: `step_result.usage == query_output.usage`; serde round-trip with field absent passes |
| FR-25b | **`TokenUsage` extended** with `cached_tokens: Option<u32>` (Anthropic prompt-cache / Codex `cached_input_tokens`) and `reasoning_tokens: Option<u32>` (Codex `reasoning_output_tokens` / o-series). `total_tokens` continues to mean `prompt + completion`. Existing `prompt_tokens` / `completion_tokens` / `total_tokens` semantics unchanged | Must | `TokenUsage` constructor with all four fields; serde tests for missing optional fields |
| FR-26 | Gemini backend extracts `stats.promptTokenCount` / `candidatesTokenCount` from JSON envelope into `QueryOutput.usage` | Must | `QueryOutput.usage` populated |
| FR-27 | Ollama backend already extracts `prompt_eval_count` / `eval_count` from `/api/chat` response (verified at `src/backend/ollama.rs:155`). FR retained as a regression-pin: the conductor copy (FR-25a) is the new work, the backend extraction must continue to function | Must | Unit test on existing extraction; conductor test on the copy |
| FR-27a | Claude API mode already populates `QueryOutput.usage` from `ClaudeUsage` (`src/backend/claude.rs:194`); Bedrock backend already populates from Bedrock metering (`src/backend/bedrock.rs:199`). Claude CLI mode currently does not (text-only stdout); FR-6's `--output-format json` switch is what enables CLI-mode usage extraction | Must | Regression-pin Claude API + Bedrock paths; new test for Claude CLI JSON path |
| FR-28 | Run summary aggregates `StepResult.usage` across all steps (including cached + reasoning totals) and prints in verbose mode | Should | `lok run -v` shows totals split by category |
| FR-29 | JSON output mode (`lok run --output json`) includes per-step `usage` | Should | Snapshot test includes `cached_tokens` and `reasoning_tokens` fields |
| FR-30 | Optional `token_budget` per workflow aborts when sum exceeds limit | Could | Defer if scope creeps |

### FR Group: Ollama Remote-Ready

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-31 | `BackendConfig.headers: Option<HashMap<String, String>>` attached to reqwest client | Must | TOML round-trip + request-header integration test |
| FR-32 | `BackendConfig.danger_accept_invalid_certs: bool` plus `tls_ca_cert: Option<PathBuf>` for custom CAs | Must | Integration test against self-signed TLS server |
| FR-33 | Ollama backend supports `system` prompt and `options` map (temperature, top_p, num_ctx, keep_alive) | Must | Request body snapshot test |
| FR-33a | Ollama backend supports tool calling for models that advertise it (`/api/chat` with `tools` array). Capability registry declares `tools: true` for Ollama; per-step `tools` definition flows through `StepContext.options` until a typed `tools` field is introduced in Phase 3. The conductor refuses to route a `tools`-using step to a backend whose health-checked model does not list tool support | Should | Request body snapshot includes tools; integration test with a model known to support function calling |
| FR-33b | Ollama backend supports the `think: bool \| { effort: "low" \| "medium" \| "high" }` field for thinking-capable models (DeepSeek-R1, etc.). Plumbed via `StepContext.options.think` until promoted to a typed field. Capability `thinking: true` declared per-model based on the model's metadata from `/api/show` (cached via FR-11) | Should | Request body snapshot with `think: true`; capability test that non-thinking models reject the option with a warning |
| FR-34 | Ollama `keep_alive` defaults to match the step timeout so models unload on lok timeout | Should | Sent on every request |
| FR-35 | Ollama backend classifies "model not found", "no model loaded", "connection refused" into typed `BackendError` variants. Pre-flight model verification (FR-11a) catches "model not found" before the HTTP call where possible | Must | Unit test per error string |
| FR-36 | `done_reason` surfaces as a structured field in `QueryOutput.metadata`. Documented values from current Ollama: `"stop"` (normal), `"length"` (max tokens hit — validator may retry), `"load"` (model unloaded mid-stream — retry on a fresh request). Unknown values pass through verbatim for forward compat | Should | Field added; unit test per known value; validator policy: `length` → retry, `load` → retry, `stop` → success |

### FR Group: Session Hygiene & CI Cleanliness

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-37 | Codex default args include `--ephemeral` (when supported by detected version) | Must | Argv snapshot; `~/.codex/sessions/` unchanged after run |
| FR-37a | Codex backend passes `--ignore-user-config` (Codex ≥ v0.122.0) by default when running under CI (detected via `CI` env var) or when `lok.toml` sets `backends.codex.ignore_user_config = true`. Rationale: CI runs should not inherit a developer's `~/.codex` profile | Should | Argv snapshot when `CI=1`; off when unset and not configured |
| FR-37b | Codex backend passes `--ignore-rules` (Codex ≥ v0.122.0) by default when running under CI or when explicitly configured. Same rationale as FR-37a | Should | Argv snapshot |
| FR-37c | Codex backend passes `--skip-git-repo-check` (Codex ≥ v0.119.0) when the workflow does not require git context (e.g., `lok ask` one-shots). Workflows that consume git state opt-in to the check | Should | Argv snapshot for `lok ask`; not present for `lok run review-pr` |
| FR-38 | Gemini supports optional `session_export` / `session_import` per workflow step | Could | Defer if scope creeps; track behind feature flag |
| FR-39 | Failed runs preserve the JSONL / `-o` last-message file / stderr for the failing step under `.lok/debug/<run-id>/` | Should | Postmortem artifact for support |

### FR Group: Integration Test Fixtures (cross-cutting)

This group exists because every FR above is testable only against a fixture that mirrors current backend output. Without these fixtures, regressions slip in silently.

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|--------------------|
| FR-40 | Repo ships a `tests/fixtures/codex/` directory with at least: a `turn.completed`-final JSONL stream, a `turn.failed` stream, a multi-turn stream with intermediate reasoning items, and a stream missing `agent_message` (older codex). Used to pin FR-3a parsing | Must | `cargo test backend::codex` consumes fixtures |
| FR-41 | `tests/fixtures/gemini/` contains JSON envelope samples for: success with `stats`, success without `stats`, error envelope, malformed JSON (parse-fail path) | Must | `cargo test backend::gemini` consumes fixtures |
| FR-42 | `tests/fixtures/ollama/` contains recorded `/api/chat` responses for: text completion with usage, schema-mode response, tool-calling response, `done_reason: "length"`, `done_reason: "load"`, `done_reason: "stop"`. Mocked via `mockito` or `wiremock` (whichever is already a dev-dep; prefer adding none) | Must | `cargo test backend::ollama` consumes fixtures without a live Ollama instance |
| FR-43 | `tests/fixtures/claude/` contains both API responses (with `ClaudeUsage`) and CLI JSON envelope (FR-6) samples | Must | `cargo test backend::claude` covers both modes |

## 5. Non-Functional Requirements

| Category | Requirement | Target |
|----------|-------------|--------|
| Performance | Schema-mode roundtrip overhead vs text-mode | < 50ms per call (tmpfile + parse) |
| Performance | Health check overhead per `lok` invocation | < 200ms cumulative when CLIs are installed and cached. Gemini's `npx @google/gemini-cli --version` can take 5–15 s on a cold npx cache (first invocation on a fresh machine); health check has a 2 s timeout and reports `available: false, reason: "version probe exceeded 2s; run 'npx @google/gemini-cli --version' once to warm the npx cache"` rather than blocking workflow start |
| Reliability | Schema parse failures retried via existing `RetryExecutor` | Parse → `BackendError::Parse` (non-retryable, surfaces to validator) |
| Backward compat | Existing `lok.toml` and workflows continue to work unmodified | All new fields use `#[serde(default)]` **including nested struct fields** (e.g., inside `headers`, `tls`, `options`, `tools`) — `#[serde(deny_unknown_fields)]` on `BackendConfig` stays, but every newly-added nested struct field must be tested for missing-from-toml round-trip |
| Backward compat | Default behavior matches Phase 1 when no schema/sandbox/model/history is set | Snapshot tests on existing workflows; `StepContext::default()` produces Phase-1-equivalent argv per backend |
| Backward compat | `StepResult.usage` and `StepResult.rendered_prompt` are `Option` so existing JSON consumers of `StepResult` do not break | Serde round-trip test with v2 JSON omits both fields |
| Process I/O | Prompts up to 1 MiB pass through subprocess backends without truncation or shell-quoting errors | Tempfile / stdin path (FR-19d); regression test with binary-clean multi-line input |
| Observability | Token usage available in `lok run --output json` | `usage` populated for ≥ 4 of 5 backends (Codex, Gemini, Ollama, Claude API + Claude CLI once FR-6 lands; Bedrock already covered) |
| Security | New `danger_accept_invalid_certs` defaults false; logs a warning when true | Audit log line on every query when set |
| Disk | No session artifacts written by default for non-interactive runs | Codex `--ephemeral` on; documented for users |

## 6. Scope & Phasing

### In Scope (Phase 2)

- FR Groups above (trait/IO hardening, schema output, real health, capability registry, per-step config, usage observability, Ollama remote, session hygiene + CI cleanliness, integration test fixtures).
- New `lok doctor` output, optional `lok backends` capability matrix.
- Built-in schemas for `hunt`, `audit`, `diff`, `fix`.
- Documentation: `docs/backends/*.md` updated per backend, plus a migration note for `skip_lines` deprecation.

### Out of Scope (reason)

- **Streaming output (`stream-json`)** — Phase 3. Requires changes to `Backend::query` signature (`Result<QueryOutput>` → channel-based). Not blocking; current text/JSON mode is enough for predictability.
- **Persistent Codex/Gemini daemon (app-server, ACP, subagent protocol)** — Phase 3+. Architecture is still evolving in gemini-cli; revisit once stable.
- **MCP server inside lok (bidirectional Lok ↔ Gemini)** — Phase 3+.
- **Gemini HTTPS proxy support** — defer to Phase 3. Gemini CLI honours `HTTPS_PROXY` env var so workflows can set it externally; native lok-level proxy config waits until there is a concrete enterprise consumer beyond the Ollama path.
- **GitHub Action backend type** — separate PRD; not a CLI utility integration.
- **Multimodal / image input for Ollama or Claude** — no workflow demand today.
- **Codex hooks / plugins / goals / memory** — explicitly excluded in [codex-gap-analysis.md §7](../investigations/codex-gap-analysis.md).
- **Model management (pull/delete/push for Ollama)** — not lok's role per [ollama-cli-gap-analysis.md](../investigations/ollama-cli-gap-analysis.md).

### Future Phases

| Phase | Features | Depends On |
|-------|----------|------------|
| Phase 3 | Streaming, persistent daemon, session export/import, MCP server, Gemini proxy | Phase 2 capability registry |
| Phase 4 | Multimodal, embeddings backend trait, RAG-style workflows | Phase 3 streaming + structured output |

## 7. Dependencies

| Dependency | Owner | Status | Risk if Delayed |
|------------|-------|--------|-----------------|
| **Phase 1 verification gate** — confirm CLO-180/181/182/183/184/185 have actually landed on `main` (the memory index claims 10/14; per-task PRD merges must be cross-checked against `git log`) | Lok core | To verify before kickoff | Blocks every FR that depends on `BackendError`, `RetryExecutor`, structured `FailureType` |
| Codex CLI ≥ v0.119.0 on dev/CI machines (`--output-schema`, `-o`, `--ephemeral`, `--skip-git-repo-check`) | Lok user | Available | Schema mode falls back to text; capability registry handles it |
| Codex CLI ≥ v0.122.0 for `--ignore-user-config` / `--ignore-rules` | Lok user | Likely available | CI-cleanliness FRs gate off until version probe confirms |
| gemini-cli ≥ v0.42.0 | Lok user | Available | Same fallback path |
| Ollama ≥ v0.24.0 | Lok user | Available | `format: schema` falls back to plain JSON mode |
| `serde_json::Value` plumbed in `QueryOutput.structured` | Phase 1 (llm-mux port) | Field exists, unused | None |

## 8. Risks & Open Questions

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Codex `--output-schema` silently ignored when MCP servers active (Codex issue #15451) | L (lok runs `-s read-only`, no MCP) | M | Validator step double-checks schema conformance; documented |
| Codex JSONL event names change in a future release | M | M | FR-3b's `--output-last-message` provides a stable extraction path independent of event names; JSONL is consumed only for usage + intermediate failures |
| Gemini intermediate messages also match schema (Codex issue #19816 analog) | M | M | Consume only the last schema-valid event from JSONL stream |
| `--output-format json` not on older gemini-cli installs | M | M | Version detection + capability registry fall back to text mode |
| Ollama `format: <schema>` not in older servers | L (v0.24 widespread) | M | Same fallback; `BackendError::Config` if schema strictly required |
| Adding `health_check` to `Backend` trait breaks downstream implementations | L (single repo) | L | Default trait impl returns `Available` for backward compat |
| Per-step sandbox switching surprises users who rely on global default | L | M | Documented; `lok run --dry-run` shows resolved sandbox per step |
| `danger_accept_invalid_certs` misused in production | M | H | Logged warning on every use; doc note |
| Capability matrix becomes a maintenance burden as CLIs evolve | M | L | Version-aware computation, not static table |
| Trait signature churn (FR-19a/19b) cascades through every backend at once **and every caller** (conductor, spawn, team, debate, run_query_with_config) | H | M | Single coordinated PR for trait + all backends + all callers; existing tests pin behavior; FR-20 audit-grep guards against regressions |
| Conversation history injection blows the context window for long chains | M | M | Per-step `history_window` config; default conservative (last N steps); validator emits a warning when injected history > 50% of model context |
| Tempfile-for-prompt leaks if subprocess hangs past timeout | L | L | RAII guard on tempfile; explicit cleanup in `Drop` and on timeout path |
| `warmup_backends()` adds startup latency when a backend is unreachable | M | L | Per-backend warmup is parallel + has its own timeout (default 2s); failure marks backend `available: false` and continues |
| Gemini `npx` cold start (5–15 s) breaks the 200 ms health-budget on first run | M | L | 2 s probe timeout + actionable error suggests warming the npx cache; warmup falls through with `available: false` rather than blocking |
| `StepResult` field additions break downstream serde consumers (rs-wisper reads `StepResult` over JSON) | M | M | New fields are `Option` with `#[serde(default)]`; integration test with rs-wisper consumer pins back-compat |

### Open Questions

Each question has a deadline keyed to the FR that unblocks once it's answered. Recommendation listed first; user can override.

- [ ] Should `QueryOutput.structured` stay `Option<serde_json::Value>` or move to a typed `enum StructuredOutput { Issues(Vec<Issue>), ... }`? **Recommendation: keep `serde_json::Value`** — the typed enum couples backends to task definitions and breaks the open-world schema model. Owner: MK. Deadline: before FR-1 PR opens (target 2026-05-21).
- [ ] Where do JSON Schemas live: `schemas/` directory or inline in `tasks/*.rs`? **Recommendation: `schemas/<task>.json`** as plain files, loaded at startup; keeps schemas language-agnostic and reviewable by non-Rust readers. Owner: MK. Deadline: before FR-8 (target 2026-05-24).
- [ ] Should `Backend::health_check` be on the trait or a free function that takes `&dyn Backend`? **Recommendation: trait method** — each backend's health is backend-specific (Ollama HTTP, Codex `--version`, Claude dual-mode). Owner: MK. Deadline: before FR-9 (target 2026-05-21).
- [ ] How does the conductor pick a fallback backend when the preferred one lacks `schema` capability — fail fast or downgrade to text mode? **Recommendation: fail fast in workflow runs** (FR-17) **with an explicit `--allow-capability-downgrade` flag** for ad-hoc `lok ask`. Owner: MK. Deadline: before FR-17 (target 2026-05-28).
- [ ] Should `lok doctor` block (refuse to proceed) or warn when a backend is unhealthy? **Recommendation: warn for `lok run`, exit non-zero for `lok doctor` itself**. Owner: MK. Deadline: before FR-14 (target 2026-05-28).
- [ ] What is the default `history_window` for multi-turn injection — all prior steps in the dependency chain, or last N? **Recommendation: last 3 steps in the topological chain, capped at 25% of model context**. Owner: MK. Deadline: before FR-19c (target 2026-05-21).
- [ ] Where does `StepContext` live in code — `src/backend/context.rs`, or alongside `Step` in `src/config.rs`? **Recommendation: `src/backend/context.rs`** — `Step` is config, `StepContext` is runtime construction. Owner: MK. Deadline: before FR-19a (target 2026-05-21).
- [ ] Should `Backend::query` becoming async use `async_trait` or Rust 1.75+ `impl Trait` in traits? **Recommendation: `async_trait`** — already a project dep, dyn-trait compatible (no boxed-future churn at call sites). Owner: MK. Deadline: before FR-19b (target 2026-05-21).
- [ ] Should tempfile-based prompt delivery use `tempfile::NamedTempFile` (cleaned on drop) or piped stdin? **Recommendation: piped stdin where the CLI accepts it (Codex, Gemini, Claude CLI all do), tempfile fallback only if a CLI does not**. Stdin avoids tempfile leak risk entirely. Owner: MK. Deadline: before FR-19d (target 2026-05-21).
- [ ] `--ignore-user-config` / `--ignore-rules` default-on under `CI=1`, or always opt-in? **Recommendation: default-on under `CI=1`, configurable via `backends.codex.ignore_user_config`** — matches the principle that CI runs should be reproducible. Owner: MK. Deadline: before FR-37a (target 2026-05-28).
- [ ] Should `Step.include_directories` (FR-24a) be Gemini-only or generalised across backends? **Recommendation: Gemini-only for Phase 2**; generalise in Phase 3 if Codex/Claude grow an equivalent. Owner: MK. Deadline: before FR-24a (target 2026-05-24).
- [ ] `cached_tokens` and `reasoning_tokens` semantics for cross-backend aggregation: when summing across a workflow that uses both Codex (reports `reasoning_output_tokens`) and Anthropic (reports `cache_read_input_tokens`), do we display them under one heading or separately? **Recommendation: separately in verbose mode** (`reasoning: N, cached: M`), `total_tokens` continues to mean `prompt + completion`. Owner: MK. Deadline: before FR-28 (target 2026-06-04).

## 9. Rollout & Measurement

### Release Plan

- **Sequencing.** Land FRs in the order:
  1. **Phase 1 verification gate** (no FR; confirm landed work).
  2. **Trait & I/O hardening (FR-19a–19f) including `StepResult.usage` (FR-25a)** — every later group consumes the new `StepContext` signature, async query, and `StepResult` shape.
  3. **`TokenUsage` extension (FR-25b)** — needed before Codex/Anthropic usage extraction.
  4. **Health checks + warmup (FR-9, FR-9a, FR-10–15a, FR-11a, FR-13a)**.
  5. **Capability registry (FR-16–19)**.
  6. **Per-step config plumbing (FR-20–24a)**, including the call-site sweep.
  7. **Ollama remote/options/tools/think (FR-31–36)**.
  8. **Schema output per backend (FR-1–8, FR-3a, FR-3b)**.
  9. **Usage extraction (FR-25–FR-30)**.
  10. **Session hygiene + CI cleanliness (FR-37–FR-39)**.
  11. **Integration test fixtures (FR-40–43)** — actually land alongside whichever FR consumes them; listed last only because they appear cross-cutting.

  Each group ships as its own Linear task / PR; the order keeps every PR independently mergeable.
- **Feature flags.** None at the backend level — these are additive config fields. A `LOK_FORCE_TEXT_MODE=1` escape hatch covers users hitting unknown schema bugs.
- **Migration.** `skip_lines` stays usable but is deprecated with a single-warning log; remove in Phase 3.
- **Rollback.** Each FR group reverts cleanly; tests pin Phase 1 behavior when new config fields are absent.

### Measurement Plan

- **Per-PR.** Argv snapshot tests, request-body snapshot tests, integration tests against fixtures (FR-40–43) and against real Codex / Gemini / Ollama instances where available.
- **First end-to-end check.** After capability registry + health checks land, run the rs-wisper review pipeline against a known PR; compare structured-output rejection rate to Phase 1 baseline.
- **Decision points.**
  - If schema-mode parse failures > 2% after FR-1–8 ship, pause and investigate (likely model-specific or schema-specific).
  - If health checks add > 200ms to `lok ask` on machines with warm npx cache, move them behind an explicit `--check` flag.
  - If capability-aware routing causes user confusion (workflows that "worked" now refuse to run), add a `--allow-capability-downgrade` flag and surface in `lok doctor`.

## 10. Appendix: Source-to-Requirement Map

| Investigation finding | Maps to FR |
|-----------------------|-----------|
| Codex `--output-schema` unused (codex §3.1) | FR-1, FR-3, FR-8 |
| Codex `--ephemeral` unused (codex §3.4) | FR-37 |
| Codex `-o` / `--output-last-message` unused (codex-quick-ref) | **FR-3b** |
| Codex `--ignore-user-config` / `--ignore-rules` unused (codex-quick-ref) | **FR-37a, FR-37b** |
| Codex `--skip-git-repo-check` unused (codex-quick-ref) | **FR-37c** |
| Codex usage in `turn.completed` ignored (codex §3.2) | FR-25, FR-25a (conductor copy), FR-25b (cached + reasoning) |
| Codex sandbox hardcoded `read-only` (codex §5.2) | FR-21, FR-22 |
| Codex `step.model` not threaded (codex §5.6) | FR-20 |
| Codex JSONL substring matching (codex §5.1) | FR-3, **FR-3a**, FR-7 |
| Gemini shell pipe + `skip_lines` (gemini §4.1, §6.7) | FR-4 |
| Gemini multi-line prompt fragility (gemini §6.4) | **FR-19d** |
| Gemini exit-code classification (gemini §4.3) | FR-9 (folded into health), Phase 1 `BackendError` |
| Gemini version detection (gemini §6.1) | FR-12 |
| Gemini auth detection (gemini §6.3) | FR-12 |
| Gemini `--include-directories` (gemini §6.5) | **FR-24a** (promoted from options passthrough) |
| Gemini `npx` cold start latency | NFR row + risks |
| Ollama `is_available()` lies + sync trait method (ollama-api §3.1) | FR-9, **FR-9a**, FR-10, FR-11 |
| Ollama no headers / TLS (ollama-cli §1.1, §1.3) | FR-31, FR-32 |
| Ollama no system prompt / options (ollama-cli §1.1, ollama-api §2) | FR-33 |
| Ollama no tool calling (ollama-api §1, §3) | **FR-33a** (with capability gating via FR-16, FR-17) |
| Ollama no `think` plumbing | **FR-33b** |
| Ollama single-turn only / drops history (ollama-api §1) | **FR-19c** |
| Ollama hardcoded `llama3.2` fallback / no model verification (ollama-api §3.4, §5) | **FR-11, FR-11a** |
| Ollama no `keep_alive` coordination (ollama-api §3.3) | FR-34 |
| Ollama `format` (schema) unused (ollama-api §1) | FR-5 |
| Ollama usage already extracted in code | FR-27 (regression pin) + FR-25a (conductor copy) |
| Ollama error classification (ollama-api §5.1) | FR-35 |
| Ollama `done_reason` beyond `length` (`load`, `stop`) | **FR-36** |
| Claude CLI hardcodes `--output-format text` (`src/backend/claude.rs:218`) | **FR-6** (revised: switch to JSON envelope) |
| Claude dual-mode capabilities (`ClaudeMode::{Api, Cli}`) | **FR-13a, FR-16** (per-mode capability sets) |
| Claude API usage already extracted (`src/backend/claude.rs:194`) | FR-27a (regression pin) + FR-25a (conductor copy) |
| Bedrock usage already extracted (`src/backend/bedrock.rs:199`) | FR-27a (regression pin) + FR-25a (conductor copy) |
| `BackendConfig` `deny_unknown_fields` trap (ollama-cli) | NFR row on serde(default) for nested structs |
| All backends: no capability advertising (ollama-api §5.3, gemini §5.5, codex §4.3) | FR-16–19 |
| All backends: `Backend::query` takes bare strings, no `StepContext` or history | **FR-19a, FR-19b, FR-19c** |
| All non-workflow `Backend::query` callers pass `None` for model | FR-20 (call-site sweep enumerated) |
| `StepResult` has no `usage` field (`src/workflow.rs:897`) | **FR-25a** |
| `TokenUsage` missing `cached_tokens` / `reasoning_tokens` (`src/backend/mod.rs:105`) | **FR-25b** |

## 11. Pre-Implementation Code Review Checklist

Before any Linear task is created or implementation begins, walk the following files and confirm the assumptions encoded in this PRD still hold. Findings that contradict the PRD must be reconciled here first — do **not** silently adjust requirements during implementation.

### 11.0 Phase 1 Verification Gate
- [ ] Confirm via `git log --oneline --grep CLO-18` that CLO-180, CLO-181, CLO-182, CLO-183, CLO-184, CLO-185 are merged. The memory index claims 10/14 done; confirm which 10 against `git log` before relying on `BackendError`, `RetryExecutor`, `FailureType`, validation pipeline.
- [ ] If any blocker is unmerged, list it and the FR(s) it blocks. Do not start Phase 2 implementation until the named blockers are landed.

### 11.1 `src/backend/mod.rs` — Trait Signatures & Token Usage
- [ ] **Confirm current `Backend::query` signature**: `async fn query(&self, prompt: &str, cwd: &Path, model: Option<&str>) -> Result<QueryOutput, BackendError>`. Plan the exact diff to take `StepContext` (FR-19a, FR-19b).
- [ ] **Confirm current `is_available` is sync `fn is_available(&self) -> bool`.** Today `OllamaBackend` returns `true` unconditionally — refactor cost is real. Plan FR-10 cache-only behavior.
- [ ] **Confirm the five concrete implementors**: `CodexBackend`, `GeminiBackend`, `OllamaBackend`, `ClaudeBackend` (dual `ClaudeMode::{Api, Cli}`), `BedrockBackend` (feature-gated behind `bedrock` cargo feature). Each must migrate in lockstep when the trait changes.
- [ ] **Confirm `TokenUsage` current shape** (`src/backend/mod.rs:105`): `{ prompt_tokens, completion_tokens, total_tokens }`. Plan FR-25b extension with `cached_tokens: Option<u32>` and `reasoning_tokens: Option<u32>`.
- [ ] **Confirm `QueryOutput` current shape** (`src/backend/mod.rs:158`): already has `usage: Option<TokenUsage>` and `structured: Option<serde_json::Value>`. No new fields needed at this layer.
- [ ] **Audit `run_query_with_config`** (`src/backend/mod.rs:381`): currently `backend.query(&prompt, &cwd, None)`. Plan FR-20 fix to thread `step.model` (or the backend default) instead of hardcoded `None`.

### 11.2 `src/config.rs` — Backend / Step / Workflow Config
- [ ] **Confirm `BackendConfig` uses `#[serde(deny_unknown_fields)]`** (verified at `src/config.rs:102`). Current fields: `enabled`, `command`, `args`, `skip_lines`, `api_key_env`, `model`, `timeout`, `max_retries`, `retry_delay_ms`. New fields (`schema`, `headers`, `tls_ca_cert`, `danger_accept_invalid_certs`, `options`, `keep_alive`, `tools`, `output_format`, `ignore_user_config`, `ignore_rules`) need `#[serde(default)]` and explicit round-trip tests.
- [ ] **Confirm `Step` config struct** (not `WorkflowStep` — there is no `WorkflowStep` in this codebase). Locate at `src/workflow.rs:200`. Identify whether it currently carries `model`, `sandbox`, `timeout`, `schema`, `tools`, `history_window`, `include_directories`. Plan additive fields; do not rename existing ones.
- [ ] **Audit nested structs** (`OptionsMap`, any `TlsConfig`, any `HeadersMap`). Each nested struct needs the same `serde(default)` discipline — verify with a deliberate "minimum TOML" parse test.

### 11.3 `src/workflow.rs` — `Step`, `StepResult`, Conductor Loop, History
- [ ] **Locate `Step`** (`src/workflow.rs:200`). Confirm fields. Plan additive `model`, `sandbox`, `schema`, `history_window`, `tools`, `include_directories`.
- [ ] **Locate `StepResult`** (`src/workflow.rs:897`). Confirmed shape: `{ name, output, parsed_output, success, elapsed_ms, backend, raw_output, stderr, exit_code, validation, failure }`. **Missing `usage` (FR-25a) and `rendered_prompt` (FR-19f)** — both fields must be added before usage observability or multi-turn history can function.
- [ ] **Confirm the conductor's step iteration loop.** Identify the six known `Backend::query` call sites in workflow.rs (lines 767, 778, 1737, 1943, 2041, 2171) plus the non-workflow callers (conductor.rs:186, spawn.rs:173+282, team.rs:82+121, debate.rs:230, mod.rs:381). The migration must update all of them in one PR per FR-20.
- [ ] **Identify the rule for "which prior steps' history is in scope".** Default proposal: the linear chain of completed steps that feed the current step's `depends_on`, last 3 steps, capped at 25% of model context. Confirm or adjust before FR-19c lands.

### 11.4 `src/backend/codex.rs` — JSONL Stream Parsing + Flag Support
- [ ] **Locate the existing `line.contains("\"agent_message\"")` substring match.** Document every event type currently observed in production JSONL streams (capture a real stream into a fixture per FR-40).
- [ ] **Plan the event-driven parser** (FR-3a): a `serde`-deserialized enum over `{thread.started, turn.started, turn.completed, turn.failed, item.started, item.completed, error}`, accumulating items per turn, returning the last turn's `agent_message`. Reasoning items must not feed `QueryOutput.structured`.
- [ ] **Plan `--output-last-message`** (FR-3b): pass `-o <tmpfile>` and read it as the authoritative result on success; fall back to FR-3a JSONL extraction only when the file is absent (older Codex).
- [ ] **Confirm `turn.completed.usage`** field shape on current Codex (`input_tokens`, `cached_input_tokens`, `output_tokens`, `reasoning_output_tokens`); populate FR-25 via the extended `TokenUsage` from FR-25b.
- [ ] **Plan CI-cleanliness flags** (FR-37a–c): wire `--ignore-user-config`, `--ignore-rules`, `--skip-git-repo-check` conditional on Codex version (≥ v0.122.0 for the first two, ≥ v0.119.0 for the third) and on `CI` env / config opt-in.

### 11.5 `src/backend/gemini.rs` — Subprocess I/O
- [ ] **Locate the current `Command::new("npx")` invocation** (verified). Confirm whether the prompt is passed via `.arg(prompt)`, piped to stdin, or written to a file. Multi-line / quote-bearing prompts must move to stdin/tempfile per FR-19d.
- [ ] **Locate `skip_lines = 1` usage** and confirm the call site that strips the first line of stdout. Replace with `--output-format json` envelope parsing (FR-4).
- [ ] **Confirm `npx @google/gemini-cli` is the invocation form** today; plan whether health check uses `npx ... --version` directly (subject to npx cold-start) or shells through a configurable command path. Plan the 2 s timeout NFR.
- [ ] **Plan `--include-directories` plumbing** (FR-24a) from typed `Step.include_directories` to argv.

### 11.6 `src/backend/ollama.rs` — HTTP Client & Body
- [ ] **Locate the `reqwest::Client` construction.** Confirm it currently has no header injection / TLS config; plan FR-31, FR-32.
- [ ] **Locate the `/api/chat` request body.** Confirm it sends only `{model, messages: [{role: "user", content: prompt}]}` today; plan addition of `system`, `options`, `format`, `keep_alive`, `tools`, `think` (FR-5, FR-19c, FR-33, FR-33a, FR-33b, FR-34).
- [ ] **Confirm `is_available` body** is `Ok(true)` (or similar). Plan FR-11 / FR-11a swap.
- [ ] **Confirm `prompt_eval_count` / `eval_count` extraction at `src/backend/ollama.rs:155`** still functions; FR-27 is a regression pin, FR-25a (the conductor copy into `StepResult.usage`) is the new work.
- [ ] **Plan `done_reason` field** (FR-36): currently in response; surface as `QueryOutput.metadata["done_reason"]` and document `"stop" | "length" | "load" | <unknown>` semantics.

### 11.7 `src/backend/claude.rs` — Dual-Mode Capabilities + JSON Envelope
- [ ] **Confirm `ClaudeMode` enum** (`src/backend/claude.rs:16`): `Api { api_key, model, client }` vs `Cli { command, model }`. Capabilities and health checks (FR-13a, FR-16) must be computed per-variant.
- [ ] **Locate the CLI `--output-format text` hardcode** (`src/backend/claude.rs:218`). Plan FR-6 switch to `--output-format json` and parsing the envelope for both the text response and `usage`.
- [ ] **Confirm Claude API usage extraction** at `src/backend/claude.rs:194` (already populates `with_usage(ClaudeUsage)`). FR-27a is a regression pin.

### 11.8 `src/backend/bedrock.rs` — Real Backend, Already-Populated Usage
- [ ] **Confirm Bedrock backend exists at `src/backend/bedrock.rs`** and is feature-gated behind the `bedrock` cargo feature. It is **not** speculative — `BedrockBackend` is a real implementor of `Backend`.
- [ ] **Confirm `with_usage` populated at `src/backend/bedrock.rs:199`.** FR-27a regression pin applies.
- [ ] **Confirm capability matrix entry for Bedrock**: `schema: <depends on model>`, `multi_turn: true`, `streaming: <future>`, `tools: <depends on model>`. Compute from health check, not static.

### 11.9 Cross-Cutting
- [ ] **Enumerate every call site of `Backend::query`** (verified list): `src/backend/mod.rs:381`, `src/conductor.rs:186`, `src/spawn.rs:173`, `src/spawn.rs:282`, `src/team.rs:82`, `src/team.rs:121`, `src/debate.rs:230`, plus workflow.rs:767, 778, 1737, 1943, 2041, 2171. Plan the migration: one PR for the trait + all backends + all callers (FR-19a–b + FR-20). Grep test in CI to assert no `query(..., None)` literal in `src/` outside test fixtures.
- [ ] **Confirm Phase 1 `BackendError` variants** cover the new failure modes (parse, model-not-found, auth-missing, version-mismatch). Extend if necessary; coordinate with rs-wisper consumers.
- [ ] **Locate `main.rs` / engine entry points** where `warmup_backends()` (FR-9a) will be called. Confirm there is exactly one entry per command path (`run`, `ask`, `doctor`, `backends`).
- [ ] **Confirm `StepResult` is serialised over JSON to rs-wisper or any external consumer.** If yes, the new `usage` and `rendered_prompt` fields must be `Option` with `#[serde(default)]` (already required by the NFR back-compat row) and the rs-wisper side must tolerate them.

The output of this checklist is a short reconciliation note (per file, one bullet: "matches PRD" / "PRD needs update because X"). That note becomes the gate between this PRD and the Linear-task breakdown.
