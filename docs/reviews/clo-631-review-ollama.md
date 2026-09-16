# Ollama Rust review - CLO-631

## Findings

### F1 [minor] Runtime "verbatim" comparisons must account for `run_shell` stdout trimming
**Where:** docs/designs/clo-631-shell-safe-workflow-interpolation.md (§Runtime regression) vs src/workflow.rs:3429
**What:** `run_shell` captures stdout with `.trim()`, so `steps.produce.output` loses any leading/trailing blank lines and the trailing newline. Assumption 4 ("printf writes the same bytes as the heredoc") is true for rendering, but the fixture's captured output is the trimmed payload; a byte-exact comparison of `written.txt`/`composed.txt` against the payload will drift.
**Suggested fix:** Specify per-line comparison (or compare trimmed strings) in the assertions, and keep the fixture payload free of leading/trailing blank lines; state the trim in the test plan.

### F2 [minor] Quote-state pass vs heredoc bodies: contract and test missing
**Where:** docs/designs/clo-631-shell-safe-workflow-interpolation.md (§Static gate, rule 8; `quote_contexts` helper)
**What:** Rule 8 says heredoc bodies are "handled by rule 7 rather than by this state", but nothing says `quote_contexts` must skip the `heredoc_bodies` ranges. A heredoc body with unpaired quotes (exactly what the new `produce` fixture payload contains: `'; touch ...; '`) would leak quote state onto later tags in the same field and produce false `QuotedContext` violations. Today's corpus is unaffected (rewritten fields have no heredocs), but the gate is the standing authoring rule.
**Suggested fix:** Make `quote_contexts` consume `heredoc_bodies` ranges (skip or resynchronize at boundaries) and add a unit test: unpaired quote inside a heredoc body followed by a legitimate escaped tag in the same field yields no violation.

### F3 [minor] Violation granularity unspecified; "39" expectation is ambiguous
**Where:** docs/designs/clo-631-shell-safe-workflow-interpolation.md (§Static gate rule 9; Test plan manual step 1)
**What:** A bare tag inside a heredoc satisfies both `MissingShellEscape` and `InsideHeredoc`; a quoted unescaped tag (full-heal `issue`/`pr`, rework `comment`) satisfies both `MissingShellEscape` and `QuotedContext`. "Confirm that the gate lists all 39 current violations" only holds under one-violation-per-tag dedup; per tag×rule reporting yields more lines. I verified the underlying inventory against the tree: 39 step-referencing tags in 19 shell fields across 11 files, so the count itself is right — the semantics need pinning.
**Suggested fix:** State whether `Violation` is emitted per tag (first/most-specific rule) or per tag-rule pair, and align the manual verification wording.

### F4 [nit] `ends_with_shell_escape` edge forms unspecified
**Where:** docs/designs/clo-631-shell-safe-workflow-interpolation.md (§New test-only surface)
**What:** Unit tests only cover the spaced form. MiniJinja also accepts `|shell_escape` (no spaces) and `| shell_escape()`; a naive `ends_with("shell_escape")` check misses both or the paren form.
**Suggested fix:** Specify that the last-filter match is whitespace- and paren-tolerant, or add tests for both forms.

### F5 [nit] `gh` stub PATH handling and log record separator underspecified
**Where:** docs/designs/clo-631-shell-safe-workflow-interpolation.md (§Changed test helpers)
**What:** `write_gh_stub(bin_dir)` exists but the design never says PATH is prepended (must keep `sh`, `jq` reachable — `Command` resolution follows the child's PATH on Unix), nor how `read_gh_invocations` separates records.
**Suggested fix:** State "prepend `bin_dir` to PATH via `run_workflow_with_env`, keep the rest of the parent PATH", and define the invocation record separator (one newline per invocation, NUL within argv).

### F6 [nit] Follow-up tests hard-fail on hosts without `jq`
**Where:** docs/designs/clo-631-shell-safe-workflow-interpolation.md (Test plan, `test_review_pr_followups_*`)
**What:** `create_followups` exits 1 when `jq` is missing, so the three tests fail rather than skip on a machine without it. CI images have `jq` (Assumption 7, medium confidence), but local failures will look like regressions.
**Suggested fix:** Assert `command -v jq` in the Rust test up front with a message, or document the local prerequisite in the test header comment.

## Verdict
PASS_WITH_NOTES

The design is grounded in the actual engine and tree: `shell_escape`/`json_encode` are registered (src/template/filters.rs), `regex`/`toml` are non-optional dependencies so the gate needs no `cli` feature, `workflow` lives outside the lib so the `toml::Value` approach is correct, `run_shell` inherits parent env so `run_workflow_with_env` is viable, the `steps.X.field` fenced-JSON fallback (src/template/context.rs) makes the shell-stub `synthesize` step viable, `[OK] <step>` output exists for the success assertions, `apply_command_wrapper` matches Assumption 9, the 39/19/11 inventory matches the current files, the spec/pre-pr/design-review guards match the rewrite table, nothing depends on `run_followups`, and the hyphenated-step bracket rewrite is genuinely required (rework-pr and the README example are broken today). Remaining issues are minor test-spec gaps: stdout trimming in verbatim comparisons, quote-pass/heredoc interplay, violation granularity, and two helper contracts.
