# CLO-370: Add `usage` field to `StepResult` for end-to-end token observability

**Task**: [CLO-370](https://linear.app/cloud-ai/issue/CLO-370)
**Source PRD**: `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §FR-25a, §9 step 1
**Blocks**: CLO-371 (TokenLedger aggregation)
**Related**: CLO-207 (added `usage` to `QueryOutput`)

## Problem Statement

`QueryOutput.usage: Option<TokenUsage>` was added in CLO-207 so every backend can
surface prompt/completion/total token counts. The field stops there: when the
workflow conductor copies a backend's response into `StepResult`, the `usage` value
is dropped on the floor. As a result, no caller of `WorkflowRunner` (`lok query`,
multi-step orchestration, structured JSON output, downstream cost ledgers) can see
which steps consumed how many tokens, even when the underlying backend reported it.

CLO-371 plans to aggregate per-step usage into a `TokenLedger`. That aggregation
cannot start until `StepResult` carries the data through the workflow boundary.

## Acceptance Criteria

1. `StepResult` exposes `pub usage: Option<TokenUsage>` (re-exporting / using
   `crate::backend::TokenUsage`).
2. The field is populated from the underlying `QueryOutput.usage` on every LLM
   success path: single-backend, multi-backend consensus, and `for_each` iteration.
3. The field is `None` for shell-only steps, error-path constructors
   (`StepResult::error`), and any path that did not produce a `QueryOutput`.
4. `cargo test` passes (existing 472+ tests stay green; one new test added).
5. `cargo clippy --all-targets -- -D warnings` is clean.
6. PR description names which backends populate `usage` today (Bedrock, Ollama)
   versus which still leave it as `None` (Claude CLI, Codex CLI, Gemini CLI as of
   CLO-207 landing state).

## Implementation

### Site 1 - `StepResult` struct (`src/workflow.rs:897`)

Add `pub usage: Option<TokenUsage>` alongside the other optional metadata fields.
Import `TokenUsage` from `crate::backend` (already in scope).

### Site 2 - `StepResult::error` (`src/workflow.rs:929`)

Default the new field to `None` in the struct literal.

### Site 3 - Single-backend success path (`src/workflow.rs:2483`)

In the retry loop at `src/workflow.rs:2171`, the `Ok(Ok(qo))` arm already
destructures `qo.stdout`, `qo.stderr`, `qo.exit_code` into locals. Add a sibling
`step_usage: Option<TokenUsage>` local (declared next to `step_stderr`/
`step_exit_code` at `src/workflow.rs:2151-2152`) and assign `qo.usage` inside the
match arm. Wire `usage: step_usage` into the `StepResult { ... }` literal at 2483.

### Site 4 - Multi-backend consensus path (`src/workflow.rs:2109`)

**Drift from PRD**: The PRD says "copy `output.usage.clone()` from the underlying
`QueryOutput`", but the consensus code already discarded `QueryOutput` by line
1944 (`Ok(qo.stdout)` only). Reconciliation:

1. Extend `BackendResponse` in `src/consensus.rs:96` with
   `pub usage: Option<TokenUsage>`.
2. In the per-backend spawn at `src/workflow.rs:1943-1948`, return
   `(bn, Ok((qo.stdout, qo.usage)))` and populate the `BackendResponse` accordingly
   at `src/workflow.rs:1958`.
3. After consensus selects a winner, aggregate `usage` across all responses that
   contributed to that winner. For `First`, copy the winner's `usage` directly.
   For `Vote` / `WeightedVote`, sum the `usage` of every response whose content
   matches the winner. For `Synthesis`, sum the `usage` of every input response
   AND add the synthesis call's own `qo.usage`. Use `TokenUsage::saturating_add`
   so we never panic on overflow. None values are skipped (a `None` from one
   backend does not poison the aggregate).
4. Wire the aggregated `Option<TokenUsage>` into the `StepResult { ... }` literal
   at 2109.

### Site 5 - `for_each` path (`src/workflow.rs:1788`)

**Drift from PRD**: Same as Site 4 - iterations already discard `QueryOutput` by
line 1739 (`iter_output = qo.stdout`). Reconciliation:

1. Capture `iter_usage: Option<TokenUsage>` per iteration alongside
   `iter_output` / `iter_success` (declared just above the per-iteration backend
   query block).
2. Maintain `aggregate_usage: Option<TokenUsage>` outside the loop, folding each
   iteration's `iter_usage` in via `TokenUsage::saturating_add`. Skip `None`
   iterations.
3. Wire `aggregate_usage` into the `StepResult { ... }` literal at 1788.

### Site 6 - Shell-only path (`src/workflow.rs:1868`)

No backend was queried; `usage: None`.

### Site 7 - Test/helper construction sites (~26 sites)

Mechanically add `usage: None` to every `StepResult { ... }` literal in
`src/workflow.rs` test modules and `src/template/{mod,context}.rs` helpers. No
semantic change.

### Site 8 - Unit test

Add a test in the `workflow.rs` tests module that constructs a `QueryOutput` with
a non-empty `usage` (use `TokenUsage::new(100, 50)`), runs it through one of the
existing `Backend` mock harnesses (or an inline mock if no general one exists),
and asserts that the resulting `StepResult.usage` is `Some` with the expected
counts. If no mock backend covers the single-step LLM path, add a small inline
`StepResult` round-trip test that simply demonstrates the field exists and is
preserved through the struct.

## Constraints

### Must

- Keep the change additive: no trait signature changes, no public API breaks.
- Use `crate::backend::TokenUsage` directly - do not invent a parallel type.
- Use `TokenUsage::saturating_add` for any aggregation (existing helper).
- Preserve current behavior for paths that did not previously surface usage
  (shell, error, missing backend) - they continue to return `None`.

### Must Not

- Change the `Backend` trait or any backend's `query()` signature.
- Make `usage` a non-`Option` field (backends without metering must still work).
- Introduce a new error path: aggregation must not fail; if no backend reported
  usage, the aggregate is `None`.
- Touch `TokenLedger` or any aggregation surface beyond `StepResult` - that work
  is CLO-371.
