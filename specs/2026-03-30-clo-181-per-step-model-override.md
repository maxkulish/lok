# Spec: Add per-step model override for Backend::query()

**Created**: 2026-03-30
**Estimated scope**: M (11 files, ~8 sub-tasks)
**Dependency**: CLO-180 (Done) - `docs/design-docs/clo-180-query-output-struct.md`

## 1. Problem Statement

Lok's `Backend::query()` signature is `async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput>`. Each backend stores its model at construction time from `BackendConfig.model` (e.g., `claude-sonnet-4-20250514` for Claude API, `llama3.2` for Ollama). There is no way for a workflow step to override the model - every step using the same backend gets the same model.

The validation pipeline (CLO-184) needs cheap/fast models (Haiku, Gemini Flash) for validation steps while keeping expensive models for primary queries. Without per-step model override, users must define separate backend entries (e.g., `claude-haiku`, `claude-sonnet`) in `lok.toml` for every model variant, which is configuration bloat.

**Current state**:

- `Step` struct (`src/workflow.rs:249`) has no `model` field
- `Backend::query()` trait (`src/backend/mod.rs:53`) takes `(prompt, cwd)` - no model parameter
- Claude API: model hardcoded at init (`src/backend/claude.rs:66`, `src/backend/claude.rs:102`)
- Claude CLI: model passed as `--model` flag from `ClaudeMode::Cli.model` (`src/backend/claude.rs:156-158`)
- Gemini CLI: supports `--model` flag but current code doesn't pass it (`src/backend/gemini.rs:50-55`)
- Codex CLI: supports `--model` flag but current code doesn't pass it (`src/backend/codex.rs:62-67`)
- Ollama API: model in `ChatRequest.model` from `self.model` (`src/backend/ollama.rs:67`)
- Bedrock SDK: model in `self.model_id` passed to `invoke_model()` (`src/backend/bedrock.rs:107`)

**Call sites** (13 total across 6 files) that pass `backend.query()`:
- `src/workflow.rs`: 6 sites (lines ~797, ~968, ~1072, ~1183, ~1390, ~1438)
- `src/conductor.rs`: 1 site (line ~185)
- `src/spawn.rs`: 2 sites (lines ~173, ~282)
- `src/debate.rs`: 1 site (line ~230)
- `src/team.rs`: 2 sites (lines ~82, ~121)
- `src/backend/mod.rs`: 1 site in `run_query_with_config` (line ~171)

## 2. Acceptance Criteria

- [ ] `Step` struct has `pub model: Option<String>` field, deserialized from TOML `model = "haiku"`
- [ ] `Backend::query()` trait signature accepts `model: Option<&str>` as third parameter
- [ ] All 5 backend implementations accept and use the model override
- [ ] When `model` is `None`, every backend uses its configured default (zero behavior change)
- [ ] When `model` is `Some("haiku")`, Claude API uses `"haiku"` as the model field in the API request
- [ ] When `model` is `Some("haiku")`, Claude CLI passes `--model haiku` flag
- [ ] When `model` is `Some("gemma3")`, Ollama sends `"gemma3"` in the chat request model field
- [ ] When `model` is `Some(id)`, Bedrock uses `id` as the model_id in `invoke_model()`
- [ ] Gemini CLI passes `--model` flag when override is provided
- [ ] Codex CLI passes `--model` flag when override is provided
- [ ] All 13 call sites pass `None` (preserving current behavior) except workflow step execution which passes `step.model.as_deref()`
- [ ] `run_query` and `run_query_with_config` pass `None` to `backend.query()` (no model override at that layer)
- [ ] `cargo test` passes with 0 failures
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo build --features bedrock` compiles

**Verification method**: `cargo test && cargo clippy -- -D warnings && cargo build --features bedrock`

## 3. Constraints

**Must**:
- Keep `Backend::query()` as the single trait method - do not add a separate `query_with_model()`
- Pass model override as `Option<&str>` (not owned String) to avoid cloning at call sites
- Preserve all existing default model behavior when override is `None`
- Each backend MUST treat `Some("")` identically to `None` - use pattern: `let effective_model = model.filter(|m| !m.is_empty()).or(default)`

**Must-not**:
- Do not change `BackendConfig` or `lok.toml` schema
- Do not change `QueryOutput` struct
- Do not change `run_query` / `run_query_with_config` signatures (they use `Arc<dyn Backend>` without step context)
- Do not add model to `query_with_system()` in `claude.rs` (that's an internal helper, not a workflow call site)

**Prefer**:
- Add the `model` field after `backend`/`backends` fields in the `Step` struct for logical grouping
- Log model overrides with `eprintln!` when verbose mode is active (e.g., "Using model override: haiku for step X")

**Escalate when**:
- The trait signature change causes unexpected compile errors beyond the known 13 call sites
- Any existing test fails for reasons unrelated to the signature change

## 4. Decomposition

1. **Change trait signature**: Add `model: Option<&str>` to `Backend::query()` - file: `src/backend/mod.rs:53`
2. **Update Claude backend**: Modify private `query_api()` to accept `model: Option<&str>`, use override in API request body and CLI `--model` flag. Do NOT modify `query_with_system()` - it calls `query_api()` internally and always uses the default model. - file: `src/backend/claude.rs`
3. **Update Gemini backend**: Accept parameter, pass `--model` flag to CLI when override provided - file: `src/backend/gemini.rs`
4. **Update Codex backend**: Accept parameter, pass `--model` flag to CLI when override provided - file: `src/backend/codex.rs`
5. **Update Ollama backend**: Add `model: Option<&str>` to `chat()`, use override in `ChatRequest.model` field, fall back to `self.model` when `None` - file: `src/backend/ollama.rs`
6. **Update Bedrock backend**: Use override model_id in `invoke_model()` - file: `src/backend/bedrock.rs`
7. **Update all call sites to pass `None`**: Files: `src/workflow.rs`, `src/conductor.rs`, `src/spawn.rs`, `src/debate.rs`, `src/team.rs`, `src/backend/mod.rs` (run_query_with_config). Note: synthesis query at workflow.rs:~1072 passes `None` (synthesis uses backend default, not step model).
8. **Add `model` field to `Step` struct and wire it through workflow execution**: Pass `step.model.as_deref()` at the 6 workflow.rs call sites instead of `None`. The synthesis query remains `None`. - file: `src/workflow.rs`

**Dependency order**: Task 1 must be first (breaks all impls). Tasks 2-6 can be done in any order. Task 7 depends on 2-6. Task 8 depends on 7.

**Note**: Bedrock requires full model IDs (e.g., `us.anthropic.claude-3-haiku-20240307-v1:0`), not short aliases. This is documented behavior - no validation needed in Lok.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Build compiles | 0 errors | `cargo build` |
| 2 | All tests pass | 0 failures | `cargo test` |
| 3 | Clippy clean | 0 warnings | `cargo clippy -- -D warnings` |
| 4 | Bedrock feature compiles | 0 errors | `cargo build --features bedrock` |
| 5 | Bedrock tests pass | 0 failures | `cargo test --features bedrock` |
| 6 | Step struct has model field | Field exists with `#[serde(default)]` | `grep 'pub model: Option<String>' src/workflow.rs` |
| 7 | Trait has model param | Signature includes `model: Option<&str>` | `grep 'model: Option<&str>' src/backend/mod.rs` |
| 8 | Claude API uses override | When model override provided, API request body uses it | Unit test: `query(prompt, cwd, Some("haiku"))` constructs request with `"model": "haiku"` |
| 9 | Ollama uses override | When model override provided, chat request uses it | Unit test: verify `ChatRequest.model` equals override |
| 10 | None preserves default | When model is None, backend uses configured default | All existing tests still pass (they call with None) |

**Edge cases to verify**:
- `model = ""` in TOML: should deserialize as `Some("")` - backends MUST treat empty string same as `None`
- Step with `shell` command (no backend query): `model` field is parsed but never used
- Multi-backend consensus step: model override applies to all backends in the fan-out
- Synthesis query in consensus steps: uses backend default, NOT step model override
