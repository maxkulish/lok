# Design Review: CLO-181 - Per-step model override for Backend::query()

**Reviewer**: Claude (code-validated)
**Reviewed**: 2026-03-30
**Document**: specs/2026-03-30-clo-181-per-step-model-override.md

---

## 1. Completeness Check

| Section | Present | Assessment |
|---------|---------|------------|
| Problem Statement | Yes | Clear explanation of why per-step model override is needed. Correctly identifies configuration bloat as the motivator. |
| Current State Analysis | Yes | Thorough - enumerates all 6 backend implementations with line numbers and current model handling. |
| Acceptance Criteria | Yes | 15 criteria, all testable. Covers trait signature, all backends, call sites, and build verification. |
| Constraints | Yes | Well-structured Must/Must-not/Prefer/Escalate categories. |
| Decomposition | Yes | 8 sub-tasks with clear dependency order. |
| Evaluation | Yes | 10 test scenarios including edge cases. |
| Architecture/Detailed Design | **Partial** | No architecture diagram or component overview. The spec is structured as a task blueprint rather than a design document - acceptable for a focused internal change but lacks visual representation of the change. |
| Background/Context | **Partial** | References CLO-184 validation pipeline but does not link to its PRD or design doc. No reference to the completed CLO-180 predecessor. |

**Assessment**: The spec is thorough for a focused trait signature change. It reads more like an implementation specification than a design document, which is appropriate for the scope.

---

## 2. Architecture Assessment

**Strengths**:
- The approach of adding `Option<&str>` (borrowed, not owned) to the trait signature is the right design choice - avoids allocation at 13 call sites that will pass `None`.
- The decomposition correctly identifies that Task 1 (trait change) must come first, tasks 2-6 are parallelizable, and tasks 7-8 are sequential. This is an accurate dependency graph.
- The constraint to NOT add a separate `query_with_model()` method is correct - it prevents API surface sprawl.
- Silently ignoring model override for CLI backends without model flag support (Gemini, Codex) is the right UX choice.

**Concerns**:
- **Claude API mode model override path is underspecified.** The spec says "Claude API uses `haiku` as the model field in the API request" but doesn't trace the code path. Currently, `Backend::query()` for Claude API mode calls `self.query_with_system("You are a helpful assistant.", prompt)` (claude.rs:209), which calls `query_api()` (claude.rs:92), which reads `model` from `self.mode` (claude.rs:102). The override would need to either: (a) thread the model parameter through `query_with_system` and `query_api`, or (b) construct the API request directly in the `Backend::query()` impl. The spec's constraint says "Do not add model to `query_with_system()`" but doesn't specify the alternative path.
- **Bedrock model override may break invocations.** The spec says "Bedrock uses `id` as the model_id in `invoke_model()`" but `invoke_with_messages()` (bedrock.rs:88) uses `self.model_id` at line 107. This public method is called by `conductor.rs` via the Bedrock backend directly. If the override only applies to `Backend::query()`, that's fine - but the spec should note that `invoke_with_messages()` is not affected.

---

## 3. Codebase Alignment

**Validated against source code:**

| Claim in Spec | Source Code | Verdict |
|---------------|-------------|---------|
| 13 call sites across 7 files | `rg '.query(' src/` returns 13 matches across 6 files (workflow.rs:6, conductor.rs:1, spawn.rs:2, debate.rs:1, team.rs:2, backend/mod.rs:1) | **MISMATCH**: Spec says "7 files" but there are 6 files. The spec lists `src/backend/mod.rs` as a separate file from the 6 caller files, but it is one of the 6. Total is 13 sites across 6 files. |
| `Step` struct at workflow.rs:249 | `pub struct Step` is at line 249 | Correct |
| `Backend::query()` at mod.rs:53 | Line 53: `async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput>` | Correct |
| Claude API model at claude.rs:66 | Line 66: `model` in `ClaudeMode::Api` variant | Correct |
| Claude CLI model at claude.rs:156-158 | Lines 156-158: `cmd.arg("--model").arg(m)` | Correct |
| Gemini: no model flag | Confirmed - gemini.rs has no model flag support | Correct |
| Codex: no model flag | Confirmed - codex.rs has no model flag support | Correct |
| Ollama model at ollama.rs:67 | Line 67: `model: self.model.clone()` in ChatRequest | Correct |
| Bedrock model at bedrock.rs:107 | Line 107: `.model_id(&self.model_id)` | Correct |
| `run_query_with_config` at mod.rs line ~171 | Line 171: `backend.query(&prompt, &cwd)` | Correct |

**Pattern compliance:**
- Adding `Option<&str>` to an async_trait method follows the existing pattern - other methods already use `&str` params.
- The `#[serde(default)]` annotation for the new `model` field on `Step` follows the existing pattern (all optional fields use `#[serde(default)]`).
- Adding the field "after `backend`/`backends` fields" follows the logical grouping convention already used in `Step`.

**Violations**: None found. The design follows established patterns.

---

## 4. Security Review

- **No security concerns.** The model parameter is a string passed to API request bodies or CLI flags. No new external input surfaces are introduced - the model string comes from TOML configuration which is already a trusted source.
- **Subprocess injection risk**: For CLI backends, the model string would be passed as a separate `arg()` call to `Command`, not interpolated into a shell string. This is safe. Gemini backend uses shell command interpolation (gemini.rs:50), but the spec says to ignore the override for Gemini, so no new injection surface.
- API key handling is unaffected.

---

## 5. Implementation Concerns

### 5.1 Claude API Mode - Threading the Model Override

The spec's most significant gap is the Claude API mode implementation path. The current call chain is:

```
Backend::query() -> query_with_system() -> query_api() -> uses self.mode.model
```

The constraint says "Do not add model to `query_with_system()`". This means the `Backend::query()` impl must either:

(a) **Duplicate the API request construction** when model override is provided (bypassing `query_with_system`), or
(b) **Read the model from a temporary override field** on `self` (requires interior mutability, bad for `Send + Sync`), or
(c) **Have `query_api` accept an optional model override** (different from `query_with_system`).

Option (c) is the cleanest - the constraint specifically says not to modify `query_with_system`, but `query_api` is a private method that can be changed. The spec should specify this.

### 5.2 Ollama Backend

The Ollama `chat()` method (ollama.rs:65) uses `self.model.clone()`. The override needs to thread through `chat()` or the model needs to be resolved in `Backend::query()` before calling `chat()`. This is straightforward but unspecified.

### 5.3 Multi-Backend Consensus Steps

The spec's edge case section mentions "Multi-backend consensus step: model override applies to all backends in the fan-out." This is correct but the implementation path is unclear. In the consensus code (workflow.rs:948-973), backends are created per-step from `config.backends.get(&bn)`. The model override would need to be passed through the `tokio::spawn` closure. The spec should confirm this is just passing `step.model.as_deref()` to `backend.query()` inside the spawn, which is straightforward.

### 5.4 Synthesis Backend in Consensus

At workflow.rs:1072, there's a synthesis query after multi-backend consensus. This backend is created independently (synth_backend_name). Should the synthesis step also use the step's model override? The spec doesn't address this. The most consistent behavior would be to pass the override, but synthesizers may benefit from using their configured default (typically a stronger model).

### 5.5 Dependency on CLO-180

The spec references CLO-180 (QueryOutput struct) as a predecessor but doesn't explicitly state it as a dependency. Looking at the codebase, CLO-180 is already completed (the current trait signature returns `Result<QueryOutput>`). The spec was written assuming CLO-180 is complete, which it is. No issue here.

---

## 6. Blind Spots

### Missing Edge Cases

1. **Empty string model override**: The spec mentions `model = ""` in TOML should deserialize as `Some("")` and "backends should treat empty string same as `None`". But the implementation guidance for this is missing. Each backend would need `let effective_model = model.filter(|m| !m.is_empty()).unwrap_or(&self.model);` or similar. This normalization should be specified once (trait-level helper or convention).

2. **Model validation**: No validation is specified for the model string. Passing `model = "nonexistent-model-xyz"` to Claude API will return a 400 error at runtime. Should there be early validation? At minimum, the error message should be clear. The spec should note this is deferred to runtime error handling.

3. **Bedrock model_id format**: Bedrock model IDs have a specific format (e.g., `us.anthropic.claude-sonnet-4-20250514-v1:0`). Short aliases like "haiku" won't work. The spec should note that Bedrock model overrides must use the full model ID.

4. **Gemini CLI model flag**: The spec says "Gemini: no CLI model flag support" - but Gemini CLI does support `--model` flag. The current codebase doesn't pass it, but could. Should this be a future enhancement or addressed now?

5. **Codex CLI model flag**: Similarly, Codex CLI supports `--model` flag via `codex exec --model MODEL`. The spec explicitly says to ignore it, but this seems like a missed opportunity since the infrastructure to support it already exists in the Codex command builder.

### Unstated Assumptions

- Assumes the TOML parser handles `model = "haiku"` on `Step` without conflicts with the existing `BackendConfig.model` field. Since they're on different structs, this is fine, but the identical field name could cause confusion for users writing TOML files.
- Assumes all backends can handle arbitrary model strings without crashing. Claude API will return HTTP errors; Ollama will return errors if the model isn't pulled. These error paths are handled by existing error handling but aren't tested.

### Missing Cross-Cutting Concerns

- **Logging**: No specification for logging when a model override is active. It would be helpful to log "Using model override: X (default: Y)" at debug level.
- **Metrics/Tracing**: The `StepResult` doesn't capture which model was actually used. If the override is applied, there's no way to distinguish in the output which model ran. This matters for debugging workflows.
- **Documentation**: No mention of updating the workflow TOML schema documentation or README with the new `model` field.

---

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec is well-structured, accurately maps the codebase, and proposes a clean minimal change. The trait signature addition is the right approach. The main gap is the underspecified implementation path for Claude API mode's model override (how to thread it through the existing call chain without modifying `query_with_system`). The edge cases around empty strings and multi-backend synthesis also need clarification.

---

## 8. Actionable Feedback

Ordered by priority:

1. **[HIGH] Specify Claude API mode implementation path.** The constraint says "Do not add model to `query_with_system()`" but doesn't describe how the override reaches the API request. Recommend: modify `query_api()` to accept `model: Option<&str>` and use it in the request body construction. This is a private method change, distinct from the constraint.

2. **[HIGH] Specify Ollama `chat()` threading.** Either: (a) add `model: Option<&str>` to `chat()`, or (b) resolve the effective model in `Backend::query()` and pass it to `chat()` (which currently takes only `prompt`). Option (b) is simpler.

3. **[MEDIUM] Define empty-string normalization convention.** Add to constraints: "Each backend MUST treat `Some("")` identically to `None` - use the configured default model." Optionally provide a one-line helper pattern: `let effective = model.filter(|m| !m.is_empty()).unwrap_or(default)`.

4. **[MEDIUM] Clarify synthesis backend behavior.** In multi-backend consensus steps (workflow.rs:1072), should the synthesis query also use `step.model`? Recommend: no - synthesis uses its configured default, since it serves a different purpose.

5. **[MEDIUM] Fix file count.** The spec says "13 call sites across 7 files" in the section header and "13 total across 7 files" in Current State. The actual count is 13 sites across 6 files (workflow.rs, conductor.rs, spawn.rs, debate.rs, team.rs, backend/mod.rs).

6. **[LOW] Consider Gemini/Codex model flag support.** Both CLIs support `--model`. Adding it now is trivial (same pattern as Claude CLI) and prevents a follow-up task. If deferred, note it as a known enhancement.

7. **[LOW] Add debug logging for model overrides.** When `step.model` is `Some(m)`, log which model is being used vs. the default. This is essential for debugging workflows.

8. **[LOW] Note that `StepResult` doesn't capture the actual model used.** This is a future concern for CLO-182 (StepResult extensions) but worth noting as a known gap.

9. **[LOW] Note Bedrock model ID format requirement.** Users must pass the full Bedrock model ID (e.g., `us.anthropic.claude-haiku-3-20240307-v1:0`), not short aliases.

10. **[LOW] Add reference to CLO-180 as a completed dependency.** The spec assumes CLO-180 is done but doesn't link to its design doc or mention it in a Dependencies section.
