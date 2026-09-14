# Spec: Stop shell `${#VAR}` from opening a Jinja comment, and report template errors without false blame

**Created**: 2026-09-14
**Task**: [CLO-655](https://linear.app/cloud-ai/issue/CLO-655), with [CLO-656](https://linear.app/cloud-ai/issue/CLO-656) bundled
**Estimated scope**: M (8 files, 6 sub-tasks)

## 1. Problem Statement

### What breaks

lok renders every workflow step's `prompt`, `shell` and `verify` field through one MiniJinja 2.19.0 environment, built in `TemplateEngine::new` (`src/template/mod.rs:70-75`) and called from `WorkflowRunner::interpolate_with_fields` (`src/workflow.rs:2930-2944`, invoked at `src/workflow.rs:1762-1778`). That environment uses MiniJinja's default delimiters, so `{#` opens a template comment.

The shell string-length expansion `${#VAR}` contains `{#`. MiniJinja reads it as the start of a comment, finds no closing `#}`, and fails with `SyntaxError: unexpected end of comment`. The step never runs.

`.lok/workflows/design-review.toml:40` contains exactly this:

```bash
if [ -z "$OUTPUT" ] || [ ${#OUTPUT} -lt 100 ]; then
```

The failure happens at `ollama_review`, which is upstream of `claude_fallback` and `synthesis`. `/design-doc:review` therefore produces no review files at all, and the Claude fallback that exists to guarantee one review is unreachable. The defect reproduces on the installed `lok 20260913.0.0`. The same line is present in the `gcm` and `via` repositories (3 sites each), so their design reviews are broken too.

Markdown prompts can hit the same bug. Pandoc-style heading attributes such as `## Setup {#setup}` also contain `{#`.

### A second defect behind the first in `design-review.toml`

Fixing line 40 is not enough for the pipeline to finish. When a step's `when` condition is false, the runner skips it with `continue` and never inserts a result (`src/workflow.rs:1613-1622`). When the Ollama review succeeds, `claude_fallback` is skipped, so `steps.claude_fallback` is undefined. `design-review.toml` reads it without a guard at lines 157-158 (synthesis prompt) and 230 and 234 (`write_reviews`). A probe through `interpolate_with_fields` confirmed that `success={{ steps.claude_fallback.success }}` fails with an undefined-value error when the step is absent. The `?` at `src/workflow.rs:1762-1772` then aborts the workflow before `synthesis` renders, so no review file is written on the success path either.

`spec-review.toml` does not have this problem. It wraps every fallback reference in `{% if steps.claude_fallback is defined %}` (lines 127 and 167-172), and the same probe rendered that guarded form as `SKIPPED`. Line 230 of `design-review.toml` has a second hazard when the fallback does run: `FALLBACK_OUTPUT='{{ steps.claude_fallback.output }}'` puts review text inside single quotes, so an apostrophe in the review ends the string. `spec-review.toml` writes the same output through a quoted heredoc and has no such assignment.

CLO-655's own acceptance criteria require that the pipeline "produces review files" and "degrades to the Claude fallback". Both require these guards, so porting the `spec-review.toml` pattern is in scope.

### Why it was misdiagnosed (CLO-656)

`map_template_error` (`src/workflow.rs:3124-3151`) turns every `TemplateError` into `WorkflowError::UnknownVariable`. It names the variable using the text at `TemplateError::source_range()`. When that slice is unusable, it falls back to `GENERIC_VAR_RE`, the first `{{ ... }}` anywhere in the template.

MiniJinja documents `Error::range()` as the location of the failing expression. It does not promise a variable name. Probes against lok's real `TemplateContext` and `protect_loop_vars` produced these results:

| Template | MiniJinja kind | Text at `range()` | Current lok report |
|---|---|---|---|
| `{{ a }} x=${#X} {{ a }}` | `SyntaxError`, "unexpected end of comment" | none, range is past the end | blames the first `{{ a }}` |
| `{% raw %}{{ item }}{% endraw %}` | `SyntaxError`, "unknown statement endraw" | `endraw` | `unknown variable '{{ endraw }}'` |
| `{{ steps.missing.output }}` | `UndefinedError` | `.missing.output` | `unknown variable '{{ .missing.output }}'` |
| `{{ steps.first.absent }}` | `UndefinedError` | `.first.absent` | `unknown variable '{{ .first.absent }}'` |
| `{{ steps.first.absent \| upper }}` | `UndefinedError` | `upper` | `unknown variable '{{ upper }}'` |
| `{{ env.NOPE }}`, `{{ arg.9 }}`, `{{ foo }}` | `UndefinedError` | `env.NOPE`, `arg.9`, `foo` | correct name |

The range text is therefore not a variable name. For attribute access it drops the root namespace, so every undefined `steps.X.Y` is reported today with a leading dot. The existing `test_map_template_error_reports_offending_variable_in_multi_expression` still passes because it checks only `contains("missing")`. When a filter receives an undefined value, the range points at the filter, and lok would report the filter's name as an unknown variable.

`TemplateError::from_minijinja` (`src/template/mod.rs:25-36`) already separates `UndefinedVariable`, `ParseError` and `RenderError`, but the split is imprecise. It sends `InvalidOperation` to `ParseError` whenever the error has a line, and render-time errors have one too: `{{ 1 + "x" }}` fails at render with `InvalidOperation` on line 2. Mapping `ParseError` straight to a syntax error would mislabel that render failure.

### Design decision: disable Jinja comment syntax engine-wide

MiniJinja cannot turn comments off. `SyntaxConfigBuilder::build` rejects an empty comment start delimiter (`minijinja-2.19.0/src/syntax.rs:973-995`). In effect, comments are disabled by setting the comment delimiters to a placeholder string that no workflow text contains. This needs MiniJinja's `custom_syntax` feature. That feature adds `aho-corasick`, which is already in `Cargo.lock` through `regex`. `aho-corasick` 1.1.4 declares `rust-version = "1.60.0"` and minijinja 2.19.0 declares `1.70`, both below lok's 1.83.

The change applies to the whole engine, not only to `shell` fields, for three reasons:

- **One syntax for every field.** A shell-only engine would make the same text render differently in `prompt` and in `shell`.
- **Prompts have the same exposure.** Markdown `{#id}` attributes are ordinary text.
- **No known workflow uses Jinja comments.** A sweep of all 14 `.lok/` directories under `~/Code` and `~/Work` found `{#` only inside shell `${#`. `examples/`, `tests/`, `README.md` and `docs/guides/` contain no Jinja comments either.

A probe with the placeholder config confirmed that `{% if %}...{% else %}...{% endif %}`, `{% raw %}...{% endraw %}`, `{{ }}` and whitespace control (`{{-`, `-%}`) behave the same. This matters because `spec-review.toml` and `pre-pr-validation.toml` use real `{% if %}` blocks, and `protect_loop_vars` (`src/workflow.rs:2993`) relies on `{% raw %}`.

**This is a user-visible behaviour change.** Only the two comment delimiters lose their meaning. Everything between them is still ordinary template text and is still evaluated:

| Template | Before | After |
|---|---|---|
| `{# {{ steps.first.output }} #}` | renders nothing | `{# ok #}` |
| `{# {{ steps.missing.output }} #}` | renders nothing | undefined-variable error |
| `{# {% if steps.first.output %} #}` | renders nothing | syntax error, unclosed block |

The local sweep cannot see workflows outside this machine. The setup guide documents the change, and the PR description calls it out under a "Behaviour change" heading.

This fix was attempted once before. On 2026-05-19 the CLO-373 branch set `comment_delimiters("{#%", "%#}")` in `src/template/mod.rs` without a ticket. Both validation reviewers flagged it as out of scope for a fixture-only task, and `docs/reviews/clo-373-validation-synthesis.md:20` asked for it to be split into a separate, designed PR or dropped. It was dropped, nothing replaced it, and the bug reached CLO-653 in August. This spec is that separate change. It has its own ticket, records the design decision above, and states the user-visible effect.

Rejected alternatives:

- **Pre-process `${#` before rendering.** A regex rewrite is fragile and does not cover `{#id}` in prompts.
- **Opt-in comment syntax per workflow.** Nobody uses comments, so a configuration switch would carry cost with no user.
- **Fix only `design-review.toml`.** This leaves the trap in place for every repo, and CLO-655's acceptance criteria ask for a test that `${#VAR}` runs.

### Existing escaping limits the docs must state

Two limits apply to literal `{{` or `{%` text. Both come from how loop variables are handled, which this change does not alter:

- **Raw blocks cannot contain loop-variable references.** `protect_loop_vars` wraps every `{{ item }}`, `{{ item.X }}` and `{{ index }}` in its own raw block, even inside a user's raw block. `{% raw %}{{ item }}{% endraw %}` becomes a nested raw block and fails with "unknown statement endraw". `{% raw %}{{ literal }} {% if %}{% endraw %}` renders correctly as `{{ literal }} {% if %}`.
- **`for_each` steps render each field twice.** Text that a raw block emits in the first pass is rendered again by `interpolate_loop_vars` (`src/workflow.rs:1860-1863`, `2955`). That function swallows render errors and returns its input unchanged. A probe showed that `Item {{ item }} literal {% raw %}{{ foo }}{% endraw %}` ends as `Item {{ item }} literal {{ foo }}`, so `{{ item }}` is never substituted and no error is shown.

## 2. Acceptance Criteria

### Template engine (CLO-655)

- [ ] **AC1 - comment openers render verbatim**: `TemplateEngine::render` of a template containing `${#OUTPUT}`, `${#}`, `${##}`, `## Setup {#setup}` and `{# note #}` next to a `{{ }}` interpolation returns every `{#` construct unchanged and renders the interpolation.
- [ ] **AC2 - blocks still work**: under the new config, `{% if %}...{% else %}...{% endif %}` renders the right branch. All existing tests in `src/template/` and `src/workflow.rs` pass. That includes the `test_condition_*` and `test_evaluate_condition_error_recovery` tests, which cover `when` conditions through `compile_expression`, and the `test_interpolate_loop_vars_*` tests.
- [ ] **AC3 - documented escape works on the workflow path**: `interpolate_with_fields` on `echo {% raw %}{{ literal }} {% if %}{% endraw %}` returns `echo {{ literal }} {% if %}`. This goes through `protect_loop_vars`, which an engine-only test does not.
- [ ] **AC4 - former comment contents are evaluated**: `interpolate_with_fields` on `{# {{ steps.first.output }} #}` returns `{# ok #}`, and on `{# {{ steps.missing.output }} #}` returns `UnknownVariable` naming `steps.missing.output`.
- [ ] **AC5 - end-to-end regression**: a workflow file with a dependent shell step that interpolates `{{ steps.health_check.output }}` and uses `${#OUTPUT}` runs to success through `lok run`, and its output contains `short` and `hello`. The test fails when the `set_syntax` call is reverted.

### Error reporting (CLO-656)

- [ ] **AC6 - syntax errors are reported as syntax errors**: `interpolate_with_fields` returns `WorkflowError::TemplateSyntax` for both of these cases. Its `Display` includes the MiniJinja detail and the line. It contains neither "unknown variable" nor "valid forms are" nor any `{{ }}` expression from the template.
  - `echo {{ steps.first.output }}\n{% if steps.first.success %}\nyes`, with detail "unexpected end of input, expected end of block" on line 2.
  - `{% raw %}{{ item }}{% endraw %}`, with detail "unknown statement endraw". Its range is in bounds, so this case also guards against blame based on the range text.
- [ ] **AC7 - reliable attribution names the full path**: `{{ steps.first.output }} then {{ steps.missing.output }}` returns `UnknownVariable` whose `variable` equals `steps.missing.output` exactly, not `.missing.output`, and whose `Display` contains `valid forms are`. The existing `test_map_template_error_reports_offending_variable_in_multi_expression` is tightened to these two assertions. `{{ env.LOK_TEST_UNSET_VAR }}` returns `UnknownVariable` naming `env.LOK_TEST_UNSET_VAR`.
- [ ] **AC8 - unreliable attribution names no variable**: `{{ steps.first.absent | upper }}` returns `WorkflowError::TemplateRender`, not `UnknownVariable`. Its `Display` quotes the enclosing expression `{{ steps.first.absent | upper }}` and the line. It does not report `upper` as a variable, and it has no valid-forms hint.
- [ ] **AC9 - other failures are render errors**: `{{ steps.first.output }}\n{{ 1 + "x" }}` returns `TemplateRender` with line 2 and MiniJinja's "unsupported types" detail, not `TemplateSyntax`. A hand-built `TemplateError::UndefinedVariable(minijinja::Error::new(ErrorKind::UndefinedError, "..."))`, which has no range, maps to `TemplateRender` with no expression quoted. `GENERIC_VAR_RE` and the first-`{{ }}` fallback are deleted.
- [ ] **AC10 - placeholder collision does not panic**: a template containing `{#lok-comments-disabled` with no end marker returns `TemplateSyntax`.

### Pipeline, docs and gate

- [ ] **AC11 - lok's design-review workflow is fixed**: `.lok/workflows/design-review.toml` contains no `${#`. Line 40 uses `[ "$(printf '%s' "$OUTPUT" | wc -c)" -lt 100 ]`. Every `steps.claude_fallback` reference in the `synthesis` prompt and in `write_reviews` sits inside a `{% if steps.claude_fallback is defined %}` block, following `spec-review.toml:127` and `167-172`. The `FALLBACK_OUTPUT='...'` assignment is replaced by a quoted heredoc inside that block. `rg -n '\{#' .lok/workflows/` returns nothing. `rg -n '\{%' .lok/workflows/` returns only `if`/`else`/`endif` lines that guard `steps.claude_fallback`, as the tree stands at implementation time.
- [ ] **AC12 - live gate 1, template rendering (required)**: with the worktree build, a design-review run gets past every template render with no `Workflow '...'` template error. The log shows `[step] ollama_review` executing and reaches `[step] synthesis`. A failure here is a defect in this change.
- [ ] **AC13 - live gate 2, pipeline completion on the success path (required to close CLO-655)**: the same run writes fresh `docs/reviews/clo-655-live-ok-review-ollama.md` and `clo-655-live-ok-review-synthesis.md`, and no `clo-655-live-ok-review-claude-fallback.md`.
- [ ] **AC14 - live gate 3, pipeline completion on the fallback path (required to close CLO-655)**: a separate run with slug `clo-655-live-fallback` and the Ollama leg forced to fail executes `claude_fallback`. It writes fresh `clo-655-live-fallback-review-claude-fallback.md` and `clo-655-live-fallback-review-synthesis.md`.
- [ ] **AC15 - documented**: the "Template Variables" section of `docs/guides/lok-setup-guide.md` states four things. `{#` and `#}` have no special meaning. Text between them is still evaluated. `{% raw %}...{% endraw %}` passes literal `{{` or `{%` through. The two limits in "Existing escaping limits the docs must state" apply, with a pointer to the follow-ups.
- [ ] **AC16 - gate green**: `make check` passes, which runs `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test`. `cargo +1.83.0 check --locked --all-targets` also passes, because the MSRV CI job from CLO-638 builds against 1.83.

**Test targets**: `template` and `workflow` are modules of the `lok` binary (`src/main.rs:16,18`). `src/lib.rs` declares only `backend`, so `cargo test --lib template::tests` passes with 0 tests run. Every focused command, including the revert checks, uses `cargo test --bin lok <filter>`. A focused run counts only if its summary shows a non-zero `passed` count.

**Live-run preconditions and procedure (AC12-AC14)**: `ollama list` returns at least one model, `ollama launch codex` works with the model pinned in `design-review.toml`, and the `claude` CLI is authenticated. Run from the worktree root with the worktree build, one run at a time, each with its own slug:

1. Delete any `docs/reviews/clo-655-live-*` files, then create a marker file in the scratchpad.
2. Run the success path with slug `clo-655-live-ok`.
3. Run the fallback path with slug `clo-655-live-fallback` and `OLLAMA_MODEL=lok-nonexistent-model`.
4. Confirm freshness with `find docs/reviews -name 'clo-655-live-*' -newer <marker>`.
5. Record the commands, the step log lines and the file list in the PR description, then delete the `clo-655-live-*` files instead of committing them.

**Verification method**: AC1-AC4 and AC6-AC10 are unit tests. AC5 is an integration test. AC5 and AC1 are each checked once by reverting the `set_syntax` call and watching them fail. AC6 and AC8 are checked once against the current `map_template_error` and must fail there. AC11 uses the two `rg` commands plus a read of `synthesis` and `write_reviews`. AC12-AC14 are live runs. AC15 is a read of the section. AC16 is the commands.

## 3. Constraints

**Must**:
- Keep one `TemplateEngine` and one `minijinja::Environment` per runner. Set the placeholder syntax in `TemplateEngine::new`.
- Keep `{{ }}` and `{% %}` delimiters and whitespace-control markers unchanged.
- Define the comment start and end placeholders as named constants with a doc comment explaining why they exist. Use start `{#lok-comments-disabled` and end `lok-comments-disabled#}`. The start must not begin with `{{` or `{%`.
- Enable `custom_syntax` on the existing optional `minijinja` dependency in `Cargo.toml` without changing its version requirement or making it non-optional.
- Narrow `TemplateError::from_minijinja` so `ParseError` holds only `ErrorKind::SyntaxError`. `InvalidOperation` and every other non-undefined kind become `RenderError`. The existing `test_parse_error` must still pass.
- Add two `WorkflowError` variants, `TemplateSyntax { workflow, step, line, detail }` and `TemplateRender { workflow, step, line: Option<usize>, detail }`. `detail` is MiniJinja's kind plus `detail()`, without the `(in <string>:N)` suffix. `TemplateRender` shows the line when present. Neither variant carries the valid-forms hint. `TemplateSyntax`'s hint points to the "Template Variables" section of `docs/guides/lok-setup-guide.md` and does not recommend `{% raw %}` by itself.
- Map in `map_template_error` by variant and by reliable attribution:
  - `ParseError` becomes `TemplateSyntax`.
  - `UndefinedVariable` becomes `UnknownVariable` only when the attribution is reliable. Take the error range, extend its start leftwards over `[A-Za-z0-9_.]`, and call the result the candidate path. Attribution is reliable when the candidate path matches `^(steps|env|arg|workflow)(\.[A-Za-z0-9_]+)+$`. These roots are the forms the valid-forms hint lists. `variable` is the candidate path.
  - Every other case becomes `TemplateRender`. When a range exists, `detail` quotes the enclosing expression. That is the text from the nearest `{{` or `{%` at or before the range start to the nearest `}}` or `%}` at or after the range end, truncated to 120 characters. If no enclosing tag is found, quote the trimmed source line instead.
- Port the guard pattern from `spec-review.toml` to `design-review.toml` without changing either workflow's prompts, models, timeouts or reviewer behaviour.
- Every new regression test must fail against the pre-change code. Check this by temporarily reverting the behaviour it guards, as was done for CLO-653 and CLO-633.

**Must-not**:
- Must not pre-process template text with regex to escape `{#`.
- Must not change `evaluate_condition` (`src/workflow.rs:2911-2920`). Its rule that a parse error runs the step is a separate, documented contract.
- Must not change `protect_loop_vars` or `interpolate_loop_vars`. The two escaping limits are documented and filed as follow-ups, not fixed here.
- Must not edit the `gcm` or `via` repositories.
- Must not fix the neighbouring shell-safety defects in the review workflows beyond the `FALLBACK_OUTPUT` assignment that the guard port removes. That excludes heredoc delimiter injection (flagged in CLO-373), CLO-631 and CLO-649.

**Prefer**:
- Put the new error helpers on `TemplateError` in `src/template/mod.rs`: a `line()` accessor, a message formatter and the raw range. `workflow.rs` then does not reach into `minijinja::Error`. The attribution and expression-context logic stays in `workflow.rs` next to `map_template_error`.
- Use the same `wc -c` form in `design-review.toml` as `.lok/workflows/spec-review.toml:47`, quoted.
- Size the new tests like their neighbours: one focused test per acceptance criterion.

**Escalate when**:
- Any existing test fails under the placeholder config. That would mean a workflow relies on comment syntax, or the delimiter matcher behaves differently from the probe.
- `cargo +1.83.0 check` fails because of the `custom_syntax` feature or its dependency.
- AC12 fails. This is a defect in this change and blocks the PR.
- AC13 or AC14 fails after the guard port for a reason outside template rendering, such as backend availability, reviewer configuration or shell handling of review text. Record the output as a follow-up. The PR description must state which live gates passed, and CLO-655 stays open instead of being closed as done.

## 4. Decomposition

1. **Disable comment syntax in the engine**: enable `custom_syntax`, add the placeholder constants, call `env.set_syntax` in `TemplateEngine::new`, and add unit tests for AC1 and AC2 - files: `Cargo.toml`, `Cargo.lock`, `src/template/mod.rs`
2. **Report template errors by kind and by reliable attribution**: narrow `ParseError` in `from_minijinja`, add the `TemplateError` helpers, add the `TemplateSyntax` and `TemplateRender` variants, rewrite `map_template_error` without `GENERIC_VAR_RE`, update its doc comment, tighten the existing multi-expression test, and add unit tests for AC3, AC4 and AC6-AC10 - files: `src/template/mod.rs`, `src/workflow.rs`
3. **End-to-end regression workflow**: add the CLO-655 reproduction as a shell-only workflow and an integration test for AC5 - files: `tests/workflows/test_shell_hash_expansion.toml`, `tests/integration.rs`
4. **Fix lok's design-review workflow**: replace `${#OUTPUT}` at line 40 and port the `claude_fallback` guards for AC11. The `wc -c` change also works with the installed binary - files: `.lok/workflows/design-review.toml`
5. **Document the syntax rule and the escaping limits** for AC15 - files: `docs/guides/lok-setup-guide.md`
6. **Live pipeline verification** for AC12-AC14 and the full gate for AC16, run with the worktree build - files: none committed

**Dependency order**: sub-task 4 is independent. Sub-task 2 follows sub-task 1, because both edit `src/template/mod.rs` and AC3 and AC4 need the placeholder config. Sub-tasks 3 and 5 depend on 1. Sub-task 6 depends on 1-4, and on 2 in particular, because the new error messages are what explain a failed live run.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Engine renders `${#OUTPUT}`, `${#}`, `${##}`, `{#setup}`, `{# note #}` beside `{{ steps.x.output }}` | `{#` constructs unchanged, interpolation rendered | `cargo test --bin lok template::tests` |
| 2 | Engine renders `{% if %}` with `else` | correct branch | `cargo test --bin lok template::tests` |
| 3 | `interpolate_with_fields` on the documented raw escape | `echo {{ literal }} {% if %}` | `cargo test --bin lok workflow::tests` |
| 4 | `interpolate_with_fields` on `{# {{ steps.first.output }} #}` and `{# {{ steps.missing.output }} #}` | `{# ok #}`, then `UnknownVariable` `steps.missing.output` | `cargo test --bin lok workflow::tests` |
| 5 | Integration test runs `tests/workflows/test_shell_hash_expansion.toml` | exit 0, output has `short` and `hello` | `cargo test --test integration test_shell_hash_expansion` |
| 6 | Revert `set_syntax`, rerun rows 1 and 5 | both fail | manual, before committing |
| 7 | Unclosed `{% if %}`, and `{% raw %}{{ item }}{% endraw %}` | `TemplateSyntax` with line and detail, no "unknown variable", no hint, no `{{ }}` quoted | `cargo test --bin lok workflow::tests` |
| 8 | `{{ steps.missing.output }}` after a valid one, and `{{ env.LOK_TEST_UNSET_VAR }}` | `UnknownVariable` with `variable == "steps.missing.output"` and `"env.LOK_TEST_UNSET_VAR"`, `Display` contains `valid forms are` | `cargo test --bin lok workflow::tests` |
| 9 | `{{ steps.first.absent \| upper }}` | `TemplateRender` quoting `{{ steps.first.absent \| upper }}`, `upper` not named as a variable, no hint | `cargo test --bin lok workflow::tests` |
| 10 | Render-time `{{ 1 + "x" }}` on line 2 | `TemplateRender`, line 2, "unsupported types", not `TemplateSyntax` | `cargo test --bin lok workflow::tests` |
| 11 | Hand-built `UndefinedVariable` error with no range | `TemplateRender`, no expression quoted, no hint | `cargo test --bin lok workflow::tests` |
| 12 | Template containing `{#lok-comments-disabled` with no end marker | `TemplateSyntax`, no panic | `cargo test --bin lok workflow::tests` |
| 13 | Rows 7 and 9 against the unmodified `map_template_error` | both fail | manual, before committing |
| 14 | `rg -n 'GENERIC_VAR_RE' src/` | no matches | shell |
| 15 | `rg -n '\{#' .lok/workflows/` and `rg -n '\{%' .lok/workflows/`, plus a read of `design-review.toml` `synthesis` and `write_reviews` | no `{#`; `{%` only on `claude_fallback` guard lines; no unguarded `steps.claude_fallback` | shell and read |
| 16 | Live gate 1: success-path run | log reaches `[step] synthesis` with no template error | `cargo run --quiet --bin lok -- run .lok/workflows/design-review.toml docs/designs/clo-638-msrv-ci-gate.md clo-655-live-ok --dir .` |
| 17 | Live gate 2: same run | fresh `clo-655-live-ok-review-ollama.md` and `-review-synthesis.md`, no fallback file | `find docs/reviews -name 'clo-655-live-ok-*' -newer <marker>` |
| 18 | Live gate 3: fallback run | `claude_fallback` executes; fresh `clo-655-live-fallback-review-claude-fallback.md` and `-review-synthesis.md` | `OLLAMA_MODEL=lok-nonexistent-model cargo run --quiet --bin lok -- run .lok/workflows/design-review.toml docs/designs/clo-638-msrv-ci-gate.md clo-655-live-fallback --dir .` |
| 19 | Full gate | all green | `make check` and `cargo +1.83.0 check --locked --all-targets` |

**Edge cases to verify**:
- Literal `#}` with no opener renders unchanged. The probe showed `a #} b {# c` renders verbatim.
- Whitespace control still works. The probe rendered `x {{- a -}} y {%- if a %}Z{% endif -%} w` as `xAyZw`.
- `protect_loop_vars` inserts raw tags but no newlines, so line numbers in `TemplateSyntax` and `TemplateRender` match the original field. Byte ranges refer to the protected text, so the expression-context scan must run on the protected text passed to `map_template_error`.
- `when` conditions go through `compile_expression`, which has no comment syntax, so their behaviour does not change.

## 6. Follow-ups (not in this change)

- `protect_loop_vars` nests a raw block inside a user's raw block when the user's block contains `{{ item }}`, `{{ item.X }}` or `{{ index }}`, which fails with "unknown statement endraw".
- `interpolate_loop_vars` re-renders `for_each` fields and swallows render errors. Escaped `{{` text then leaves every loop variable in the field unsubstituted, and no error is shown.
- `gcm` and `via` still contain `${#OUTPUT}` in `design-review.toml` (lines 46, 52, 107 in each). They recover when a lok release ships this fix.
- `design-review.toml` `write_reviews` still interpolates model output into fixed heredoc delimiters. A review line that equals the delimiter ends the heredoc (flagged in CLO-373, CLO-631 family).
