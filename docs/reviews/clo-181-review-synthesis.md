# Review Synthesis: CLO-181 - Per-step model override for Backend::query()

**Synthesized**: 2026-03-30
**Pipeline**: lok design-review + Claude code-validated review
**Reviewers**: Claude (code-validated), Codex/Ollama (glm-5:cloud)
**Failed Reviewers**: Gemini 3.1 Pro (produced MCP initialization noise, no review content)
**Design Document**: specs/2026-03-30-clo-181-per-step-model-override.md

---

## Agreement (High Confidence)

Items where 2+ reviewers independently identified the same concern.

| # | Finding | Reviewers | Severity |
|---|---------|-----------|----------|
| 1 | **Empty string handling unspecified**: `model = ""` in TOML should be treated as `None`. No implementation guidance provided - each backend would need normalization logic. Both reviewers recommend a helper function or convention. | Claude, Ollama | HIGH |
| 2 | **No logging for model overrides**: No specification for logging when a model override is active. Both recommend `tracing::debug!` or similar when override is applied. | Claude, Ollama | MEDIUM |
| 3 | **Gemini/Codex silent ignore should be documented**: Both backends silently ignore the model override. This should be documented as an intentional limitation for users writing TOML workflows. | Claude, Ollama | MEDIUM |
| 4 | **Bedrock model ID format differs from Claude API**: Users may pass short model names (e.g., "haiku") that work for Claude API but not for Bedrock (which needs `us.anthropic.claude-haiku-3-20240307-v1:0`). Both reviewers flag this as a documentation need. | Claude, Ollama | LOW |
| 5 | **No model validation**: Invalid model strings will produce runtime errors from backend APIs/CLIs, not from Lok. Both reviewers note this is acceptable but should be documented. | Claude, Ollama | LOW |
| 6 | **Codebase verification passes**: All claims about line numbers, call sites, and current behavior verified against source code. Both reviewers confirmed the spec's analysis is accurate. | Claude, Ollama | INFO |
| 7 | **Design follows existing patterns**: `Option<&str>` parameter, `#[serde(default)]` annotation, and field grouping all match established codebase conventions. No pattern violations found. | Claude, Ollama | INFO |

---

## Disagreement (Needs Human Decision)

| # | Topic | Claude Position | Ollama Position |
|---|-------|----------------|-----------------|
| 1 | **Call site file count** | 13 sites across 6 files (workflow.rs, conductor.rs, spawn.rs, debate.rs, team.rs, backend/mod.rs) | 13 sites across 7 files (matches spec's claim). The difference is whether backend/mod.rs is counted separately from the other 6 files - technically it is one of the 6 unique files. |
| 2 | **Overall verdict** | APPROVE_WITH_SUGGESTIONS (gaps in Claude API threading and synthesis backend behavior) | APPROVE (well-scoped and backward-compatible) |

---

## Novel Insights (Single Reviewer)

| # | Finding | Reviewer | Severity |
|---|---------|----------|----------|
| 1 | **Claude API mode implementation path is underspecified.** The constraint says "Do not add model to `query_with_system()`" but doesn't describe how the override reaches the API request body. The call chain is `Backend::query()` -> `query_with_system()` -> `query_api()`. Recommend modifying the private `query_api()` method to accept `model: Option<&str>`. | Claude | HIGH |
| 2 | **Ollama `chat()` method threading unspecified.** `chat()` takes only `prompt` but uses `self.model.clone()`. Override must be threaded through `chat()` or resolved before calling it. | Claude | HIGH |
| 3 | **Synthesis backend in multi-backend consensus unclear.** At workflow.rs:1072, a synthesis query runs after collecting multi-backend responses. Should this synthesis also use `step.model`? The spec doesn't address it. Recommend: no, synthesis uses its configured default. | Claude | MEDIUM |
| 4 | **Gemini CLI actually supports `--model` flag.** The spec says "no CLI model flag support" but Gemini CLI does support `--model`. Current codebase doesn't pass it, but could trivially. Missed opportunity. | Claude | MEDIUM |
| 5 | **Codex CLI also supports `--model` flag.** Same as Gemini - `codex exec --model MODEL` is supported. The spec explicitly says to ignore it, which seems like a missed opportunity. | Claude | MEDIUM |
| 6 | **`StepResult` doesn't capture actual model used.** After override, there's no way to know which model actually ran a step. This matters for debugging. Likely belongs in CLO-182 (StepResult extensions). | Claude | LOW |
| 7 | **`query_with_system()` is not part of Backend trait - out of scope.** Conductor uses it directly but it's not a workflow call site. | Ollama | LOW |
| 8 | **`cache.rs` is unaffected.** Model override is at query time and doesn't affect `QueryResult`. Worth noting for reviewer confidence. | Ollama | LOW |
| 9 | **Consensus step fan-out sends same model to all backends.** Intentional but should be documented - users might expect per-backend model overrides in consensus steps. | Ollama | LOW |
| 10 | **Spec should reference CLO-180 as completed dependency.** The spec assumes CLO-180 (QueryOutput struct) is done but doesn't link to its design doc. | Claude | LOW |

---

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

- Claude: APPROVE_WITH_SUGGESTIONS
- Ollama: APPROVE
- Gemini: REVIEW_FAILED

Consensus rule: not unanimous APPROVE, so APPROVE_WITH_SUGGESTIONS.

The spec is well-researched, accurately maps the codebase (all line numbers and call sites verified), and proposes a clean, backward-compatible trait change. The main gaps are: (1) underspecified implementation paths for Claude API mode and Ollama's `chat()` method, (2) missing empty-string normalization convention, and (3) missed opportunity to support Gemini/Codex model flags which both CLIs support.

---

## Priority Actions

Ordered by severity, agreement items first:

| # | Action | Severity | Source | Agreement |
|---|--------|----------|--------|-----------|
| 1 | **Specify Claude API mode implementation path**: Modify private `query_api()` to accept `model: Option<&str>` - the constraint only prohibits changes to `query_with_system()`. | HIGH | Claude | Single |
| 2 | **Specify Ollama `chat()` threading**: Either add `model: Option<&str>` to `chat()` or resolve the effective model in `Backend::query()` before calling `chat()`. | HIGH | Claude | Single |
| 3 | **Define empty-string normalization convention**: Add to constraints: "Each backend MUST treat `Some("")` identically to `None`." Provide pattern: `let effective = model.filter(\|m\| !m.is_empty()).unwrap_or(default)`. | HIGH | Claude, Ollama | Agreed |
| 4 | **Add debug logging for model overrides**: `tracing::debug!("Using model override: {}", m)` when override is applied. | MEDIUM | Claude, Ollama | Agreed |
| 5 | **Clarify synthesis backend behavior in consensus steps**: Should synthesis at workflow.rs:1072 use `step.model`? Recommend: no. | MEDIUM | Claude | Single |
| 6 | **Consider Gemini/Codex model flag support**: Both CLIs support `--model`. Adding now is trivial; if deferred, note as known enhancement. | MEDIUM | Claude | Single |
| 7 | **Fix file count**: Spec says "7 files" but actual count is 6 unique files. | LOW | Claude | Single |
| 8 | **Document Bedrock model ID format requirement**: Full model IDs required, not short aliases. | LOW | Claude, Ollama | Agreed |
| 9 | **Document that Gemini/Codex silently ignore model override**: User-facing documentation for TOML workflow authors. | LOW | Claude, Ollama | Agreed |
| 10 | **Add reference to CLO-180 as completed dependency**: Link to `docs/design-docs/clo-180-query-output-struct.md`. | LOW | Claude | Single |
