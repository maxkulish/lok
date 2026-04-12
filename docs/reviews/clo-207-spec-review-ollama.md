# Spec Review: clo-207

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-11
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Strong**: The problem statement is thorough and well-defined. It correctly identifies that `QueryOutput` lacks fields available in llm-mux's `BackendResponse`. The references to specific code locations (src/workflow.rs lines 782, 1186, 1949, 2177) are verifiable. The connection to PRD Phase 2 is clear.

**Minor Gap**: The spec references line numbers (e.g., `src/workflow.rs:2175-2182`) but the actual file uses different line numbers based on my analysis. The exact lines for `step_stderr` and `step_exit_code` are at 2179-2181, which is close but not exact. This is acceptable since line numbers shift during development.

## 2. Acceptance Criteria Review

**Strong**:
- Field definitions are precise (types, visibility)
- Constructor signatures are well-specified with `impl Into<String>` for backend
- Builder pattern (`with_model`, `with_usage`) is idiomatic Rust
- Test requirements are explicit (`cargo test`, `cargo clippy`, feature build)
- Each backend's expected behavior is enumerated separately

**Gaps**:
- **Missing: `Default` impl for `QueryOutput`** - The spec requires `TokenUsage::default()` returns all-zero but doesn't specify whether `QueryOutput` should derive or implement `Default`. This matters for test fixtures and step initialization.
- **Vague: "auto-populate `structured`"** - Criterion says "Both constructors auto-populate `structured` by attempting `serde_json::from_str::<Value>(stdout.trim())`" but `from_process()` receives stdout already - should this apply there too? The edge cases mention "stdout with leading/trailing whitespace" but the criterion isn't explicit about whether `from_process` also auto-parses.
- **Missing: Error handling for malformed JSON** - What happens if stdout contains invalid UTF-8 or JSON that's too large? The spec says "Large stdout (500KB JSON) -> structured parses successfully" but doesn't specify behavior when parsing fails (other than storing `None`).
- **Implicit: `structured` auto-parse only on `from_text`?** - The spec says "Both constructors" but `from_process` receives pre-parsed CLI output. Should Gemini CLI output with JSON text also get auto-parsed? This could cause unexpected behavior if CLI backends emit text that happens to be valid JSON.

## 3. Constraints Check

**Aligned**:
- `duration` as `std::time::Duration` matches PRD and is idiomatic
- `TokenUsage` with `u32` and `saturating_add` is correct for overflow safety
- `backend: String` (owned) is the right choice for multi-tenant scenarios
- Additive-only constraint preserves backward compatibility

**Concerns**:
- **Constraint: "Auto-populate structured parses JSON from stdout"** - This contradicts the established pattern from CLO-180 where parsing happens at consumption time (via `extract_json_from_text` in workflow.rs and template/context.rs). Auto-parsing at construction time duplicates logic and could cause issues if CLI backends emit text that happens to be valid JSON.
- **Missing constraint: `structured` should be optional/lazy** - Parsing JSON at construction time for every query may have performance implications. Consider whether this should be lazy (parsed on first access) or only populated when explicitly requested.
- **Constraint contradiction**: The spec says `structured` auto-parses, but CLO-180 design doc shows `template/context.rs:61` calls `extract_json_from_text` at render time. Now we'd have two places doing JSON extraction.

## 4. Decomposition Quality

**Well-scoped**:
- Sub-task 1 (struct definitions) is ~30 minutes - appropriate foundation
- Sub-tasks 2-6 (backend updates) are independent and parallelizable
- Sub-tasks 7-9 (tests) can be done in parallel with backend updates

**Issues**:
- **Sub-task 2 (ClaudeBackend) is undersized** - It covers both API mode (populate model/usage from response) AND CLI mode (populate model from config). These are significantly different code paths. Consider splitting into 2a (API) and 2b (CLI).
- **Missing sub-task: Update response structs** - Adding `usage` field to `ClaudeResponse` and `ChatResponse` requires updating the serde structs, which isn't called out explicitly. The response deserialization needs new fields.
- **Missing sub-task: Update consumers of new fields** - The spec mentions downstream consumers (workflow.rs, template/context.rs) but doesn't include a sub-task for updating them to USE the new fields. Currently they'd just be populated but ignored.
- **Implicit sub-task: Claude API response struct changes** - Claude API responses include `model` and `usage` fields that need to be added to `ClaudeResponse` struct - this isn't explicitly mentioned.

## 5. Evaluation Coverage

**Covered**:
- Unit tests for `TokenUsage` computation and saturation
- Unit tests for `QueryOutput` field population
- Backend response parsing tests (serde deserialization)
- RetryExecutor mock compilation test

**Gaps**:
- **Missing: Integration test for actual backend query with new fields** - The spec tests serde deserialization but not whether real API responses populate fields correctly.
- **Missing: Test for `structured` with array JSON** - `extract_json_array_from_text` exists separately from `extract_json_from_text`. Should `structured` handle arrays too? The edge cases don't cover this.
- **Missing: Test for `structured` with nested JSON** - What if stdout is `{"outer": {"inner": 1}}`? Should `structured` contain the full value or be extracted differently?
- **Missing: Performance test** - Criterion mentions "Large stdout (500KB JSON)" but no test verifies parsing doesn't timeout or OOM.

## 6. Codebase Alignment

**Violations**:
1. **Auto-parse `structured` at construction time** contradicts existing pattern. Currently `src/workflow.rs:330-336` and `src/workflow.rs:2987-2991` call `extract_json_from_text` / `extract_json_array_from_text` at consumption time. The template context (`src/template/context.rs:49-57`) also calls `extract_json_from_text`. Auto-parsing at construction duplicates this logic.

2. **Response struct additions not mentioned** - Claude's `ClaudeResponse` struct at `src/backend/claude.rs:34-36` needs `model` and `usage` fields added. Ollama's `ChatResponse` needs similar additions. This should be explicit.

**Alignment**:
- `QueryOutput::from_text` / `from_process` constructors follow CLO-180 pattern exactly
- Builder pattern (`with_model`, `with_usage`) is idiomatic
- `Duration` type matches existing `std::time` usage in retry.rs
- Feature gating for Bedrock follows established `#[cfg(feature = "bedrock")]` pattern

## 7. Blind Spots

1. **No plan for updating downstream consumers** - `workflow.rs` and `template/context.rs` currently do their own JSON extraction. Adding `structured` to `QueryOutput` creates a choice: use the pre-parsed field OR keep extracting. No migration path is specified.

2. **`structured` could be wrong type** - `Option<serde_json::Value>` is fine, but CLI backends often return markdown-wrapped JSON (e.g., ` ```json\n{...}\n``` `). The spec says "parse stdout.trim()" but existing `extract_json_from_text` handles markdown blocks. Which behavior should `structured` have?

3. **Usage field naming mismatch** - Claude API uses `input_tokens`/`output_tokens`. Ollama uses `prompt_eval_count`/`eval_count`. Bedrock uses Anthropic format. The spec correctly maps these, but the response structs need these optional fields added.

4. **No logging/observability** - The PRD mentions this is foundation for observability, but no logging is specified for the new fields. Consider adding `tracing::debug!` logs when populating model/usage.

5. **Ollama response model field** - The spec says Ollama response has `model` field, but `ChatResponse` struct in ollama.rs doesn't include it. This needs to be added.

6. **Missing: `QueryResult` impact** - `QueryResult` in mod.rs already has `elapsed_ms: u64`. The new `QueryOutput.duration` duplicates this. Should `QueryResult` be updated to use `output.duration` instead? This isn't addressed.

7. **RetryExecutor duration semantics** - The spec correctly says "duration reflects the successful attempt, not cumulative retry time" but doesn't explain how to achieve this. The current `RetryExecutor::query` would need to let inner backends measure their own duration.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, thorough, and largely aligned with existing patterns. However, there are several issues that should be addressed before implementation to avoid technical debt and ensure consistency with the codebase.

## 9. Actionable Feedback

**High Priority** (address before implementation):

1. **Clarify `structured` auto-parse behavior**: Decide whether it should:
   - Parse at construction time (current spec) - simpler but duplicates logic
   - Parse lazily on first access - requires interior mutability
   - Not auto-parse, rely on consumers to use `extract_json_from_text` as today
   
   **Recommendation**: Skip auto-parse in `QueryOutput`. Keep existing extraction logic in workflow.rs. Add a separate `structured: Option<Value>` that callers can populate explicitly via `with_structured()`.

2. **Add sub-task for response struct updates**: Explicitly list changes to:
   - `ClaudeResponse` → add `model: Option<String>`, `usage: Option<ClaudeUsage>`
   - `ChatResponse` → add `model: Option<String>`, `eval_count: Option<u32>`, `prompt_eval_count: Option<u32>`
   - `BedrockResponse` → add `usage: Option<BedrockUsage>`

3. **Document `QueryResult` vs `QueryOutput.duration` relationship**: Either update `QueryResult` to use `output.duration` or explain why both exist.

**Medium Priority** (address during implementation):

4. **Split ClaudeBackend sub-task**: Separate API mode (response parsing) from CLI mode (config-based model population).

5. **Add `Default` impl specification**: Explicitly state whether `QueryOutput` derives `Default` and what the defaults should be.

6. **Specify Ollama token usage extraction**: The current `ChatResponse` doesn't have the fields. Add test case showing expected JSON format.

**Low Priority** (optional improvements):

7. **Consider `tracing::debug!` logs**: Log model/usage when populated for observability.

8. **Add integration test**: Test that a real Claude API response populates `model` and `usage` correctly.

9. **Clarify JSON array handling**: Should `structured` handle `extract_json_array_from_text` cases?
