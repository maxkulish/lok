# Pre-PR validation: clo-393

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-25
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS_WITH_NOTES

## Findings

- MEDIUM: `doctor --output json` can emit non-JSON when no enabled backends are present. The handler prints `No backends configured.` before dispatching on `output`, so machine consumers do not always get the valid JSON array required by the design. See src/main.rs:749 and the JSON contract in docs/design-docs/clo-393-doctor-renderer.md:72.

- MEDIUM: The integration test explicitly allows the non-JSON fallback, so it would not catch the issue above even though ST6 says it should verify a valid JSON array. See tests/integration.rs:226.

- LOW: Unknown output formats silently fall back to table output. `lok doctor --output jsno` should probably fail fast or use a clap value parser for `table|json`; otherwise scripted usage can silently get human output. See src/main.rs:752.

- LOW: `git diff --check main...HEAD` reports trailing whitespace in docs/status/clo-393-workflow.yaml:72.

## Missing Items

- Acceptance criterion 2 is not fully covered for the empty/no-enabled-backend case: JSON mode should still produce a JSON array, likely `[]`.
- Acceptance criterion 5 was not verified here because the environment is read-only and cargo commands would write build artifacts. I did verify `git diff --check`, which currently fails on whitespace.

## Recommendations

- In JSON mode, always route through `print_doctor_json`, including empty `entries`.
- Tighten `test_doctor_json_output` so stdout must parse as JSON and must be an array; do not allow `No backends configured.` for JSON mode.
- Replace `output: String` with a clap `ValueEnum` or `value_parser(["table", "json"])`.
- Remove the trailing whitespace in the workflow YAML.
