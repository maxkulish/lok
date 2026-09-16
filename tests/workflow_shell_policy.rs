use regex::Regex;
use std::fs;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use toml::Value;

const SCAN_ROOTS: [&str; 3] = [".lok/workflows", "examples/workflows", "tests/workflows"];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Rule {
    MissingShellEscape,
    InsideHeredoc { delimiter: String },
    QuotedContext { quote: char },
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingShellEscape => write!(f, "MissingShellEscape"),
            Self::InsideHeredoc { delimiter } => write!(f, "InsideHeredoc({delimiter})"),
            Self::QuotedContext { quote } => write!(f, "QuotedContext({quote})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    file: PathBuf,
    step: String,
    line: usize,
    expression: String,
    rule: Rule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputTag {
    start: usize,
    end: usize,
    line: usize,
    expression: String,
}

fn workflow_files(root: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn shell_fields(source: &str) -> Result<Vec<(String, String)>, toml::de::Error> {
    let document: Value = source.parse()?;
    Ok(document
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|step| {
            let name = step.get("name")?.as_str()?.to_owned();
            let shell = step.get("shell")?.as_str()?.to_owned();
            Some((name, shell))
        })
        .collect())
}

fn strip_raw_blocks(template: &str) -> String {
    let raw = Regex::new(r"(?s)\{%[-+]?\s*raw\s*[-+]?%\}.*?\{%[-+]?\s*endraw\s*[-+]?%\}").unwrap();
    raw.replace_all(template, |captures: &regex::Captures<'_>| {
        captures[0]
            .chars()
            .map(|character| if character == '\n' { '\n' } else { ' ' })
            .collect::<String>()
    })
    .into_owned()
}

fn output_tags(template: &str) -> Vec<OutputTag> {
    let tag = Regex::new(r"(?s)\{\{\-?\s*(.*?)\s*\-?\}\}").unwrap();
    tag.captures_iter(template)
        .map(|captures| {
            let whole = captures.get(0).unwrap();
            let expression = captures.get(1).unwrap().as_str().trim().to_owned();
            OutputTag {
                start: whole.start(),
                end: whole.end(),
                line: template[..whole.start()]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1,
                expression,
            }
        })
        .collect()
}

fn references_steps(expression: &str) -> bool {
    let bytes = expression.as_bytes();
    let mut offset = 0;
    while let Some(relative) = expression[offset..].find("steps") {
        let start = offset + relative;
        let end = start + "steps".len();
        let before_ok = start == 0
            || (!bytes[start - 1].is_ascii_alphanumeric()
                && bytes[start - 1] != b'_'
                && bytes[start - 1] != b'.');
        let after_ok = matches!(bytes.get(end), Some(b'.' | b'['));
        if before_ok && after_ok {
            return true;
        }
        offset = end;
    }
    false
}

fn ends_with_shell_escape(expression: &str) -> bool {
    Regex::new(r"(?x)\|\s*shell_escape\s*(?:\(\s*\))?\s*$")
        .unwrap()
        .is_match(expression)
}

fn heredoc_bodies(template: &str) -> Vec<(RangeInclusive<usize>, String)> {
    let opener = Regex::new(
        r#"<<(-?)\s*(?:'([A-Za-z_][A-Za-z0-9_]*)'|\"([A-Za-z_][A-Za-z0-9_]*)\"|([A-Za-z_][A-Za-z0-9_]*))"#,
    )
    .unwrap();
    let lines = template.lines().collect::<Vec<_>>();
    let mut bodies = Vec::new();
    let mut line = 0;
    while line < lines.len() {
        let captures = opener.captures_iter(lines[line]).collect::<Vec<_>>();
        if captures.is_empty() {
            line += 1;
            continue;
        }
        let mut search_from = line + 1;
        for capture in captures {
            let delimiter = capture
                .get(2)
                .or_else(|| capture.get(3))
                .or_else(|| capture.get(4))
                .unwrap()
                .as_str()
                .to_owned();
            let strip_tabs = capture
                .get(1)
                .is_some_and(|value| !value.as_str().is_empty());
            let close = (search_from..lines.len()).find(|candidate| {
                let candidate_line = lines[*candidate];
                let closing = if strip_tabs {
                    candidate_line.trim_start_matches('\t')
                } else {
                    candidate_line
                };
                closing == delimiter
            });
            let last = close.unwrap_or(lines.len());
            if line + 1 < last {
                bodies.push((line + 2..=last, delimiter));
            }
            match close {
                Some(close) => search_from = close + 1,
                None => break,
            }
        }
        line = search_from;
    }
    bodies
}

fn line_in_heredoc(line: usize, bodies: &[(RangeInclusive<usize>, String)]) -> Option<String> {
    bodies
        .iter()
        .find_map(|(range, delimiter)| range.contains(&line).then(|| delimiter.clone()))
}

fn statement_tags(template: &str) -> Vec<(usize, usize)> {
    let tag = Regex::new(r"(?s)\{%[-+]?\s*.*?\s*[-+]?%\}").unwrap();
    tag.find_iter(template)
        .map(|match_| (match_.start(), match_.end()))
        .collect()
}

fn quote_contexts(template: &str, tags: &[OutputTag]) -> Vec<Option<char>> {
    let bodies = heredoc_bodies(template);
    let mut skip_ranges = tags
        .iter()
        .map(|tag| (tag.start, tag.end))
        .chain(statement_tags(template))
        .collect::<Vec<_>>();
    skip_ranges.sort_by_key(|(start, _)| *start);
    let mut contexts = vec![None; tags.len()];
    let mut tag_index = 0;
    let mut skip_index = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    let mut line = 1;
    let mut index = 0;

    while index < template.len() {
        if let Some((start, end)) = skip_ranges.get(skip_index).copied() {
            if index == start {
                while tag_index < tags.len() && tags[tag_index].start == start {
                    contexts[tag_index] = quote;
                    tag_index += 1;
                }
                let skipped = &template[start..end];
                line += skipped.bytes().filter(|byte| *byte == b'\n').count();
                comment = false;
                escaped = false;
                index = end;
                skip_index += 1;
                continue;
            }
        }

        let in_body = line_in_heredoc(line, &bodies).is_some();
        let byte = template.as_bytes()[index];
        if byte == b'\n' {
            line += 1;
            comment = false;
            escaped = false;
            index += 1;
            continue;
        }
        if in_body {
            index += 1;
            continue;
        }
        if comment {
            index += 1;
            continue;
        }
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if byte == b'\\' && quote != Some('\'') {
            escaped = true;
            index += 1;
            continue;
        }
        match quote {
            Some(current) if byte == current as u8 => quote = None,
            Some('"') => {}
            None if byte == b'\'' || byte == b'"' => quote = Some(byte as char),
            None if byte == b'#'
                && (index == 0
                    || template.as_bytes()[index - 1].is_ascii_whitespace()
                    || matches!(
                        template.as_bytes()[index - 1],
                        b';' | b'&' | b'|' | b'(' | b')'
                    )) =>
            {
                comment = true;
            }
            _ => {}
        }
        index += 1;
    }
    contexts
}

fn format_violation(violation: &Violation) -> String {
    format!(
        "{}: step '{}' shell line {}: {}: {}",
        violation.file.display(),
        violation.step,
        violation.line,
        violation.expression,
        violation.rule
    )
}

fn check_shell_field(file: &Path, step: &str, template: &str) -> Vec<Violation> {
    let template = strip_raw_blocks(template);
    let tags = output_tags(&template);
    let bodies = heredoc_bodies(&template);
    let contexts = quote_contexts(&template, &tags);
    let mut violations = Vec::new();

    for (tag, context) in tags.iter().zip(contexts) {
        if !references_steps(&tag.expression) {
            continue;
        }
        if !ends_with_shell_escape(&tag.expression) {
            violations.push(Violation {
                file: file.to_owned(),
                step: step.to_owned(),
                line: tag.line,
                expression: tag.expression.clone(),
                rule: Rule::MissingShellEscape,
            });
        }
        if let Some(delimiter) = line_in_heredoc(tag.line, &bodies) {
            violations.push(Violation {
                file: file.to_owned(),
                step: step.to_owned(),
                line: tag.line,
                expression: tag.expression.clone(),
                rule: Rule::InsideHeredoc { delimiter },
            });
        }
        if let Some(quote) = context {
            violations.push(Violation {
                file: file.to_owned(),
                step: step.to_owned(),
                line: tag.line,
                expression: tag.expression.clone(),
                rule: Rule::QuotedContext { quote },
            });
        }
    }
    violations
}

fn scan_repository() -> (Vec<Violation>, usize, usize) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();
    let mut files_seen = 0;
    let mut escaped_tags = 0;
    for relative_root in SCAN_ROOTS {
        let directory = root.join(relative_root);
        let files = workflow_files(&directory);
        assert!(
            !files.is_empty(),
            "scan root is empty: {}",
            directory.display()
        );
        files_seen += files.len();
        for file in files {
            let source = fs::read_to_string(&file).unwrap();
            let fields = shell_fields(&source)
                .unwrap_or_else(|error| panic!("failed to parse {}: {error}", file.display()));
            for (step, shell) in fields {
                let tags = output_tags(&strip_raw_blocks(&shell));
                escaped_tags += tags
                    .iter()
                    .filter(|tag| {
                        references_steps(&tag.expression) && ends_with_shell_escape(&tag.expression)
                    })
                    .count();
                violations.extend(check_shell_field(&file, &step, &shell));
            }
        }
    }
    (violations, files_seen, escaped_tags)
}

#[test]
fn unit_output_tag_and_filter_rules() {
    let tags = output_tags("printf '%s' {{- steps.a.output | shell_escape() -}}\n");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].line, 1);
    assert!(references_steps(&tags[0].expression));
    assert!(ends_with_shell_escape(&tags[0].expression));
    assert!(!ends_with_shell_escape(
        "steps.a.output | shell_escape | trim"
    ));
    assert!(!references_steps("workflow.steps.a"));
    assert!(!references_steps("notsteps.a"));
    assert!(check_shell_field(
        Path::new("fixture.toml"),
        "step",
        "printf '%s' {{ steps[\"fetch-pr\"].output | string | shell_escape }}",
    )
    .is_empty());
    assert!(check_shell_field(
        Path::new("fixture.toml"),
        "step",
        "{{ arg.1 }} {{ workflow.backends }}",
    )
    .is_empty());
}

#[test]
fn unit_raw_blocks_and_line_numbers() {
    let template = "{% raw %}\n{{ steps.hidden.output }}\n{% endraw %}\nprintf '%s' {{ steps.visible.output }}";
    let stripped = strip_raw_blocks(template);
    let tags = output_tags(&stripped);
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].line, 4);
    let whitespace_control = strip_raw_blocks(
        "{%- raw -%}\n{{ steps.hidden.output }}\n{%- endraw -%}\nprintf '%s' {{ steps.visible.output }}",
    );
    assert_eq!(output_tags(&whitespace_control).len(), 1);
}

#[test]
fn unit_heredoc_and_quote_rules() {
    let template = "cat <<'DATA'\n{{ steps.a.output | shell_escape }}\nDATA\nprintf '%s' {{ steps.b.output | shell_escape }}\n";
    let violations = check_shell_field(Path::new("fixture.toml"), "step", template);
    assert!(violations.iter().any(|violation| matches!(
        violation.rule,
        Rule::InsideHeredoc { ref delimiter } if delimiter == "DATA"
    )));
    assert!(!violations
        .iter()
        .any(|violation| matches!(violation.rule, Rule::QuotedContext { .. })));

    let quoted = "printf '%s' '{{ steps.a.output | shell_escape }}'\nprintf '%s' \"{{ steps.b.output | shell_escape }}\"";
    let quoted_violations = check_shell_field(Path::new("fixture.toml"), "step", quoted);
    assert_eq!(
        quoted_violations
            .iter()
            .filter(|violation| matches!(violation.rule, Rule::QuotedContext { .. }))
            .count(),
        2
    );
}

#[test]
fn unit_multiple_heredocs_and_quote_lexer_edges() {
    let template = "cat <<A <<B\nfirst\nA\n{{ steps.a.output | shell_escape }}\nB\ncat <<-TAB\n\t{{ steps.b.output | shell_escape }}\n\tTAB\nprintf '%s' {{ steps.c.output | shell_escape }}\n";
    let violations = check_shell_field(Path::new("fixture.toml"), "step", template);
    assert_eq!(
        violations
            .iter()
            .filter(|violation| matches!(violation.rule, Rule::InsideHeredoc { .. }))
            .count(),
        2
    );
    assert!(!violations
        .iter()
        .any(|violation| matches!(violation.rule, Rule::QuotedContext { .. })));

    let comment = ":;# '\nprintf '%s\\n' '{{ steps.a.output | shell_escape }}'\n";
    let comment_violations = check_shell_field(Path::new("fixture.toml"), "step", comment);
    assert!(comment_violations
        .iter()
        .any(|violation| matches!(violation.rule, Rule::QuotedContext { .. })));

    let statement = "{% if \"quoted\" == \"quoted\" %}printf '%s' {{ steps.c.output | shell_escape }}{% endif %}";
    assert!(check_shell_field(Path::new("fixture.toml"), "step", statement).is_empty());

    let multiline = "printf '%s\\n' {{ steps.a.output | shell_escape }}\\\nprintf \"%s\" {{ steps.b.output | shell_escape }}";
    assert!(check_shell_field(Path::new("fixture.toml"), "step", multiline).is_empty());
}

#[test]
fn unit_static_heredoc_quotes_do_not_leak() {
    let template =
        "cat <<'DATA'\n'; unpaired quote\nDATA\nprintf '%s' {{ steps.a.output | shell_escape }}\n";
    let violations = check_shell_field(Path::new("fixture.toml"), "step", template);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn unit_diagnostics_include_file_step_and_line() {
    let violations = check_shell_field(
        Path::new("fixture.toml"),
        "named-step",
        "echo\n'{{ steps.a.output }}'\n",
    );
    assert_eq!(violations[0].line, 2);
    assert_eq!(violations[0].step, "named-step");
    assert_eq!(violations[0].file, Path::new("fixture.toml"));
    assert_eq!(
        format_violation(&violations[0]),
        "fixture.toml: step 'named-step' shell line 2: steps.a.output: MissingShellEscape"
    );
}

#[test]
fn checked_in_workflows_escape_in_shell_fields() {
    let (violations, files_seen, escaped_tags) = scan_repository();
    assert!(files_seen > 0);
    assert!(escaped_tags > 0, "no escaped step output tags were found");
    if !violations.is_empty() {
        let report = violations
            .iter()
            .map(format_violation)
            .collect::<Vec<_>>()
            .join("\n");
        panic!("unsafe step output in shell fields:\n{report}");
    }
}
