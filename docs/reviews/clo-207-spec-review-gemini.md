# Spec Review: clo-207

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-04-11
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment
The problem statement is clear, complete, and accurately describes the missing fields in `QueryOutput` compared to `llm-mux`. It correctly identifies the downstream consumers (like `workflow.rs`) and the observability/accountability limitations caused by the current struct.

## 2. Acceptance Criteria Review
**Strong**: The criteria are highly specific, measurable, and testable. They cover both the struct definitions, constructor signatures, and the expected behavior of each backend implementation.
**Gaps**: The criteria do not account for setting the `model` field in Gemini and Codex when a user explicitly provides a `model_override` at the call site.

## 3. Constraints Check
**Aligned**: The constraints are well-thought-out, particularly regarding the use of `u32` with `saturating_add`, the `String` type for backend identifiers, and retaining `duration` as `std::time::Duration` to match llm-mux.
**Concerns**: There is a contradiction regarding JSON parsing. The Problem Statement explicitly lists *"Structured output requires a post-hoc extract_json_from_text call"* as a broken behavior. However, a Constraint explicitly forbids moving `extract_json_from_text` out of `workflow.rs` and mandates a naive `serde_json::from_str`. Because LLMs almost always wrap JSON in markdown blocks (e.g., ````json {...} ````), `stdout.trim()` will fail to parse and `structured` will silently evaluate to `None`. This means the task will fail to solve the stated problem for the vast majority of real-world responses.

## 4. Decomposition Quality
**Well-scoped**: The sub-tasks are independent, logically ordered, and sized appropriately. Test updates and mock updates are explicitly called out.
**Issues**:
- Steps 5 (Gemini) and 6 (Codex) explicitly state "no model/usage setters", which contradicts the Constraint to populate the "EFFECTIVE model (override if present)".
- Steps 2, 3, and 4 instruct chaining `.with_model(response.model)` where `response.model` is an `Option<String>`. If `with_model` is defined as `with_model(self, model: impl Into<String>)`, this will not compile without unwrapping or conditional branching.

## 5. Evaluation Coverage
**Covered**: The test table provides excellent coverage for the new struct methods, JSON auto-parsing, and Serde deserialization of backend responses.
**Gaps**: No explicit test is defined to verify that `QueryOutput::with_model` and `QueryOutput::with_usage` behave correctly when passed `None` (if the builders are designed to accept `Option`).

## 6. Codebase Alignment
**Alignment**: The specification perfectly respects the `Backend` trait contract and correctly targets `src/backend/retry.rs` for mock updates. It accurately references existing variables like the `start` Instant in `OllamaBackend`.
**Violations**: None found.

## 7. Blind Spots
- **Ollama `model` fallback**: Ollama's API returns `model` in the response. If the API unexpectedly omits it, the output model will be `None`. It should fall back to the effective model requested, just like the Bedrock implementation specifies.
- **Refactoring backend inner methods**: For Claude and Ollama, the inner `query_api` and `chat` methods currently return `Result<String, BackendError>`. Changing them to construct and return `QueryOutput` directly requires a small signature change that is implied but not explicitly stated.

## 8. Verdict
APPROVE_WITH_SUGGESTIONS

## 9. Actionable Feedback
1. **Resolve JSON Parsing Contradiction**: The naive `serde_json::from_str(stdout.trim())` will fail on markdown-fenced JSON, meaning `structured` will almost always be `None` in CLI/chat scenarios. To actually solve the stated problem, move `extract_json_from_text` (and its helpers) from `src/workflow.rs` to a shared `src/utils.rs` (or `json_utils.rs`), and update `QueryOutput` constructors to use it *before* parsing with Serde.
2. **Fix Builder Ergonomics**: If `QueryOutput::with_model` takes `impl Into<String>`, chaining `.with_model(response.model)` will not compile since `response.model` is an `Option`. Either change the builder to `with_model(self, model: Option<impl Into<String>>)` and set the internal option directly, or instruct the developer to use `if let Some(m) = response.model` before calling the builder.
3. **Populate Effective Model for Gemini and Codex**: Update Decomposition steps 5 and 6 to conditionally set `.with_model(model)` if the user provided a `model_override`, rather than strictly forbidding model setters. This aligns with your constraint to reflect the effective requested model.
4. **Ollama Model Fallback**: In Decomposition step 3, specify falling back to the requested effective model if Ollama's response doesn't contain it, similar to the Bedrock fallback: `.with_model(resp.model.or_else(|| Some(effective_model.to_string())))`.
