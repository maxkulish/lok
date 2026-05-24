# Pre-PR validation: clo-392

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

HIGH: Warmup warning does not implement the design requirement.
[src/backend/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-392/src/backend/mod.rs:535) warns whenever a backend reports any `unusable_flags`, but it does not scan workflow steps, does not check whether a step actually requests the flag, and does not name the minimum required version. This misses ST4 and the design's acceptance criterion: warn at warmup when a workflow step requests an unusable flag.

MEDIUM: The health probe timeout can leave a hung `codex --version` child running.
[src/backend/codex.rs](/Users/mk/Code/orchestrator/lok--feat-clo-392/src/backend/codex.rs:340) wraps `Command::output()` in `tokio::time::timeout`, but the command is not configured with `kill_on_drop(true)`. If the timeout fires, dropping the future may not terminate the spawned process. The main Codex query path already uses `kill_on_drop(true)`, and the probe should do the same.

MEDIUM: Planned health-check integration coverage is missing.
The added tests in [src/backend/codex.rs](/Users/mk/Code/orchestrator/lok--feat-clo-392/src/backend/codex.rs:694) cover `parse_version`, `compare_versions`, and matrix filtering only. There are no fake-command tests for `health_check` success, missing binary, unparseable version, timeout, non-zero exit/stderr, or cache-aware behavior, despite those being explicitly listed in the design and plan.

## Missing Items

- ST2 acceptance is incomplete: no subprocess-backed `codex_health` tests.
- ST3 is not implemented: no `test_codex_health_cached`.
- ST4 is not implemented as designed: no workflow-step reconciliation, no per-flag minimum version in warning, no `warmup_warning` test.
- I could not run tests in this read-only sandbox: `cargo test parse_version` failed opening `target/debug/.cargo-lock` with `Operation not permitted`.

## Recommendations

- Move or add the unusable-flag reconciliation where workflow steps are available, likely `workflow.rs` validation/warmup flow.
- Expose a small lookup for Codex flag requirements, or include `{ flag, min_version }` in warning logic, so warnings can say which step requested which unsupported flag and the required Codex version.
- Add `kill_on_drop(true)` to the health probe command.
- Add the planned fake Codex command tests before merging; they are the main guard against regressions in this feature.
