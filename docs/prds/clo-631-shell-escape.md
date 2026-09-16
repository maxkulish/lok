# CLO-631: Safe step-output interpolation in workflow shell fields

## Problem

Workflow authors can interpolate `steps.*` values into a step's `shell` field. Lok renders the resulting text and passes it to `sh -c`. Several checked-in workflows currently place model or command output inside shell source without applying the existing `shell_escape` filter. Quotes, command substitutions, metacharacters, or a line matching a fixed heredoc delimiter can therefore break the command or execute unintended shell code.

## Users and impact

- Operators running the checked-in review and validation workflows can execute text originating in design documents, specifications, PR bodies, diffs, or model responses.
- Workflow authors have a registered defence (`shell_escape`) but no authoring rule or automated check requiring it.
- The `review-pr` example deliberately asks a model to emit shell commands and executes them, turning model output into an unrestricted command-generation boundary.

## Goals

1. No checked-in `.lok/workflows/` or `examples/workflows/` shell field treats a `steps.*` value as shell source.
2. Replace fixed-delimiter, output-writing heredocs with safe single-argument writes.
3. Preserve `review-pr` follow-up issue creation without executing model-authored commands: parse and validate the synthesis JSON, then invoke `gh issue create` with quoted arguments.
4. Quote the fetched PR head ref before passing it to `git`.
5. Document the rule for workflow authors.
6. Add a repository test that rejects new unescaped `steps.*` output tags in workflow shell fields.
7. Exercise a shell-only workflow containing apostrophes, backticks, `$()`, semicolons, and every removed heredoc delimiter, proving none is executed.

## Non-goals

- Automatically escaping all workflow shell interpolation in the engine. MiniJinja expressions are embedded in many shell contexts, and silently changing their semantics would be a broad compatibility break.
- Preventing a repository-controlled workflow from containing an explicit malicious shell command. This task protects dynamic interpolation, not the trust boundary for selecting project workflows.
- Changing interpolation in `prompt`, `when`, or other non-shell fields.
- Adding a general command-approval UI.

## Requirements

### R1: Safe interpolation

Every `{{ ... }}` output tag that references `steps.*` inside a checked-in workflow `shell` field must end in `| shell_escape`. The escaped result must be used as a complete shell word or assignment value, not nested inside another quoted shell string.

### R2: Content writes

Review and validation outputs must be written with `printf '%s\n' {{ value | shell_escape }}` (or an equivalently safe complete-argument mechanism). No model output may occur in a heredoc body.

### R3: Follow-up issues

`examples/workflows/review-pr.toml` must not ask a model for shell source or run model output. A shell step must:

- extract the synthesis result's parsed `followups` field, serialize it with `json_encode`, and pass it as one shell-escaped value;
- validate the complete serialized `followups` array before creating any issue;
- require each item to contain string `title`, `body`, and `label` fields;
- allow only the documented `bug` and `enhancement` labels;
- pass each value to `gh issue create` as a quoted argument;
- perform no side effect when the list is empty.

### R4: Static regression gate

A Rust test run by `cargo test` must parse the checked-in TOML workflows and fail with file and step names when a shell output tag references `steps.*` without `shell_escape`. It must scan `.lok/workflows/`, `examples/workflows/`, and shell-only fixtures under `tests/workflows/`.

### R5: Runtime regression

A shell-only workflow test must pass hostile text through a step output into a later shell step. The payload must contain `'`, backticks, `$()`, `;`, and standalone lines matching `ENDOLLAMA`, `ENDFALLBACK`, `ENDCLAUDE`, `ENDSYNTH`, `LOKEOF`, `EOF`, `LOK_WF_CODEX_OUTPUT_EOF`, `LOK_WF_SYNTH_OUTPUT_EOF`, and `LOK_WF_FALLBACK_OUTPUT_EOF`. The test must assert that injected marker commands did not run and that the text remains data.

### R6: Documentation

The workflow authoring guide and README example must show `shell_escape` as a complete shell argument and warn that ordinary quoting or heredoc delimiters do not make untrusted interpolation safe.

## Acceptance criteria

- All current workflow TOML files pass the static regression gate.
- The hostile-output integration workflow succeeds without creating either marker file.
- The removed fixed heredoc delimiters are absent from shell blocks that write dynamic output.
- `review-pr` creates issues only from a validated JSON array and never executes generated shell.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.
