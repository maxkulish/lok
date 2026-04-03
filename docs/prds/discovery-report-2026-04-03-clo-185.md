# PRD Discovery Report: Structured Failure Data for Step Errors (CLO-185)

**Date**: 2026-04-03
**PRD Score (baseline)**: 71% - Needs Work
**Discovery Debt**: 3 killer assumptions
**Verdict**: Needs Iteration - architectural conflict with CLO-182 design contract

## Critical Finding: CLO-182 Design Conflict

The PRD's core approach - extending `FailureType` with `Timeout`/`BackendError` and populating `ValidationResult` for all failure paths - **directly contradicts** the CLO-182 design contract:

- CLO-182 design doc line 32: *"FailureType must be scoped to validation failures only"*
- CLO-182 design doc line 134: *"FailureType has 2 variants, not 5. If structured execution failure metadata is needed later, a separate `failure_info: Option<FailureInfo>` field can be added."*
- CLO-182 design doc line 343: *"Must not add execution-level failure variants to FailureType"*
- CLO-182 discovery report: Flagged mixing execution/validation failures as *"domain confusion"* and marked removing `Timeout`/`BackendError` from `FailureType` as a **MUST FIX**

**Resolution**: Use a separate `StepFailure` struct on `StepResult` instead of repurposing `ValidationResult`.

## Prior Art Summary

| Source | Type | Relevance | Implication |
|--------|------|-----------|-------------|
| Temporal Failures | Product | High | Typed hierarchy with `nonRetryable` flag; validates need for structured failure types |
| Dagster Failure + RetryRequested | Product | High | Separates permanent (`Failure`) from transient (`RetryRequested`); supports two-axis classification |
| Prefect State Model | Product | Med | `FAILED` vs `CRASHED` split maps to validation-failed vs backend-error |
| Airflow AirflowException subtypes | Product | Med | Even minimal engines benefit from skip/fail/timeout distinctions |
| Temporal error-handling blog | Pattern | High | Recommends explicit "error bucketing layer" between backends and retry logic |
| Rust thiserror enum pattern | Pattern | High | Variant-level payloads with `#[derive(thiserror::Error)]` is idiomatic |
| AWS Builders' Library | Standard | Med | Transient vs permanent failure distinction; bounded retries with backoff |

**Key takeaway**: Industry converges on two-axis classification: **(1) failure origin** (user logic vs infrastructure vs timeout) and **(2) retryability** (transient vs permanent). Temporal is the strongest reference.

## Persona Review Summary

### Consensus Concerns (raised by 2+ personas)

1. **ValidationResult is the wrong struct for execution failures** - raised by Engineer, Adversarial - severity: critical
   - `ValidationResult` was designed for validation outcomes; stuffing timeouts and backend errors into it creates semantic confusion
   - Fix: Add `failure: Option<StepFailure>` field to `StepResult` as CLO-182 recommended

2. **No consumer exists for the output** - raised by User Advocate, Strategist - severity: high
   - Degradation engine is "future"; no workflow TOML syntax exposes failure types to authors
   - Fix: Either build alongside a consumer, or add a compile-time/test-time assertion that enforces the contract

3. **BackendError is overloaded** - raised by Engineer, Adversarial - severity: high
   - 5 distinct failure modes (shell exit, missing backend, consensus failure, edit failure, verify exhaustion) all map to one variant
   - Fix: Split into sub-variants or add structured payloads per Temporal's pattern

### Unique Perspectives

- **Engineer**: Signature change risk understated - need a mapping table of every call site to its target classification
- **User Advocate**: No workflow author interface defined - how do TOML workflows access failure type?
- **Strategist**: Consider minimal version (just Timeout vs Other) that unblocks the critical retry use case
- **Adversarial**: "~18 failure paths" is unmeasurable without full enumeration

## Assumption Map

### Killer Assumptions (validate before building)

| # | Assumption | Importance | Certainty | Suggested Validation |
|---|-----------|-----------|-----------|---------------------|
| 1 | Reusing `ValidationResult` for execution failures is appropriate | 5 | 1 | **INVALIDATED** - CLO-182 design explicitly prohibits this. Use separate struct. |
| 2 | 5 FailureType variants are sufficient | 4 | 3 | Review Temporal's taxonomy; consider if `BackendError` needs sub-variants |
| 3 | Downstream consumers will exist to use FailureType | 4 | 2 | Tie to concrete consumer milestone or add enforcement mechanism |

### Discovery Debt Score: 3 killer assumptions (1 invalidated) - Moderate debt

## Stress Test Results

1. **ValidationResult becomes overloaded dumping ground**
   - Every failure stuffs data into a validation struct. Downstream consumers can't tell if `validation` means "a validator ran" or "the step failed before validation."
   - Missing in PRD: Acknowledged by CLO-182 design as anti-pattern
   - Fix: Separate `failure: Option<StepFailure>` field

2. **BackendError becomes the new unstructured string**
   - Five distinct failure modes collapse into one variant. `BackendError` with string `failure_reason` recreates the original problem at a different level of abstraction.
   - Missing in PRD: FR-2 acceptance criteria too broad
   - Fix: At minimum, carry structured payload (exit_code, backend_name, error_kind) not just a reason string

3. **No contract enforcement - classification drifts**
   - Future failure paths skip populating structured data because nothing checks. The data becomes unreliable.
   - Missing in PRD: Section 9 (Rollout) - missing entirely
   - Fix: Add test that asserts every `StepResult` with `success=false` has `failure.is_some()`

## Recommended PRD Changes (prioritized)

1. **[MUST FIX]**: Replace the approach. Do NOT extend `FailureType` or reuse `ValidationResult` for execution failures. Instead, add a new `failure: Option<StepFailure>` field to `StepResult` with its own enum, as CLO-182's design doc recommended. Keep `ValidationResult` and `FailureType` scoped to validation only.

2. **[MUST FIX]**: Define `StepFailure` enum with richer variants than the current PRD proposes:
   ```rust
   pub enum StepFailureKind {
       Timeout,           // step or backend timed out
       BackendError,      // backend returned error or non-zero exit
       EmptyOutput,       // backend returned empty/whitespace (no validate clause)
       ConditionSkipped,  // step skipped due to condition/dependency
       EditFailed,        // edit parse or apply failed
       VerifyFailed,      // verify/fix loop exhausted
   }

   pub struct StepFailure {
       pub kind: StepFailureKind,
       pub message: String,         // human-readable error (same as current output)
       pub backend: Option<String>,
       pub exit_code: Option<i32>,
       pub elapsed_ms: u64,
   }
   ```

3. **[MUST FIX]**: Enumerate all failure paths explicitly in a table (not "~15" or "~18"). Map each `StepResult::error()` call site to its target `StepFailureKind`.

4. **[SHOULD FIX]**: Add missing Rollout section with contract enforcement: a test that asserts `success=false` implies `failure.is_some()`.

5. **[SHOULD FIX]**: Reduce Must-haves from 77% to under 60%. Move FR-9 (empty output from non-validation), FR-10 (edit failures), FR-11 (verify exhaustion) to Should.

6. **[COULD FIX]**: Add a concrete TOML workflow example showing the before/after experience for a workflow author.

## Blind Spots Identified

- No consideration of serialization - `StepFailure` needs `Serialize`/`Deserialize` if workflow results are persisted or sent to external systems
- No consideration of `for_each` loop iteration failures - these have different semantics (partial success) not covered by any FR
- No discussion of how `StepFailure` and `ValidationResult` interact when both exist (step succeeds execution but fails validation)

## Research Reading List

- [Temporal Failures Reference](https://docs.temporal.io/references/failures) - typed failure hierarchy with retryability
- [Dagster Op Events](https://docs.dagster.io/guides/build/ops/op-events) - Failure vs RetryRequested pattern
- [Structured Errors in Rust Applications](https://home.expurple.me/posts/why-use-structured-errors-in-rust-applications/) - thiserror enum patterns
