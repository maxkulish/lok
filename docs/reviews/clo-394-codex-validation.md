# Pre-PR validation: clo-394

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

Review scope note: `git diff main...HEAD` is empty. The implementation exists only as unstaged/untracked working-tree changes, so the branch as committed does not implement CLO-394.

## Findings

- CRITICAL: Branch diff is empty.
  `git diff main...HEAD` produced no changes, while `git status` shows modified/untracked CLO-394 files. A PR from this branch would not contain the implementation.

- MEDIUM: Legacy pinned Gemini CLI execution compatibility regresses.
  In src/backend/gemini.rs:531, non-`opencode` commands now return `base_args + prompt` only. That drops the previous model/sandbox flag behavior and the stdin pipe workaround for `npx @google/gemini-cli`. The design keeps a legacy parser fallback for pinned `command = "npx"` configs, but this execution path may no longer reach successful JSON output.

- LOW: Parser can over-collect non-assistant JSON text if opencode event shape drifts.
  src/backend/gemini.rs:347 treats any no-`type` JSON object with `text`/`content`/`output` as response text. That is tolerant, but it can mix tool/progress output into `QueryOutput.stdout` if opencode emits untyped or `kind`-based events. The design asks for final response text, not arbitrary visible event text.

- LOW: Doctor install hint is misleading as a copy-paste command.
  src/main.rs:720 uses `brew install ... | curl ...` as if it were a pipeline. README correctly uses "or". This should be two alternatives, not a pipe.

## Missing Items

- The committed branch is missing all implementation changes because `main...HEAD` is empty.
- I could not run the test gate. `cargo fmt --check` and `git diff --check` passed, but `cargo test gemini --bin lok --test gemini_fixtures` failed before build because the read-only sandbox cannot open `target/debug/.cargo-lock`.

## Recommendations

- Commit/stage the working-tree changes before review/PR.
- Decide whether legacy `command = "npx"` configs are intended to remain executable. If yes, preserve old model/sandbox argument injection and stdin behavior, or explicitly document that only parser compatibility remains.
- Tighten opencode parsing around known assistant/final event fields and add fixture coverage for tool/progress events without `type`.
- Change the doctor hint to `brew install anomalyco/tap/opencode or curl -fsSL https://opencode.ai/install | bash`.
