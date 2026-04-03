# Spec Review Synthesis: clo-183

**Synthesized**: 2026-04-03
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)

N/A - Single reviewer available.

## Disagreement (Needs Human Decision)

N/A - Ollama review timed out after 120s; only Gemini review available.

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `raw_output` should NOT be populated on heuristic failure - struct contract says it's only for when validators *mutate* output; string checks don't mutate | Gemini | High |
| 2 | `fix_retries` interaction unclear - validation is wired *after* the fix loop (`break 'fix_loop`), so heuristic failures bypass LLM self-healing entirely. Needs explicit decision. | Gemini | High |
| 3 | `ValidationResult` fields underspecified - no guidance on `validator` string format (e.g., `"heuristic:not_empty"`) or `failure_reason` formatting | Gemini | Medium |
| 4 | `StepResult.output` behavior on failure unspecified - should remain original LLM/shell output, not be replaced with error message | Gemini | Medium |
| 5 | Missing test for mixed config (both `check` and `backend` fields present before CLO-184) | Gemini | Low |
| 6 | Missing test for validation failure + `fix_retries > 0` interaction | Gemini | Medium |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

(Ollama review failed - timed out after 120s. Verdict based on Gemini review only.)

## Priority Actions

1. **Remove `raw_output` assignment on heuristic failure** (High) - Violates existing struct contract in `src/workflow.rs`. Heuristic checks don't mutate output, so `raw_output` must remain `None`.

2. **Decide `fix_retries` interaction** (High) - Either move validation inside `'fix_loop` to enable LLM self-healing on heuristic failures, or explicitly document that heuristic failures deliberately bypass the retry loop. Current spec is ambiguous.

3. **Specify `ValidationResult` field formats** (Medium) - Define `validator` naming convention (e.g., `"heuristic:not_empty"`) and `failure_reason` string patterns for debuggability.

4. **Clarify `StepResult.output` on failure** (Medium) - Ensure spec states that `output` retains original content on validation failure rather than being overwritten.

5. **Add test for `fix_retries` + validation failure** (Medium) - Cover the interaction between these two features.

6. **Add test for mixed heuristic/LLM config** (Low) - Ensure `backend` field is ignored gracefully when only heuristic `check` is specified.
