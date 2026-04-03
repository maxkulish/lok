# Design Review: CLO-185 - Structured Failure Data for Step Errors

**Reviewed**: 2026-04-03
**Reviewer**: Claude Opus 4.6
**Design Document**: docs/design-docs/clo-185-structured-failure-data.md

---

## 1. Completeness Check

| Section | Present | Assessment |
|---------|---------|------------|
| Summary | Yes | Clear problem statement. Correctly frames the gap between validation failures (handled by CLO-182) and execution failures. |
| Background | Yes | Strong. References CLO-182 contract lines, discovery report, and prior art (Temporal, Dagster). |
| Architecture | Yes | Component overview is clear. Affected components table is accurate - single file change. |
| Detailed Design | Yes | Thorough. Complete type definitions, call site mapping, interaction matrix. |
| Implementation Plan | Yes | 4 phases, well-ordered. Tasks are granular enough to be checkboxable. |
| Acceptance Criteria | Yes | Testable with grep commands and cargo test. Concrete verification commands. |
| Evaluation | Yes | 10 test scenarios with expected results and commands. Edge cases documented. |
| Testing Strategy | Yes | Unit, integration, and contract tests specified. |
| Open Questions | Yes | Two deferred items (Serialize, for_each Vec) - appropriately scoped out. |

**Assessment**: All required sections are present and substantive.

## 2. Architecture Assessment

**Strengths**:

1. **Clean domain separation**: The two-domain model (execution failures vs validation failures) is architecturally sound and follows the CLO-182 contract precisely. The mutual exclusion invariant (`failure.is_some()` XOR `validation.passed == false`) is well-reasoned.

2. **Exhaustive call site enumeration**: The failure path mapping table (16 call sites) was verified against the codebase. The actual `grep -c 'StepResult::error('` count is 16, matching the table. Line numbers are accurate to within normal code drift tolerance.

3. **Single construction point**: Routing all failure metadata through `StepResult::error()` prevents classification divergence. This is the strongest architectural decision in the design.

4. **Backward compatibility**: The design preserves `StepResult.output` content unchanged, adds `failure: None` to success paths, and does not touch `ValidationResult` or `FailureType`.

5. **Prior art grounding**: The Temporal SDK pattern reference validates the typed failure hierarchy approach. The 6-variant enum avoids the `BackendError` overloading problem flagged in discovery.

**Concerns**:

1. **`EmptyOutput` variant has no current call site**: The `StepFailureKind::EmptyOutput` variant is defined as "Backend returned empty/whitespace output (no validate clause present)", but no existing `StepResult::error()` call site produces this error. There is no code path today that checks for empty output at the execution level and fails - empty output without a `validate` clause is treated as a success. This variant is speculative. It either needs:
   - A new code path that checks for empty output and fails (scope creep beyond "classify existing failures")
   - To be deferred until such a path exists
   - Clear documentation that it is a forward-looking placeholder

2. **`output.clone()` in `StepResult::error()`**: The proposed `error()` signature clones `output` to populate both `StepResult.output` and `StepFailure.message`. The design says "Keep `message` field content identical to `StepResult.output`" - if they are always identical, consider whether `message` is redundant. However, the field enables future divergence (e.g., truncated message for logs vs full output), so the duplication may be justified.

3. **`for_each` loop failure classification is underspecified**: The `for_each` loop at line 1374 builds a `StepResult` directly (not via `StepResult::error()`) with `success: all_success`. When `all_success` is false, this StepResult has `success: false` but `failure: None` and `validation: None`. This violates the contract invariant: "every StepResult with success=false has failure.is_some() OR validation.passed==false". The design acknowledges this in Open Questions ("Should for_each aggregate results carry a Vec<StepFailure>?") but defers it. This means the contract test will fail unless `for_each` is special-cased. The design should be explicit: either (a) classify the for_each aggregate failure with a new StepFailureKind, or (b) add an explicit exception in the contract test for for_each results.

## 3. ADR Compliance

No `docs/adrs/` directory exists in this project. However, the CLO-182 design document serves as the primary architectural decision record.

**CLO-182 Design Contract**:
- Line 32: "FailureType must be scoped to validation failures only" - **FOLLOWED**. The design creates a separate `StepFailureKind` enum.
- Line 134: "If structured execution failure metadata is needed later, a separate `failure_info: Option<FailureInfo>` field can be added" - **FOLLOWED**. The naming changed from `failure_info`/`FailureInfo` to `failure`/`StepFailure`, which is reasonable.
- `ValidationResult` and `FailureType` are explicitly listed as unchanged - **FOLLOWED**.

**Violations**: None.
**New ADR Needed**: No - this design implements a planned extension, not a new architectural decision.

## 4. Security Review

- **No secrets or credentials**: The design operates purely on in-memory data structures.
- **No new external dependencies**: Explicitly constrained.
- **No user input handling changes**: Failure classification is internal metadata, not exposed to external inputs.
- **No serialization**: `Serialize`/`Deserialize` are deferred, avoiding premature exposure.

**Assessment**: No security concerns. This is an internal data model change.

## 5. Implementation Concerns

1. **Phase ordering is correct**: Types first (Phase 1), then classification (Phase 2), then context threading (Phase 3), then tests (Phase 4). This minimizes compilation errors during implementation.

2. **Test construction site updates are tedious but mechanical**: Approximately 15 test `StepResult { ... }` construction sites need `failure: None` added. The design accounts for this but does not enumerate them - it should reference this as boilerplate work.

3. **`exit_code` threading in Phase 3**: The design mentions threading `exit_code` from shell output into `StepFailure` for call site #3, but the current `StepResult::error()` sets `exit_code: None`. The shell error path at line 1464 is inside the `Ok(Err(e))` branch where the process returned an error - the exit code may not be available from the error itself. The implementation may need to capture exit code from the shell output before entering the error path.

4. **Line number accuracy**: The design references specific line numbers (e.g., 1112, 1149, 1464). These were verified and are currently accurate. However, any prior modifications to `workflow.rs` between design authoring and implementation will shift them. The design should note that line numbers are approximate references.

## 6. Blind Spots

1. **`for_each` contract violation** (already mentioned above): The for_each loop produces `StepResult { success: false, failure: None, validation: None }` when any iteration fails. The design defers this but the contract test will surface it immediately. This needs a plan, not just an open question.

2. **No consumer for `StepFailure` in this task**: The design adds structured data but no code reads it. The discovery report flagged this as a high-severity concern. Without a consumer, the classification could silently drift. The contract test partially addresses this, but a concrete use case (e.g., conditional retry based on failure kind) would strengthen justification.

3. **`Eq` vs `PartialEq` on `StepFailureKind`**: The design derives `PartialEq` but not `Eq`. Since all variants are fieldless (no floating-point or other non-Eq types), `Eq` should also be derived. This is a minor omission but would be needed for `HashMap<StepFailureKind, _>` use cases.

4. **`Display` impl for `StepFailureKind`**: Not mentioned in the design. For logging and error messages, a `Display` or `Debug` format would be useful. `Debug` is derived, but human-readable display (e.g., "timeout", "backend_error") is not addressed.

5. **No `#[allow(dead_code)]` annotation**: The existing `StepResult` fields (`raw_output`, `stderr`, `exit_code`, `validation`) all have `#[allow(dead_code)]`. The new `failure` field will also trigger a dead_code warning since no code reads it. The design should note this.

6. **Multi-backend consensus failure classification**: Call site #6 (line 1546) "All backends failed" is classified as `BackendError`, but the error aggregates multiple backend failures that could individually be timeouts, creation errors, etc. The classification loses the per-backend failure detail. This is acceptable for the first iteration but worth noting.

7. **`elapsed_ms` accuracy for skip paths**: Call sites #1-2 (skip/dependency failures) set `elapsed_ms: 0` in the current code. The `StepFailure.elapsed_ms` will inherit this zero value, which is technically correct (no time was spent) but may be confusing in reporting. The design should confirm this is intentional.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is well-researched, architecturally sound, and properly respects the CLO-182 contract. The exhaustive call site mapping and mutual exclusion invariant demonstrate thorough analysis. The three suggestions below should be addressed before implementation begins but do not require fundamental design changes.

## 8. Actionable Feedback

**Priority 1 (Address before implementation)**:

1. **Resolve `for_each` contract violation**: Either (a) add a `StepFailureKind::PartialFailure` variant for for_each aggregate failures, (b) classify for_each failures as `BackendError` when appropriate, or (c) explicitly document that the contract test excludes for_each results and explain why. This is blocking because the contract test will fail without resolution.

2. **Justify or defer `EmptyOutput` variant**: Since no current call site produces this failure kind, either (a) add a code path that detects and fails on empty output at the execution level (requires scope expansion), or (b) remove the variant and add it when a code path needs it. Unused enum variants create confusion about what the system actually handles.

**Priority 2 (Address during implementation)**:

3. **Add `#[allow(dead_code)]` to `failure` field**: Consistent with existing `StepResult` field annotations. Without it, `cargo clippy` may warn.

4. **Derive `Eq` alongside `PartialEq` on `StepFailureKind`**: All variants are fieldless, so `Eq` is trivially correct and enables use in `HashMap` keys and `BTreeSet` if needed.

5. **Add `#[allow(dead_code)]` to `StepFailure` struct and `StepFailureKind` enum**: Same pattern as existing `ValidationResult` and `FailureType`.

**Priority 3 (Nice to have)**:

6. **Note line numbers are approximate**: Add a caveat that line numbers reference the codebase at design time and may shift.

7. **Consider `Display` impl for `StepFailureKind`**: Would enable `println!("Failure: {}", failure.kind)` rather than relying on `Debug` output.

---

*This review was performed by analyzing the design document against the actual source code in `src/workflow.rs` (5042 lines), the CLO-182 design document, the discovery report, and project context files.*
