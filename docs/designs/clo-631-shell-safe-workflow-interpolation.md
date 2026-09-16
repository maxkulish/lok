# Design: CLO-631 - Escape or remove step output interpolated into workflow shell fields

## Problem

Operators who run lok's checked-in workflows, and authors who copy them, pass step output straight into shell source. `WorkflowRunner::interpolate_with_fields` (`src/workflow.rs`) renders a step's `shell` field with MiniJinja, and `run_shell` hands the rendered text to `sh -c`. Once rendering is done, nothing can tell data from shell syntax. Discovery (`docs/discovery/clo-631.md`) and the PRD (`docs/prds/clo-631-shell-escape.md`) found review output from models written through fixed heredoc delimiters, health-check output inside literal apostrophes, parsed model fields inside quoted `gh` and `git` arguments, and a `review-pr` example that asks a second model for `gh issue` commands and runs its reply verbatim. An apostrophe in a review breaks a command. A `$()`, a backtick, or a line that matches a heredoc delimiter turns text from a PR body, design doc, or model reply into commands that run with the operator's credentials. A defence already exists: `shell_escape` in `src/template/filters.rs` is registered and unit-tested. No checked-in workflow uses it, however, the README teaches the unsafe apostrophe pattern, and no test stops new workflows from copying it. This matters now for two reasons. First, `.lok/workflows/design-review.toml`, `spec-review.toml` and `pre-pr-validation.toml` run inside lok's own task pipeline on every review. Second, a design-time inventory of the same files found 39 unescaped step-output tags in 19 shell fields across 11 files. That count includes `examples/workflows/fix.toml` step `comment`, which the discovery list missed.

## Goals / Non-goals

### Goals

- Every `steps.*` output tag in a `shell` field under `.lok/workflows/`, `examples/workflows/` and `tests/workflows/` ends in `| shell_escape`. Each escaped value is a complete shell word or the whole value of an assignment (PRD R1).
- Every heredoc that writes dynamic output is replaced with `printf '%s\n'` and a complete escaped argument (R2).
- `examples/workflows/review-pr.toml` no longer asks a model for shell. One shell step validates the whole `followups` array with `jq` before any side effect, then calls `gh issue create` with quoted arguments (R3).
- The PR head ref in `examples/workflows/rework-pr.toml` is quoted before it reaches `git` (PRD goal 4).
- A static gate, `tests/workflow_shell_policy.rs`, parses the checked-in TOML and fails with the file, step and line when a shell field has an unescaped step-output tag, a step-output tag inside a heredoc body, or an escaped tag inside shell quotes (R4, R2).
- A shell-only runtime regression sends hostile text through a step output into later shell steps and proves that nothing in it runs (R5).
- `README.md` and `docs/guides/lok-setup-guide.md` document the rule (R6).

### Non-goals

- Escaping every shell interpolation in the engine by default (discovery Approach B). The renderer has no shell AST, so automatic escaping would change the meaning of every existing user workflow.
- Protecting against a repository-controlled workflow that contains a deliberately malicious command. The trust boundary for choosing project workflows does not change.
- Changing interpolation in `prompt`, `when`/`if`, or other fields that are not shell.
- A command-approval UI.
- Changes to `shell_escape`, `run_shell`, `apply_command_wrapper`, the `Step`/`Workflow` schema, or any Rust type in `src/`.
- Protecting workflows that users already copied into their own `.lok/workflows/` or `~/.config/lok/workflows/`. The documentation reaches them; the gate does not.

## Architecture

### Notation

This document never writes MiniJinja delimiters literally. `<OUT: expr>` stands for an output tag that renders `expr`. `<IF: cond>`, `<ELSE>`, `<ENDIF>`, `<RAW>` and `<ENDRAW>` stand for the matching statement tags. `<ARG_1>` stands for the rendered first CLI argument.

### Trust boundary

The engine does not change. The design moves the trust decision into each call site and adds two tests to hold it in place:

```
 step output (model / command)      lok engine (unchanged)                    sh -c (run_shell)
 -----------------------------      ----------------------------------        ---------------------------------
 StepResult.output              ->  interpolate_with_fields               ->  one single-quoted word per value
 parsed JSON fields                 <OUT: x | shell_escape>                    printf '%s\n' '...'
                                    <OUT: n | string | shell_escape>           VAR='...' ; "$VAR"
                                    (shell_escape: src/template/filters.rs)

 tests/workflow_shell_policy.rs  -- static: TOML -> shell fields -> output tags -> rules
 tests/integration.rs            -- runtime: hostile payload through lok run -> no marker files
```

`shell_escape` removes NUL bytes, wraps the value in single quotes, and turns each embedded apostrophe into `'\''`. POSIX `sh` treats everything inside single quotes literally, including newlines, `$()`, backticks and lines that match a heredoc delimiter. The escaped value is safe only as a whole word, though. Inside `'...'`, the quotes cancel and the payload ends up unquoted. Inside `"..."`, the outer shell still expands `$()`. Inside a heredoc body, the quotes are just text and a delimiter line still ends the body. The patterns below keep every escaped value in word position.

### Safe composition patterns

| ID | Use | Shape |
|----|-----|-------|
| P1 | Write output to a file | `printf '%s\n' <OUT: steps.X.output \| shell_escape> > "$FILE"` |
| P2 | Compose a message body | `VALUE=<OUT: steps.X.field \| shell_escape>` then `BODY=$(printf 'Header\n\n%s\n' "$VALUE")` and `--body "$BODY"` |
| P3 | Test output content | `if printf '%s\n' <OUT: steps.X.output \| shell_escape> \| grep -q 'pattern'; then` |
| P4 | Pass a CLI argument | `git fetch origin <OUT: ref \| shell_escape>` |
| P5 | Non-string parsed fields | `<OUT: steps.X.number \| string \| shell_escape>` |
| P6 | Model JSON to CLI calls | escaped assignment, `jq -e` validation of the whole document, then `jq -r` per field into `"$VAR"` arguments |

Use `printf` rather than `echo`. `dash`'s `echo` interprets backslash escapes, and `echo` treats a leading `-n` as an option.

P5 exists because `shell_escape` takes `&str`. MiniJinja 2.19.0 rejects a non-string argument with `invalid operation: value is not a string`, and lok exposes parsed JSON numbers as numbers. A scratch workflow run with the installed `lok 20260915.0.0` confirmed both the error and the fix through the built-in `string` filter.

Rendering still supplies the trimmed `StepResult.output` for heredoc replacement. A quoted heredoc writes that rendered body plus one newline, and `printf '%s\n'` writes the same effective bytes. Existing `<IF: steps.X is defined ...>` guards stay around the rewritten lines, as `docs/lessons/clo-655-l5.md` requires.

### Call-site rewrites

| File | Step | Current pattern | Rewrite |
|------|------|-----------------|---------|
| `.lok/workflows/design-review.toml` | `ollama_review` | `echo '<OUT: steps.health_check.output>' \| grep` | P3 |
| `.lok/workflows/design-review.toml` | `write_reviews` | heredocs `ENDOLLAMA`, `ENDFALLBACK`, `ENDSYNTH` | P1, inside the existing fallback guard |
| `.lok/workflows/spec-review.toml` | `write_reviews` | heredocs `ENDOLLAMA`, `ENDCLAUDE`, `ENDSYNTH` | P1. The synthesis guard becomes `printf '%s\n' <IF: steps.synthesis is defined><OUT: steps.synthesis.output \| shell_escape><ELSE>'NO_REVIEWS_AVAILABLE'<ENDIF> >> "$FILE"` |
| `.lok/workflows/pre-pr-validation.toml` | `write_reports` | heredocs `LOK_WF_CODEX_OUTPUT_EOF`, `LOK_WF_SYNTH_OUTPUT_EOF`, `LOK_WF_FALLBACK_OUTPUT_EOF` into `$TMP_DIR/*.md` | P1 into the same temp paths; `write_review` is unchanged |
| `examples/workflows/fix.toml` | `comment` | `$(cat <<'LOKEOF' ...)` with `steps.debate.output` | P2 |
| `examples/workflows/pick-and-propose.toml` | `fetch`, `comment` | bare `steps.pick.number`; `LOKEOF` heredoc with `steps.debate.output` | P5 for the number, P2 for the body |
| `examples/workflows/review-pr.toml` | `comment` | `LOKEOF` heredoc with five `steps.synthesize.*` fields | P2, one assignment per field |
| `examples/workflows/review-pr.toml` | `create_followups` (LLM), `run_followups` (shell) | model writes shell, shell runs it | replaced by one shell step `create_followups` (P6), see below |
| `examples/workflows/full-heal.toml` | `issue`, `commit`, `pr` | fields inside `'...'` and an `EOF` heredoc | P2 for titles and bodies, P5 for `steps.pick.line` |
| `examples/workflows/rework-pr.toml` | `checkout`, `commit`, `comment` | unquoted `headRefName`; `EOF` heredoc; field inside `'...'` | P4 for the ref, P2 for the message and comment; use bracket access for every reference to a hyphenated step name in this file |
| `tests/workflows/test_interpolation.toml` | `step2`, `step3` | `echo 'step1 said: <OUT: ...>'` | `printf 'step1 said: %s\n' <OUT: ... \| shell_escape>`; existing assertions still match |
| `tests/workflows/test_parallel.toml` | `final` | three tags inside `'...'` | `printf 'All parallel steps: %s, %s, %s\n'` with three escaped words |
| `tests/workflows/test_shell_hash_expansion.toml` | `ollama_review` | same as the design-review health check | P3 |

When a rewrite moves a value out of a heredoc that is not a `steps.*` value, such as `workflow.backends` (always a string), that value is passed the same escaped way. `arg.*` references are left as they are because this PRD and regression gate are scoped to `steps.*` values.

`rework-pr` has a pre-existing render failure. A scratch run confirmed that `steps.fetch-pr.headRefName` renders as undefined, because MiniJinja reads the `-` as subtraction. Quoting alone does not make that step render. Every reference to a hyphenated step name in `rework-pr.toml` therefore moves to bracket access (`steps["fetch-pr"]`, `steps["fetch-diff"]`, and so on) in the same rewrite. Leaving the other prompt references broken would make the repaired checkout step untestable and preserve a workflow that cannot complete.

### `review-pr` follow-up flow

```
synthesize (claude, JSON reply)
        |
        | steps.synthesize.followups
        v
create_followups (shell, depends_on = ["synthesize"])
        |  command -v jq          -- missing -> stderr message, exit 1
        |  FOLLOWUPS=<OUT: steps.synthesize.followups | json_encode | shell_escape>
        |  jq -e <whole-array check>
        |        -- false / missing parsed field -> error, zero issues created
        |  COUNT=$(jq 'length')
        |        -- 0 -> "No follow-up issues needed", exit 0
        v
   for i in 0..COUNT:  TITLE/BODY/LABEL=$(jq -r --argjson i "$i" '.[$i].<field>')
                       gh issue create --title "$TITLE" --body "$BODY" --label "$LABEL" || exit 1
```

Lok first extracts the parsed `followups` field from the synthesis result, including JSON inside a Markdown fence, then `json_encode` serializes that array for the shell. A missing field or malformed model response fails during template rendering before the shell starts. The shell check requires the value to be an array and every element to be an object with string `title`, string `body`, and `label` equal to `bug` or `enhancement`. An empty array is the only no-op success. The loop stops at the first `gh` failure. Validation runs before any side effect, so one bad item blocks every issue. The loop is an index loop in the main shell, not `jq -c | while read`, so `|| exit 1` exits the step directly instead of a pipeline subshell.

### Static gate: `tests/workflow_shell_policy.rs`

The gate is an integration test. `workflow.rs` belongs to the binary crate (`src/main.rs` declares `mod workflow;`), so `tests/` cannot import `Workflow`. The gate reads the TOML with `toml::Value` from the non-optional `toml` dependency and matches tags with `regex`. Neither needs the `cli` feature.

1. For each root in `SCAN_ROOTS`, list `*.toml` files. Assert that each root has at least one file, so a path typo cannot pass with nothing scanned.
2. Parse each file and collect `(step name, shell source)` for every `[[steps]]` entry that has a `shell` key.
3. Remove `<RAW>` ... `<ENDRAW>` regions, keeping line breaks so line numbers stay correct.
4. Find heredoc body ranges first. Quote-state analysis skips each complete heredoc body and resumes unquoted after its closing delimiter, so quote characters in static heredoc data cannot leak into following shell commands.
5. Find output tags non-greedily, including the whitespace-control forms with `-`. Record the 1-based line in the field and the trimmed expression.
6. An expression references steps when it contains `steps` followed by `.` or `[` and no identifier character or `.` comes right before it.
7. Rule `MissingShellEscape`: a step-referencing expression whose last filter is not `shell_escape`. The matcher accepts whitespace variants and an optional empty call (`|shell_escape`, `| shell_escape`, and `| shell_escape()`), while `x | shell_escape | trim` is flagged.
8. Rule `InsideHeredoc`: a step-referencing tag on a line between a heredoc opener (`<<` or `<<-`, then an optional quote and an identifier) and the line that closes it (the delimiter alone, with leading tabs allowed after `<<-`). This holds even when the tag is escaped.
9. Rule `QuotedContext`: a small lexical pass tracks unquoted, single-quoted, and double-quoted state while skipping output/block tags and escaped characters. A step-referencing output tag encountered in either quote state is flagged, even when it has `shell_escape`; wrapping the generated single-quoted word in another quote context defeats the filter. Heredoc bodies are handled by rule 8 rather than by this state.
10. Emit one `Violation` per tag-rule pair. A bare tag inside a heredoc therefore reports both `MissingShellEscape` and `InsideHeredoc`; an unescaped quoted tag reports both `MissingShellEscape` and `QuotedContext`. Collect every violation and fail once, one line per violation: `<file>: step '<step>' shell line <n>: <expression>: <rule>`.
11. Assert that at least one escaped step-output tag was seen across all roots, so a broken tag regex cannot pass with nothing matched.

Statement tags (`<IF: steps.X is defined>`) are not output tags and are never flagged. The quote pass is intentionally not a general shell parser; it enforces the checked-in convention and has direct tests for multiline quotes, backslash escapes, tag-local string literals, and tags after shell comments.

### Runtime regression

`tests/workflows/test_shell_escape_hostile.toml` is a shell-only fixture, so the gate scans it and it shows the expected authoring style:

- `produce` writes a static payload through a quoted heredoc with the delimiter `LOK_TEST_PAYLOAD`. The step contains no output tags, so the heredoc is static text. The payload holds `'; touch "$LOK_TEST_MARKER_DIR/quote"; '`, a backtick `touch`, a `$(touch ...)`, a `; touch ...`, and for each of `ENDOLLAMA`, `ENDFALLBACK`, `ENDCLAUDE`, `ENDSYNTH`, `LOKEOF`, `EOF`, `LOK_WF_CODEX_OUTPUT_EOF`, `LOK_WF_SYNTH_OUTPUT_EOF` and `LOK_WF_FALLBACK_OUTPUT_EOF`, a standalone delimiter line followed by `touch "$LOK_TEST_MARKER_DIR/after-<DELIMITER>"`.
- `write_file` applies P1 to `$LOK_TEST_OUT_DIR/written.txt`.
- `compose` applies P2 and writes the body to `$LOK_TEST_OUT_DIR/composed.txt`.
- `pipe` applies P3 with `grep -qx 'ENDSYNTH'` and prints `LOK_TEST_DELIMITER_IS_DATA` on a match.

Each step starts with `: "${LOK_TEST_MARKER_DIR:?}" "${LOK_TEST_OUT_DIR:?}"`, so a manual run without the harness fails fast. The Rust test sets both variables to fresh `tempfile::TempDir` paths and runs the fixture through `cargo run --bin lok`, as `docs/lessons/clo-625-l3.md` and `docs/lessons/clo-656-l4.md` require. It asserts that the marker directory is empty and that the output files match the trimmed payload line by line.

### Documentation

- `README.md`: the introductory `comment` step becomes `shell = "gh issue comment 123 --body <OUT: steps[\"deep-dive\"].output | shell_escape>"`, with a short warning below it and bracket access for the hyphenated step name.
- `docs/guides/lok-setup-guide.md`: a "Step output in shell fields" subsection next to the variable reference. It states the rule (escape, complete word, no quotes or heredoc around the tag), patterns P1 to P6, `printf` over `echo`, `string` for non-strings, and "never execute model output; validate model JSON with `jq` and pass fields as quoted arguments".

## Public API surface

This change touches no public Rust API. These signatures stay as they are:

```rust
// src/template/filters.rs
fn shell_escape(value: &str) -> String;

// src/workflow.rs
async fn run_shell(cmd: &str, cwd: &Path, wrapper: Option<&str>) -> Result<ShellOutput>;

impl WorkflowRunner {
    fn interpolate_with_fields(
        &self,
        template: &str,
        results: &HashMap<String, StepResult>,
        workflow_name: &str,
        current_step: &str,
        field: &'static str,
    ) -> Result<String, WorkflowError>;
}
```

`Step`, `Workflow` and `Config` keep all their fields and serde attributes.

### New test-only surface: `tests/workflow_shell_policy.rs`

Bodies are elided.

```rust
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

/// Workflow directories scanned by the gate, relative to `CARGO_MANIFEST_DIR`.
const SCAN_ROOTS: [&str; 3] = [".lok/workflows", "examples/workflows", "tests/workflows"];

/// The authoring rule a shell field broke.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rule {
    /// The output tag references `steps` and its last filter is not `shell_escape`.
    MissingShellEscape,
    /// The output tag references `steps` and sits inside a heredoc body.
    InsideHeredoc { delimiter: String },
    /// The output tag is already inside a shell quote context.
    QuotedContext { quote: char },
}

/// One rule violation, located by file, step, and line within the shell field.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    file: PathBuf,
    step: String,
    line: usize,
    expression: String,
    rule: Rule,
}

/// An output tag found in a shell field after raw blocks are removed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputTag {
    line: usize,
    expression: String,
}

fn workflow_files(root: &Path) -> Vec<PathBuf>;
fn shell_fields(toml_source: &str) -> Result<Vec<(String, String)>, toml::de::Error>;
fn strip_raw_blocks(template: &str) -> String;
fn output_tags(template: &str) -> Vec<OutputTag>;
fn references_steps(expression: &str) -> bool;
fn ends_with_shell_escape(expression: &str) -> bool;
fn heredoc_bodies(template: &str) -> Vec<(RangeInclusive<usize>, String)>;
fn quote_contexts(template: &str, tags: &[OutputTag]) -> Vec<Option<char>>;
fn check_shell_field(file: &Path, step: &str, template: &str) -> Vec<Violation>;
```

### Changed test helpers: `tests/integration.rs`

Before:

```rust
fn run_workflow(workflow_path: &str) -> (bool, String);
```

After (`run_workflow` calls the new helper with no extra variables):

```rust
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn run_workflow(workflow_path: &str) -> (bool, String);
fn run_workflow_with_env(workflow_path: &Path, envs: &[(&str, &OsStr)]) -> (bool, String);

/// Writes an executable `gh` stub that appends its NUL-separated argv to `$LOK_TEST_GH_LOG`.
#[cfg(unix)]
fn write_gh_stub(bin_dir: &Path) -> PathBuf;

/// Builds a two-step workflow in `dir`: a `synthesize` shell step that prints
/// `$LOK_TEST_SYNTHESIS_FILE`, plus the `create_followups` table copied verbatim
/// from `examples/workflows/review-pr.toml`.
fn review_pr_followups_workflow(dir: &Path) -> PathBuf;

/// One entry per `gh` invocation, each the argv after `gh`.
fn read_gh_invocations(log: &Path) -> Vec<Vec<String>>;
```

### Workflow authoring surface: `examples/workflows/review-pr.toml`

Before (two steps; the model writes shell and the shell step runs it):

```toml
[[steps]]
name = "create_followups"
backend = "claude"
depends_on = ["synthesize"]
prompt = """ ... Create a gh issue command for EACH followup ... """

[[steps]]
name = "run_followups"
depends_on = ["create_followups"]
shell = "<OUT: steps.create_followups.output>"
```

After (one deterministic shell step; `run_followups` is removed):

```toml
[[steps]]
name = "create_followups"
depends_on = ["synthesize"]
shell = '''
command -v jq >/dev/null 2>&1 || { echo "create_followups: jq is required" >&2; exit 1; }
FOLLOWUPS=<OUT: steps.synthesize.followups | json_encode | shell_escape>
if ! printf '%s' "$FOLLOWUPS" | jq -e '
  type == "array"
  and all(.[];
    type == "object"
    and (.title | type) == "string"
    and (.body | type) == "string"
    and (.label == "bug" or .label == "enhancement"))
' >/dev/null; then
  echo "create_followups: follow-up validation failed; no issues created" >&2
  exit 1
fi
COUNT=$(printf '%s' "$FOLLOWUPS" | jq 'length')
if [ "$COUNT" -eq 0 ]; then
  echo "No follow-up issues needed"
  exit 0
fi
i=0
while [ "$i" -lt "$COUNT" ]; do
  TITLE=$(printf '%s' "$FOLLOWUPS" | jq -r --argjson i "$i" '.[$i].title')
  BODY=$(printf '%s' "$FOLLOWUPS" | jq -r --argjson i "$i" '.[$i].body')
  LABEL=$(printf '%s' "$FOLLOWUPS" | jq -r --argjson i "$i" '.[$i].label')
  gh issue create --title "$TITLE" --body "$BODY" --label "$LABEL" || exit 1
  i=$((i + 1))
done
'''
```

### Workflow authoring surface: output writes (`.lok/workflows/*.toml`)

Before:

```sh
cat >> docs/reviews/<ARG_2>-review-synthesis.md << 'ENDSYNTH'
<OUT: steps.synthesis.output>
ENDSYNTH
```

After:

```sh
printf '%s\n' <OUT: steps.synthesis.output | shell_escape> >> docs/reviews/<ARG_2>-review-synthesis.md
```

## Assumptions

- `sh` on the CI runners (`dash` on `ubuntu-latest`, bash in POSIX mode on `macos-latest`) reads a single-quoted word literally, including newlines, `$()`, backticks and heredoc-delimiter lines, and provides `printf` as a builtin. Confidence: high. Verification: a scratch run with the installed `lok 20260915.0.0` on macOS created no marker files, and the new runtime test covers both CI operating systems.
- `shell_escape` rejects non-string values with `value is not a string`, and `<OUT: x | string | shell_escape>` renders numbers. Confidence: high. Verification: the MiniJinja 2.19.0 `ArgType for &str` implementation and a scratch workflow run.
- Dot access on a hyphenated step name (`steps.fetch-pr.headRefName`) renders as undefined, and bracket access (`steps["fetch-pr"]`) works. Confidence: high. Verification: a scratch workflow run with the installed binary.
- `printf '%s\n' <escaped>` writes the trimmed `StepResult.output` plus the same terminating newline as the quoted heredoc it replaces, so the review files keep their effective format. Confidence: high. Verification: the hostile fixture has no leading/trailing blank lines, the runtime test compares trimmed content line by line, and one manual `design-review` run on the branch (see Test plan).
- Integration tests under `tests/` can use the non-optional package dependencies `toml`, `regex` and `tempfile` without the `cli` feature. Confidence: high. Verification: `cargo test --test workflow_shell_policy` compiles and runs.
- The three scan roots are flat directories, and no checked-in workflow uses `extends`, so parsing each file's own `[[steps]]` sees every shell field. Confidence: high. Verification: a directory listing and `rg '^extends'` returned nothing; the gate asserts that each root is non-empty.
- `jq` 1.5 or later (for `--argjson`) is on the GitHub-hosted `ubuntu-latest` and `macos-latest` images and on the machines of operators who run `review-pr`. Confidence: medium. Verification: the CI logs for the new follow-up tests; without `jq`, the step exits with an explicit message rather than creating issues.
- Operators leave `defaults.command_wrapper` unset or use a documented form (`'<CMD>'` in single quotes, or bare). `apply_command_wrapper` re-escapes apostrophes only for the single-quoted form, and a double-quoted wrapper would let the outer shell expand `$()` inside escaped values. Confidence: medium. Verification: README documents only single-quoted and bare forms; CLO-794 tracks validation/hardening of custom forms.
- No sibling worktree or open PR is working on CLO-631. Confidence: medium. Verification: the discovery check under `docs/lessons/clo-656-l3.md`; check again before opening the PR.

## Test plan

### Unit tests (in `tests/workflow_shell_policy.rs`, inline template strings)

- `flags_unescaped_step_output` - a bare `<OUT: steps.a.output>` gives `MissingShellEscape` with step name and line.
- `accepts_shell_escape_as_last_filter` - `<OUT: steps.a.output | shell_escape>`, `<OUT: steps.a.output|shell_escape()>`, and `<OUT: steps.a.n | string | shell_escape>` pass.
- `flags_shell_escape_before_another_filter` - `<OUT: steps.a.output | shell_escape | trim>` is flagged.
- `flags_bracket_step_access` - `<OUT: steps["fetch-pr"].output>` is flagged.
- `ignores_statement_tags_and_other_namespaces` - `<IF: steps.a is defined>`, `<OUT: arg.1>` and `<OUT: workflow.backends>` produce no violation.
- `ignores_raw_blocks` - a step tag inside a raw block is not flagged, and line numbers after the block stay correct.
- `handles_whitespace_control_markers` - output tags written with `-` trim markers are found and checked.
- `flags_escaped_step_output_inside_heredoc` - an escaped tag inside `<<'ENDSYNTH'`, `<<EOF` and `<<-LOKEOF` bodies gives `InsideHeredoc` with the delimiter, and a tag after the closing line does not.
- `flags_step_output_inside_shell_quotes` - escaped tags inside single and double quotes give `QuotedContext`; unquoted complete arguments and assignment values pass.
- `tracks_multiline_quotes_and_escapes` - quote state crosses newlines, honors shell backslash rules, skips quote characters inside MiniJinja tags, and does not treat a quote in a shell comment as opening a context.
- `heredoc_quotes_do_not_leak_into_following_commands` - an unpaired quote in a static heredoc body followed by a legitimate escaped tag after the delimiter produces no `QuotedContext` violation.
- `reports_file_step_and_line` - the failure message has the form `<file>: step '<step>' shell line <n>: <expression>: <rule>`.

### Repository gate

- `checked_in_workflows_escape_step_output_in_shell_fields` - scans `SCAN_ROOTS` and fails with every tag-rule violation. It also asserts that each root has files and that at least one escaped step tag was seen. Before rewrites the inventory is 39 unsafe tags; additional heredoc/quote diagnostics mean the violation-line count is greater than 39.

### Integration tests (`tests/integration.rs`, through `cargo run --bin lok`)

- `test_shell_escape_hostile_output_workflow` - runs `tests/workflows/test_shell_escape_hostile.toml` with temporary marker and output directories. The static payload intentionally has no leading/trailing blank lines because `run_shell` trims captured stdout. The test asserts that the workflow succeeds, `[OK]` appears for `write_file`, `compose` and `pipe`, the marker directory is empty, the output files match the trimmed payload line by line, and output contains `LOK_TEST_DELIMITER_IS_DATA`.
- `test_review_pr_followups_create_issues_from_validated_json` (`#[cfg(unix)]`) - first asserts `jq` is discoverable with an actionable prerequisite message. It prepends the temporary stub directory to the existing `PATH`, preserving `sh` and `jq`. Two valid items follow; the title holds `'` and `$(touch <marker>)`. The stub writes each argv as NUL-separated fields plus a second NUL between invocations. The test asserts exactly two `gh` invocations with the exact `issue create --title <T> --body <B> --label <L>` argv, and no marker file.
- `test_review_pr_followups_empty_list_has_no_side_effect` (`#[cfg(unix)]`) - `"followups": []` succeeds, prints `No follow-up issues needed`, and makes no `gh` call.
- `test_review_pr_followups_invalid_document_blocks_all_issues` (`#[cfg(unix)]`) - covers a valid item followed by an item labelled `wontfix`, a missing `followups` key, a non-string `body`, and malformed non-JSON output. Each case fails the workflow and makes zero `gh` calls. A valid reply wrapped in a Markdown fence is accepted because lok extracts the parsed `followups` field before `json_encode`.
- The same file checks the structure of `examples/workflows/review-pr.toml`: `create_followups` has `shell` and no `backend`/`prompt`, and no step named `run_followups` exists.
- The existing `test_interpolation_workflow`, `test_parallel_workflow` and `test_shell_hash_expansion_workflow` keep their assertions and pass against the rewritten fixtures.

### Per-backend test matrix

Not applicable. The change does not touch the `Backend` trait, `StepContext`, or any backend in `src/backend/`. The LLM-backed example workflows are not run in CI. Their shell steps are covered by the static gate and, for `review-pr` follow-ups, by the stubbed integration tests above.

### Manual verification

1. Before rewriting any workflow, run `cargo test --test workflow_shell_policy` and confirm that all 39 current unsafe tags appear at least once, including `examples/workflows/fix.toml` step `comment`. Tags in quotes or heredocs produce an additional contextual violation, so do not equate the tag count with the diagnostic-line count. After the rewrites, confirm that it passes.
2. As a negative control, temporarily remove `| shell_escape` from the `write_file` step of the hostile fixture and confirm that `test_shell_escape_hostile_output_workflow` fails (a marker is created or the step errors). Revert without committing.
3. Run `cargo run --bin lok -- run .lok/workflows/design-review.toml <existing design doc> <slug>` on the branch. Diff the header and body layout of the new `docs/reviews/<slug>-review-*.md` files against a review written before this change.
4. Run the pre-merge gate: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`. Per `docs/lessons/clo-655-l2.md`, confirm that the new tests appear in the output instead of relying on `cargo test --lib`.

## Migration / rollout

- No Rust API, config schema, CLI flag, or engine behaviour changes. No feature flag is needed. Within `src/`, the change adds nothing and removes nothing.
- The checked-in workflows change behaviour in these user-visible ways:
  - `review-pr` makes one fewer model call. It now needs `jq` at runtime and fails the `create_followups` step, without creating issues, when the synthesis reply is not a valid follow-up document. Before, it ran whatever the second model wrote. When a `gh issue create` fails partway through the list, the step stops. Issues already created remain.
  - A parsed field used in a shell step that is not a string now fails to render instead of printing its JSON form. For example, an array in `critical`, `important` or `minor` in `review-pr` fails closed. Numeric fields use `string` first.
  - Review files written by `design-review`, `spec-review` and `pre-pr-validation` keep their paths and layout.
- Workflows copied earlier into user or project directories stay unsafe until their owners apply the documented pattern. The guide change is the only thing that reaches them.
- Rollout order, all in one PR on `feat/clo-631-shell-escape`:
  1. Add `tests/workflow_shell_policy.rs` with the unit tests and the gate. The gate fails at this point; do not commit it alone.
  2. Rewrite the shell-only fixtures in `tests/workflows/` and add `test_shell_escape_hostile.toml` with its integration test.
  3. Rewrite `.lok/workflows/design-review.toml`, `spec-review.toml` and `pre-pr-validation.toml`, then do manual step 3. These three run inside lok's own task pipeline.
  4. Rewrite `examples/workflows/fix.toml`, `pick-and-propose.toml`, `full-heal.toml` and `rework-pr.toml`.
  5. Replace the `review-pr` follow-up steps and add the stubbed follow-up tests.
  6. Update `README.md` and `docs/guides/lok-setup-guide.md`.
  7. Run the full pre-merge gate. The PR commit must leave the gate passing.

## Open questions

None remain for CLO-631. The design decisions are:

1. The static gate tracks simple shell quote state and rejects escaped output tags inside single quotes, double quotes, or heredoc bodies. Direct tests bound the deliberately small lexer; it is not presented as a general shell parser.
2. `review-pr` uses `steps.synthesize.followups | json_encode | shell_escape`. This preserves lok's existing fenced-JSON extraction, fails rendering when `followups` is absent, and lets `jq` validate the complete array before side effects.
3. All references to hyphenated step names in `rework-pr.toml` move to bracket syntax. Fixing only the shell reference would leave the same workflow unable to complete.
4. Gate scope stays at `steps.*` output tags in `shell` fields under `.lok/workflows/`, `examples/workflows/`, and `tests/workflows/`. `arg`, `env`, loop variables, `verify`, and embedded workflows are explicitly outside this PRD.
5. The GitHub branch API enforces a ref shape that cannot begin with `-`; `gh --title/--body/--label` consume the following token as each option's value. Shell quoting is therefore sufficient for the affected sites without tool-specific separator changes.
6. CLO-631 supports the documented command-wrapper forms only: no wrapper, a bare `{cmd}`, or `{cmd}` inside single quotes. Unsafe custom double-quoted forms are outside this checked-in-workflow change and are tracked as [CLO-794](https://linear.app/cloud-ai/issue/CLO-794/prevent-command-wrapper-from-re-expanding-shell-escaped-workflow).
