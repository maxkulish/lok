# Plan: CLO-631 Escape or remove step output interpolated into workflow shell fields

## Context

- Design: `docs/designs/clo-631-shell-safe-workflow-interpolation.md`
- Discovery: `docs/discovery/clo-631.md`
- PRD: `docs/prds/clo-631-shell-escape.md`
- Linear: https://linear.app/cloud-ai/issue/CLO-631/escape-or-remove-step-output-interpolated-into-workflow-shell-fields
- Branch: `feat/clo-631-shell-escape`

The implementation is a checked-in workflow and test change. The Rust runtime and public API remain unchanged. The final scope is 39 `steps.*` output tags in 19 shell fields across 11 workflow files, plus a TOML-aware static policy test, hostile runtime coverage, follow-up validation coverage, and documentation.

## Sub-tasks

### ST1 Build the TOML-aware shell interpolation policy test

**Files:** `tests/workflow_shell_policy.rs` (new)

Implement the scanner and focused unit tests from the design: TOML shell-field extraction, raw-block removal, output-tag detection, `steps` reference detection, whitespace/empty-call tolerant last-filter matching, heredoc ranges, quote-state tracking that skips heredoc bodies, and file/step/line diagnostics. Include tests for quoted contexts, heredoc contexts, bracket access, statement tags, whitespace controls, and the 39-tag inventory. Keep the repository-wide assertion enabled so the test fails until all checked-in workflows are migrated.

**Acceptance:** `cargo test --test workflow_shell_policy -- unit_`

**Estimate:** L

### ST2 Add shell-safe fixture and hostile-output runtime regression

**Files:** `tests/integration.rs`, `tests/workflows/test_interpolation.toml`, `tests/workflows/test_parallel.toml`, `tests/workflows/test_shell_hash_expansion.toml`, `tests/workflows/test_shell_escape_hostile.toml` (new)

Extend the integration helper with environment injection and add the shell-only hostile fixture. Route apostrophes, backticks, command substitutions, semicolons, and historical heredoc delimiter lines through escaped output. Assert that output files contain the trimmed payload and that no marker file is created. Preserve existing workflow assertions while moving their interpolations into complete escaped words.

**Acceptance:** `cargo test --test integration shell_escape`

**Estimate:** M

### ST3 Rewrite lok's own review workflows

**Files:** `.lok/workflows/design-review.toml`, `.lok/workflows/spec-review.toml`, `.lok/workflows/pre-pr-validation.toml`

Replace dynamic heredoc writes and quoted output tags with `printf '%s\\n'` and complete `shell_escape` arguments. Preserve `when` guards around optional or skipped steps, preserve output paths and review layout, and retain the existing fallback behavior. Do not alter prompt interpolation or runtime Rust code.

**Acceptance:** `cargo test --test workflow_shell_policy -- checked_in_workflows_escape_in_shell_fields`

**Estimate:** M

### ST4 Rewrite non-follow-up example workflows and hyphenated references

**Files:** `examples/workflows/fix.toml`, `examples/workflows/pick-and-propose.toml`, `examples/workflows/full-heal.toml`, `examples/workflows/rework-pr.toml`

Apply the documented P2-P5 patterns to issue bodies, comments, numeric fields, refs, and commit messages. Replace dynamic heredocs, quote complete escaped values, add `string` before `shell_escape` for non-string parsed values, and convert every hyphenated step reference in `rework-pr.toml` to bracket access while preserving guards.

**Acceptance:** `cargo test --test workflow_shell_policy -- checked_in_workflows_escape_in_shell_fields`

**Estimate:** M

### ST5 Replace model-authored follow-up commands with validated JSON execution

**Files:** `examples/workflows/review-pr.toml`, `tests/integration.rs`

Remove the LLM-authored shell and `run_followups` execution path. Make `create_followups` serialize the parsed `followups` array, require `jq`, validate the complete array and `bug|enhancement` label allowlist before side effects, then invoke `gh issue create` with quoted extracted fields. Add the inherited-PATH `gh` stub and NUL-framed invocation reader, covering valid, empty, invalid, missing, non-string, malformed, and fenced-JSON cases. Ensure the integration test fails clearly if `jq` is unavailable.

**Acceptance:** `cargo test --test integration review_pr_followups`

**Estimate:** L

### ST6 Document the safe authoring contract

**Files:** `README.md`, `docs/guides/lok-setup-guide.md`

Replace the unsafe introductory example with bracket access and `shell_escape`. Document complete-word/assignment placement, the prohibition on surrounding escaped tags with quotes or dynamic heredocs, `printf`, `string` for non-strings, JSON validation before CLI side effects, and the supported command-wrapper limitation with CLO-794 as follow-up scope.

**Acceptance:** `git diff --check -- README.md docs/guides/lok-setup-guide.md`

**Estimate:** S

### ST7 Run repository policy and regression gates

**Files:** all files changed by ST1-ST6

Run the policy scanner after every workflow rewrite, verify the final inventory has no violations, run all existing integration tests and follow-up regressions, and perform the documented negative-control/manual checks without committing temporary edits. Confirm no source API or schema change was introduced and inspect the final diff for skipped-step guards and accidental model-command execution.

**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

**Estimate:** M

## Pre-merge gate

- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
- The policy gate must report zero violations across `.lok/workflows`, `examples/workflows`, and `tests/workflows`.
- The hostile runtime test must leave its marker directory empty.
- Follow-up validation must create no `gh` calls for invalid documents.

## Risks

- Shell escaping is safe only when the rendered escaped value is a complete shell word or assignment value; a scanner false negative would reopen the vulnerability.
- `run_shell` trims captured stdout, so hostile fixture assertions must compare the trimmed payload and avoid boundary blank lines.
- Optional `when` guards must remain around references to steps that may be skipped.
- `review-pr` now requires `jq` and fails closed before issue creation when synthesis is invalid.
- Custom double-quoted `command_wrapper` forms can re-expand escaped values; this is explicitly tracked by CLO-794 and is not broadened into this change.
- CLO-631 is archived in Linear; local workflow state is authoritative until the issue can be unarchived and synchronized.
