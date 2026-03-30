# PRD Discovery Report: Output Validation Pipeline for Lok

**Date**: 2026-03-30
**PRD Score (baseline)**: 60% - Needs Work
**Discovery Debt**: 11 killer assumptions
**Verdict**: Needs Iteration

---

## Prior Art Summary

| Source | Type | Relevance | Implication |
|--------|------|-----------|-------------|
| Guardrails AI | Framework | High | Validates combined heuristic + LLM validation pattern. Their RAIL spec's stacked validators + on-fail policies are direct prior art for `[steps.validate]` |
| Instructor (Pydantic) | Library | High | Retry-with-feedback pattern answers Open Question #2 - validation failures should feed error context into retry prompts, not just re-run blindly |
| LLM-as-Judge research (Eugene Yan) | Research | High | Self-enhancement bias (10-25% score inflation) confirms PRD's cross-model validation choice. Never validate with the same model that generated output |
| Tokio subprocess patterns | Implementation | High | `ChildStdout`/`ChildStderr` with `take()` + `tokio::join!` is the canonical Rust pattern for FR-1. No custom Future needed |
| Azure CLI exit-code-0 failure | Case study | High | Exit code 0 with empty output is a documented real-world pattern. Validates that exit codes alone are insufficient (FR-5 + FR-19 together) |
| DSPy Assertions | Framework | High | Boolean check + failure message pattern maps to heuristic validators. Combined boolean + LLM retry parallels FR-23 |
| CrewAI task guardrails | Framework | Medium | Closest to model-as-validator pattern but Python/agent-focused. Their structured JSON result `{valid, feedback, score}` informs FR-28 design |
| Circuit breaker patterns | Pattern | Medium | Graduated retry (vary prompt on retry) relevant to Open Question #2. Circuit breakers could inform health check design |

**Key gap lok fills**: No existing tool combines CLI subprocess orchestration (stdout/stderr/exit_code capture) with LLM output validation (heuristic + model-as-validator) in a declarative TOML config. All validation frameworks wrap API calls; all workflow engines ignore LLM output quality.

---

## Persona Review Summary

### Consensus Concerns (raised by 2+ personas)

1. **REVIEW_FAILED signal format is fragile** - raised by Engineer, Adversarial, User Advocate - severity: **high**
   - LLMs don't reliably produce exact string formats. Building the entire validation architecture on prefix-match parsing is risky.
   - Suggested fix: Use structured JSON response from validator, or a multi-signal approach (check for both `REVIEW_FAILED` and absence of expected content markers).

2. **Cleaned output replacing raw output creates data integrity risk** - raised by User Advocate, Adversarial, Stress Test #1 - severity: **high**
   - FR-16 says cleaned output replaces raw. No `{{ steps.X.raw_output }}` interpolation is defined. Validator could remove legitimate content.
   - Suggested fix: Add `{{ steps.X.raw_output }}` to Phase 2 template interpolation. Add `validation_confidence` field.

3. **Validator backend failure mode is unaddressed** - raised by Engineer, Stress Test #2 - severity: **high**
   - What happens when Haiku is rate-limited during validation? PRD has no "validator fails" branch.
   - Suggested fix: Add FR-NEW: validation infrastructure failure falls back to heuristic check or passes raw output with `validation.status = skipped`.

4. **Retry/validation interaction is contradicted** - raised by Engineer, Adversarial - severity: **medium**
   - Out-of-Scope asserts validation triggers existing retries. Open Question #2 asks whether it should. These contradict.
   - Suggested fix: Resolve the contradiction. Likely: validation failure triggers retry of full step (query + validate).

5. **Phase 1 scope is too large for an MVP** - raised by Strategist - severity: **medium**
   - 25 of 33 FRs in Phase 1. A real MVP: Backend trait change + heuristic checks only. LLM-as-validator can be Phase 2.
   - Suggested fix: Split Phase 1 into 1a (trait change + heuristics) and 1b (LLM validation + model override).

### Unique Perspectives

- **Engineer**: Per-step model override assumes all backends support model switching - Gemini CLI does not (model is determined by installed version, not a CLI flag)
- **User Advocate**: No debug journey for "validation keeps rejecting my output" - users can't see what the validator did or run validation in isolation
- **Strategist**: LLM validation cost unquantified - a 3-backend review workflow goes from 4 to 7 LLM calls (75% cost increase)
- **Adversarial**: `contains(text)` check (FR-22) has no escaping/quoting spec. Is `check` a string or array field?

---

## Assumption Map

### Killer Assumptions (validate before building)

| # | Assumption | Imp. | Cert. | Suggested Validation |
|---|-----------|:----:|:-----:|---------------------|
| 1 | LLMs reliably produce exact `REVIEW_FAILED: <reason>` signal strings | 5 | 2 | Run 100 validation prompts across Haiku/Flash, measure format compliance rate |
| 2 | The motivating incident represents a class of problems, not a one-off | 4 | 3 | Audit last 30 days of workflow runs for additional silent failures |
| 3 | Users will write effective validation prompts in TOML | 4 | 2 | User testing: 5 users write validate clauses for given scenarios |
| 4 | Validator backend is available when primary backend fails | 5 | 2 | Measure failure correlation across backends (do rate limits cluster?) |
| 7 | Changing Backend::query() signature is the right refactor path | 4 | 3 | Prototype trait change; measure LOC delta across all backends |
| 8 | The ~5% silent failure rate is real | 3 | 2 | Instrument current workflows with post-hoc output analysis for 2 weeks |
| 10 | Users won't configure self-validation | 3 | 2 | Add code-level guardrail/warning instead of relying on docs |
| 11 | Existing retry mechanism is correct for validation failures | 4 | 2 | Map retry flow with validation added; identify retry boundary |
| 12 | Per-step model override works across all backends uniformly | 4 | 2 | Test model override with each backend; document which support it |
| 13 | Cleaned/validated output is always preferable to raw | 4 | 2 | Generate 50 cleaning examples, have humans judge content loss |
| 15 | Lok's user base beyond rs-wisper exists and benefits | 3 | 1 | Count distinct users/projects; survey for validation needs |

### Discovery Debt Score: 11 / 22 total assumptions

**Rating: High debt** - PRD needs another iteration before implementation.

---

## Stress Test Results

### 1. Validator Hallucinates - Rejects Valid But Unconventional Output
- **Scenario**: Gemini produces legitimate review without markdown headers. Haiku validator rejects it as "no structured sections found." Valid review discarded silently.
- **Missing in PRD**: FR-15/FR-16 treat validation as binary pass/fail. No uncertain/low-confidence state.
- **Fix**: Add `validation_confidence` field. When confidence is low, mark `success = true, validation.confidence = low` instead of `success = false`. Add `{{ steps.X.raw_output }}` interpolation.

### 2. Correlated Backend Failures Kill All Validation
- **Scenario**: 15 CI workflow runs hit Anthropic rate limits. Reviews succeed (queued before limit), but all Haiku validation calls fail with 429. Every step marked failed. 15 green runs become red.
- **Missing in PRD**: FR-12-18 don't address validator infrastructure failure.
- **Fix**: Add requirement: validation infrastructure failure falls back to heuristic check or passes raw output with `validation.status = skipped`. Validator failure must not convert successful step to failed step.

### 3. Validation Latency Tax Kills Adoption
- **Scenario**: Simple 2-step workflow goes from 20s to 45s with validation. Users abandon lok because orchestrator feels slower than manual invocation.
- **Missing in PRD**: NFR only measures validation call in isolation, not end-to-end impact.
- **Fix**: Validation should run concurrently with independent steps where possible. Report per-step query-time vs. validation-time breakdown. NFR: end-to-end latency increase from validation must not exceed 20% for parallel workflows.

---

## Recommended PRD Changes (prioritized)

1. **[MUST FIX]**: Resolve retry/validation contradiction - Out-of-Scope asserts a decision that Open Question #2 says is undecided
2. **[MUST FIX]**: Add validator failure fallback requirement - what happens when the validation backend itself fails (rate limit, timeout, network error)
3. **[MUST FIX]**: Specify `REVIEW_FAILED` signal format with fallback parsing - exact match is fragile; define prefix match + JSON alternative + graceful degradation
4. **[MUST FIX]**: Split Phase 1 into 1a (trait change + heuristics) and 1b (LLM validation + model override) - current Phase 1 is 25/33 FRs
5. **[SHOULD FIX]**: Add `{{ steps.X.raw_output }}` to template interpolation (move from implicit struct field to explicit access) so users can inspect what validation changed
6. **[SHOULD FIX]**: Document which backends support per-step model override (Gemini CLI does not support model selection at query time)
7. **[SHOULD FIX]**: Add self-validation warning/guard - prevent or warn when `validate.backend` matches the step's `backend`
8. **[SHOULD FIX]**: Quantify LLM validation cost delta per workflow (50-75% more API calls for validated workflows)
9. **[COULD FIX]**: Add Users & Use Cases section (formal personas, debug journey for validation failures)
10. **[COULD FIX]**: Add Dependencies and Rollout sections (both scored 0 in baseline)

---

## Blind Spots Identified

- No consideration of concurrent validation (validation running parallel with next depth-level steps)
- No specification of `check` field as string vs. array (can you combine multiple heuristic checks?)
- No escaping/quoting grammar for `contains(text)` check syntax
- No metrics emitted by the software itself (success metrics rely on manual audit)
- Gemini backend already separates stderr via `Stdio::piped()` - the PRD's problem statement about "merging stderr" is partially inaccurate for the current codebase
- No debug/inspect mode for validation decisions

---

## Research Reading List

- Guardrails AI documentation - validator stacking patterns and on-fail policies
- Eugene Yan's LLM Patterns - LLM-as-judge bias research and cross-model validation
- Instructor library retry mechanisms - validation feedback loops
- Tokio subprocess patterns - `take()` + `join!` for concurrent stream capture
- DSPy Assertions/Refine - boolean validation with retry prompts
- Circuit breaker patterns for AI agent error handling
