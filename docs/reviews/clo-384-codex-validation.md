# Pre-PR validation: clo-384

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-20
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- **HIGH**: Ollama can still time out before the resolved per-step timeout.
  [src/backend/ollama.rs:53](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/backend/ollama.rs:53) builds the reqwest client with `config.timeout.unwrap_or(300s)`. Because backend construction does not know the step timeout, a workflow step with `timeout = "10m"` against Ollama still has an internal 300s HTTP client timeout when `backends.ollama.timeout` is unset. That violates the required `step > backend > global` override for this backend.

- **MEDIUM**: Duplicate workflow timeout resolution remains and still includes `Workflow.timeout`, contrary to the design.
  [src/workflow.rs:1689](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:1689) computes `step.timeout.or(workflow.timeout)` and [src/workflow.rs:1697](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:1697) falls back to 120s. This path is still used for shell steps, format, and verification. The design explicitly says resolution should move into `effective_timeout()` and `Workflow.timeout` should be removed from the chain.

- **MEDIUM**: `Workflow.timeout` is still validated as an effective step timeout.
  [src/workflow.rs:135](/Users/mk/Code/orchestrator/lok--feat-clo-384-per-step/src/workflow.rs:135) uses `step.timeout.or(self.timeout)` during validation. A top-level workflow timeout below 100ms can still reject steps even though the design says workflow-level timeout is out of scope for resolution.

- **LOW**: Generated artifact is committed while also ignored.
  `autoresearch.jsonl` is added to the branch, and `.gitignore` now ignores it. It appears to be local workflow metadata, not source or task documentation.

## Missing Items

- The ST5 integration tests from the implementation plan are not implemented: sleepy backend timeout propagation and end-to-end `StepResult.failure.kind == Timeout`.
- I did not see evidence the full pre-merge gate was run. I also did not run it because the environment is read-only and Cargo would need to write build artifacts.

## Recommendations

- Remove Ollama's fixed/request-level timeout or make it no shorter than the resolved `StepContext.timeout`. The outer `tokio::time::timeout` should be the authoritative timeout layer.
- Replace remaining workflow-runner timeout calculations with `effective_timeout(step.timeout, backend_name, config)` where they are meant to be governed by FR-23.
- Decide explicitly whether shell/format/verify timeouts are in scope. If yes, use the same effective timeout. If no, document that they intentionally keep legacy workflow timeout behavior.
- Remove `autoresearch.jsonl` from the branch.
- Add the planned timeout integration tests before merge.
