# Pre-PR validation: clo-392

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Full FAIL report with 1 HIGH + 2 MEDIUM findings; could not run `cargo test` (read-only sandbox) |
| Gemini | REVIEW_FAILED | Both `gemini-3.1-pro-preview` and `gemini-2.5-pro` returned empty output; YOLO mode overridden to default because folder is not trusted |
| Claude fallback | SKIPPED | Codex succeeded |

## Verdict
FAIL

## Must Fix Before PR
- **ST4 not implemented as designed (HIGH).** `src/backend/mod.rs:535-559` warns whenever *any* `unusable_flags` are reported, regardless of whether a workflow step requests them, and does not include the minimum required Codex version. The design's resolved open question #1 explicitly requires reconciliation against workflow steps in `workflow.rs`/engine layer, and the plan's ST4 acceptance criterion calls for a `warmup_warning` test that exercises this behavior. Current code will spam warnings on any older Codex install even when no step uses the unsupported flag. Fix: move the warning emission to the engine/workflow validation phase, iterate workflow steps, and include `min_version` (from `FLAG_MATRIX`) in the message.
- **Health probe leaks child process on timeout (MEDIUM).** `src/backend/codex.rs:340-350` wraps `Command::output()` in `tokio::time::timeout` but does not set `kill_on_drop(true)`. The main Codex query path already sets it. Add `.kill_on_drop(true)` on the probe `Command` so a hung `codex --version` is reaped when the 2s timeout fires.
- **Missing acceptance tests called out in the plan (MEDIUM).** ST2 calls for fake-command-backed `codex_health` tests (success, missing binary, unparseable version, timeout, non-zero exit/stderr). ST3 requires `test_codex_health_cached`. ST4 requires `warmup_warning`. None of these exist in `src/backend/codex.rs:694-820` — only pure-function tests for `parse_version`, `compare_versions`, and matrix filtering are present. The plan's pre-merge gate (`cargo test`) is therefore not validating the new subprocess and warning paths. Add the listed tests before merge.

## Out of Scope / Deferred
- (None — every finding maps to a design/plan acceptance criterion that should land in this PR.)

## False Positives / Tooling Artifacts
- Gemini's empty output is a tooling artifact (untrusted-workspace guard rejecting headless YOLO mode), not a substantive finding. It should not be re-classified as a code issue.
- Codex's note that it could not run `cargo test parse_version` is a read-only-sandbox limitation; the tests do exist and look well-formed in source, so this is not a code defect — but it also does not confirm green tests. The plan's pre-merge gate still needs to be run locally before opening the PR.

## Recommendation
PROCEED_WITH_FIXES. The Codex backend probe and version helpers are solid, but the change diverges materially from the design's ST4 acceptance criterion (workflow-step reconciliation + min-version in warning) and is missing the subprocess/cache/warmup integration tests that the plan made pre-merge gates. All three issues are bounded and resolvable in one iteration: (1) relocate the warmup warning to engine/workflow layer, scan workflow steps against `unusable_flags`, and include `min_version` in the message; (2) add `.kill_on_drop(true)` to the `codex --version` probe command; (3) add the `codex_health` (fake shell wrapper) + `codex_health_cached` + `warmup_warning` tests listed in ST2/ST3/ST4. Re-run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` locally before re-submitting.
