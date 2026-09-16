# Pre-PR validation: clo-631

**Reviewer**: Codex (gpt-5.6-sol)
**Validated**: 2026-09-16
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- **HIGH** — The policy scanner can miss an exploitable quoted interpolation. [workflow_shell_policy.rs:221](/Users/mk/Code/orchestrator/lok--feat-clo-631-shell-escape/tests/workflow_shell_policy.rs:221) recognizes `#` as a comment only after whitespace. POSIX `sh` also accepts comments after control operators such as `;`. Consequently, this passes the scanner while the interpolated payload can execute:

  ```sh
  :;# '
  printf '%s
' '{{ steps.a.output | shell_escape }}'
  ```

  The quote in the comment incorrectly changes scanner state; the next quote resets it before the tag. At runtime, the tag is inside shell quotes, defeating `shell_escape`.

- **MEDIUM** — Multiple heredocs on one command line bypass `InsideHeredoc`. [workflow_shell_policy.rs:134](/Users/mk/Code/orchestrator/lok--feat-clo-631-shell-escape/tests/workflow_shell_policy.rs:134) records only the first opener and then jumps to its delimiter. An escaped tag in a second heredoc is treated as an ordinary complete argument and passes, although its generated quotes are only literal heredoc data.

- **MEDIUM** — Follow-up integration tests do not exercise the production workflow. [integration.rs:341](/Users/mk/Code/orchestrator/lok--feat-clo-631-shell-escape/tests/integration.rs:341) duplicates the `create_followups` shell instead of loading it from [review-pr.toml:161](/Users/mk/Code/orchestrator/lok--feat-clo-631-shell-escape/examples/workflows/review-pr.toml:161). The structure test only checks that a shell exists. Production validation could regress while all follow-up tests continue passing.

- **LOW** — `git diff --check main...HEAD` fails because [clo-631.md:55](/Users/mk/Code/orchestrator/lok--feat-clo-631-shell-escape/docs/discovery/clo-631.md:55), lines 72 and 89 contain trailing whitespace.

## Missing Items

- The static gate does not fully satisfy R4 or the design's lexer contract.
- Several promised tests are absent: shell comments, multiline quoting/escapes, multiple heredocs, unquoted and tab-stripped heredocs, bracket access, other namespaces, tag-local quotes, and full diagnostic formatting.
- Raw-block removal only recognizes `{% raw %}`; MiniJinja's valid `+`/`-` whitespace-control forms are not handled.
- Production `review-pr` behavior is not directly bound to its integration tests.

## Recommendations

- Correct comment/token-boundary and multiple-heredoc handling, then add explicit negative-control tests demonstrating these bypasses.
- Support all MiniJinja raw-tag forms accepted by the pinned version.
- Build the follow-up fixture from the actual production `create_followups` step, or at minimum assert byte-for-byte equality with the tested shell.
- Remove the trailing whitespace.
- Re-run the full pre-merge gate afterward. Direct `cargo fmt --check`, Clippy with warnings denied, and `cargo test` all passed; `nix-shell` verification was unavailable because it is not installed. The only worktree dirt is untracked `docs/status/clo-631-workflow.yaml`.
