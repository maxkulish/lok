# Pre-PR validation: clo-632

**Reviewer**: Codex (gpt-5.6-sol)
**Validated**: 2026-09-15
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- **MEDIUM — Project-controlled backend names can inject terminal control sequences.** [`src/config.rs`](/Users/mk/Code/orchestrator/lok--fix-clo-632-toml/src/config.rs:590) interpolates `name` directly into the error. TOML accepts quoted names containing escaped control characters such as `\u001b`; the resulting ANSI sequence reaches stderr unescaped. This undermines the spec's terminal-safe error requirement.

- **LOW — AC8 is incomplete.** The later [`command_wrapper` examples in README.md`](/Users/mk/Code/orchestrator/lok--fix-clo-632-toml/README.md:501) set non-default gated values without marking them user-config-only.

- **LOW — The branch fails `git diff --check`.** [`clo-632-spec-review-claude-fallback.md`](/Users/mk/Code/orchestrator/lok--fix-clo-632-toml/docs/reviews/clo-632-spec-review-claude-fallback.md:29) contains trailing whitespace.

The core gate otherwise matches the design: all four fields are checked, violations are accumulated, trusted values win, explicit/user config remains unrestricted, and the end-to-end test has a valid positive control.

## Missing Items

- Complete AC8 by labeling every non-default gated example as user-config-only.
- Add coverage proving project-controlled backend names cannot emit raw control characters.
- AC9 could not be independently reconfirmed in this read-only environment. Formatting passed, but Cargo test/clippy commands were blocked from creating lock and temporary files.

## Recommendations

- Escape backend names before building dotted keys, for example with `char::escape_default`, while preserving ordinary names such as `codex`.
- Add a unit test using a backend name containing `\u001b`; assert stderr contains no raw escape byte.
- Mark the README command-wrapper subsection explicitly as user-config-only.
- Remove the trailing whitespace.
- Resolve the uncommitted `docs/status/clo-632-workflow.yaml` update before opening the PR; it is not included in `main...HEAD`.
