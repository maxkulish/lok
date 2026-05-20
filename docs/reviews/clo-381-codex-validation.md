# Pre-PR validation: clo-381

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-20
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

LOW — `docs/PROJECT.md:9` still lists CLO-381 phase as `Discovery`, while the branch has completed design/plan and landed implementation commits. This is tracking drift only, not a runtime/code issue.

LOW — Working tree has an unstaged update to `docs/status/clo-381-workflow.yaml`. The committed branch metadata still lags the latest validation/status notes, so either commit that status update or intentionally drop it before PR.

## Missing Items

None against the design/plan. The source changes implement the requested parser mapping:

- `cached_input_tokens` / `reasoning_output_tokens` are now `Option<u32>`.
- `parse_jsonl_stream` maps them into `TokenUsage.with_cached(...)` and `.with_reasoning(...)`.
- Fixture tests assert `Some(7552)/Some(0)` and `Some(30464)/Some(51)`.
- Inline tests cover present values, omitted fields, `Some(0)`, and last-turn semantics.
- `#[allow(dead_code)]` was removed from the now-used builders and `CodexUsage`.

## Recommendations

Commit or clean up the unstaged workflow-status change before opening the PR.

Verification note: `cargo fmt --check` passed locally. `cargo test codex_event` and `cargo clippy --all-targets -- -D warnings` could not run in this read-only sandbox because Cargo could not open `target/debug/.cargo-lock`; commit `c0ea891` claims the full gate passed with 531 tests.
