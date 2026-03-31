# PRD Discovery Report: CLO-182 - Extend StepResult

**Date**: 2026-03-31
**PRD Score (baseline)**: 64% - Needs Work (but FR-8 through FR-11 and Architecture section are strong)
**Discovery Debt**: 3 killer assumptions
**Verdict**: Needs Iteration (on 3 specific design decisions before implementation)

## Prior Art Summary

| Source | Type | Relevance | Implication |
|--------|------|-----------|-------------|
| Guardrails AI | Product | High | Wrap-validate-retry pattern validates inline; lok's `validate` clause mirrors this |
| Temporal SDK | Pattern | High | Typed error enums with retry semantics per variant; lok's FailureType should separate execution vs validation failures |
| DSPy Assertions | Framework | Medium | Declarative constraints on LLM output; validates lok's heuristic check approach |
| Rust Railway pattern | Pattern | High | `Result` chaining with `.and_then()` is idiomatic for validation pipelines |
| `thiserror` enum pattern | Pattern | High | Variant-level metadata fields (step, retryable, source) - standard Rust approach for structured errors |

**Key takeaway**: No Rust-native LLM output validation layer exists. Temporal's structured error propagation and Guardrails' inline validation are the closest design references. The `FailureType` design should follow Temporal's pattern of separating failure domains.

## Persona Review Summary

### Consensus Concerns (raised by both personas)

1. **FailureType mixes execution and validation failures** - severity: high
   - `Timeout`, `BackendError`, `HealthCheckFailed` are execution failures (pre-validation)
   - `ValidationFailed`, `EmptyOutput` are content failures (post-validation)
   - Suggested fix: Either split into two enums, or move execution-level failures out of `ValidationResult`

2. **QueryOutput data is discarded before reaching StepResult** - severity: high
   - `qo.stdout` extracted at lines 975, 1191, 804; `stderr`/`exit_code` dropped
   - Suggested fix: Thread `QueryOutput` (or at minimum stderr + exit_code) through intermediate pipeline structures

3. **`raw_output: String` is wasteful when no validation exists** - severity: medium
   - 90%+ of steps have no `validate` clause; `raw_output` would clone `output` for nothing
   - Suggested fix: Make `raw_output: Option<String>`, populate only when validation modifies output

### Unique Perspectives

- **Skeptical Engineer**: Clone cost increases on StepResult (which is cloned at multiple points); blast radius extends beyond construction sites to destructuring/pattern-match consumers
- **Adversarial Reviewer**: PRD's Section 8 example (`{{ steps.X.success }}` in templates) doesn't work because interpolation engine only reads `parsed_output` JSON, not struct fields; this is a later-task concern but signals the PRD's architecture section is incomplete

## Assumption Map

### Killer Assumptions (validate before building)

| # | Assumption | Importance | Certainty | Suggested Validation |
|---|-----------|-----------|-----------|---------------------|
| 1 | `raw_output` should be `String` not `Option<String>` | 4 | 2 | Decide: what populates `raw_output` on error-path StepResults (backend not found, parse failed)? If answer is "empty string" or "error message", Option is cleaner |
| 2 | `FailureType` belongs inside `ValidationResult` | 4 | 2 | Decide: should execution failures (Timeout, BackendError) populate `validation` field even without a `validate` clause? If no, these variants don't belong in ValidationResult |
| 3 | `stderr`/`exit_code` can reach StepResult without restructuring execution pipeline | 4 | 2 | Trace data flow: QueryOutput -> intermediate structures -> StepResult at each path (single-backend, consensus, for_each, shell) |

### Discovery Debt Score: 3 / 7 assumptions

## Stress Test Results

1. **Pipeline threading is structural, not additive**: Populating `StepResult.stderr` requires changing how data flows from `Backend::query()` through consensus voting, for_each iteration, and single-backend paths - intermediate tuples/structs currently only carry stdout
   - **Missing in PRD**: No mention of intermediate data structure changes
   - **Fix**: Design doc must address data threading through each execution path

2. **Memory doubling for output strings**: `raw_output: String` clones output at every construction site; wasteful for non-validation steps
   - **Missing in PRD**: No consideration of memory impact
   - **Fix**: Use `Option<String>` for `raw_output`

3. **FailureType domain confusion**: Execution failures stuffed into ValidationResult creates a "validation populated without validation" paradox
   - **Missing in PRD**: Section 7 architecture doesn't address when `validation` is populated for non-validation failures
   - **Fix**: Keep ValidationResult purely for validation; represent execution failures via existing `success: false` + `output` error message, or add a separate `failure: Option<FailureInfo>` field

## Recommended Changes (prioritized, CLO-182 scoped)

1. **[MUST FIX]**: Make `raw_output: Option<String>` - only populated when validation modifies the output. Avoids mandatory allocation at 35 sites, clearer semantics.

2. **[MUST FIX]**: Remove execution-level variants (`Timeout`, `BackendError`, `HealthCheckFailed`) from `FailureType`. Keep only `ValidationFailed` and `EmptyOutput`. Execution failures are already represented by `success: false`. If structured execution failure data is needed later, add a separate field.

3. **[MUST FIX]**: Address QueryOutput threading in design doc. At minimum, the single-backend and shell paths must pass `stderr`/`exit_code` to StepResult. Consensus and for_each paths can defer to a follow-up task if the intermediate structure changes are large.

4. **[SHOULD FIX]**: Consider `#[derive(Default)]` on StepResult with `#[default]` attributes so construction sites can use `StepResult { name, output, success, ..Default::default() }` instead of listing every field.

## Blind Spots Identified

- PRD Section 7 "Backend Trait Changes" is stale - `query()` already has `model: Option<&str>` parameter from CLO-180/181
- Template interpolation for new fields (`{{ steps.X.stderr }}`) not scoped in any FR - will need a separate task
- No consideration of `serde::Serialize` on StepResult (needed if step results are ever serialized to JSON for logging/debugging)

## Research Reading List

- Temporal Rust SDK error handling patterns (structured Activity failures with metadata)
- Guardrails AI architecture docs (inline validation with retry)
- `thiserror` + `snafu` patterns for domain-separated error enums in Rust
