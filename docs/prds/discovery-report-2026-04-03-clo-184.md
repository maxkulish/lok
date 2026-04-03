# PRD Discovery Report: LLM-Based Step Validation (CLO-184)

**Date**: 2026-04-03
**PRD Score (baseline)**: 73% - Needs Work
**Discovery Debt**: 4 killer assumptions
**Verdict**: Needs Iteration (address signal protocol and error handling before design)

## Prior Art Summary

| Source | Type | Relevance | Implication |
|--------|------|-----------|-------------|
| MT-Bench / LLM-as-Judge (NeurIPS 2023) | Paper | High | Validates model-as-validator pattern. Strong LLM judges achieve 80%+ agreement with humans. Using a different model avoids self-enhancement bias (CALM paper). |
| DSPy Assertions (ICLR 2024) | Paper | High | Assert/Suggest maps to hard/soft validation. Retry-with-feedback improved compliance by 164%. Lok's `fix_retries` already implements this for `verify`. |
| SagaLLM (VLDB 2025) | Paper | High | Independent validation agent + compensation/rollback on failure. Directly analogous to Lok's per-step validation with `continue_on_error`. |
| Guardrails AI | Framework | High | 150+ validators with on-fail actions (REASK, FIX, FILTER, EXCEPTION). Key pattern: validators define both check AND action. |
| Instructor / instructor-rs | Library | High | Structured output validation + retry-with-error-feedback. Rust port exists. Semantic validation feature is the same pattern. |

**Key finding**: Lok's approach is unique in combining (1) cheap model validates expensive model, (2) heuristic-then-LLM tiering, (3) output cleaning (not just pass/fail), and (4) CLI process awareness. No existing tool offers all four.

**Gap found**: Every mature framework (Guardrails AI, Instructor, LangChain) has moved toward **structured output** (JSON with verdict + content) rather than in-band text signals. The PRD's `REVIEW_FAILED:` prefix approach is fragile by comparison.

## Persona Review Summary

### Consensus Concerns (raised by both Engineer and Adversarial Reviewer)

1. **REVIEW_FAILED: signal protocol is fundamentally fragile** - Severity: CRITICAL
   - In-band text signal in unstructured channel. Step output containing "REVIEW_FAILED:" causes false positives. Prompt injection via `{{ output }}` can suppress/trigger the signal. Case sensitivity, whitespace, line-matching rules unspecified.
   - **Fix**: Use structured JSON output: `{"status": "pass", "output": "..."}` or `{"status": "fail", "reason": "..."}`. Parse with serde_json, treat malformed response as validator error. Falls back to text parsing only if JSON parsing fails.

2. **Large outputs overflow validation model context window** - Severity: HIGH
   - No constraint on `{{ output }}` size. 500KB step output sent to Haiku (200K context) will fail with 400 error. No truncation, chunking, or size gate specified.
   - **Fix**: Add `validate.max_input_length` (default ~100K chars). Truncate with marker before interpolation, or skip LLM validation and fall through to heuristic-only with warning.

3. **Three wiring points create maintenance and correctness hazard** - Severity: HIGH
   - Identical validation logic needed at shell (~L1076), LLM (~L1304), and apply/verify (~L1624). Currently heuristic blocks are already copy-pasted. Adding LLM validation triples the risk of divergence.
   - **Fix**: Extract `async fn run_validation(output: &str, config: &ValidateConfig, ...) -> (Option<ValidationResult>, Option<String>)`. Call from all three sites. Independently testable.

4. **Validation infrastructure failure vs. validation rejection conflated** - Severity: MEDIUM
   - Two distinct failure modes: (a) validator says output is bad, (b) validator call itself fails (timeout, rate limit, network). PRD treats both as "validation failure" with no distinction.
   - **Fix**: Add `FailureType::ValidatorError` variant. Add `validate.on_error` config: `"pass"` (optimistic), `"fail"` (default), `"skip"` (treat as no validation).

5. **Cleaned output replacement with no downstream access to original** - Severity: MEDIUM
   - `raw_output` preserved but no `{{ steps.X.raw_output }}` interpolation pattern exists. Downstream steps can't access the pre-cleaning version. If validator hallucinates during cleaning, entire DAG is poisoned.
   - **Fix**: Add `{{ steps.X.raw_output }}` to interpolation system. Log diff summary when cleaning mutates output. Consider `validate.replace_output = true` as opt-in (default: pass/fail only without mutation).

### Unique Perspectives

- **Engineer**: Validation LLM should not be called for trivially cheap steps (shell `echo`). Consider `validate.llm = false` default requiring explicit opt-in. Also raised cost concern for `for_each` loops multiplying validation calls.
- **Adversarial**: `for_each` + validation interaction unspecified (per-iteration or aggregated?). `apply_edits` + validation ordering unclear (before or after edits applied?). `consensus` + validation unclear (individual or synthesized result?).

## Assumption Map

### Killer Assumptions (validate before building)

| # | Assumption | Importance | Certainty | Suggested Validation |
|---|-----------|-----------|-----------|---------------------|
| 1 | A cheap LLM (Haiku/Flash) can reliably distinguish noise from valid content | 5 | 3 | Test with 20 real Gemini CLI noise samples + 20 valid reviews. Measure accuracy. |
| 2 | REVIEW_FAILED: prefix is a reliable signal LLMs produce consistently | 5 | 2 | **Mitigated by design change**: switch to JSON structured output. |
| 3 | Step output fits within validation model's context window | 4 | 3 | **Mitigated by design change**: add max_input_length truncation. |
| 4 | Validation LLM won't hallucinate when "cleaning" output | 4 | 2 | Test with 10 clean outputs - measure content preservation rate. Consider pass/fail-only mode (no cleaning) as safer default. |

### Discovery Debt Score: 4 / 10 assumptions - Moderate (2 mitigated by design changes)

## Stress Test Results

1. **Validation LLM strips valid content**: Validator tasked with "cleaning noise" aggressively removes content that looks like noise but is actually evidence (code blocks with init patterns, log excerpts). Downstream synthesis quality degrades.
   - **Missing in PRD**: No content fidelity guarantee. No audit of what was removed.
   - **Fix**: Support pass/fail-only mode (no output replacement) as safer default. Add `raw_output` preservation + interpolation access.

2. **REVIEW_FAILED signal inconsistency across workflows**: Different authors use different signal formats. Prefix matching fails silently.
   - **Missing in PRD**: Exact parsing rules undefined.
   - **Fix**: Switch to JSON structured output. Document exact format with examples.

3. **Prompt injection via step output**: Adversarial content in step output hijacks validation prompt. Validator always passes, defeating the purpose.
   - **Missing in PRD**: No mention of prompt injection risk.
   - **Fix**: Use XML-tagged separation (`<step_output>...</step_output>`) in validation prompts. Document as defense-in-depth, not a guarantee.

## Recommended PRD Changes (prioritized)

1. **[MUST FIX]**: Replace REVIEW_FAILED: text signal with JSON structured output. Parsing: `serde_json::from_str` for `{"status": "pass"|"fail", "output": "...", "reason": "..."}`. Text fallback for CLI backends that can't produce JSON.
2. **[MUST FIX]**: Add `validate.on_error` field (`"pass"`, `"fail"`, `"skip"`) to distinguish validator infrastructure failure from validation rejection.
3. **[MUST FIX]**: Add `validate.max_input_length` with sensible default to prevent context window overflow.
4. **[SHOULD FIX]**: Specify validation behavior for feature interactions: `for_each` (per-iteration), `apply_edits` (validate output before edit application), `consensus` (validate synthesized result).
5. **[SHOULD FIX]**: Add `{{ steps.X.raw_output }}` to interpolation system so downstream steps can access pre-cleaning content.
6. **[COULD FIX]**: Support pass/fail-only mode (no output cleaning) as safer default, with output replacement as opt-in via `validate.replace_output = true`.
7. **[COULD FIX]**: Document prompt injection risk and recommend XML-tagged output separation in validation prompts.

## Blind Spots Identified

- **Retry interaction**: Does validation failure trigger step-level `retries`? If so, each retry re-runs step AND validation (cost amplification). Need explicit policy.
- **Validation metrics/observability**: No mention of logging validation pass/fail rates, cleaning diffs, or cost tracking across workflow runs.
- **Concurrent validation**: If multiple steps in the same depth group all have LLM validation, all those validation calls fire in parallel. Could spike API rate limits.

## Research Reading List

- [MT-Bench & Chatbot Arena](https://arxiv.org/abs/2306.05685) - Foundation for LLM-as-Judge pattern
- [CALM Framework](https://arxiv.org/abs/2410.02736) - Bias quantification in LLM judges (position bias, self-enhancement)
- [DSPy Assertions](https://arxiv.org/abs/2312.13382) - Assert/Suggest computational constraints for self-refining pipelines
- [SagaLLM](https://arxiv.org/abs/2503.11951) - Transaction guarantees and validation agents in multi-step LLM workflows
- [Guardrails AI docs](https://docs.guardrailsai.com/) - Validator design patterns and on-fail action taxonomy
