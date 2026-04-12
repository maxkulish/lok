# Spec: Extend QueryOutput with model, duration, usage, structured, backend

**Created**: 2026-04-11
**Linear**: CLO-207
**Estimated scope**: M (8 files, ~10 sub-tasks)
**Review verdict**: APPROVE_WITH_SUGGESTIONS (Gemini + Ollama, 2026-04-11)

## 1. Problem Statement

lok's `QueryOutput` struct in `src/backend/mod.rs:101-105` only carries `stdout`, `stderr`, and `exit_code`. Five fields that llm-mux's `BackendResponse` provides are missing:

1. **`model`** - which model actually responded (may differ from requested due to routing, fallbacks, or provider aliases)
2. **`duration`** - wall-clock time for the query (currently measured separately by `run_query_with_config` as `QueryResult.elapsed_ms` but not available on the output itself)
3. **`usage`** - token counts for cost tracking (prompt/completion/total)
4. **`structured`** - parsed JSON if the response is structured output
5. **`backend`** - which backend produced this output (currently only available on the outer `QueryResult.backend` via the caller loop)

**What's broken**: Downstream consumers (`src/workflow.rs` at 782, 1186, 1949, 2177) extract `qo.stdout` / `qo.stderr` / `qo.exit_code` only. Cost tracking is impossible. Actual model accountability is impossible. Structured output requires a post-hoc `extract_json_from_text` call. And the caller has to separately remember which backend produced which `QueryOutput` rather than the output carrying that identity.

**Who's affected**: 
- Cost tracking code (future) cannot observe token usage per step
- Debugging/observability cannot attribute outputs to models at the response level (today, if Claude API fallback returns a different model, lok has no signal)
- `src/workflow.rs:2175-2182` loses per-step duration data at the backend boundary
- `src/template/context.rs:61` calls `workflow::extract_json_from_text` at render time instead of reading a pre-parsed field

**What triggers it**: Every successful backend query. The fields must be populated at the point of query return so they survive through `QueryResult` construction and into step execution.

**Why it matters**: This task is the foundation for observability and cost-tracking features listed in `docs/prds/prd-llm-mux-port.md:35-66` (Phase 2 of the llm-mux port). CLO-207 is the natural extension of CLO-180 (which introduced `QueryOutput`) and the final Phase 2 port task. It also unblocks future cost-aware routing work (not yet ticketed).

**Source**: `docs/prds/prd-llm-mux-port.md:35-66` (Phase 2: Richer QueryOutput), mirroring llm-mux's `BackendResponse`.

## 2. Acceptance Criteria

- [ ] `QueryOutput` struct in `src/backend/mod.rs` has 5 new public fields: `model: Option<String>`, `duration: Duration`, `usage: Option<TokenUsage>`, `structured: Option<serde_json::Value>`, `backend: String`
- [ ] `TokenUsage` struct exists in `src/backend/mod.rs` with `prompt_tokens: u32`, `completion_tokens: u32`, `total_tokens: u32` (all public)
- [ ] `TokenUsage::new(prompt, completion)` constructor computes `total_tokens = prompt + completion` (consistency guarantee)
- [ ] `QueryOutput::from_text(text, backend, duration)` signature requires `backend: impl Into<String>` and `duration: Duration` (enforces "always populated")
- [ ] `QueryOutput::from_process(stdout, stderr, exit_code, backend, duration)` signature requires `backend: impl Into<String>` and `duration: Duration`
- [ ] `QueryOutput::with_model(self, model: Option<impl Into<String>>) -> Self` builder setter (accepts `Option` so chaining with API response `Option<String>` compiles without `if let` guards)
- [ ] `QueryOutput::with_usage(self, usage: Option<TokenUsage>) -> Self` builder setter
- [ ] `QueryOutput::with_structured(self, structured: Option<serde_json::Value>) -> Self` builder setter (consumer-populated; constructors do NOT auto-parse — see constraints)
- [ ] Constructors default `structured` to `None` — no implicit parsing (avoids markdown-fenced false negatives and duplication with `workflow::extract_json_from_text`)
- [ ] `ClaudeBackend` (API mode) populates `model` from response JSON and `usage` from response `usage.input_tokens`/`usage.output_tokens`
- [ ] `ClaudeBackend` (CLI mode) populates `model` from the effective model (override first, else configured `default_model`), leaves `usage` as `None`
- [ ] `GeminiBackend` (CLI) populates `model` when `model_override` is provided (effective model), leaves `usage` as `None`
- [ ] `OllamaBackend` populates `model` from response `model` field with fallback to effective requested model when API omits it; `usage` from `prompt_eval_count`/`eval_count` when both present
- [ ] `CodexBackend` populates `model` when `model_override` is provided (effective model), leaves `usage` as `None`
- [ ] `BedrockBackend` populates `model` from response JSON with fallback to configured model ID; `usage` from Anthropic-on-Bedrock response `usage` block (feature-gated behind `bedrock` feature)
- [ ] `duration` is populated by each backend measuring `Instant::now()` at function entry and `start.elapsed()` at return
- [ ] `backend` is populated via `self.name()` at each backend's construction site
- [ ] `RetryExecutor::query` passes through `QueryOutput` from inner backend unchanged (duration reflects the successful attempt, not cumulative retry time)
- [ ] All existing unit tests in `src/backend/mod.rs::tests` that call `from_text`/`from_process` are updated to pass `backend` + `duration` arguments
- [ ] `src/backend/retry.rs` test mocks updated to new constructor signature
- [ ] `cargo test` passes with no failures
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo build --features bedrock` compiles (feature-gated bedrock path)

**Verification method**: `cargo test && cargo clippy -- -D warnings && cargo build --features bedrock`

## 3. Constraints

**Must**:
- `QueryOutput` keeps existing public fields (`stdout`, `stderr`, `exit_code`) unchanged - this is additive
- `duration` is a `std::time::Duration`, not a `u64` millis - preserves full resolution and matches llm-mux's `BackendResponse.duration` type
- `TokenUsage` uses `u32` for all three counts - sufficient for any realistic LLM context (`u32::MAX` = ~4 billion tokens)
- `TokenUsage::new(prompt, completion)` computes total via `prompt_tokens.saturating_add(completion_tokens)` to avoid overflow panic on pathological inputs
- `backend` field is owned `String` (not `&'static str`), to allow multi-tenant or renamed backends without lifetime gymnastics
- `structured` is populated explicitly by callers via `with_structured()` builder - constructors leave it `None`. Callers that want structured output should invoke `workflow::extract_json_from_text(&output.stdout)` and pass the result through
- Builder methods accept `Option<...>` so they compose with API response fields (`response.model` is already `Option<String>`) without `if let Some(...)` guards at every call site
- Each backend measures duration with `let start = Instant::now();` at the first line of `query()` and `start.elapsed()` at return. Error paths return `BackendError`, not `QueryOutput`, so only success path needs duration
- `RetryExecutor` returns the successful inner attempt's `QueryOutput` unmodified (`self.inner.query(...).await?`). Because each inner backend measures its own duration, the final `QueryOutput.duration` reflects only the last successful attempt, NOT cumulative retry time. This is intentional and documented in the module-level doc comment
- Effective model semantics: `model` field on `QueryOutput` reflects what was actually requested by the caller. If `model_override` is `Some`, use that; otherwise use the backend's configured default. This applies to ALL backends (including CLI backends like Codex/Gemini) when caller provides an override — the spec previously said "leave as None", but this contradicts the "effective model" invariant

**Must-not**:
- Do not implement `Default` for `QueryOutput` — the `backend` and `duration` fields are always-populated invariants, and a `Default` of `backend = ""` / `duration = Duration::ZERO` would undermine them. Force callers to use `from_text` / `from_process` constructors
- Do not auto-parse `structured` in constructors. Two reasons: (a) naive `serde_json::from_str(stdout.trim())` fails on markdown-fenced JSON (```json ... ```) which is the common case for CLI backends, leaving `structured = None` and defeating the purpose, and (b) `workflow.rs` and `template/context.rs` already do extraction via `extract_json_from_text` at consumption time, and duplicating that logic in the constructor would create two extraction paths
- Do not change `QueryResult` struct (that's separate from `QueryOutput` and not in scope) — `QueryResult.elapsed_ms` remains as wall-clock timing measured by `run_query_with_config` around the entire task spawn (which includes backend query plus task overhead). The new `QueryOutput.duration` is the backend's internal measurement. These may differ by a few milliseconds; both have value. Document this coexistence in the `QueryOutput` doc comment
- Do not migrate downstream consumers (`workflow.rs`, `template/context.rs`) to read the new `structured` / `model` / `usage` / `duration` fields in this task. The fields will be populated but unused — that's acceptable. Consumer migration is a separate follow-up (no Linear ticket yet), and mixing it in expands scope beyond "extend struct"
- Do not add a `duration` field to `BackendError` (errors have `Timeout { elapsed_ms }` already for that case)
- Do not compute `duration` inside `RetryExecutor` as cumulative — backends own their own measurement
- Do not move `extract_json_from_text` out of `workflow.rs` — it stays put. Consumers continue to call it explicitly
- Do not introduce a new dependency — all parsing uses existing `serde_json` already in Cargo.toml

**Prefer**:
- Builder pattern (`with_model`, `with_usage`) over direct field assignment - clearer in call sites and avoids struct literal init with all 8 fields
- `impl Into<String>` on `backend` parameter - accepts `&str`, `String`, `Cow<str>` uniformly (matches ergonomic lok conventions in utils.rs)
- `TokenUsage::default()` derives all-zero - useful for test fixtures
- Keep from_text/from_process as the primary constructors - they encode "is this API or CLI output" in their names and are already well-understood
- For backends that receive `model` from the user as `Option<&str>` override, populate `QueryOutput.model` with the EFFECTIVE model (override if present, else config default) so the output reflects what was actually requested

**Escalate when**:
- A backend's response JSON doesn't match expected shape (new provider API revision) - add a comment and fall back to `None` rather than failing the query
- `TokenUsage` overflow is observed in practice (unlikely) - revisit to `u64`
- A backend needs its own usage struct (e.g., bedrock reports cache-read tokens separately) - add per-backend helper but flatten to the common `TokenUsage` for the public API

## 4. Decomposition

1. **Define `TokenUsage` struct + `QueryOutput` new fields** - files: `src/backend/mod.rs`
   - Add `TokenUsage` struct with `prompt_tokens`, `completion_tokens`, `total_tokens` (all `u32`, all `pub`)
   - Implement `TokenUsage::new(prompt, completion)` using `saturating_add`
   - Derive `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`
   - Add 5 new fields to `QueryOutput`: `model: Option<String>`, `duration: Duration`, `usage: Option<TokenUsage>`, `structured: Option<serde_json::Value>`, `backend: String`
   - Do NOT derive `Default` on `QueryOutput` (backend/duration are always-populated invariants)
   - Update `from_text(text, backend, duration)` signature - defaults `structured` to `None`
   - Update `from_process(stdout, stderr, exit_code, backend, duration)` signature - defaults `structured` to `None`
   - Add builder methods: `with_model(self, model: Option<impl Into<String>>) -> Self`, `with_usage(self, usage: Option<TokenUsage>) -> Self`, `with_structured(self, structured: Option<serde_json::Value>) -> Self`
   - Update module-level doc comment explaining relationship between `QueryOutput.duration` and `QueryResult.elapsed_ms`

2a. **Update ClaudeBackend - API mode** - files: `src/backend/claude.rs`
   - Extend `ClaudeResponse` struct: add `model: Option<String>` and `usage: Option<ClaudeUsage>` fields (both optional to survive API revisions)
   - Add private `ClaudeUsage { input_tokens: u32, output_tokens: u32 }` struct with serde derive
   - In `query_api`: `let start = Instant::now();` at top, after successful response: `QueryOutput::from_text(text, "claude", start.elapsed()).with_model(response.model).with_usage(response.usage.map(|u| TokenUsage::new(u.input_tokens, u.output_tokens)))`
   - Add test: `test_claude_response_deserialize_with_usage` - serde parse of literal JSON with usage block

2b. **Update ClaudeBackend - CLI mode** - files: `src/backend/claude.rs`
   - In `query_cli`: `let start = Instant::now();` at top; compute `effective_model = model_override.or(config.default_model)`; `QueryOutput::from_process(stdout, stderr, exit_code, "claude", start.elapsed()).with_model(effective_model)`
   - No `usage` (CLI mode doesn't report tokens)

3. **Update OllamaBackend** - files: `src/backend/ollama.rs`
   - Extend `ChatResponse` struct: add `model: Option<String>`, `prompt_eval_count: Option<u32>`, `eval_count: Option<u32>` fields
   - Refactor `chat()` to return `QueryOutput` (was `String`) OR return a tuple with metadata — simpler to change signature to `Result<QueryOutput, BackendError>`
   - Use existing `let start = Instant::now();` at line 82 for `start.elapsed()` measurement
   - Build: `QueryOutput::from_text(text, "ollama", start.elapsed()).with_model(resp.model.or_else(|| Some(effective_model.to_string()))).with_usage(usage_opt)` where `usage_opt = resp.prompt_eval_count.zip(resp.eval_count).map(|(p, c)| TokenUsage::new(p, c))`
   - Fallback model to effective requested model if API omits it (mirrors Bedrock fallback pattern)
   - Add test: `test_ollama_response_deserialize_with_counts`

4. **Update BedrockBackend** - files: `src/backend/bedrock.rs`
   - Extend `BedrockResponse` struct: add `model: Option<String>` and `usage: Option<BedrockUsage>` fields
   - Add `BedrockUsage { input_tokens: u32, output_tokens: u32 }` (Anthropic-on-Bedrock format, feature-gated)
   - In `query()`: `let start = Instant::now();` at top; after `invoke_with_messages_model`, build `QueryOutput::from_text(text, "bedrock", start.elapsed()).with_model(response.model.or_else(|| Some(effective_model_id.to_string()))).with_usage(response.usage.map(|u| TokenUsage::new(u.input_tokens, u.output_tokens)))`
   - Feature-gated by existing `#[cfg(feature = "bedrock")]`
   - Add test: `test_bedrock_response_deserialize_with_usage` (feature-gated)

5. **Update GeminiBackend** - files: `src/backend/gemini.rs`
   - In `query()`: `let start = Instant::now();` at top; compute effective model from override/config; `QueryOutput::from_process(stdout, stderr, exit_code, "gemini", start.elapsed()).with_model(effective_model.map(String::from))`
   - `usage` stays `None` (CLI backend, no metadata)
   - Populate effective model when `model_override` is `Some` to match the invariant

6. **Update CodexBackend** - files: `src/backend/codex.rs`
   - In `query()`: `let start = Instant::now();` at top; compute effective model from override/config; `QueryOutput::from_process(stdout, stderr, exit_code, "codex", start.elapsed()).with_model(effective_model.map(String::from))`
   - `usage` stays `None`
   - Populate effective model when `model_override` is `Some` to match the invariant

7. **Update RetryExecutor mock + pass-through** - files: `src/backend/retry.rs`
   - `RetryExecutor::query` passes through inner `QueryOutput` unchanged — no code change, just confirm compile
   - Update `MockBackend::query` in tests module: `QueryOutput::from_text("success", "mock", Duration::from_millis(0))`
   - Verify `test_retry_success_after_failures` still passes

8. **Update existing QueryOutput tests in mod.rs** - files: `src/backend/mod.rs` (tests module)
   - `test_query_output_from_text`: pass `backend="test"`, `duration=Duration::ZERO`
   - `test_query_output_from_process_with_stderr`: same
   - `test_query_output_from_process_empty_stderr_normalized`: same
   - `test_query_output_from_process_empty_stdout`: same
   - Add new tests (see Evaluation table for full list)

9. **Add usage deserialization tests** - files: `src/backend/claude.rs`, `src/backend/ollama.rs`, `src/backend/bedrock.rs` (test modules)
   - Tests only exercise serde deserialization on literal JSON strings — no network I/O
   - Listed per-backend in sub-tasks 2a, 3, 4 above

**Dependency order**:
- Sub-task 1 must be done first (struct definitions)
- Sub-tasks 2a, 2b, 3, 4, 5, 6 can be done in parallel after 1 (each backend is independent)
- Sub-tasks 7, 8, 9 can be done in parallel after 1 (tests + mock adjustments)
- Sub-task 7 depends on compile success of 2a-6 when `cargo test` is run holistically

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | `TokenUsage::new(10, 20)` computes total | `total_tokens == 30` | `cargo test test_token_usage_new_computes_total` |
| 2 | `TokenUsage::new(u32::MAX, 1)` saturates | `total_tokens == u32::MAX` (no panic) | `cargo test test_token_usage_new_saturates_on_overflow` |
| 3 | `TokenUsage::default()` returns all-zero | `prompt_tokens == 0, completion_tokens == 0, total_tokens == 0` | `cargo test test_token_usage_default_zero` |
| 4 | `QueryOutput::from_text("ok", "claude", Duration::from_millis(100))` sets backend + duration + default None structured | `backend == "claude"`, `duration == 100ms`, `structured.is_none()` | `cargo test test_query_output_from_text_populates_backend_and_duration` |
| 5 | `QueryOutput::from_process(...)` sets backend + duration + default None structured | Backend field, duration, and structured == None | `cargo test test_query_output_from_process_populates_backend_and_duration` |
| 6 | `QueryOutput::with_model(Some("sonnet"))` sets model | `model == Some("sonnet".into())` | `cargo test test_query_output_with_model_some` |
| 7 | `QueryOutput::with_model(None)` leaves model None | `model.is_none()` | `cargo test test_query_output_with_model_none` |
| 8 | `QueryOutput::with_usage(Some(TokenUsage::new(5, 10)))` sets usage | `usage == Some(TokenUsage { 5, 10, 15 })` | `cargo test test_query_output_with_usage_some` |
| 9 | `QueryOutput::with_usage(None)` leaves usage None | `usage.is_none()` | `cargo test test_query_output_with_usage_none` |
| 10 | `QueryOutput::with_structured(Some(json!({"a":1})))` sets structured | `structured == Some(json!({"a":1}))` | `cargo test test_query_output_with_structured_some` |
| 11 | `QueryOutput::with_structured(None)` leaves structured None | `structured.is_none()` | `cargo test test_query_output_with_structured_none` |
| 12 | Claude response parses `model` + `usage.input_tokens`/`output_tokens` | Deserialization succeeds, fields match literal JSON | `cargo test test_claude_response_deserialize_with_usage` |
| 13 | Claude response without `usage` field still parses | `usage.is_none()`, no error | `cargo test test_claude_response_deserialize_without_usage` |
| 14 | Ollama response parses `model` + `prompt_eval_count`/`eval_count` | Deserialization succeeds, fields match literal JSON | `cargo test test_ollama_response_deserialize_with_counts` |
| 15 | Ollama response with one of the counts missing | Both counts become `None`, `TokenUsage` NOT constructed | `cargo test test_ollama_response_deserialize_partial_counts` |
| 16 | Bedrock response parses `model` + `usage` | Deserialization succeeds (feature-gated) | `cargo test --features bedrock test_bedrock_response_deserialize_with_usage` |
| 17 | `RetryExecutor` test mock compiles with new constructor | `cargo test test_retry_success_after_failures` passes | `cargo test test_retry_success_after_failures` |
| 18 | Full test suite | All existing + new tests pass | `cargo test` |
| 19 | Bedrock feature build | Compiles | `cargo build --features bedrock` |
| 20 | Clippy clean | No warnings | `cargo clippy -- -D warnings` |

**Edge cases to verify**:
- Claude API response missing `usage` field (older API version) -> `usage` on `ClaudeResponse` is `Option`, QueryOutput `.usage` stays `None` (no panic, no ser error)
- Ollama response missing `prompt_eval_count` OR `eval_count` (streaming aborted mid-response) -> both fields are `Option<u32>`, only construct `TokenUsage` when both present via `.zip()`
- Ollama response missing `model` field -> fall back to effective requested model so output `model` is always `Some` when we know what was requested
- Claude CLI mode has no response JSON to parse -> `model` comes from effective (override or config), `usage` stays `None`
- Gemini / Codex CLI backend with no `model_override` -> `model` comes from config default; may be `None` if config has no default (acceptable — represents "effective model unknown")
- RetryExecutor retries 3 times, each with duration 100ms -> final QueryOutput `duration` reflects only the successful (last) attempt's measurement, NOT 400ms total. Documented intent in module comment.
- Bedrock feature disabled -> Bedrock code not compiled; no test impact (tests guarded by `#[cfg(feature = "bedrock")]`)
- `QueryOutput` struct has NO `Default` impl — compile-time error if someone tries `QueryOutput::default()` (intentional, protects always-populated invariants)
- Callers wanting structured parsing continue to call `workflow::extract_json_from_text(&output.stdout)` — NOT changed in this task, covered in a separate follow-up
