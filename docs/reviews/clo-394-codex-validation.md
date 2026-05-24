# Pre-PR validation: clo-394

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

MEDIUM: `parse_opencode_output` returns all text events, not the final response text required by the design.
The design explicitly requires `QueryOutput.stdout` to contain "final response text" (design). The implementation accumulates every parsed `type == "text"` event and joins them (gemini.rs:413, gemini.rs:445). For agentic opencode runs with tool calls or intermediate assistant messages, this can pollute `stdout` with non-final text and break downstream workflows that expect a final answer or final JSON/edit payload.

LOW: Legacy pinned `npx @google/gemini-cli` configs lose prior model/sandbox behavior.
The plan says legacy `command = "npx"` configs should keep working (plan). The new non-opencode branch appends only the prompt and returns early (gemini.rs:531), so model overrides and sandbox/apply-edits routing are no longer passed to legacy Gemini CLI. This is likely acceptable if legacy support is parser-only, but it contradicts the migration note that pinned users continue to invoke the old CLI without behavior loss.

## Missing Items

- No parser test covers multi-step opencode NDJSON with multiple text events and verifies only the final response is surfaced.
- Cargo failed opening `target/debug/.cargo-lock` with `Operation not permitted` in read-only environment.

## Recommendations

- Change opencode parsing to return the final assistant text, or text associated with the final `step_finish`/stop event, rather than joining every text event.
- Add a fixture with intermediate text, tool use, and final text to lock that behavior.
- Decide whether legacy `npx` configs need model/sandbox compatibility; if yes, preserve old flag injection for that path.
- Run the planned gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
