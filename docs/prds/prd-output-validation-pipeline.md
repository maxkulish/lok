# PRD: Output Validation Pipeline for Lok

| Field | Value |
|-------|-------|
| Author | MK |
| Status | Draft |
| Created | 2026-03-29 |
| Last Updated | 2026-03-29 |
| Related | [rs-wisper review pipeline PRD](/Users/mk/Code/wisper/rs-wisper/docs/prds/prd-multi-model-review-pipeline.md), [rs-wisper investigation 030](/Users/mk/Code/wisper/rs-wisper/docs/investigations/030-ai-review-pipeline-improvements.md) |

## 1. Overview

Add output validation capabilities to lok so that workflow steps can detect and handle noise, empty output, and silent failures from LLM backends. The primary use case is multi-model code/doc review workflows where Gemini CLI and Ollama produce invalid output (stderr noise, empty files, timeouts) that currently gets passed downstream as if it were real content.

**Current state**: Lok has a mature workflow engine with DAG execution, parallel steps, multi-backend consensus, retry logic, conditional execution, and template interpolation. What it lacks is the ability to distinguish "a backend returned text" from "a backend returned useful text." An empty string or MCP initialization noise is treated identically to a valid review.

**Why now**: The rs-wisper project runs Gemini CLI and Ollama reviews through lok-style workflows. CLO-175 showed both models failing silently - Gemini produced MCP stderr noise (12s, exit 0), Ollama timed out (300s, empty). The workflow engine faithfully passed the noise downstream. Adding validation to lok makes every workflow that uses it more reliable, not just rs-wisper reviews.

## 2. Problem & Objectives

### Problem Statement

Lok's Backend trait returns `Result<String>` from `query()`. A successful result means "the process ran and produced output" - but it says nothing about whether the output is meaningful. Three failure modes are invisible to the workflow engine today:

1. **Noise output**: Gemini CLI merges stderr into stdout when run with `2>&1` redirection. When Gemini exits early (rate limit, MCP failure), only initialization noise gets captured: `Registering notification handlers for server 'pencil'...`. Lok sees a successful query with non-empty output.

2. **Empty output**: Ollama's codex wrapper writes nothing when killed by timeout. Lok's timeout handler returns an error, but if the process exits cleanly with no output (model not loaded, cold start stall), Lok sees a successful query with an empty string.

3. **Truncated output**: A model might produce partial output before timing out. The first 3 sections of a review exist but sections 4-8 are missing. Lok has no way to detect this.

The underlying issue: **Lok validates process execution (did it run? did it timeout?) but not output content (is the result what the workflow expected?)**

### Objectives

- **O1**: Workflow steps can validate that their output meets content expectations before passing it downstream
- **O2**: Validation can use fast/cheap LLMs (model-as-validator pattern) in addition to heuristic checks
- **O3**: Failed validation produces structured error data (not just an error string) so downstream steps can make degradation decisions
- **O4**: Stderr from CLI backends is captured separately and available for diagnostics
- **O5**: Backend exit codes are captured and exposed to the workflow

### Success Metrics

| Metric | Current | Target | How Measured |
|--------|---------|--------|--------------|
| Silent failures in review workflows | ~5% (noise treated as content) | 0% | Audit workflow outputs - every output is valid content or structured failure |
| Validation step overhead | N/A | < 5s per step (fast model) | Measure validation step execution time |
| False positive rate (valid output rejected) | N/A | < 1% | Manual audit of validation decisions |

## 3. What Lok Already Has

Before specifying new features, here's what lok already provides that this PRD builds on:

| Capability | Status | How It Works |
|------------|--------|--------------|
| DAG step execution with `depends_on` | Done | Depth-grouped parallel execution |
| Template interpolation `{{ steps.X.output }}` | Done | Regex-based substitution with brace escaping |
| Retry with exponential backoff | Done | `retries` + `retry_delay` per step |
| `continue_on_error` per step | Done | Soft failure - error message passed to dependents |
| Conditional execution (`when`/`if`) | Done | `contains()`, `equals()`, `not()` conditions |
| Multi-backend consensus | Done | `synthesis`, `vote`, `weighted_vote`, `first` strategies |
| Timeout handling | Done | `tokio::time::timeout`, per-step and per-workflow |
| Shell command steps | Done | `shell = "command"` instead of LLM prompt |
| Output format parsing | Done | `output_format = "json\|lines\|text"` |
| `for_each` loops | Done | Iterate over parsed output arrays |
| Backend availability check | Done | `is_available()` trait method (but Ollama always returns true) |

**Key architectural constraint**: Lok's `Backend` trait returns `Result<String>`. Any validation solution must work within or extend this interface.

## 4. Functional Requirements

### FR Group: Stderr Separation

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-1 | CLI backends (Gemini, Codex) capture stderr separately from stdout | Must | Stderr written to a sidecar string, not merged into the query result |
| FR-2 | Stderr content accessible in workflow via `{{ steps.X.stderr }}` | Should | Downstream steps can inspect stderr for diagnostics |
| FR-3 | Backend configuration supports `stderr = "separate"` option | Must | When set, `Command::new()` uses `Stdio::piped()` for stderr instead of inheriting or merging |
| FR-4 | Default behavior (`stderr` not set) remains unchanged | Must | No breaking change to existing workflows |

### FR Group: Exit Code Capture

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-5 | CLI backends capture process exit code | Must | Available as integer alongside output |
| FR-6 | Exit code accessible in workflow via `{{ steps.X.exit_code }}` | Should | Enables conditional logic: `when = "equals(steps.review.exit_code, '124')"` for timeout-specific handling |
| FR-7 | Exit code semantics documented for each backend | Should | 124 = timeout, 137 = OOM/SIGKILL, 0 = success, non-zero = tool-specific error |

### FR Group: Step Result Extension

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-8 | `StepResult` extended with `stderr: Option<String>` | Must | Populated when backend captures stderr separately |
| FR-9 | `StepResult` extended with `exit_code: Option<i32>` | Must | Populated for CLI backends, None for API backends |
| FR-10 | `StepResult` extended with `validation: Option<ValidationResult>` | Must | Populated when step has a `validate` clause |
| FR-11 | `StepResult.success` considers validation result | Must | A step that ran successfully but failed validation has `success = false` |

### FR Group: Step-Level Validation

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-12 | Steps support a `validate` clause for output validation | Must | Validation runs after the step completes, before output is passed downstream |
| FR-13 | `validate.backend` specifies which backend runs the validation check | Must | Typically a fast model: `validate.backend = "haiku"` |
| FR-14 | `validate.prompt` is the validation prompt template | Must | Receives `{{ output }}` (the step's raw output) and optionally `{{ stderr }}` |
| FR-15 | Validation prompt response is parsed for pass/fail signal | Must | Validator returns cleaned output (pass) or `REVIEW_FAILED: <reason>` (fail) |
| FR-16 | On validation pass, the cleaned output replaces the raw output | Must | Downstream steps see validated/cleaned content, not raw noise |
| FR-17 | On validation fail, step marked as failed with structured error | Must | `StepResult.success = false`, `StepResult.output` contains the failure reason |
| FR-18 | Validation failure respects `continue_on_error` | Must | Soft validation failure passes error to dependents; hard failure stops workflow |

**TOML syntax for step-level validation:**

```toml
[[steps]]
name = "gemini-review"
backend = "gemini"
prompt_file = "prompts/review.md"
timeout = 300000
continue_on_error = true

[steps.validate]
backend = "haiku"
prompt = """You are a review output validator.
If this text contains a structured code/design review, clean any noise
(MCP initialization messages, stderr artifacts, log lines) and return
ONLY the review content.
If there is no actual review content, return exactly:
REVIEW_FAILED: <one-line reason>

Input:
{{ output }}"""
```

### FR Group: Heuristic Validators (No LLM)

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-19 | `validate.check` supports built-in heuristic checks without calling an LLM | Should | Cheaper alternative for simple validation |
| FR-20 | `check = "not_empty"` validates output is non-empty and non-whitespace | Should | Catches Ollama empty output without LLM call |
| FR-21 | `check = "min_length(N)"` validates output exceeds N characters | Should | Catches truncated output |
| FR-22 | `check = "contains(text)"` validates output contains expected marker | Should | Catches noise-only output by checking for expected section headers |
| FR-23 | `check` and `backend` can be combined - heuristic runs first, LLM only if heuristic passes | Should | Saves LLM cost: skip validation call if output is obviously empty |

**TOML syntax for heuristic validation:**

```toml
[[steps]]
name = "ollama-review"
backend = "ollama"
prompt_file = "prompts/review.md"
continue_on_error = true

[steps.validate]
check = "min_length(200)"  # Must be >200 chars to be a real review
```

**Combined heuristic + LLM validation:**

```toml
[steps.validate]
check = "min_length(200)"   # Fast check first
backend = "haiku"            # LLM validation only if heuristic passes
prompt = "Clean noise from: {{ output }}"
```

### FR Group: Health Checks

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-24 | Ollama backend `is_available()` actually checks connectivity | Should | HTTP GET to `http://localhost:11434/api/tags` with 5s timeout |
| FR-25 | Ollama backend checks if requested model is loaded | Should | Checks model name in `/api/tags` response |
| FR-26 | Steps support `health_check = true` to run availability check before execution | Should | Skips step (soft failure) if backend is unavailable, instead of waiting for timeout |
| FR-27 | Health check results cached per workflow run | Should | Don't re-check same backend on every step |

### FR Group: Structured Failure Data

| ID | Requirement | Priority | Acceptance Criteria |
|----|-------------|----------|-------------------|
| FR-28 | When a step fails (timeout, validation, error), the failure is structured | Must | Contains: step name, backend, duration, exit code, failure type, failure reason |
| FR-29 | Structured failure accessible as JSON via `{{ steps.X.error }}` | Should | Downstream steps can inspect failure details for degradation logic |
| FR-30 | Failure types enumerated: `timeout`, `validation_failed`, `empty_output`, `backend_error`, `health_check_failed` | Must | Enables targeted error handling in downstream steps |

## 5. Non-Functional Requirements

| Category | Requirement | Target |
|----------|-------------|--------|
| Performance | Heuristic validation overhead | < 1ms (string operations only) |
| Performance | LLM validation step (Haiku/Flash) | < 5s per validation |
| Compatibility | Existing workflows without `validate` clause | Unchanged behavior, no breaking changes |
| Compatibility | Existing `StepResult` consumers | New fields are `Option<T>`, always backward compatible |
| Testing | Validation logic | Unit tests for heuristic checks, integration tests for LLM validation with shell mock |

## 6. Scope & Phasing

### Phase 1: Core Validation (Smallest Useful Change)

- Stderr separation for CLI backends (FR-1, FR-3, FR-4)
- Exit code capture (FR-5)
- StepResult extensions (FR-8, FR-9, FR-10, FR-11)
- Step-level `validate` clause with LLM backend (FR-12 through FR-18)
- Heuristic `check` validators: `not_empty`, `min_length(N)`, `contains(text)` (FR-19 through FR-22)
- Combined heuristic + LLM validation (FR-23)
- Structured failure data (FR-28, FR-30)

### Phase 2: Diagnostics & Health

- Stderr accessible via `{{ steps.X.stderr }}` (FR-2)
- Exit code accessible via `{{ steps.X.exit_code }}` (FR-6)
- Structured failure via `{{ steps.X.error }}` (FR-29)
- Ollama health check fix (FR-24, FR-25)
- Step-level `health_check` option (FR-26, FR-27)
- Exit code semantics documentation (FR-7)

### Out of Scope

- Streaming validation (validate output while it streams) - reason: lok doesn't stream step output today; adds complexity without clear benefit
- Custom validator plugins (Rust trait-based) - reason: heuristic checks + LLM validation cover all known use cases; plugin system is premature
- Automatic retry on validation failure - reason: lok already has `retries` per step; validation failure triggers existing retry logic
- Cross-step validation (compare outputs between steps) - reason: downstream synthesis steps already handle this via template interpolation

## 7. Architecture

### StepResult Changes

```rust
// Current
pub struct StepResult {
    pub name: String,
    pub output: String,
    pub parsed_output: Option<serde_json::Value>,
    pub success: bool,
    pub elapsed_ms: u64,
    pub backend: String,
}

// Proposed additions
pub struct StepResult {
    pub name: String,
    pub output: String,           // After validation: cleaned output or error message
    pub raw_output: String,       // NEW: original output before validation
    pub parsed_output: Option<serde_json::Value>,
    pub success: bool,
    pub elapsed_ms: u64,
    pub backend: String,
    pub stderr: Option<String>,   // NEW: captured stderr (CLI backends only)
    pub exit_code: Option<i32>,   // NEW: process exit code (CLI backends only)
    pub validation: Option<ValidationResult>,  // NEW
}

pub struct ValidationResult {
    pub passed: bool,
    pub failure_type: Option<FailureType>,
    pub failure_reason: Option<String>,
    pub validator: String,        // "heuristic:min_length" or "llm:haiku"
    pub elapsed_ms: u64,
}

pub enum FailureType {
    Timeout,
    ValidationFailed,
    EmptyOutput,
    BackendError,
    HealthCheckFailed,
}
```

### Backend Trait Changes

```rust
// Current
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, prompt: &str, cwd: &Path) -> Result<String>;
    fn is_available(&self) -> bool;
}

// Proposed: extend return type
pub struct QueryOutput {
    pub stdout: String,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}

pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput>;
    fn is_available(&self) -> bool;
}
```

**Migration path**: Change `query()` return type from `Result<String>` to `Result<QueryOutput>`. Update all backend implementations. Callers that only need stdout use `result.stdout`. This is a breaking internal change but not a breaking workflow change.

### Validation Execution Flow

```
Step executes
    │
    ▼
Backend.query() returns QueryOutput { stdout, stderr, exit_code }
    │
    ▼
Has validate clause?
    │
    ├── No ──► StepResult { output: stdout, success: true }
    │
    ├── Yes: has check?
    │      │
    │      ├── check fails ──► StepResult { output: error, success: false,
    │      │                     validation: { passed: false, type: ValidationFailed } }
    │      │
    │      ├── check passes, has backend?
    │      │      │
    │      │      ├── LLM returns cleaned output ──► StepResult { output: cleaned, success: true }
    │      │      │
    │      │      └── LLM returns REVIEW_FAILED ──► StepResult { output: error, success: false }
    │      │
    │      └── check passes, no backend ──► StepResult { output: stdout, success: true }
    │
    └── Yes: no check, has backend?
           │
           ├── LLM returns cleaned output ──► StepResult { output: cleaned, success: true }
           │
           └── LLM returns REVIEW_FAILED ──► StepResult { output: error, success: false }
```

### Workflow TOML Changes (Summary)

New fields added to step definition:

```toml
[[steps]]
name = "example"
backend = "gemini"
prompt = "..."

# NEW: Validation clause
[steps.validate]
check = "not_empty"              # Heuristic check (no LLM needed)
check = "min_length(200)"        # Minimum content length
check = "contains('## Summary')" # Expected content marker
backend = "haiku"                # LLM validator (optional, runs after check passes)
prompt = "Validate: {{ output }}" # Validation prompt ({{ output }} = step's raw output)

# NEW: Health check before execution
health_check = true              # Check backend availability first
```

## 8. Example: rs-wisper Design Review Workflow

This is what the rs-wisper review pipeline looks like using lok with validation:

```toml
name = "design-review"

[[steps]]
name = "gemini-review"
backend = "gemini"
prompt_file = "prompts/design-review.md"
timeout = 300000
continue_on_error = true
health_check = true

[steps.validate]
check = "min_length(200)"
backend = "gemini-flash"
prompt = """You are a review output validator.
If this contains a structured design review (sections with findings and a verdict),
remove any noise (MCP initialization messages, log lines, stderr artifacts)
and return ONLY the clean review content.
If there is no actual review content, return exactly:
REVIEW_FAILED: <one-line reason>

Input:
{{ output }}"""

[[steps]]
name = "ollama-review"
backend = "ollama"
prompt_file = "prompts/design-review.md"
timeout = 300000
continue_on_error = true
health_check = true

[steps.validate]
check = "not_empty"
backend = "haiku"
prompt = """You are a review output validator.
If this contains a structured design review, return ONLY the clean review content.
If empty, truncated, or no review content, return exactly:
REVIEW_FAILED: <one-line reason>

Input:
{{ output }}"""

[[steps]]
name = "synthesis"
backend = "haiku"
depends_on = ["gemini-review", "ollama-review"]
min_deps_success = 1
prompt = """Cross-reference these reviews and produce a synthesis.

Gemini review ({{ steps.gemini-review.success }}):
{{ steps.gemini-review.output }}

Ollama review ({{ steps.ollama-review.success }}):
{{ steps.ollama-review.output }}

Instructions:
- If both reviews are present, identify agreement (high confidence) and disagreement (needs human decision)
- If one review failed, synthesize from the available review only, noting the gap
- If both failed (you should not reach this step), return: NO_REVIEWS_AVAILABLE
- Output a structured synthesis with: Agreement, Disagreement, Unique Insights, Consolidated Verdict"""
```

**Key details:**
- `continue_on_error = true` on review steps - one failure doesn't block the other
- `min_deps_success = 1` on synthesis - runs if at least one review succeeded
- Heuristic check runs first (free), LLM validation only if heuristic passes
- `{{ steps.X.success }}` in synthesis prompt tells the synthesizer which reviews are valid
- Different validation backends (gemini-flash for Gemini output, haiku for Ollama) to avoid self-validation

## 9. Alternatives Considered

### Alternative A: External Validation Script

Add a `validate_script` field that runs a shell command to validate output, similar to the existing `verify` field for `apply_edits`.

```toml
[steps.validate]
shell = "grep -q '## Summary' && echo PASS || echo FAIL"
```

**Pros**: No LLM cost. Deterministic. Fast.

**Rejected because**: Shell/regex validation is brittle. Checking for `## Summary` breaks when the review format changes. Can't distinguish MCP noise from partial reviews. The investigation doc at `030-ai-review-pipeline-improvements.md` showed that heuristic detection ("does it contain `## 1. Problem Statement`?") works for known noise patterns but misses novel failures. Model-as-validator generalizes better.

**Compromise**: Heuristic `check` field (FR-19-22) covers simple cases. LLM validation handles ambiguous cases. Both can be combined.

### Alternative B: Validation as a Separate Step

Instead of adding `validate` to the step definition, use a dedicated validation step with `depends_on`:

```toml
[[steps]]
name = "gemini-review"
backend = "gemini"
prompt = "..."

[[steps]]
name = "gemini-validate"
backend = "haiku"
depends_on = ["gemini-review"]
prompt = "Validate: {{ steps.gemini-review.output }}"
```

**Pros**: Already works today - no lok changes needed. Clear, explicit, composable.

**Rejected as the only approach because**: Validation-as-step doesn't update `StepResult.success` on the original step. The synthesis step sees `gemini-review.success = true` even when validation fails. You'd need extra conditional logic in every downstream step. Also doubles the step count in every workflow.

**However**: This approach still works and may be preferred for complex validation logic. The `validate` clause is syntactic sugar for the common case. Users can use either approach.

### Alternative C: Backend-Level Validation (Validate Inside the Backend)

Add validation logic to the Backend trait itself - each backend knows what "good output" looks like.

**Rejected because**: Validation is workflow-specific, not backend-specific. Gemini CLI producing MCP noise is a valid output for some workflows (e.g., debugging MCP issues) but invalid for review workflows. The backend should faithfully return what the process produced; the workflow decides if it's acceptable.

### Alternative D: Regex-Based Output Filters in Config

Add a `noise_patterns` config field to backends that strips known noise before returning output:

```toml
[backends.gemini]
noise_patterns = [
  "^Registering notification handlers.*$",
  "^YOLO mode is enabled.*$",
  "^Connected to server.*$"
]
```

**Pros**: Fast, deterministic, no LLM cost.

**Rejected as the only approach because**: Only catches known noise patterns. New MCP servers or new initialization messages require config updates. Doesn't detect "output exists but is not a review." Works well as a complement to LLM validation but not as a replacement.

**Potential future addition**: Could be added to backend config as a pre-filter. Not in scope for this PRD but compatible with the architecture.

## 10. Open Questions

| # | Question | Impact | Decision Needed By |
|---|----------|--------|-------------------|
| 1 | Should `Backend::query()` return `QueryOutput` (breaking internal change) or should stderr/exit_code be captured via a separate mechanism? | Affects all backend implementations | Before implementation |
| 2 | Should validation failure trigger the existing `retries` mechanism, or should it be a separate retry path? | Affects retry behavior | During implementation |
| 3 | Should `{{ output }}` in validate prompt include stderr when `stderr = "separate"` is set? | Affects validation accuracy for Gemini noise detection | During implementation |
| 4 | Should heuristic checks be extensible (user-defined regex patterns in TOML) or fixed to the built-in set? | Scope of heuristic validation | Phase 2 |
| 5 | What's the right `REVIEW_FAILED` signal format? Exact string match vs prefix match vs JSON? | Affects validation prompt design | Before implementation |
