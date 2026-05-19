# Pre-PR validation: clo-379

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-19
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

HIGH: `CodexBackend::query` does not parse JSONL failures on non-zero Codex exits.
At [src/backend/codex.rs](/Users/mk/Code/orchestrator/lok--feat-clo-379-codex-json/src/backend/codex.rs:123), the method returns on `!output.status.success()` before parsing stdout. The design requires `turn.failed` and top-level `error` events to surface as `BackendError` from the JSONL stream, and the fixture documents that `turn-failed.jsonl` comes from a Codex command that exited `1` while emitting the useful error in stdout. In practice, bad-model failures will return `Codex failed: {stderr}` and can lose the upstream JSONL error message.

MEDIUM: Required integration test target is missing.
The plan requires `tests/codex_parse_output.rs` and `cargo test --test codex_parse_output`. That file is absent. I confirmed the command fails with `error: no test target named codex_parse_output`.

LOW: Unknown events and unparseable lines are skipped silently, not logged at debug level.
The design and plan require debug logging for unknown events / parse errors. [src/backend/codex_event.rs](/Users/mk/Code/orchestrator/lok--feat-clo-379-codex-json/src/backend/codex_event.rs:99) and [src/backend/codex_event.rs](/Users/mk/Code/orchestrator/lok--feat-clo-379-codex-json/src/backend/codex_event.rs:146) just continue with comments.

## Missing Items

- `tests/codex_parse_output.rs` integration test from ST3.
- Query-path coverage proving a non-zero Codex process with JSONL stdout surfaces the JSONL `error` / `turn.failed` message.
- Debug logging for skipped unknown/unparseable JSONL lines.

## Recommendations

- In `query()`, decode stdout before the non-zero exit branch and attempt `parse_jsonl_stream(&stdout)` when Codex exits unsuccessfully. If it returns `ExecutionFailed`, surface that JSONL error instead of only stderr.
- Add the required fixture-driven test target, or adjust the plan if this binary-only crate cannot cleanly expose parser internals to integration tests.
- Add a logging dependency or use the project's existing logging pattern, then emit `debug!` for skipped unknown events and JSON parse failures.
- I could not run the full Rust suite because this environment is read-only; `cargo test codex_event --no-run` failed with `Operation not permitted` while creating the target dir.
