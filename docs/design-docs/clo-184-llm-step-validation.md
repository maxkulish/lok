# CLO-184: Implement LLM-Based Step Validation

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-184
**Status**: Finalized
**Finalized**: 2026-04-03
**Approved By**: MK
**Author**: MK
**Created**: 2026-04-03

---

## Summary

Add LLM-based output validation to Lok workflow steps. After a step executes and passes its optional heuristic check (CLO-183), a cheap/fast LLM (Haiku, Gemini Flash) evaluates the output for semantic validity, returning a structured JSON verdict. On pass, the cleaned output optionally replaces the raw output for downstream consumption. On fail, the step is marked as failed with structured error data.

---

## Background

Lok's validation pipeline currently has two layers:
- **Layer 0**: Process-level checks (exit code, timeout) - built into the Backend trait
- **Layer 1**: Heuristic checks (not_empty, min_length, contains) - implemented in CLO-183

Layer 1 catches obvious failures (empty output, truncated output, missing markers) but cannot distinguish MCP initialization noise from partial reviews, or detect semantically invalid output that happens to pass string checks. CLO-184 adds:

- **Layer 2**: LLM-based semantic validation using a cheap model as an independent judge

The layered approach minimizes cost: heuristic failure skips the LLM call entirely.

### Prior Research

**Discovery report**: `docs/prds/discovery-report-2026-04-03-clo-184.md`

Key findings from multi-model discovery:

1. **LLM-as-Judge pattern is well-validated** (MT-Bench NeurIPS 2023): Strong LLM judges achieve 80%+ agreement with humans. Using a different model avoids self-enhancement bias (CALM framework).

2. **Structured output over in-band signals**: Every mature framework (Guardrails AI, Instructor, LangChain) uses JSON/typed output rather than text-based pass/fail signals. The original PRD's `REVIEW_FAILED:` prefix was identified as fragile (prompt injection, false positives from step output containing the signal).

3. **Retry-with-feedback pattern** (DSPy Assertions ICLR 2024): Feeding validator failure reasons back to the generator improved compliance by 164%. Lok's existing `fix_retries` mechanism already implements this for `verify` - extending to validation is natural.

4. **Lok's unique differentiators**: No existing tool combines (a) cheap model validates expensive model, (b) heuristic-then-LLM tiering, (c) output cleaning, and (d) CLI process awareness in a declarative workflow config.

5. **Critical design changes from discovery**:
   - Use JSON structured output instead of REVIEW_FAILED: text signal
   - Add `validate.on_error` for infrastructure failure policy
   - Add `validate.max_input_length` for context window protection
   - Extract unified `run_validation()` function
   - Pass/fail mode by default, output replacement opt-in

### Dependencies (completed)

- **CLO-180** (Done): QueryOutput struct with stdout/stderr/exit_code
- **CLO-181** (Done): Per-step model override for Backend::query()
- **CLO-182** (Done): StepResult with stderr, exit_code, validation fields
- **CLO-183** (Done): Heuristic validators (check field) with ValidateConfig parsing

---

## Architecture

### Component Overview

The validation system sits between step execution and result propagation:

```
Step executes -> Backend.query() returns QueryOutput
                          |
                  Has validate config?
                          |
                 +--------+--------+
                 |                 |
              No |              Yes|
                 |                 |
           pass through    Has heuristic check?
                          /              \
                    Yes  /                \ No
                        /                  \
               Run check                    |
              /         \                   |
          Fail           Pass               |
           |              \                 |
      skip LLM        Has LLM backend?  Has LLM backend?
      mark fail       /           \     /            \
                   Yes             No  Yes            No
                    |               |   |              |
             Run LLM validation  pass  Run LLM     pass through
                    |           through validation
                    |                   |
             Parse JSON verdict         |
             /              \           |
          pass              fail        |
            |                 |         |
     replace output?    mark failed     |
     (if opt-in)                        |
            |                           |
         StepResult                  StepResult
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/workflow.rs` | Modified | Add `run_llm_validation()`, `run_step_validation()` functions; replace 3 inline heuristic blocks with unified call; add new ValidateConfig fields |
| `src/workflow.rs` | Modified | Add `ValidatorError` to `FailureType` enum |
| `src/backend/mod.rs` | Read-only | Use existing `create_backend()` and `Backend::query()` |
| `tests/integration.rs` | Modified | Add LLM validation integration tests using shell mock |

### Dependencies

- **Internal**: `backend::create_backend()`, `Backend::query()` (model override), `run_heuristic_check()`, `ValidateConfig`, `ValidationResult`, `FailureType`, `StepResult`
- **External**: `serde_json` (already a dependency), `tokio` (already a dependency)

---

## Detailed Design

### 1. ValidateConfig Extensions

Add three new fields to the existing `ValidateConfig` struct (lines 250-260):

```rust
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ValidateConfig {
    #[serde(default)]
    pub check: Option<String>,         // existing - heuristic check
    #[serde(default)]
    pub backend: Option<String>,       // existing field, now wired
    #[serde(default)]
    pub model: Option<String>,         // existing field, now wired
    #[serde(default)]
    pub prompt: Option<String>,        // existing field, now wired
    #[serde(default = "default_on_error")]
    pub on_error: Option<String>,      // NEW: "fail" (default), "pass", "skip"
    #[serde(default)]
    pub max_input_length: Option<usize>, // NEW: truncate {{ output }} before interpolation
    #[serde(default)]
    pub replace_output: bool,          // NEW: whether to replace output with cleaned version
    #[serde(default)]
    pub timeout_ms: Option<u64>,       // NEW: validation-specific timeout (overrides backend default)
}

fn default_on_error() -> Option<String> {
    Some("fail".to_string())
}
```

### 2. FailureType Extension

Add a `ValidatorError` variant for infrastructure failures:

```rust
#[derive(Debug, Clone)]
pub enum FailureType {
    ValidationFailed,   // existing - output failed validation check
    EmptyOutput,        // existing - output was empty/whitespace
    ValidatorError,     // NEW - validation backend itself failed (timeout, API error)
}
```

### 3. Validation Prompt Interpolation

The validation prompt uses `{{ output }}` and optionally `{{ stderr }}` - a different namespace from the step interpolation system (`{{ steps.X.output }}`). This is intentional: validation prompts operate on the current step's output, not on named step results.

```rust
fn interpolate_validation_prompt(
    prompt: &str,
    output: &str,
    stderr: Option<&str>,
    max_input_length: Option<usize>,
) -> String {
    let truncated_output = match max_input_length {
        Some(max) if output.len() > max => {
            format!(
                "{}\n\n[TRUNCATED - original was {} chars, showing first {}]",
                &output[..max],
                output.len(),
                max
            )
        }
        _ => output.to_string(),
    };

    // IMPORTANT: Single-pass replacement to prevent injection.
    // If output contains "{{ stderr }}", sequential .replace() would expand it.
    // Instead, scan the prompt once and replace each placeholder as encountered.
    let stderr_val = stderr.unwrap_or("");
    let mut result = String::with_capacity(prompt.len() + truncated_output.len() + stderr_val.len());
    let mut remaining = prompt;

    while !remaining.is_empty() {
        if let Some(pos) = remaining.find("{{") {
            result.push_str(&remaining[..pos]);
            let after = &remaining[pos..];
            if after.starts_with("{{ output }}") {
                result.push_str(&truncated_output);
                remaining = &after["{{ output }}".len()..];
            } else if after.starts_with("{{ stderr }}") {
                result.push_str(stderr_val);
                remaining = &after["{{ stderr }}".len()..];
            } else {
                // Unknown placeholder - preserve as-is
                result.push_str("{{");
                remaining = &after[2..];
            }
        } else {
            result.push_str(remaining);
            break;
        }
    }

    result
}
```

### 4. LLM Validation Response Parsing

The validator LLM must return JSON with this structure:

```json
{"status": "pass", "output": "cleaned content here"}
```

or:

```json
{"status": "fail", "reason": "why the output is invalid"}
```

Parsing logic:

```rust
#[derive(Debug, Deserialize)]
struct ValidationResponse {
    status: String,           // "pass" or "fail"
    output: Option<String>,   // cleaned content (when pass)
    reason: Option<String>,   // failure reason (when fail)
}

/// Strip markdown code fences that LLMs frequently wrap JSON in.
/// Handles ```json ... ``` and ``` ... ``` patterns.
fn strip_markdown_fences(response: &str) -> &str {
    let trimmed = response.trim();
    if let Some(after_fence) = trimmed.strip_prefix("```json") {
        if let Some(content) = after_fence.strip_suffix("```") {
            return content.trim();
        }
    }
    if let Some(after_fence) = trimmed.strip_prefix("```") {
        if let Some(content) = after_fence.strip_suffix("```") {
            return content.trim();
        }
    }
    trimmed
}

fn parse_validation_response(response: &str) -> Result<ValidationResponse, String> {
    // Strip markdown fences that LLMs commonly add around JSON
    let cleaned = strip_markdown_fences(response);

    // Try JSON first (preferred structured format)
    if let Ok(parsed) = serde_json::from_str::<ValidationResponse>(cleaned) {
        if parsed.status == "pass" || parsed.status == "fail" {
            return Ok(parsed);
        }
        return Err(format!("Invalid status value: '{}' (expected 'pass' or 'fail')", parsed.status));
    }

    // Fallback: check for REVIEW_FAILED: prefix (backward compat)
    if cleaned.starts_with("REVIEW_FAILED:") {
        let reason = cleaned.strip_prefix("REVIEW_FAILED:").unwrap().trim();
        return Ok(ValidationResponse {
            status: "fail".to_string(),
            output: None,
            reason: Some(reason.to_string()),
        });
    }

    // FAIL-CLOSED: unrecognized response format is a ValidatorError, NOT a pass.
    // This prevents LLM refusals, garbage, or off-topic responses from silently
    // passing validation and replacing the step's output.
    Err(format!(
        "Unrecognized validation response format (expected JSON or REVIEW_FAILED: prefix). Got: {}",
        &cleaned[..cleaned.len().min(200)]
    ))
}
```

The parsing chain is **fail-closed**:
1. **JSON response** (with markdown fence stripping): Preferred, structured, unambiguous
2. **REVIEW_FAILED: prefix**: Backward compatibility with PRD examples
3. **Unrecognized format**: Returns `Err` - treated as `ValidatorError`, NOT a pass. This prevents LLM refusals, garbage, or off-topic responses from silently passing validation.

### 5. Unified Validation Function

Extract a single async function that handles both heuristic and LLM validation:

```rust
async fn run_step_validation(
    output: &str,
    stderr: Option<&str>,
    validate_config: &ValidateConfig,
    config: &Config,
    cwd: &Path,
) -> (Option<ValidationResult>, Option<String>) {
    // Returns: (validation_result, optional_cleaned_output)

    // Phase 1: Heuristic check
    if let Some(check) = validate_config.check.as_deref().filter(|c| !c.trim().is_empty()) {
        let heuristic_result = run_heuristic_check(check, output);
        if !heuristic_result.passed {
            // Heuristic failed - skip LLM validation (save cost)
            return (Some(heuristic_result), None);
        }
    }

    // Phase 2: LLM validation (only if backend is configured)
    if let Some(backend_name) = validate_config.backend.as_deref() {
        return run_llm_validation(output, stderr, validate_config, backend_name, config, cwd).await;
    }

    // No LLM validation configured - pass through
    (None, None) // heuristic-only result already returned above if it ran
}
```

Note: When only a heuristic check is configured (no `backend`), the heuristic result is returned. When both are configured, heuristic gates LLM. When only LLM is configured, it runs directly.

### 6. LLM Validation Core

```rust
async fn run_llm_validation(
    output: &str,
    stderr: Option<&str>,
    validate_config: &ValidateConfig,
    backend_name: &str,
    config: &Config,
    cwd: &Path,
) -> (Option<ValidationResult>, Option<String>) {
    let start = std::time::Instant::now();

    // 1. Get backend config and create backend instance
    let backend_config = match config.backends.get(backend_name) {
        Some(cfg) => cfg,
        None => {
            return (Some(ValidationResult {
                passed: false,
                failure_type: Some(FailureType::ValidatorError),
                failure_reason: Some(format!("Validation backend not found: {}", backend_name)),
                validator: format!("llm:{}", backend_name),
                elapsed_ms: start.elapsed().as_millis() as u64,
            }), None);
        }
    };

    let backend = match create_backend(backend_name, backend_config) {
        Ok(b) => b,
        Err(e) => {
            return (Some(ValidationResult {
                passed: false,
                failure_type: Some(FailureType::ValidatorError),
                failure_reason: Some(format!("Failed to create validation backend: {}", e)),
                validator: format!("llm:{}", backend_name),
                elapsed_ms: start.elapsed().as_millis() as u64,
            }), None);
        }
    };

    // 2. Build validation prompt
    let prompt = match validate_config.prompt.as_deref() {
        Some(p) => interpolate_validation_prompt(
            p, output, stderr, validate_config.max_input_length,
        ),
        None => {
            return (Some(ValidationResult {
                passed: false,
                failure_type: Some(FailureType::ValidatorError),
                failure_reason: Some("validate.prompt is required when validate.backend is set".to_string()),
                validator: format!("llm:{}", backend_name),
                elapsed_ms: start.elapsed().as_millis() as u64,
            }), None);
        }
    };

    // 3. Query validation backend (with optional validation-specific timeout)
    let model_override = validate_config.model.as_deref();
    let query_future = backend.query(&prompt, cwd, model_override);
    let query_result = match validate_config.timeout_ms {
        Some(timeout) => {
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout),
                query_future,
            ).await {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!("Validation timed out after {}ms", timeout)),
            }
        }
        None => query_future.await,
    };
    match query_result {
        Ok(query_output) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;

            // 4. Parse response
            match parse_validation_response(&query_output.stdout) {
                Ok(response) => {
                    if response.status == "pass" {
                        let cleaned = response.output;
                        (Some(ValidationResult {
                            passed: true,
                            failure_type: None,
                            failure_reason: None,
                            validator: format!("llm:{}", backend_name),
                            elapsed_ms,
                        }), cleaned) // cleaned output returned separately
                    } else {
                        let reason = response.reason.unwrap_or_else(|| "Validation failed".to_string());
                        (Some(ValidationResult {
                            passed: false,
                            failure_type: Some(FailureType::ValidationFailed),
                            failure_reason: Some(reason),
                            validator: format!("llm:{}", backend_name),
                            elapsed_ms,
                        }), None)
                    }
                }
                Err(parse_err) => {
                    (Some(ValidationResult {
                        passed: false,
                        failure_type: Some(FailureType::ValidatorError),
                        failure_reason: Some(format!("Failed to parse validation response: {}", parse_err)),
                        validator: format!("llm:{}", backend_name),
                        elapsed_ms,
                    }), None)
                }
            }
        }
        Err(e) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let on_error = validate_config.on_error.as_deref().unwrap_or("fail");

            match on_error {
                "pass" => (Some(ValidationResult {
                    passed: true,
                    failure_type: None,
                    failure_reason: None,
                    validator: format!("llm:{}:error_passthrough", backend_name),
                    elapsed_ms,
                }), None),
                "skip" => (None, None),
                _ => (Some(ValidationResult { // "fail" default
                    passed: false,
                    failure_type: Some(FailureType::ValidatorError),
                    failure_reason: Some(format!("Validation backend error: {}", e)),
                    validator: format!("llm:{}", backend_name),
                    elapsed_ms,
                }), None),
            }
        }
    }
}
```

### 7. Wiring Into Step Execution

Replace the three inline heuristic validation blocks with a single unified call. Each of the three sites (shell ~L1076, multi-backend ~L1304, apply/verify ~L1624) changes from:

```rust
// BEFORE: inline heuristic only (12 lines, duplicated 3x)
let validation = validate_config
    .as_ref()
    .and_then(|vc| vc.check.as_deref())
    .filter(|c| !c.trim().is_empty())
    .map(|check| run_heuristic_check(check, &output));
let validation_passed = validation.as_ref().map(|v| v.passed).unwrap_or(true);
```

To:

```rust
// AFTER: unified validation (heuristic + LLM)
let (validation, cleaned_output) = match validate_config.as_ref() {
    Some(vc) => run_step_validation(&output, stderr.as_deref(), vc, &config, &cwd).await,
    None => (None, None),
};
let validation_passed = validation.as_ref().map(|v| v.passed).unwrap_or(true);

if !validation_passed {
    if let Some(ref v) = validation {
        let reason = v.failure_reason.as_deref().unwrap_or("validation failed");
        println!("  {} Validation failed ({}): {}", "✗".red(), v.validator, reason);
    }
}

// Apply cleaned output if validation passed and replace_output is enabled
let (final_output, raw_output) = if let Some(cleaned) = cleaned_output {
    if validate_config.as_ref().map(|vc| vc.replace_output).unwrap_or(false) {
        (cleaned, Some(output))  // replace output, preserve original
    } else {
        (output, None)  // keep original
    }
} else {
    (output, None)
};
```

### 8. Feature Interaction Rules

| Feature | Validation Behavior |
|---------|-------------------|
| `for_each` | Validation runs on the aggregated loop output (not per-iteration). The `for_each` code path returns a combined StepResult. Per-iteration validation is deferred as a future enhancement. |
| `apply_edits` | Validation runs on the LLM's text output before edit parsing/application |
| `consensus` (multi-backend) | Validation runs once on the synthesized/final output |
| `retries` | Validation failure does NOT trigger step retries (validation rejection is deterministic - retrying the same output gives the same result) |
| `fix_retries` | Validation runs after the final fix attempt. A future enhancement could feed validation failure into the fix loop. |
| `continue_on_error` | Validation failure respects the step's `continue_on_error` setting (soft failure passes error message to dependents) |

### API/Interface Design

| Function | Parameters | Returns | Description |
|----------|-----------|---------|-------------|
| `run_step_validation` | `output: &str, stderr: Option<&str>, config: &ValidateConfig, config: &Config, cwd: &Path` | `(Option<ValidationResult>, Option<String>)` | Unified entry point: heuristic then LLM |
| `run_llm_validation` | `output: &str, stderr: Option<&str>, validate_config: &ValidateConfig, backend_name: &str, config: &Config, cwd: &Path` | `(Option<ValidationResult>, Option<String>)` | LLM validation core |
| `interpolate_validation_prompt` | `prompt: &str, output: &str, stderr: Option<&str>, max_len: Option<usize>` | `String` | Template interpolation for validation prompts |
| `parse_validation_response` | `response: &str` | `Result<ValidationResponse, String>` | JSON -> ValidationResponse with text fallback |

---

## Implementation Plan

### Phase 1: Core Validation Logic

- [ ] Add `on_error`, `max_input_length`, `replace_output`, `timeout_ms` fields to `ValidateConfig`
- [ ] Add `ValidatorError` variant to `FailureType` enum
- [ ] Implement `interpolate_validation_prompt()` with single-pass `{{ output }}` and `{{ stderr }}` replacement (prevents injection)
- [ ] Implement `strip_markdown_fences()` for JSON wrapped in ```json fences
- [ ] Implement `parse_validation_response()` with JSON-first + REVIEW_FAILED fallback + fail-closed error for unrecognized formats
- [ ] Implement `run_llm_validation()` with backend creation, query, response parsing, `on_error` handling, and validation-specific timeout
- [ ] Implement `run_step_validation()` orchestrating heuristic-then-LLM flow

### Phase 2: Wire Into Step Execution

- [ ] Replace shell step validation block (~L1076) with `run_step_validation()` call
- [ ] Replace multi-backend validation block (~L1304) with `run_step_validation()` call
- [ ] Replace apply/verify validation block (~L1624) with `run_step_validation()` call
- [ ] Handle `replace_output` and `raw_output` preservation at all three sites

### Phase 3: Testing & Validation

- [ ] Unit tests for `interpolate_validation_prompt()` (output, stderr, truncation)
- [ ] Unit tests for `parse_validation_response()` (JSON pass, JSON fail, REVIEW_FAILED, plain text, malformed)
- [ ] Integration test: shell mock as validation backend (echo back JSON pass)
- [ ] Integration test: shell mock returning JSON fail
- [ ] Integration test: combined heuristic + LLM (heuristic fails, LLM skipped)
- [ ] Integration test: `on_error = "pass"` when validation backend fails
- [ ] Integration test: `replace_output = true` (downstream sees cleaned output)
- [ ] Unit tests for `strip_markdown_fences()` (json fence, plain fence, no fence)
- [ ] Unit test for fail-closed parsing (unrecognized format -> error)
- [ ] Unit test for single-pass interpolation safety (output containing `{{ stderr }}`)
- [ ] TOML parsing tests for new ValidateConfig fields (on_error, max_input_length, replace_output, timeout_ms)

---

## Constraints

**Must**:
- Use existing `Backend` trait and `create_backend()` - no new trait definitions
- Maintain backward compatibility: steps without `validate` behave identically
- `run_step_validation()` must be a single function called from all three wiring points (DRY)
- Validation failure must respect `continue_on_error` semantics

**Must-not**:
- Must not block the tokio runtime with sync operations in the validation path
- Must not retry the step when validation rejects the output (deterministic rejection)
- Must not modify the `Backend` trait signature

**Prefer**:
- JSON structured output from validator over text-based parsing
- Use existing `FailureType` enum rather than introducing parallel error types
- Validation-specific timeout via `validate.timeout_ms` (falls back to backend timeout if not set)
- When `replace_output = true` AND `max_input_length` causes truncation, log a warning that cleaned output may be incomplete (truncation + replacement is lossy)

**Concurrency note**: `run_step_validation()` has no shared mutable state. Multiple steps validating in parallel is safe - each creates its own backend instance and makes independent API calls.

**Trust model**: Cleaned output from the validator LLM inherits the same trust level as the step's original output. The validator can alter content - this is intentional (noise removal). The `replace_output = false` default protects against unintended mutations.

**Escalate when**:
- The three wiring points cannot be refactored to use a single validation call (async context issues)
- `for_each` loop validation requires changes to the loop iteration structure

---

## Acceptance Criteria

- [ ] `validate.backend = "claude"` + `validate.prompt` triggers LLM validation after step execution - verify with `cargo test test_llm_validation`
- [ ] `{{ output }}` in validation prompt is replaced with step's raw output - verify with `cargo test test_validation_prompt_interpolation`
- [ ] JSON `{"status": "fail", "reason": "..."}` response correctly marks step as failed - verify with `cargo test test_validation_response_parsing`
- [ ] `REVIEW_FAILED:` prefix response also works (backward compat) - verify with `cargo test test_validation_response_parsing`
- [ ] Combined heuristic + LLM: heuristic failure skips LLM call - verify with `cargo test test_combined_heuristic_llm`
- [ ] `validate.on_error = "pass"` treats validator failure as pass - verify with `cargo test test_validation_on_error`
- [ ] `validate.replace_output = true` replaces output with cleaned version - verify with `cargo test test_replace_output`
- [ ] `StepResult.validation.validator` set to `"llm:<backend>"` - verify with `cargo test test_llm_validation`
- [ ] All existing tests pass unchanged - verify with `cargo test`
- [ ] No clippy warnings - verify with `cargo clippy`

**Verification method**: `cargo test && cargo clippy`

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | Shell mock returns JSON pass | Step succeeds, output optionally replaced | `cargo test test_llm_validation_pass` |
| 2 | Shell mock returns JSON fail | Step fails with reason in ValidationResult | `cargo test test_llm_validation_fail` |
| 3 | Shell mock returns REVIEW_FAILED: prefix | Step fails (backward compat) | `cargo test test_llm_validation_review_failed` |
| 4 | Shell mock returns plain text | Treated as pass with cleaned output | `cargo test test_llm_validation_plain_text` |
| 5 | Heuristic fails + LLM configured | LLM not called, step fails from heuristic | `cargo test test_combined_heuristic_llm` |
| 6 | Heuristic passes + LLM configured | LLM called with output | `cargo test test_combined_heuristic_then_llm` |
| 7 | Validation backend not found | ValidatorError failure type | `cargo test test_validation_backend_not_found` |
| 8 | `on_error = "pass"` + backend fails | Step passes despite validator error | `cargo test test_validation_on_error_pass` |
| 9 | `replace_output = true` + LLM pass | output = cleaned, raw_output = original | `cargo test test_replace_output` |
| 10 | `replace_output = false` (default) + LLM pass | output = original, raw_output = None | `cargo test test_no_replace_output` |
| 11 | `max_input_length` truncation | Prompt contains truncated output + marker | `cargo test test_validation_prompt_truncation` |
| 12 | TOML parsing of new fields | ValidateConfig correctly populated | `cargo test test_validate_config_parsing` |
| 13 | Markdown fence stripping | JSON inside ```json fences parsed correctly | `cargo test test_strip_markdown_fences` |
| 14 | Unrecognized response format | ValidatorError, not pass | `cargo test test_validation_response_unrecognized` |
| 15 | Output contains `{{ stderr }}` literal | Not expanded by interpolation | `cargo test test_interpolation_injection_safety` |
| 16 | Validation-specific timeout | Validation fails fast when timeout_ms set | `cargo test test_validation_timeout` |

**Edge cases to cover**:
- Validation backend returns empty string (fail-closed: unrecognized format -> ValidatorError)
- Validator returns `{"status": "pass", "output": ""}` - empty cleaned output is valid (means "output is clean, nothing to replace")
- Validator wraps JSON in markdown fences (```json ... ```) - stripped before parsing
- Very large step output exceeding max_input_length
- Validation prompt with no `{{ output }}` placeholder (prompt sent as-is, output not included)
- Step with `validate.backend` but no `validate.prompt` (error: prompt required)
- Output contains `{{ stderr }}` literal - single-pass interpolation prevents expansion
- Validation-specific timeout fires before backend timeout

---

## Testing Strategy

- **Unit Tests**: `interpolate_validation_prompt()`, `parse_validation_response()`, TOML deserialization of new ValidateConfig fields. These can be tested in `workflow.rs` module tests.
- **Integration Tests**: Use shell backend as mock validator. Shell scripts that `echo` JSON pass/fail responses test the full validation pipeline end-to-end without real LLM calls. Add to `tests/integration.rs`.
- **Manual Testing**: Create a test workflow TOML with `validate.backend = "claude"`, `validate.model = "haiku"`, and a validation prompt. Run against a real step to verify end-to-end.

---

## Open Questions

- [x] ~~Should `{{ steps.X.raw_output }}` be added to the interpolation system in this task?~~ **Decision: Deferred.** Adding `raw_output` to the interpolation system requires extending `FIELD_RE` regex and `interpolate_with_fields()`. This is a separate concern from validation itself. Tracked as future scope - will be needed if `replace_output` sees adoption.

---

## References

- [Linear Task](https://linear.app/cloud-ai/issue/CLO-184)
- [PRD: Output Validation Pipeline](../prds/prd-output-validation-pipeline.md)
- [Discovery Report](../prds/discovery-report-2026-04-03-clo-184.md)
- [CLO-183 Spec: Heuristic Validators](../specs/2026-04-03-clo-183-heuristic-validators.md) (predecessor)
- [CLO-181 Spec: Per-Step Model Override](../specs/2026-03-30-clo-181-per-step-model-override.md) (predecessor)
