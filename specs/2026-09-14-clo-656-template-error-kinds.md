# Spec: Report workflow template failures by their real kind (CLO-656)

**Created**: 2026-09-14
**Linear**: [CLO-656](https://linear.app/cloud-ai/issue/CLO-656)
**Estimated scope**: S (2 source files, 5 sub-tasks)

## 1. Problem Statement

### How step bodies are rendered

Every workflow step's `prompt`, `shell` and `verify` field goes through `WorkflowRunner::interpolate_with_fields` (`src/workflow.rs:2930-2943`) before the step runs (call sites `src/workflow.rs:1762-1778`). That function wraps loop variables in `{% raw %}` blocks (`protect_loop_vars`, `src/workflow.rs:2993-3009`), then renders with `TemplateEngine::render` (`src/template/mod.rs:78-85`), a MiniJinja 2.19 environment in `SemiStrict` undefined mode. On failure it calls `map_template_error(err, &protected, workflow_name, current_step)` (`src/workflow.rs:3117-3151`). `WorkflowError` is binary-only (`mod workflow` is declared in `src/main.rs:18`, not in `src/lib.rs`), so changing it does not touch the library's public API.

### What goes wrong

`map_template_error` turns **every** `TemplateError` into `WorkflowError::UnknownVariable` (`src/workflow.rs:43-48`), whose message ends with `hint: valid forms are steps.X.output, steps.X.field, env.VAR, arg.N, workflow.backends`. It picks the variable to blame from `err.source_range()`. If that range is missing, out of bounds or blank, it falls back to `GENERIC_VAR_RE`, which returns the **first** `{{ ... }}` in the whole template.

A probe against MiniJinja 2.19.0 showed three defects in this path.

1. **Syntax errors blame an innocent variable.** In CLO-655, a shell body used the shell length expansion `${#OUT}`. MiniJinja reads `{#` as the start of a template comment that never closes, and reports `SyntaxError` with detail `unexpected end of comment`. That error's range points at end-of-input: past the template's length (`template.get` returns `None`), or at a lone `\n` that trims to empty. The fallback then named the first variable, and the user saw:

   ```
   Error: Workflow 'design-review': step 'ollama_review' has unknown variable '{{ steps.health_check.output }}'
     hint: valid forms are steps.X.output, steps.X.field, env.VAR, arg.N, workflow.backends
   ```

   `steps.health_check.output` was defined and correct. Both halves of the message were wrong, and the diagnosis took a bisect. MiniJinja's own `line()` for this error is also end-of-input: for a 6-line body with `${#OUT}` on line 3, it reports line 6.

2. **Render errors that are not about undefined values are misreported the same way.** `{{ steps.x.output | nosuch }}` fails with `ErrorKind::UnknownFilter` and becomes `unknown variable '{{ nosuch }}'`. `{{ steps.x.output + 1 }}` fails with `ErrorKind::InvalidOperation` and gets the same misreport. Note that `TemplateError::from_minijinja` (`src/template/mod.rs:25-35`) sorts `InvalidOperation` errors that carry a line into `TemplateError::ParseError`. Render-time type errors carry a line, so the `ParseError` variant is **not** a reliable "syntax error" signal. The real kind is `minijinja::ErrorKind`.

3. **Genuine undefined variables are named only in part.** For an undefined attribute, MiniJinja's range starts at the failing attribute, not at the root identifier. `{{ steps.missing.output }}` yields range text `.missing.output`, so the message reads `unknown variable '{{ .missing.output }}'`. Lookups whose root name is itself undefined (`{{ nope }}`, `{{ nope.field }}`) already produce the full name. Whitespace before the dot (`{{ steps .missing.output }}`) and bracket access (`{{ steps["missing"].output }}`) also give ranges that start after the root. A newline before the dot (`{{ steps\n.missing.output }}`) gives no range at all. The existing test `test_map_template_error_reports_offending_variable_in_multi_expression` (`src/workflow.rs:4816-4863`) only checks `contains("missing")`, so it passes today.

### Who is affected

Anyone whose workflow step body contains a syntax or render mistake gets sent to debug variables, dependencies and step outputs that are all fine. That includes shell bodies, where `${#VAR}` is common. This repo's own `.lok/workflows/design-review.toml:40` contains `${#OUTPUT}`. That breakage is CLO-655 and is **not** fixed here. This ticket fixes how the failure is reported.

## 2. Acceptance Criteria

- [ ] **AC1** - A step field whose template fails with `minijinja::ErrorKind::SyntaxError` returns a new `WorkflowError::TemplateSyntax` variant, never `UnknownVariable`. Its Display contains the field name (`prompt`, `shell` or `verify`), the MiniJinja detail text (e.g. `unexpected end of comment`), and `line N` when MiniJinja provides a line. N counts lines within that field's value, not within the workflow file. It does not contain `unknown variable` or `valid forms are`.
- [ ] **AC2** - A template that fails with any other non-undefined kind (e.g. `UnknownFilter`, `InvalidOperation`) returns a new `WorkflowError::TemplateRender` variant. Its Display contains the field name, MiniJinja's kind description and detail (e.g. `unknown filter: filter nosuch is unknown`), and `line N` when available. It does not contain `unknown variable` or `valid forms are`.
- [ ] **AC3** - No error produced by `map_template_error` names a variable or construct taken from a location other than the MiniJinja error's own range, with exactly two exceptions: (a) the unclosed-comment hint required by AC4, which lives in its own `hint` field and never replaces the parser's message; (b) AC5's root-identifier recovery, which may prepend the one identifier that immediately precedes a *usable* range, separated from it by nothing but ASCII whitespace. The `GENERIC_VAR_RE` "first `{{ }}` in the template" fallback is deleted.
- [ ] **AC4** - For a `SyntaxError` whose detail is `unexpected end of comment`, the error also carries a hint naming the line number and trimmed text of the unclosed `{#` (see Constraints for how it is found). When the character before that `{#` is `$`, the hint also says that shell `${#VAR}` opens a template comment and can be wrapped in `{% raw %}...{% endraw %}`. When no candidate `{#` is found, there is no hint.
- [ ] **AC5** - A genuine `UndefinedError` with a usable range (as defined in AC6) still returns `WorkflowError::UnknownVariable`, with the valid-forms hint unchanged. Let `text` be the trimmed range text. `variable` is chosen by the first rule that applies:
  1. **Range already starts at the root.** If `text` starts with an ASCII letter or `_`, `variable` is `text` verbatim, so `{{ nope.field }}` gives `nope.field`. `{{ env.MISSING }}` must give `env.MISSING`. In the real context `env` is a defined lazy object (`src/template/context.rs:85`), so that result may come from rule 1 or rule 2, and test 6 checks the output, not which rule produced it.
  2. **Root recoverable.** `text` starts with `.` or `[`. Going backwards from the range start, skip zero or more ASCII whitespace bytes, then take the maximal run of ASCII letters, digits and `_`. The run must be non-empty, must start with a letter or `_`, and must not be preceded by `.`. Then `variable` is that run followed by `text`, with all ASCII whitespace removed from `text` when it consists only of identifier characters, `.` and whitespace. `{{ steps.missing.output }}`, `{{ steps .missing.output }}` and `{{ steps . missing . output }}` all give `steps.missing.output`. `{{ steps["missing"].output }}` gives `steps["missing"].output`.
  3. **Root not recoverable.** Otherwise `variable` is `text` verbatim, with no leading `.` stripped, because a stripped path reads as a different variable. `{{ (steps).missing.output }}` gives `.missing.output`.
- [ ] **AC6** - A range is *usable* only if it is present, `template.get(range)` returns `Some` (in bounds and on char boundaries), and the text is non-empty after trimming. This check runs **before** any backward recovery, so recovery never starts from an unusable range. An `UndefinedError` whose range is absent, out of bounds, off a char boundary, empty or whitespace-only returns `TemplateRender` with message `undefined value` and names no variable. The real-runtime case is `{{ steps\n.missing.output }}`, for which MiniJinja 2.19 gives no range.
- [ ] **AC7** - `evaluate_condition` (`src/workflow.rs:2911-2920`) keeps its behavior: undefined gives `false`, any other error gives `true`.
- [ ] **AC8** - `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.

**Verification method**: unit tests in the `src/workflow.rs` test module, driven through `WorkflowRunner::interpolate_with_fields` (the path every step field takes). For AC6, the tests also call the name-recovery helper directly, because MiniJinja's span setter is `pub(crate)` (`minijinja-2.19.0/src/error.rs:231`), so a test cannot build an `UndefinedError` with an arbitrary range. A manual `cargo run --bin lok -- run` of the fixture in test 10 shows the message before and after the change.

## 3. Constraints

**Must**:
- Classify on `minijinja::ErrorKind` (`SyntaxError` -> `TemplateSyntax`, `UndefinedError` -> `UnknownVariable` or AC6, anything else -> `TemplateRender`). Expose what `map_template_error` needs from `TemplateError` through accessors on `TemplateError` in `src/template/mod.rs` (e.g. `kind()`, `detail()`, `line()`, or one accessor returning the inner `&minijinja::Error`). Refactor `source_range` to use the same accessor rather than repeating the three-arm match.
- Build the message from MiniJinja's kind description and `detail()`, formatted `<kind>: <detail>`, or detail alone for `TemplateSyntax`, where the variant name already says "syntax error". Report the line as a separate field. Never store MiniJinja's `Display` text: it ends with an `(in <string>:N)` suffix, where `<string>` is noise and the line is already reported. If `detail()` is `None` (real `UndefinedError`s have no detail), use the kind description alone (`err.kind().to_string()`, e.g. `undefined value`). MiniJinja 2.19 implements `Display` for `ErrorKind` (`minijinja-2.19.0/src/error.rs:177`).
- Keep line numbers correct against the user's original field text. `protect_loop_vars` inserts `{% raw %}` wrappers without newlines, so MiniJinja's line for the protected text equals the original line. Byte ranges refer to the protected text, so slice `protected`, as the code does today.
- Find the unclosed-comment candidate (AC4) with a single left-to-right scan of the template passed to `map_template_error` that visits delimiters in the same order MiniJinja's lexer does. A "skip every raw region, then search" two-pass approach or a single regex is **not** acceptable, because it disagrees with the lexer in the cases below, which were verified against MiniJinja 2.19.0:
  - At `{#`: search for the next `#}`. If there is none, this `{#` is the candidate. Otherwise resume after that `#}`. The lexer does not recognise `{% raw %}` inside a comment, so a `#}` inside a raw block **does** close an earlier comment.
  - At a raw open tag, find the next endraw tag, resume after it, and return no candidate if there is none. Raw blocks do not nest in MiniJinja: the first endraw closes the block. Both tags have the grammar `{%` [`-` | `+`] *ASCII-whitespace* `raw` or `endraw` *ASCII-whitespace* [`-` | `+`] `%}`, where each whitespace run may be empty and may include `\n` and `\t`. MiniJinja 2.19 accepts all of `{%+ raw %}`, `{%raw%}`, `{% raw +%}`, `{%+ endraw +%}`, `{%-raw-%}` and `{%\traw\n%}` (`+` is documented at https://docs.rs/minijinja/latest/minijinja/syntax/#whitespace-control). A scanner that misses one of them would treat that raw block's `${#A}` as the candidate and blame the wrong line.
  - At `{{`: resume after the next `}}`, so a `{#` inside an expression such as `{{ "{#" }}` is not a candidate.
  - At any other `{%`: resume after the next `%}`.

  Report the candidate's 1-based line and that line's trimmed text, truncated to 80 bytes with the existing `crate::utils::truncate_utf8` (already imported in `src/workflow.rs:22`). This is a hint, so string literals containing `}}` or `%}` inside a block may misplace or suppress it. That is acceptable, and those cases must not panic.
- Implement AC5 and AC6 in one pure helper, `undefined_variable_name(template: &str, range: Option<Range<usize>>) -> Option<String>`. `None` means the range is unusable, and the caller then builds `TemplateRender`. Walk backwards over bytes and match only ASCII. A validated range start is a char boundary, and ASCII bytes never occur inside a multi-byte UTF-8 sequence, so the walk cannot split a character.
- Keep the existing variants' Display strings, `UnknownVariable`'s included, byte-for-byte.
- Give both new variants `workflow`, `step` and `field` fields, with a Display prefix matching the siblings: `Workflow '{workflow}': step '{step}' ...`, followed by the field name and the line, e.g. `... has a template syntax error in its shell field at line 6: unexpected end of comment`. Add a `field: &str` parameter to `interpolate_with_fields` and `map_template_error`. The three production call sites (`src/workflow.rs:1762-1778`) pass `"prompt"`, `"shell"` and `"verify"`. Existing test call sites pass whichever field their template stands for (`"prompt"` when it doesn't matter).

**Must-not**:
- Do not change `TemplateError::from_minijinja` classification or its variants. `evaluate_condition` and `src/template/mod.rs` tests depend on them.
- Do not fix CLO-655 (the `${#OUTPUT}` in `.lok/workflows/design-review.toml`, or making `{#` safe in shell fields). Do not change MiniJinja syntax configuration or delimiters.
- Do not add a "first `{{ }}`" or any other location-independent guess on any path.
- Do not attach the valid-forms hint to `TemplateSyntax` or `TemplateRender`.
- Do not touch `interpolate_loop_vars`, which swallows render errors on purpose (`src/workflow.rs:2983-2985`).
- Do not add a dependency.

**Prefer**:
- Small private helpers next to `map_template_error` (`unclosed_comment_opener(template) -> Option<(usize, String, bool)>` and `undefined_variable_name` above) over growing one function.
- Plain `str::find` / byte-index scanning for the comment scanner. If a regex helps with the raw-tag forms, anchor it at the current position and follow the file's `static ..._RE: LazyLock<regex::Regex>` idiom.
- Rust doc comments on the new variants and helpers in the style of the surrounding code. No comments narrating the change.

**Escalate when**:
- MiniJinja 2.19 does not expose a kind description string usable for AC2 (`ErrorKind` has no `Display` or `description`), and the only alternative is hand-maintaining a kind-to-text table.
- An existing test outside the one named in Sub-task 5 fails because it asserts on the old `UnknownVariable` text for a non-undefined error.
- The change needs to touch any file other than `src/workflow.rs` and `src/template/mod.rs` (docs and status files excluded).

## 4. Decomposition

1. **`TemplateError` accessors**: Add an accessor (or `kind`/`detail`/`line` accessors) on `TemplateError`, and route `source_range` through it. Behavior is unchanged. Files: `src/template/mod.rs:24-51`.
2. **New `WorkflowError` variants**: Add `TemplateSyntax { workflow, step, field, message, line: Option<usize>, hint: Option<String> }` and `TemplateRender { workflow, step, field, message, line: Option<usize> }` with thiserror Display strings meeting AC1, AC2 and AC4. Optional parts render as empty strings when `None`, so the same helper-call style as `duplicates.join(", ")` at `src/workflow.rs:50` is fine. Files: `src/workflow.rs:28-82`.
3. **Rewrite `map_template_error`**: Dispatch on kind. Delete `GENERIC_VAR_RE`. Add `undefined_variable_name` (AC5 and AC6) and the unusable-range fallback to `TemplateRender`. Thread the new `field` parameter through `interpolate_with_fields` and its three production call sites, then update the existing test call sites. Update the `map_template_error` doc comment and the `interpolate_with_fields` doc line `Any remaining undefined variable surfaces as ...` so they describe all three outcomes. Files: `src/workflow.rs:1762-1778`, `src/workflow.rs:2922-2943`, `src/workflow.rs:3117-3151`, and `interpolate_with_fields` calls in the test module.
4. **Unclosed-comment scanner**: A private helper implementing the lexer-order scan from Constraints, wired into the `TemplateSyntax` branch when `detail() == Some("unexpected end of comment")` to fill `hint` (AC4). Files: `src/workflow.rs` (next to `map_template_error`).
5. **Tests**: Add tests 1-7, 9 and 11 from the Evaluation table to the `src/workflow.rs` test module next to `test_map_template_error_reports_offending_variable_in_multi_expression`. Tighten that test's assertion to `variable == "steps.missing.output"` and add a Display check for `valid forms are`. Files: `src/workflow.rs` (tests module, around `:4816`).

**Dependency order**: 1 and 2 are independent. 3 needs 1 and 2. 4 needs 2. 3 and 4 are independent of each other. 5 needs 3 and 4 to pass. Its tests need 2 to compile, so they can be written red-first once 2 has landed.

## 5. Evaluation

All automated tests use a `WorkflowRunner` built as the existing test does (`Config::default()`, `PathBuf::from(".")`), with a `results` map holding a successful step `health_check` (or `first`) whose output is `ok`.

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | CLO-655 shape: template `"OUT=\"{{ steps.health_check.output }}\"\nset -e\nif [ ${#OUT} -lt 10 ]; then\n  echo short\nfi\necho done\n"` through `interpolate_with_fields` with field `"shell"` | `TemplateSyntax` with `field == "shell"` and `line == Some(6)`, the line MiniJinja reports. Display contains `shell`, `unexpected end of comment` and `line 6`, and contains none of `unknown variable`, `valid forms are`, `steps.health_check.output`, `(in <string>` | `cargo test template_syntax` |
| 2 | Same template: the hint | `hint` is `Some`, naming `line 3`, containing `if [ ${#OUT} -lt 10 ]; then`, and mentioning `${#VAR}` and `{% raw %}` | `cargo test unclosed_comment` |
| 3 | Non-comment syntax error: `"{{ steps.first.output }} {% endfor %}"` with field `"prompt"` | `TemplateSyntax`. Display contains `prompt` and `unknown statement endfor`. `hint` is `None`. No `unknown variable` | `cargo test template_syntax` |
| 4 | Unknown filter: `"{{ steps.first.output }} {{ steps.first.output \| nosuch }}"` with field `"verify"` | `TemplateRender`. Display contains `verify`, `unknown filter` and `nosuch`. No `unknown variable`, no `valid forms are`, no `(in <string>` | `cargo test template_render` |
| 5 | Render-time type error with a line (`TemplateError::ParseError` today): `"{{ steps.first.output + 1 }}"` | `TemplateRender`, **not** `TemplateSyntax`. Display contains `invalid operation` | `cargo test template_render` |
| 6 | Genuine undefined (tightened existing test): `"{{ steps.first.output }} then {{ steps.missing.output }}"`, plus `"{{ env.LOK_CLO656_SURELY_UNSET }}"` | `UnknownVariable` with `variable == "steps.missing.output"` and `variable == "env.LOK_CLO656_SURELY_UNSET"`. Display contains `valid forms are` | `cargo test test_map_template_error_reports_offending_variable` |
| 7 | Unusable ranges (AC6). (a) Direct `undefined_variable_name` cases, each returning `None`: absent (`None`); out of bounds (`Some(30..40)` on a 24-byte template); empty (`Some(3..3)`); whitespace-only (template `"steps   .x"`, range `Some(5..8)` covering the three spaces, which must not recover `steps`); off a char boundary (template `"é{{ x }}"`, `Some(1..2)`). (b) Mapper with a detail-less error: `map_template_error(TemplateError::UndefinedVariable(minijinja::Error::from(minijinja::ErrorKind::UndefinedError)), "{{ steps.first.output }}", "wf", "step", "prompt")`. (c) Real runtime: `"{{ steps\n.missing.output }}"` through `interpolate_with_fields` | (a) `None` every time. (b) and (c) `TemplateRender` with `message == "undefined value"`. Display does not contain `steps.first.output` (b), `missing` (c), `valid forms are` or `(in <string>` | `cargo test undefined_without_range` |
| 8 | No regressions | All tests pass. Clippy and fmt are clean | `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` |
| 9 | Scanner follows lexer order, via `interpolate_with_fields`. (a) `"{# note\n{% raw %}#}\n${#B}\n"`: the raw `#}` closes the line-1 comment, so the hint names line 3, not line 1. (b) `"{{ \"{#\" }}\n${#B}\n"`: the hint names line 2. (c) `"{% raw %}${#A}{% endraw %}\nok\n${#B}\n"`: the hint names line 3. (d) The same template without its last line renders successfully. Raw-tag forms, each followed by `\nok\n${#B}\n`: (e) `{%+ raw %}${#A}{% endraw %}`, (f) `{%raw%}${#A}{%endraw%}`, (g) `{% raw +%}${#A}{%+ endraw +%}`, (h) `{%-raw-%}${#A}{%-endraw-%}`. (i) `"{%\traw\n%}${#A}{%\nendraw\t%}\nok\n${#B}\n"` | (a), (e)-(h): the hint names line 3. (b): line 2. (i): line 5 (the tags span lines, and MiniJinja also reports line 5). No hint names `${#A}`. (d) returns `Ok` | `cargo test unclosed_comment` |
| 10 | Manual end-to-end with the fixture below | Before: exit 1 with the message shown below the fixture, and no `b-ran` file. After: exit 1, a `TemplateSyntax` message naming step `b`, its `shell` field and `line 6`, a hint naming line 3, and still no `b-ran` file. Paste both outputs into the PR description | See recipe below |
| 11 | Name recovery shapes (AC5) through `interpolate_with_fields`, with no step `missing` in `results`: `"{{ steps .missing.output }}"`, `"{{ steps . missing . output }}"`, `"{{ steps[\"missing\"].output }}"`, `"{{ (steps).missing.output }}"`, `"{{ nope.field }}"` | `UnknownVariable` with `variable` equal to `steps.missing.output`, `steps.missing.output`, `steps["missing"].output`, `.missing.output` and `nope.field` respectively | `cargo test undefined_variable_name` |

**Test 10 fixture and recipe.** Checked on 2026-09-14 against `c1ea11e`: `cargo run -- run` fails with "could not determine which binary to run" because the package has `lok`, `lokomotiv` and `silence_probe` binaries, so the recipe passes `--bin lok`. Save as `wf.toml` in a fresh temp directory `$D`:

```toml
name = "clo-656-repro"
description = "Template syntax error in a shell body"

[[steps]]
name = "a"
shell = "echo ok"

[[steps]]
name = "b"
depends_on = ["a"]
shell = '''
OUT="{{ steps.a.output }}"
touch "{{ env.CLO656_DIR }}/b-ran"
if [ ${#OUT} -lt 10 ]; then
  echo short
fi
echo done
'''
```

TOML drops the newline right after `'''`, so line 1 of the field is `OUT=...`, the `{#` is on line 3, and end-of-input is line 6. Run:

```sh
CLO656_DIR="$D" cargo run --bin lok -- run "$D/wf.toml" --dir "$D"; echo "exit=$?"
test -e "$D/b-ran" && echo "FAIL: step b body executed" || echo "ok: step b body never executed"
```

Output on `c1ea11e` (before), after step `a` succeeds:

```
Error: Workflow 'clo-656-repro': step 'b' has unknown variable '{{ steps.a.output }}'
  hint: valid forms are steps.X.output, steps.X.field, env.VAR, arg.N, workflow.backends
exit=1
ok: step b body never executed
```

**Edge cases to verify** (fold into tests 2 and 3 as extra assertions, or add one small test if an assertion cannot be added to an existing template):
- A closed comment `{# ok #}` followed later by an unclosed `{#` without `$`: the hint names the unclosed one's line and omits the `${#VAR}` sentence.
- A template containing `{{ item }}` (so `protect_loop_vars` rewrites it) together with an unclosed `${#X}` on line 2 of 3: the reported line and hint line match the original template's line numbers.
- Multi-byte text on the opener's line longer than 80 bytes: the snippet is cut on a char boundary, with no panic.
- `evaluate_condition` still returns `true` for `"{% garbage"` and `false` for `"steps.missing.success"`. Existing tests should already cover this; confirm they still pass rather than adding new ones.
