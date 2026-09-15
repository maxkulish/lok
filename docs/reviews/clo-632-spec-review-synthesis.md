# Spec Review Synthesis: clo-632

**Synthesized**: 2026-09-15
**Pipeline**: lok spec-review

---

**Source status:** The Ollama review (glm-5.3:cloud) failed because the step timed out after 300s. This synthesis uses only the Claude fallback review, so every finding comes from one reviewer and none is cross-checked.

## Agreement (High Confidence)

None. Only one reviewer returned results.

## Disagreement (Needs Human Decision)

None from reviewers, because Ollama returned nothing. One item still needs a human decision (see N2).

## Novel Insights (Single Reviewer)

| # | Finding | Source | Severity |
|---|---------|--------|----------|
| N1 | AC6 can pass without testing anything. It only checks for a non-zero exit and a missing marker file. It has no positive control (the same hostile file passed via `--config` should create the marker) and does not set `current_dir` to the temp directory. If the gate broke, it could also make real paid calls to the installed `claude` or `opencode` binaries, because it keeps the inherited `PATH` | Claude | Medium |
| N2 | Clause (a)'s "accepted cost" is really a security downgrade. A project can reset `command` to a bare `codex` looked up on `PATH`, replacing the user's pinned path. It can also reset `args` and drop the user's hardening flags. Alternative: when the project value equals the built-in default but differs from the user's value, keep the user's value. **This reopens a decision the user made (HITL), so it needs the user's call** | Claude | Medium |
| N3 | A project's `.lok/workflows/<name>.toml` is found before global and built-in workflows (`workflow.rs:3642-3646`). A cloned repo can therefore still override a workflow with `shell` steps, which is the same power the gated `command_wrapper` has. The AC8 docs must not claim that the project layer cannot run commands | Claude | Medium |
| N4 | Evaluation row 12 (`HOME=$(mktemp -d) cargo run`) breaks rustup toolchain lookup. Build first, then run `./target/debug/lok backends` | Claude | Low |
| N5 | The audit does not cover `model` flag injection. The risk is low: gemini adds a `google/` prefix and puts the prompt after `--`, and codex parses flags with clap | Claude | Low |
| N6 | The audit does not cover Bedrock `enabled = true`. In a `--features bedrock` build, a project can enable Bedrock and spend the user's AWS credentials. This is a cost risk, not data exfiltration | Claude | Low |
| N7 | A rejected project file blocks every subcommand (`backends`, `doctor`, `init`), not just `ask`, because config loads before dispatch (`main.rs:498`). No AC or rollout note says this | Claude | Low |
| N8 | The `--config` fix hint is vague. `--config` skips the user config too, so the hint should name the file to pass, e.g. `--config ~/.config/lok/lok.toml` | Claude | Low |
| N9 | The spec does not require future project config locations to go through the same gate. Without that line, a later fix for session note F3 (`.lok/lok.toml` is never loaded) could bypass the gate | Claude | Low |
| N10 | The Evaluation table lists 9 unit tests, which conflicts with the "one focused test per AC" rule. Rows 1-4 could be one table-driven test | Claude | Low |
| N11 | The Bedrock `api_key_env` doc example in sub-task 4 is wrong, because Bedrock never reads `api_key_env` | Claude | Low |
| Q1 | Open question: should the error say that only `./lok.toml` in the current directory is checked, not parent directories? | Claude | Open |

The reviewer checked the main claims against the code: the loader location, the only caller, `deep_merge` behaviour, the health-check spawn path, that existing tests are unaffected, and that only `loker/lok.toml` breaks at rollout.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Claude approved with suggestions, and Ollama returned no verdict. No blocking issues were found. The boundary, where it is enforced and the key audit are sound.

## Priority Actions

1. **Harden AC6 (N1).** Set `enabled = false` for gemini, claude and ollama in the hostile `lok.toml`, or point `PATH` at an empty directory. Set `current_dir` to the temp directory. Add a `--config` positive control that asserts the marker file is created.
2. **Ask the user about clause (a) (N2).** Either adopt "keep the user's value when the project repeats the built-in default", or rewrite the accepted cost to name the downgrade (pinned path, hardening args). If adopted, Evaluation row 6 should expect `/opt/bin/codex`.
3. **Record the `.lok/workflows/` gap (N3).** Add it to the Must-not list, require the AC8 docs to say the boundary covers config keys only, and open a follow-up ticket.
4. **Fix Evaluation row 12 (N4).** Build first, then run the binary with a temp `HOME`.
5. **Make the error text and rollout note precise (N7, N8, Q1).** Say that all subcommands are blocked, name the file to pass to `--config`, and decide on the current-directory-only wording.
6. **Extend the audit (N5, N6, N9).** Add one line each for `model` flag injection, Bedrock `enabled` cost, and the gate requirement for future config locations.
7. **Tidy up (N10, N11).** Make the unit-test count match the one-test-per-AC rule, and fix the Bedrock `api_key_env` doc example.

Re-running the Ollama review with a longer timeout would give a second opinion before implementation starts. It is optional because none of the findings block implementation.
