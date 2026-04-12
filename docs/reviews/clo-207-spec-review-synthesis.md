# Spec Review Synthesis: clo-207

**Synthesized**: 2026-04-11
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| 1 | Naive `serde_json::from_str(stdout.trim())` will silently fail on markdown-fenced JSON (```json ... ```), making `structured = None` for most real LLM outputs - defeats the problem statement | HIGH |
| 2 | Backend response structs need explicit updates: `ClaudeResponse` needs `model`/`usage`, Ollama `ChatResponse` needs `model`/`eval_count`/`prompt_eval_count` - implied but not called out as sub-tasks | MEDIUM |
| 3 | Overall verdict: APPROVE_WITH_SUGGESTIONS - spec is well-structured but needs clarification before implementation | - |

## Disagreement (Needs Human Decision)
| # | Topic | Gemini Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | How to fix JSON parsing | Move `extract_json_from_text` from `workflow.rs` to shared `utils.rs`, use in constructors (violates current constraint) | Skip auto-parse entirely; keep consumer-side extraction; add explicit `with_structured()` builder instead |
| 2 | Should Gemini/Codex populate `model` | Yes - conditionally set `.with_model(model_override)` when user provides override (aligns with "effective model" constraint) | Not addressed - accepts spec's "no setters" as-is |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `.with_model(response.model)` won't compile if builder takes `impl Into<String>` since `response.model` is `Option<String>` - needs `if let Some(m)` or signature change | Gemini | HIGH |
| 2 | Ollama should fall back to effective requested model if API response omits `model` (mirror Bedrock pattern) | Gemini | MEDIUM |
| 3 | Missing tests for `with_model`/`with_usage` receiving `None` | Gemini | LOW |
| 4 | `QueryResult.elapsed_ms` vs new `QueryOutput.duration` duplication - migration path unspecified | Ollama | MEDIUM |
| 5 | `RetryExecutor` duration semantics ("successful attempt, not cumulative") need concrete implementation guidance | Ollama | MEDIUM |
| 6 | Sub-task 2 (ClaudeBackend) conflates API mode (response parsing) and CLI mode (config-based) - should split | Ollama | MEDIUM |
| 7 | Missing `Default` impl specification for `QueryOutput` (needed for test fixtures) | Ollama | LOW |
| 8 | No integration test for real backend responses populating new fields | Ollama | LOW |
| 9 | Downstream consumers (`workflow.rs`, `template/context.rs`) have no migration sub-task - fields populated but unused | Ollama | MEDIUM |

## Consolidated Verdict
**APPROVE_WITH_SUGGESTIONS**

Both reviewers approved with suggestions. The spec is sound but has one blocking contradiction (JSON parsing) and several clarifications needed before implementation.

## Priority Actions

**P0 - Blocking (resolve before implementation):**
1. **Resolve JSON parsing contradiction** (Agreement #1). Pick one: (a) move `extract_json_from_text` to shared utils and use in constructors, or (b) drop auto-parse and keep consumer-side extraction with explicit `with_structured()` builder. Current spec guarantees `structured = None` for markdown-wrapped output.
2. **Fix builder ergonomics for `Option<String>`** (Novel #1). Either change `with_model` signature to accept `Option<impl Into<String>>` or instruct `if let Some(m)` guard at call sites - current spec won't compile.
3. **Add explicit sub-task for response struct updates** (Agreement #2). Enumerate field additions to `ClaudeResponse`, `ChatResponse`, `BedrockResponse`.

**P1 - Important (address during implementation):**
4. **Populate effective model for Gemini/Codex** (Disagreement #2) when `model_override` is present - aligns with stated constraint.
5. **Ollama model fallback** (Novel #2) to effective requested model when API omits it.
6. **Split ClaudeBackend sub-task** (Novel #6) into 2a (API response parsing) and 2b (CLI config-based).
7. **Clarify `QueryResult.elapsed_ms` vs `QueryOutput.duration`** (Novel #4) - migrate or document coexistence.
8. **Specify RetryExecutor duration semantics** (Novel #5) - concrete guidance on "successful attempt" measurement.
9. **Add downstream consumer migration sub-task** (Novel #9) for `workflow.rs` and `template/context.rs`.

**P2 - Polish:**
10. `Default` impl for `QueryOutput` (Novel #7).
11. Tests for `None` cases in builders (Novel #3).
12. Integration test with real backend response (Novel #8).
