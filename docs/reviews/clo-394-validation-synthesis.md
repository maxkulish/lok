# Pre-PR validation: clo-394

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

I have enough context. Verified the branch is empty vs main (Codex finding #1 confirmed), reviewed the gemini.rs diff against the design, and checked the doctor hint string.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned structured findings; sandbox blocked the cargo test step only |
| Gemini | REVIEW_FAILED | CLI refused to run: `--skip-trust` not set / workspace not trusted; both gemini-3.1-pro-preview and gemini-2.5-pro produced empty stdout |
| Claude fallback | SKIPPED | Codex succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- **Branch contains zero committed changes** (Codex CRITICAL, verified). `git diff main...HEAD` is empty; `git status` shows the implementation (`src/backend/gemini.rs`, `src/config.rs`, `src/main.rs`, `src/backend/context.rs`, fixtures, docs) is all working-tree only. PR creation must be preceded by staging and committing these files; otherwise the PR will be a no-op.
- **Doctor install hint reads as a shell pipeline, not alternatives** (Codex LOW, verified at `src/main.rs:720`). Current string: `Install opencode: brew install anomalyco/tap/opencode | curl -fsSL https://opencode.ai/install | bash`. The `|` between the two install methods will be parsed as "pipe brew's stdout into curl into bash" by anyone copy-pasting. Replace with the README-style separator `... | or | curl ...` or simply `... or curl ...`. The design doc itself uses `"  |  "` (with spaces) intending visual separation; either way the literal `|` should not sit between two complete commands.

## Out of Scope / Deferred
- **Legacy `command = "npx"` execution path no longer injects model/approval flags or pipes empty stdin** (Codex MEDIUM, `src/backend/gemini.rs` `build_argv` non-opencode branch returns `base_args + prompt` only). The design's Migration section says pinned-legacy users "continue to invoke the legacy CLI," but the Goals section explicitly removes shell-string execution and the parser-level fallback is preserved. The implementation is consistent with Goals; the Migration prose overpromises. Either tighten the Migration note in the design doc to say "parser compatibility only" or restore the legacy execution shape in a follow-up — not a release blocker for CLO-394 because the default config has migrated and no in-tree workflow pins `command = "npx"`.
- **`response_from_json` may over-collect text from untyped opencode events** (Codex LOW, `src/backend/gemini.rs:347`). Real risk if opencode emits untyped progress/tool events with `text`/`content` fields. Mitigated by the `is_auxiliary_key` filter and by `extract_opencode_response_text` requiring `type == "text"` when `type` is present. Acceptable for v1; tighten when a problematic fixture is observed.

## False Positives / Tooling Artifacts
- **Gemini reviewer empty output** is a sandbox/trust artifact (`Gemini CLI is not running in a trusted directory`), not a code finding. Re-run with `GEMINI_CLI_TRUST_WORKSPACE=true` or `--skip-trust` for future passes.
- **Codex could not execute `cargo test`** because the read-only sandbox cannot acquire `target/debug/.cargo-lock`. The test gate has to be confirmed locally before PR, but this is not a signal that tests fail.

## Recommendation
PROCEED_WITH_FIXES. Two bounded fixes before PR: (1) `git add` and commit the CLO-394 implementation + fixtures + docs so `git diff main...HEAD` is non-empty; (2) fix the doctor install hint in `src/main.rs:720` so the two install methods are not separated by a literal `|`. After both, run `cargo fmt --check && cargo clippy -- -D warnings && cargo test` locally (the validation sandbox could not) and open the PR. Defer the legacy-npx execution-path question to a design-doc clarification follow-up.
