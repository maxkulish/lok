# Lok Validation Resilience Plan: CLO-214 / CLO-215 / CLO-216

**Date**: 2026-04-07
**Tasks**: [CLO-214](https://linear.app/cloud-ai/issue/CLO-214), [CLO-215](https://linear.app/cloud-ai/issue/CLO-215), [CLO-216](https://linear.app/cloud-ai/issue/CLO-216)
**Driven by**: Mentis pre-PR validation fail-close incident (2026-04-07) where Haiku returned unparseable markdown instead of the CLO-184 JSON contract, causing the entire pipeline to fail even though the underlying Gemini review was valid.
**Depends on**: [CLO-184](https://linear.app/cloud-ai/issue/CLO-184) (LLM validation), [CLO-182](https://linear.app/cloud-ai/issue/CLO-182) (StepResult extensions)

---

## Current State

All three tasks extend code that already exists in `src/workflow.rs`:

- `ValidateConfig` struct (`workflow.rs:166-193`) - add fields here
- `ValidationResponse` internal struct (`workflow.rs:541-545`) - small, used only by parser
- `parse_validation_response()` (`workflow.rs:549-580`) - fail-closed today, needs new branches
- `run_llm_validation()` (`workflow.rs:584-760`) - has `handle_infra_error` helper for `on_error`; parse-error branch at line 720 currently ignores it
- `ValidationResult` struct (`workflow.rs:865-872`) - needs new field for raw response
- `StepResult.raw_output` (`workflow.rs:812`) - already exists from CLO-182 (pre-validation output, different purpose)
- CLI `Run` command (`main.rs:262-277`) - add new `--explain-validation` flag

No design docs exist yet for any of the three tasks. Entry point is `/task:orchestrate CLO-XX` which triggers the standard Discovery -> Design -> Plan -> Implement -> PR flow.

---

## Recommended Implementation Order

Order by user impact (fixes the pain point first), not by dependency graph. None of the three are blockers for each other.

| # | Task | Why this order |
|---|---|---|
| 1 | **CLO-214** on_parse_error | Directly unblocks Mentis pre-PR validation. Smallest diff. Standalone. |
| 2 | **CLO-215** --explain-validation | Needs raw_response capture, which is a small shared primitive 214/216 can also use for better error messages. |
| 3 | **CLO-216** mode = "lenient" | Alternative architectural path. Independent of 214/215 behavior. Less urgent because 214 already fixes the bug. |

---

## CLO-214: `validate.on_parse_error` config

**Scope**: One new field, one branch change. ~30 lines of prod code plus tests.

### Files

- `src/workflow.rs` - `ValidateConfig` (line 166), `run_llm_validation` parse branch (line 720)
- `tests/integration.rs` - new test file or extend `test_llm_validate.toml`
- `examples/workflows/*.toml` - optional: document usage

### Config schema

```rust
/// Policy when validator response cannot be parsed: "fail" (default), "pass", "skip"
/// Separate from on_error which handles backend/network errors.
#[serde(default)]
pub on_parse_error: Option<String>,
```

### Behavior decision

Parse errors and backend errors are different failure modes. Keep them separate:

| Failure | Current | After CLO-214 |
|---|---|---|
| Backend network/timeout | `on_error` ("fail"/"pass"/"skip") | unchanged |
| Parser cannot understand response | always `ValidatorError` (fail-closed) | `on_parse_error` ("fail"/"pass"/"skip"), default "fail" |

Keeping these separate is important: a user might want `on_error = "fail"` (network flakes should fail the step) but `on_parse_error = "pass"` (format drift in cheap validator LLM should not fail the step).

### Implementation sketch

At `workflow.rs:720`:

```rust
Err(parse_err) => {
    let on_parse_error = validate_config.on_parse_error.as_deref().unwrap_or("fail");
    match on_parse_error {
        "pass" => (Some(pass_passthrough_result(...)), None),
        "skip" => (None, None),
        _ => (Some(error_result(FailureType::ValidatorError, ...)), None),
    }
}
```

Extract a `handle_parse_error` helper mirroring `handle_infra_error` on line 597. Both share the `pass`/`skip`/`fail` shape.

### Tests

- `test_on_parse_error_pass`: unparseable Haiku response plus `on_parse_error = "pass"` -> step succeeds
- `test_on_parse_error_skip`: `on_parse_error = "skip"` -> validation = None, step succeeds with original output
- `test_on_parse_error_fail_default`: no config -> fail-closed (backward compat)
- `test_on_parse_error_independent_of_on_error`: `on_error = "fail"` plus `on_parse_error = "pass"` work independently
- `test_validate_config_parsing`: TOML round-trip of new field

### Backward compatibility

Default `"fail"` preserves current fail-closed behavior. Zero risk to existing workflows.

---

## CLO-215: `--explain-validation` CLI flag

**Scope**: New field on `ValidationResult`, populate in parse error path, new CLI flag, new print path. ~100 lines.

### Files

- `src/workflow.rs` - `ValidationResult` (line 865), `run_llm_validation` response capture (line 688-733), `print_results` (line 3127)
- `src/main.rs` - `Run` command (line 262) plus handler
- `tests/` - unit test for field population, snapshot test for CLI output

### Struct change

```rust
pub struct ValidationResult {
    pub passed: bool,
    pub failure_type: Option<FailureType>,
    pub failure_reason: Option<String>,
    pub validator: String,
    pub elapsed_ms: u64,
    /// Raw validator LLM response. Populated when validation fails with
    /// ValidatorError (parse failure). Used by --explain-validation to
    /// surface the full unparseable response for debugging.
    #[allow(dead_code)]
    pub raw_response: Option<String>,
}
```

### Decision: only populate on parse failure, not always

Two reasons:

1. **Memory**: storing every validator response doubles validation memory cost for large outputs
2. **Intent**: the field is specifically for diagnosing parse failures. Success responses are already in `output` (when `replace_output = true`)

### Populate at line 720

```rust
Err(parse_err) => {
    let raw = query_output.stdout.clone();
    // ... existing ValidatorError construction ...
    raw_response: Some(raw),
}
```

### CLI flag

```rust
Run {
    name: String,
    #[arg(short, long, default_value = ".")]
    dir: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Dump raw validator responses on parse failures for debugging
    #[arg(long)]
    explain_validation: bool,
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
},
```

Thread the flag through `workflow.run()` -> step execution -> output formatter. Alternatively: add it as a field on the `Workflow` struct set at load time. Prefer the threaded approach to avoid mutable global state.

### Print output on parse failure when flag is set

```
✗ Validation failed (llm:claude): Failed to parse validation response
  Error: Unrecognized validation response format...

  --- Raw validator response (4231 chars) ---
  ### Verdict: FAIL
  ### Findings
  ...
  --- End raw response ---
```

Without the flag, current truncated error message is preserved (no behavior change).

### Tests

- `test_validation_result_captures_raw_on_parse_error`
- `test_validation_result_raw_none_on_success`
- CLI integration test: run workflow with parse failure plus `--explain-validation`, assert raw response in stdout

### Open question for design phase

Should `raw_response` be wired into `StepResult` too (for programmatic access, e.g. from synthesis prompts via `{{ steps.X.validation.raw_response }}`)? Probably yes. Same field, additional plumbing. Capture as an open question in the design doc.

---

## CLO-216: `validate.mode = "lenient"`

**Scope**: New config field, new branch in `run_llm_validation`, parser bypass. ~60 lines.

### Files

- `src/workflow.rs` - `ValidateConfig` (line 166), `run_llm_validation` (line 688)
- `tests/workflows/test_llm_validate.toml` - new test cases

### Config schema

```rust
/// Validation strictness mode: "strict" (default) requires JSON or REVIEW_FAILED,
/// "lenient" treats any non-empty response as pass with the full response as cleaned output.
#[serde(default)]
pub mode: Option<String>,
```

### Semantics

| Mode | Valid response | Parse logic |
|---|---|---|
| `"strict"` (default) | Must be JSON `{"status": ...}` or `REVIEW_FAILED:` prefix | `parse_validation_response` (current behavior) |
| `"lenient"` | Any non-empty, non-whitespace response | Skip parser. Response becomes cleaned output (if `replace_output = true`), always passes |

### Implementation

Branch before calling `parse_validation_response` at line 691:

```rust
Ok(query_output) => {
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let mode = validate_config.mode.as_deref().unwrap_or("strict");

    if mode == "lenient" {
        let trimmed = query_output.stdout.trim();
        if trimmed.is_empty() {
            return fail_result("Validator returned empty response", ...);
        }
        return pass_result_with_cleaned(trimmed.to_string(), ...);
    }

    match parse_validation_response(&query_output.stdout) {
        // existing strict logic
    }
}
```

### Interaction with replace_output

In lenient mode, the full validator response IS the cleaned output. The validator's role degrades to "summarizer/noise-stripper". Pure post-processor with no structured verdict.

### Use case

Exactly Mentis's pre-PR validation. The prompts want Haiku to strip MCP noise, not adjudicate pass/fail. Lenient mode matches that intent cleanly.

### Tests

- `test_lenient_mode_passes_any_response`: non-empty JSON garbage -> pass with full content
- `test_lenient_mode_empty_response_fails`: empty/whitespace-only -> ValidatorError
- `test_lenient_mode_with_replace_output`: content flows to downstream step
- `test_strict_mode_default`: no `mode` field -> parser still runs (backward compat)

### Validation

Add friendly error if user passes `mode = "foo"` (invalid value). Validate at config load time, not at run time.

---

## Cross-Cutting Concerns

### Documentation updates

| File | Change |
|---|---|
| `README.md` | Add section on validation modes and parse error policy |
| `docs/PROJECT.md` | Add CLO-214/215/216 to Recently Completed after each ships |
| `docs/ROADMAP.md` | Move the three from Planned -> Done |
| `docs/design-docs/clo-184-llm-step-validation.md` | Link to successor docs in References section |

### Tests consolidation

All three tasks add cases to `tests/workflows/test_llm_validate.toml` plus `tests/integration.rs`. Keep test isolation: one toml file per scenario group.

### Phase assignment

Create a new phase group in `docs/ROADMAP.md`:

```
## Phase 2.5: Validation Resilience
| CLO-214 | on_parse_error config | Backlog | CLO-184 |
| CLO-215 | --explain-validation CLI | Backlog | CLO-184 |
| CLO-216 | lenient mode | Backlog | CLO-184 |
```

### Shared refactor opportunity

Both CLO-214 and existing `on_error` use a `match { "pass" => ..., "skip" => ..., _ => fail }` pattern (lines 597, 738). After CLO-214 lands, extract a `PolicyOutcome` helper enum plus `apply_policy()` function. Defer until all three land. Premature during CLO-214 implementation.

### Migration guide for existing users

None of the three tasks are breaking. All new config fields default to current behavior. Document as additive changes in each release notes entry.

---

## Task Orchestration Flow

Using the existing `/task:orchestrate` workflow:

```
1. /task:orchestrate CLO-214
   - Discovery (lightweight - already scoped in this plan)
   - Design doc (inherits from this plan)
   - Plan
   - Implement + tests
   - PR + review + merge

2. /task:orchestrate CLO-215
   - Same flow; design phase addresses open question about StepResult wiring

3. /task:orchestrate CLO-216
   - Same flow; implementation references both CLO-214 patterns (for policy enum) and CLO-215 (for raw_response field reuse)
```

After all three land, file a follow-up task **CLO-217: Refactor validation policy handling into PolicyOutcome enum** to consolidate `on_error` and `on_parse_error` into shared code.

---

## Out of Scope

- `validate.retries` (retry validator LLM on parse failure). Would be a fourth task; premature until usage data shows parse failures are transient
- Validator response streaming. Not relevant for Haiku-sized responses
- Per-workflow override of `on_parse_error` via CLI flag. Can be added later if needed
- MCP init noise stripping as a built-in filter. Belongs in the shell step, not validation layer

The scope above is the minimum to give Mentis (and any other lok user) a stable, debuggable validation pipeline.

---

## References

- [CLO-184 Design Doc](../design-docs/clo-184-llm-step-validation.md) - parent task
- [CLO-182 Design Doc](../design-docs/clo-182-stepresult-extensions.md) - StepResult.raw_output predecessor
- [CLO-185 Design Doc](../design-docs/clo-185-structured-failure-data.md) - StepFailure predecessor
- Mentis incident context: `~/Code/mentis/.lok/workflows/pre-pr-validation.toml` pre-2026-04-07 fix
