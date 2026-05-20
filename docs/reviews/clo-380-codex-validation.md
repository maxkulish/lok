# Pre-PR validation: clo-380

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-20
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

LOW: The new fixture precedence tests reimplement production logic instead of exercising it.
[tests/codex_parse_output.rs](/Users/mk/Code/orchestrator/lok--feat-clo-380-codex/tests/codex_parse_output.rs:37) defines a parallel JSONL parser and [tests/codex_parse_output.rs](/Users/mk/Code/orchestrator/lok--feat-clo-380-codex/tests/codex_parse_output.rs:131) defines separate output-selection logic. This validates the fixtures, but it would not catch regressions in `CodexBackend::query()` or `parse_jsonl_diagnostics()` selection behavior.

LOW: A couple of design test-plan cases are only partially covered.
The composed fixture tests cover populated `-o`, missing-agent JSONL, empty `-o`, and `turn.failed` precedence, but not a composed "missing `-o` file + valid JSONL" case or "populated `-o` + no usable JSONL usage -> success with `usage == None`" case. The missing-file behavior is unit-tested at the helper level, but not in the end-to-end precedence path.

## Missing Items

- No production-path/stub-command test for `CodexBackend::query()` selecting `-o` text over JSONL while preserving JSONL usage.
- No fixture-backed composed test for an absent last-message file fallback.
- No explicit test for success with populated last-message text and `usage == None`.

## Recommendations

- Add a hermetic `CodexBackend::query()` test using a temp fake Codex executable/script that writes JSONL to stdout and writes or omits the `-o` file based on argv.
- Prefer testing `parse_jsonl_diagnostics()` directly in unit tests over duplicating parser semantics in integration tests where possible.
- Add the two missing composed scenarios from the design test plan.

I did not run `cargo test` because this session is in a read-only filesystem sandbox, but I did inspect `git diff main...HEAD` and the modified source/test files.
