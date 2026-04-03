# Review Synthesis: clo-184

**Synthesized**: 2026-04-03
**Pipeline**: lok design-review
**Reviewers**: Gemini 3.1 Pro, Codex/Ollama (glm-5:cloud)

---

## Agreement (High Confidence)
Items where both reviewers independently identified the same concern.

| # | Finding | Severity |
|---|---------|----------|
| 1 | **Validation timeout needs explicit control** - Validation steps using cheap/fast models should not inherit long backend timeouts. Both recommend an explicit timeout field on `ValidateConfig`. | P1 / Suggestion |
| 2 | **Truncation interacts poorly with downstream use** - When `max_input_length` truncates output, consequences propagate: Gemini flags silent data loss when `replace_output = true`; Ollama flags that prompt template overhead isn't accounted for in the budget. Both point to the same root cause: truncation logic is too naive. | P2 / Suggestion |

## Disagreement (Needs Human Decision)
Items where reviewers hold divergent positions.

| # | Topic | Gemini Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | **Parsing fallback behavior** | **Critical flaw** - defaulting to "pass" when response is neither valid JSON nor `REVIEW_FAILED:` prefix is dangerous. Refusals and garbage silently pass validation. Must be fail-closed. | **Strength** - JSON structured output with `REVIEW_FAILED:` prefix fallback provides good backward compatibility and operational readiness. No concern raised about the default-to-pass path. |
| 2 | **Overall design readiness** | NEEDS_REVISION - three critical flaws must be fixed before implementation. | APPROVE_WITH_SUGGESTIONS - architecture is sound, only two P1 clarifications needed before implementation. |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | **Prompt interpolation vulnerability** - Sequential `.replace()` calls allow untrusted `output` containing `{{ stderr }}` to be expanded by the second pass, altering what the LLM validates. Needs single-pass replacement. | Gemini | Critical |
| 2 | **Markdown fence stripping** - LLMs frequently wrap JSON in ` ```json ` fences. `serde_json::from_str` will fail, and combined with the fallback logic, a valid JSON response gets misrouted to the plain-text branch. | Gemini | Critical |
| 3 | **Resolve open `{{ steps.X.raw_output }}` question** - Marked "open" in design but has implementation scope implications. Decide now or explicitly defer with tracked issue. | Ollama | P1 |
| 4 | **Cleaned field security** - LLM-generated `cleaned` output replaces original output. If the LLM injects malicious content, it inherits the trust model of step output. Needs documentation. | Ollama | P2 |
| 5 | **Concurrent validation safety** - Multiple steps may validate simultaneously; confirm no shared mutable state exists. | Ollama | P2 |
| 6 | **Empty `cleaned` field ambiguity** - What if JSON returns `{"status": "pass", "cleaned": ""}`? Current spec doesn't distinguish empty string from absent field. | Ollama | P2 |
| 7 | **Cost protection mechanism** - Large workflows could accumulate validation costs. Consider per-workflow call limits. | Ollama | P3 |

## Consolidated Verdict

**Overall: NEEDS_REVISION**

Gemini's NEEDS_REVISION takes precedence. While Ollama correctly identifies the architecture as sound, Gemini's critical findings around fail-open parsing, prompt interpolation, and markdown fence handling represent real bugs that would ship if unaddressed.

## Priority Actions

Ordered by severity, agreement items first.

1. **Fix fail-open parsing fallback** (Agreement item elevated by Gemini) - Make `parse_validation_response` fail-closed. Unrecognized responses must produce `ValidatorError`, not silent pass. This is the highest-risk item.
2. **Single-pass prompt interpolation** (Gemini, Critical) - Replace sequential `.replace()` with a single-pass approach to prevent placeholder injection from untrusted step output.
3. **Strip markdown fences before JSON parse** (Gemini, Critical) - Add pre-processing to remove ` ```json ` / ` ``` ` wrappers before `serde_json::from_str`. Without this, valid LLM responses misroute through the (now fail-closed) fallback path.
4. **Add explicit `timeout_ms` to `ValidateConfig`** (Both reviewers, P1) - Validation calls should fail fast independently of backend generation timeouts.
5. **Resolve `{{ steps.X.raw_output }}` scope** (Ollama, P1) - Decide include/defer before implementation begins to prevent scope creep.
6. **Document truncation + `replace_output` interaction** (Both reviewers, P2) - Either bypass truncation when `replace_output = true`, log a warning, or fail validation when truncation occurs.
7. **Address remaining P2/P3 items** (Ollama) - Concurrent safety, cleaned field trust model, empty string semantics, cost guardrails - resolve during implementation.
