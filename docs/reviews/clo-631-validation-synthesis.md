# Pre-PR validation: clo-631

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-09-16
**Pipeline**: lok pre-pr-validation
---

I checked the Codex findings against the code, and I'm writing the synthesis now.

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned FAIL with 4 findings. `cargo fmt --check`, Clippy with `-D warnings` and `cargo test` passed. `nix-shell` was not installed, so that check did not run. |
| Claude fallback | SKIPPED | Codex review succeeded |

## Verdict
PASS_WITH_NOTES

Codex found no defects in the workflow rewrites, the `review-pr` JSON follow-up flow, the hostile-output runtime test or the docs. Its confirmed findings are about the static gate and one test fixture. They sit in two test files, don't need a design change and fit in one fix iteration. That makes this PASS_WITH_NOTES, not FAIL.

## Must Fix Before PR
- **The gate misses a quoted tag after a `#` comment that follows a control operator** (`tests/workflow_shell_policy.rs:221-222`). The lexer only treats `#` as a comment after whitespace. I checked by hand:
  - The scanner treats `:;# '` as an open single quote. The next line's quotes then flip the state, so `'{{ steps.a.output | shell_escape }}'` counts as unquoted and is not flagged.
  - `sh -c ":;# '\necho ok"` prints `ok`, which confirms that `;#` starts a comment at runtime. The tag really is inside quotes, so the escaping is cancelled and the payload runs as shell.
  - **Fix:** also start a comment when the previous byte is `;`, `&`, `|`, `(` or `)`. Add a negative-control test for this case.
- **The gate misses a tag in the second heredoc on a line** (`tests/workflow_shell_policy.rs:134-159`). `opener.captures` records only the first opener, and `line = last` skips past its body.
  - With `cat <<A <<B`, a tag in body `B` gets no `InsideHeredoc` and no `QuotedContext`, so an escaped tag passes.
  - At runtime its quotes are plain text, and a payload line `B` would end the heredoc early.
  - **Fix:** use `captures_iter` over the opener line and read the bodies one after another. Test both `cat <<A <<B` and `cmd <<A; cmd <<B`.
- **The quote lexer doesn't skip `{% ... %}` statement tags**, although design rule 9 says to skip "output/block tags". `quote_contexts` only skips the `{{ }}` ranges from `output_tags`. A quote inside a statement tag's string literal can therefore throw off the quote state. **Fix:** skip statement-tag ranges the same way output tags are skipped.
- **Unit tests promised by the design are missing.** Design line 128 and open question 1 say these tests keep the small lexer in check. The file has 5 combined tests covering part of the 12 in the plan. Missing:
  - `tracks_multiline_quotes_and_escapes` is absent: quotes spanning lines, backslash rules, quotes inside tags, quotes inside comments.
  - Bracket access (`steps["fetch-pr"]`).
  - Statement tags, `arg.1` and `workflow.backends` producing no violation.
  - Heredocs opened with `<<EOF` and with `<<-LOKEOF` (closing line with leading tabs).
  - `| string | shell_escape` being accepted.
  - The exact diagnostic line format. It is currently built inline in `checked_in_workflows_escape_in_shell_fields`, where no unit test reaches it.
- **The follow-up integration tests run a copy, not the real step** (`tests/integration.rs`, `review_pr_followups_workflow`). The shell for `create_followups` is hardcoded in the test. It matches `examples/workflows/review-pr.toml:158-190` today, but a change to the production step would not break any follow-up test. `test_review_pr_followups_workflow_structure` only checks that a `shell` key exists. **Fix:** read `create_followups.shell` from the example TOML with `toml` and put it into the generated fixture.

## Out of Scope / Deferred
- **Raw tags with whitespace control** (`{%- raw -%}`, `{%+ raw %}`) are not stripped. Tags inside them get flagged, so the gate fails closed. No checked-in workflow uses raw blocks (`rg` found none).
- **Raw-block stripping hides quotes from the scanner.** Quote characters inside `{% raw %}` do reach the shell, but the scanner blanks them out. This follows design rule 3, needs an unusual construct, and has no checked-in use. It is a follow-up for making the lexer stricter.
- **`workflow.backends` still sits inside single quotes** in `examples/workflows/fix.toml:87`, `review-pr.toml:149` and `pick-and-propose.toml:130`.
  - Design line 87 said a value moved out of a heredoc should be passed the same escaped way.
  - The value is backend names chosen by the workflow author, not step output, and the gate only covers `steps.*`.
  - This is minor drift from the design. Record it as a follow-up rather than blocking.

## False Positives / Tooling Artifacts
- **Trailing whitespace in `docs/discovery/clo-631.md` lines 55, 72 and 89.** These are the two-space Markdown line breaks after `**Effort:** M/L/S`, and they are intentional. The pre-merge gate doesn't run `git diff --check`, and the plan applies it only to `README.md` and the setup guide. Switching them to backslash line breaks is optional.
- **`nix-shell` verification not run.** It isn't installed on this machine. The direct `cargo fmt --check`, Clippy (`-D warnings`) and `cargo test` gates passed.

## Recommendation
PROCEED_WITH_FIXES. Make one fix pass limited to `tests/workflow_shell_policy.rs` and `tests/integration.rs`:
1. Recognise comments after `;`, `&`, `|`, `(` and `)`, with a negative-control test.
2. Handle every heredoc opener on a line and read the bodies in order, with tests for both forms.
3. Skip `{% %}` statement tags in the quote lexer.
4. Add the missing unit tests from the design, and move the diagnostic format into a function a test can reach.
5. Load the `create_followups` shell from `examples/workflows/review-pr.toml` instead of hardcoding it.

After that, rerun `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` and check that the new tests appear in the output. No user decision is needed.

## Re-validation

The single bounded fix iteration was applied in commit `b03f6de5c8c4662839d45a4e5ffc64fffc37984d`:

- Shell comments after `;`, `&`, `|`, `(` and `)` are recognized, with a regression test.
- Multiple heredoc openers on one command line are scanned in sequence, including unquoted and tab-stripped forms.
- Statement tags are skipped by quote-state analysis.
- Lexer edge tests now cover multiline quoting, escapes, bracket access, namespaces, filter chaining, raw whitespace controls, heredoc variants, comment boundaries, and exact diagnostics.
- Follow-up integration fixtures load the production `create_followups` shell from `examples/workflows/review-pr.toml`.

Verification on the current HEAD passed:

```text
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

The static policy gate passed with 7 tests, integration passed with 14 tests, and the full suite passed with 587 unit tests plus all integration, fixture, trust-boundary, and doctest checks. The deferred `nix-shell` probe remains unavailable because `nix-shell` is not installed; this does not affect the required gate.
