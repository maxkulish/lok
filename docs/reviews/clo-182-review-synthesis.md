# Review Synthesis: CLO-182 - StepResult Extensions

**Synthesized**: 2026-03-31
**Pipeline**: Claude Code direct analysis + lok design-review (pending)
**Design Document**: docs/design-docs/clo-182-stepresult-extensions.md

---

## 1. Completeness Check

| Section | Present | Assessment |
|---------|---------|------------|
| Summary | Yes | Clear, concise problem statement with solution scope |
| Background | Yes | Strong - links CLO-180, explains data flow gap |
| Architecture | Yes | ASCII diagram is helpful; execution path breakdown is thorough |
| Detailed Design | Yes | Rust code samples for all new types and modifications |
| Implementation Plan | Yes | 6 phases, well-ordered |
| Constraints | Yes | Explicit must/must-not/prefer/escalate boundaries |
| Acceptance Criteria | Yes | 7 criteria with exact verification commands |
| Evaluation | Yes | 8 tests with expected results and edge cases |
| Testing Strategy | Yes | Explains why no new tests needed (defers to CLO-183) |
| Open Questions | Yes | 1 explicit decision point (run_shell behavior change) |

**Missing**: No rollback plan section. If the `run_shell()` stdout/stderr separation causes downstream issues in workflows that relied on merged output, there's no documented fallback.

## 2. Architecture Assessment

**Strengths**:
- Discovery-driven design: all three killer assumptions from the discovery report are addressed. `raw_output` is `Option<String>`, `FailureType` is scoped to validation only, and threading paths are explicitly categorized.
- The design correctly defers complex threading (consensus, for_each paths) rather than over-engineering.
- `ShellOutput` kept private to workflow.rs - good encapsulation.
- `FailureType` with only 2 variants (not 5) is the right call. The discovery report's reasoning about execution vs validation failure domains is solid.

**Concerns**:
1. **Construction site count is wrong**: The design states "23 production construction sites" and "12 test construction sites" (35 total). Source code analysis shows **20 production + 13 test = 33 total**. This needs correction before implementation to avoid missing sites.
2. **Fix-loop re-query paths are overlooked**: Lines 1396-1398 and 1444-1446 do `qo.stdout` extraction during fix retries (apply/verify cycle). These `qo` accesses should also thread `stderr`/`exit_code` to keep the StepResult consistent if the final output came from a retry, not the initial query. The design document doesn't mention these paths.
3. **Synthesis-path `qo.stdout` at line 1080**: The synthesis backend query also extracts `qo.stdout` and discards stderr/exit_code. While the design says consensus paths get `None`, the synthesis sub-query within the consensus path should be mentioned explicitly.

## 3. Codebase Alignment

The design follows existing patterns well:
- Uses `Option<T>` for new fields (consistent with `parsed_output: Option<serde_json::Value>` and `backend: Option<String>`)
- `run_shell()` signature change from `Result<String>` to `Result<ShellOutput>` follows the same pattern as `Backend::query()` returning `QueryOutput`
- New types defined in workflow.rs where they're consumed (not in a separate types module) - consistent with current codebase

**Violations**: None identified.

**Pattern observation**: The codebase uses `anyhow::bail!` in `run_shell()` for error cases, and the design preserves this pattern.

## 4. Security Review

No security concerns for this change. The new fields are all data passthrough - no new subprocess spawning, no new file I/O, no new user input handling. `stderr` content is already captured by existing `Command` invocations and simply routed to a new field rather than being discarded or concatenated.

## 5. Implementation Concerns

1. **The `run_shell()` behavioral change is the riskiest part**: Currently `format!("{}{}", stdout, stderr).trim()` means shell step output may include stderr content. After the change, only stdout goes to `output`. Any existing workflow that depends on seeing stderr content in `{{ steps.X.output }}` will break silently. The design doc correctly identifies this in the Open Questions section but doesn't resolve it. **This decision must be made before implementation**, not left open.

2. **No `#[derive(Default)]` on StepResult**: The design explicitly prefers explicit `None` at each site, which is fine for CLO-182. But adding 4 fields to 33 sites without Default means a lot of repetitive `None, None, None, None` additions. Consider at minimum a constructor helper like `StepResult::error(name, output, elapsed_ms, backend)` for the 19 error-path sites.

3. **Phase ordering is correct**: Types first, then shell change, then single-backend threading, then remaining sites. This avoids intermediate compilation failures.

4. **`Serialize`/`Deserialize` not derived on new types**: The design notes `serde` as a dependency for "future use" but doesn't derive the traits. If StepResult ever needs serialization (for structured logging, workflow debugging), this becomes a follow-up. Fine for now, but worth noting.

## 6. Blind Spots

### Missing edge cases:
- **Shell step with zero-length stdout but non-empty stderr**: After the change, `StepResult.output` would be empty, `StepResult.stderr` would have content. This could cause issues if downstream steps check `contains(step.output, "...")` expecting stderr content.
- **run_shell with command_wrapper**: The design doesn't mention whether `ShellOutput` correctly handles stderr from wrapped commands (e.g., `nix-shell --run '{cmd}'`). The wrapper's own stderr would be mixed with the inner command's stderr.

### Unstated assumptions:
- **StepResult is never destructured**: The design assumes no code uses `let StepResult { name, output, .. } = result;` pattern. If any destructuring exists without `..`, adding fields would be a compile error. Verified: the codebase uses field access (`.output`, `.success`), not destructuring. Assumption holds.
- **Clone cost increase is acceptable**: StepResult derives Clone. Adding 4 Option fields increases clone cost, especially if `stderr` contains large strings (CLI backends can produce verbose stderr). The design doesn't quantify this.

### Overlooked failure modes:
- **exit_code of None for processes that were killed**: `output.status.code()` returns `None` on Unix when a process was killed by a signal (SIGKILL, SIGTERM). The design uses `Option<i32>` which handles this, but the doc says "None for API backends" without mentioning signal-killed processes.

### Missing cross-cutting concerns:
- **Template interpolation for new fields**: `{{ steps.X.stderr }}` and `{{ steps.X.exit_code }}` are not accessible via the current interpolation engine. The interpolation system reads `output` and `parsed_output` JSON fields but has no path to raw struct fields. This is explicitly deferred (noted in the discovery report) but not mentioned in the design doc itself.
- **`format_results()` and `print_results()` at lines 2620-2640**: These functions iterate over StepResult and print output. They should arguably also show stderr/exit_code when present (for verbose/debug mode). Not mentioned in the design.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is well-researched, properly scoped, and correctly addresses the three killer assumptions from the discovery report. The fundamental approach - adding Option fields to StepResult, creating scoped ValidationResult/FailureType types, and threading QueryOutput data through specific paths - is sound.

The suggestions below should be addressed before implementation but don't require a design revision.

## 8. Actionable Feedback

**Priority 1 (must fix before implementation)**:
1. **Resolve the Open Question**: Decide whether `run_shell()` separates stdout/stderr in CLO-182 or defers it. Recommendation: do it in CLO-182 since the PRD requires it (FR-1) and delaying creates a harder migration later. Add a note in CHANGELOG or migration docs about the behavioral change.
2. **Fix construction site count**: Update from "23 production + 12 test = 35" to "20 production + 13 test = 33". Incorrect counts lead to missed sites during implementation.

**Priority 2 (should fix before implementation)**:
3. **Address fix-loop re-query paths**: Lines 1396-1398 and 1444-1446 extract `qo.stdout` during fix retries. Either thread stderr/exit_code from these re-queries to update `step_stderr`/`step_exit_code`, or document why the initial query's stderr/exit_code is retained even after a fix retry.
4. **Document empty-stdout-with-stderr edge case**: When `run_shell()` produces empty stdout but non-empty stderr on success (exit 0), `StepResult.output` will be empty. Document whether this is expected behavior or if stderr should be used as fallback.

**Priority 3 (nice to have)**:
5. **Consider a `StepResult::error()` constructor**: A helper for the ~19 error-path sites would reduce boilerplate: `StepResult::error(name, output, elapsed_ms, backend)` setting all new fields to `None`.
6. **Mention `print_results()`/`format_results()`**: These functions display step results and should show stderr/exit_code when present, at least in verbose mode. Can be a follow-up but worth noting.
7. **Document signal-killed process semantics**: Note that `exit_code: None` can mean either "API backend" or "process killed by signal", not just "API backend".

---

*This review was produced by direct source code analysis against the design document. External AI reviews (Gemini, Ollama) were dispatched via lok but are pending completion.*
