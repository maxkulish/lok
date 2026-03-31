# Review Synthesis: clo-182

**Synthesized**: 2026-03-31
**Pipeline**: lok design-review
**Reviewers**: Gemini 3.1 Pro, Codex/Ollama (glm-5:cloud)

---

## Agreement (High Confidence)

Items where both reviewers independently identified the same concern.

| # | Finding | Severity |
|---|---------|----------|
| 1 | **stderr visibility regression**: Separating stderr from stdout in `run_shell()` without updating display logic (`print_results`/`format_results`) creates a breaking change where stderr becomes invisible to users. Gemini flags as critical UX regression; Ollama flags as breaking change needing migration note and integration test. | **P1 - Critical** |
| 2 | **`run_shell()` behavioral change needs testing/gating**: Both agree the stdout/stderr separation is architecturally correct but needs safeguards - Gemini wants display updates tied to the separation (or both deferred together); Ollama wants an integration test verifying `StepResult.stderr` population. | **P1 - High** |

## Disagreement (Needs Human Decision)

| # | Topic | Gemini Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | **Construction site boilerplate** | Explicit `None` at 33 sites is brittle; recommends `Default` impl or `StepResult::success()` constructor to reduce blast radius of future field additions | Praises explicit `Option<T>` and `None` as "clean abstraction" that prevents breaking changes; no concern about maintenance burden |

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | `ValidationResult.validator` field has ambiguous cardinality with chained validators - unclear semantics for pass-all vs. fail-first scenarios. Suggests `failed_validator: Option<String>` or `validators_run: Vec<String>` | Gemini | P1 - Architecture |
| 2 | `exit_code: None` conflates "not applicable" (API backends) with "killed by signal" (Unix) - should use `ExitStatusExt` to detect signals and inject synthetic stderr message | Gemini | P2 - Edge Case |
| 3 | Missing `serde` derive macros on new types if serialization is intended (mentioned in Dependencies but absent from snippets) | Ollama | P1 - Implementation |
| 4 | No stderr size limits or truncation strategy for large stderr output | Ollama | P2 - Operational |
| 5 | `raw_output` semantics unclear when validation reads but doesn't mutate output | Ollama | P2 - Clarity |
| 6 | No mention of Windows signal handling - should note Unix-focused scope | Ollama | P3 - Nit |
| 7 | Grep pattern in Evaluation table may have unescaped `\|` issue | Ollama | P3 - Nit |

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** - Both reviewers approve with suggestions; neither requires revision.

## Priority Actions

Ordered by severity, agreement items first.

1. **Tie stderr visibility to stderr separation** (Agreement #1) - Either bring `print_results()`/`format_results()` updates into CLO-182 scope, or defer both the behavioral change and display logic together. This is a user-facing regression if shipped as-is.
2. **Add integration test for `run_shell()` stderr separation** (Agreement #2) - Verify a shell step with non-empty stderr populates `StepResult.stderr` correctly and remains visible.
3. **Clarify `ValidationResult.validator` semantics** (Gemini) - Define behavior for chained validators: what populates the field on pass-all vs. fail-first?
4. **Add `serde` derives if serialization is planned** (Ollama) - Ensure new types match serialization expectations from Dependencies section.
5. **Disambiguate `exit_code: None`** (Gemini) - Use `ExitStatusExt` to distinguish signal-killed processes from non-applicable backends.
6. **Decide construction site strategy** (Disagreement #1) - Human call: accept 33 explicit `None` sites for clarity, or add `StepResult::success()` constructor to reduce future churn. Both positions have merit.
7. **Document stderr size limits and `raw_output` semantics** (Ollama) - Minor clarifications before implementation begins.
