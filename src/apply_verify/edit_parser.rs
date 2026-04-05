use crate::workflow::FileEdit;

/// Maximum input size in bytes (1 MB).
const MAX_INPUT_SIZE: usize = 1_024 * 1_024;

/// Detected edit format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EditFormat {
    /// Standard unified diff with `---/+++` headers and `@@` hunks.
    UnifiedDiff,
    /// JSON old/new pairs (AgenticOutput or bare array).
    JsonOldNew,
    /// Full file content replacement.
    FullFile,
}

/// Parsed edits normalized from any supported format.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParsedEdits {
    /// The normalized file edits.
    pub edits: Vec<FileEdit>,
    /// Which format was detected.
    pub format: EditFormat,
    /// Optional summary extracted from JSON output.
    pub summary: Option<String>,
}

/// Errors that can occur during edit parsing.
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum EditParseError {
    /// Input is empty or whitespace-only.
    #[error("no content to parse")]
    NoContent,

    /// Input exceeds the maximum allowed size.
    #[error("input exceeds maximum size of {MAX_INPUT_SIZE} bytes")]
    InputTooLarge,

    /// Unified diff could not be parsed.
    #[error("invalid unified diff: {0}")]
    InvalidDiff(String),

    /// JSON could not be parsed.
    #[error("invalid JSON edits: {0}")]
    InvalidJson(String),

    /// Format was detected but content is invalid.
    #[error("invalid format: {0}")]
    InvalidFormat(String),

    /// Multiple formats detected with equal confidence.
    #[error("ambiguous format: {0}")]
    AmbiguousFormat(String),
}

/// Edit parser with 3-format auto-detection.
///
/// Parses LLM output into normalized `Vec<FileEdit>` regardless of whether
/// the output is a unified diff, JSON old/new pairs, or full file content.
/// Handles markdown code block extraction automatically.
#[allow(dead_code)]
pub struct EditParser;

#[allow(dead_code)]
impl EditParser {
    /// Parse LLM output into normalized edits.
    ///
    /// 1. Checks input size (rejects >1MB)
    /// 2. Normalizes `\r\n` to `\n`
    /// 3. Extracts content from markdown code blocks if present
    /// 4. Auto-detects format via heuristics
    /// 5. Delegates to the appropriate format parser
    pub fn parse(input: &str) -> Result<ParsedEdits, EditParseError> {
        if input.len() > MAX_INPUT_SIZE {
            return Err(EditParseError::InputTooLarge);
        }

        let normalized = input.replace("\r\n", "\n");
        let trimmed = normalized.trim();

        if trimmed.is_empty() {
            return Err(EditParseError::NoContent);
        }

        let (content, lang_hint) = extract_code_block(trimmed);
        let content = content.trim();

        if content.is_empty() {
            return Err(EditParseError::NoContent);
        }

        let format = detect_format(content, lang_hint);

        match format {
            EditFormat::UnifiedDiff => parse_unified_diff(content),
            EditFormat::JsonOldNew => parse_json_edits(content),
            EditFormat::FullFile => parse_full_file(content),
        }
    }
}

// ---------------------------------------------------------------------------
// Markdown code block extraction
// ---------------------------------------------------------------------------

/// Extract content from a markdown code block, returning (content, language_hint).
///
/// If no code block is found, returns the input unchanged with no hint.
fn extract_code_block(input: &str) -> (String, Option<&str>) {
    // Try ```lang ... ``` blocks
    if let Some(start) = input.find("```") {
        let after_backticks = &input[start + 3..];

        // Extract language hint (text before newline)
        let (lang_hint, content_start) = if let Some(nl) = after_backticks.find('\n') {
            let hint = after_backticks[..nl].trim();
            let hint = if hint.is_empty() { None } else { Some(hint) };
            (hint, nl + 1)
        } else {
            (None, 0)
        };

        let remaining = &after_backticks[content_start..];

        // Find closing fence on its own line
        if let Some(end) = find_closing_fence(remaining) {
            return (remaining[..end].to_string(), lang_hint);
        }
    }

    (input.to_string(), None)
}

/// Find the closing fence for a markdown code block.
///
/// Must be on its own line (after a newline) to avoid matching ``` inside content.
fn find_closing_fence(text: &str) -> Option<usize> {
    if let Some(pos) = text.find("\n```") {
        return Some(pos);
    }
    if text.starts_with("```") {
        return Some(0);
    }
    None
}

// ---------------------------------------------------------------------------
// Format auto-detection
// ---------------------------------------------------------------------------

/// Detect the edit format from content structure and optional language hint.
fn detect_format(content: &str, lang_hint: Option<&str>) -> EditFormat {
    // Language hint takes priority if present
    if let Some(hint) = lang_hint {
        let hint_lower = hint.to_lowercase();
        if hint_lower == "diff" || hint_lower == "patch" {
            return EditFormat::UnifiedDiff;
        }
        if hint_lower == "json" {
            return EditFormat::JsonOldNew;
        }
    }

    // Heuristic: unified diff markers
    if (content.contains("\n--- ") || content.starts_with("--- "))
        && (content.contains("\n+++ ") || content.contains("\n@@ "))
    {
        return EditFormat::UnifiedDiff;
    }
    if content.contains("\n@@ -") || content.starts_with("@@ -") {
        return EditFormat::UnifiedDiff;
    }

    // Heuristic: JSON (starts with { or [)
    let first_char = content.chars().next().unwrap_or(' ');
    if first_char == '{' || first_char == '[' {
        return EditFormat::JsonOldNew;
    }

    // Fallback: full file content
    EditFormat::FullFile
}

// ---------------------------------------------------------------------------
// JSON old/new pairs parser
// ---------------------------------------------------------------------------

/// JSON wrapper matching `AgenticOutput` format.
#[derive(serde::Deserialize)]
struct JsonWrapper {
    #[serde(default)]
    edits: Vec<FileEdit>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Parse JSON old/new pairs into normalized edits.
///
/// Handles both `{"edits": [...]}` wrapper and bare `[{...}]` array.
fn parse_json_edits(content: &str) -> Result<ParsedEdits, EditParseError> {
    // Try AgenticOutput wrapper first
    if let Ok(wrapper) = serde_json::from_str::<JsonWrapper>(content) {
        if !wrapper.edits.is_empty() {
            return Ok(ParsedEdits {
                edits: normalize_edits(wrapper.edits),
                format: EditFormat::JsonOldNew,
                summary: wrapper.summary.or(wrapper.message),
            });
        }
    }

    // Try bare array
    if let Ok(edits) = serde_json::from_str::<Vec<FileEdit>>(content) {
        if !edits.is_empty() {
            return Ok(ParsedEdits {
                edits: normalize_edits(edits),
                format: EditFormat::JsonOldNew,
                summary: None,
            });
        }
    }

    // Try with sanitized JSON (LLM control character quirks)
    let sanitized = sanitize_json_strings(content);

    if let Ok(wrapper) = serde_json::from_str::<JsonWrapper>(&sanitized) {
        if !wrapper.edits.is_empty() {
            return Ok(ParsedEdits {
                edits: normalize_edits(wrapper.edits),
                format: EditFormat::JsonOldNew,
                summary: wrapper.summary.or(wrapper.message),
            });
        }
    }

    if let Ok(edits) = serde_json::from_str::<Vec<FileEdit>>(&sanitized) {
        if !edits.is_empty() {
            return Ok(ParsedEdits {
                edits: normalize_edits(edits),
                format: EditFormat::JsonOldNew,
                summary: None,
            });
        }
    }

    Err(EditParseError::InvalidJson(
        "could not parse as AgenticOutput or bare FileEdit array".to_string(),
    ))
}

/// Escape control characters inside JSON string values.
///
/// LLMs sometimes output literal newlines/tabs in JSON strings instead of
/// proper `\n`/`\t` escape sequences.
fn sanitize_json_strings(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_string = false;
    let mut prev_was_backslash = false;

    for ch in input.chars() {
        if in_string {
            if prev_was_backslash {
                result.push(ch);
                prev_was_backslash = false;
                continue;
            }
            match ch {
                '\\' => {
                    result.push(ch);
                    prev_was_backslash = true;
                }
                '"' => {
                    result.push(ch);
                    in_string = false;
                }
                '\n' => result.push_str("\\n"),
                '\t' => result.push_str("\\t"),
                '\r' => result.push_str("\\r"),
                c if c.is_control() => {
                    result.push_str(&format!("\\u{:04x}", c as u32));
                }
                _ => result.push(ch),
            }
        } else {
            result.push(ch);
            if ch == '"' {
                in_string = true;
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Unified diff parser
// ---------------------------------------------------------------------------

/// Parse a unified diff into normalized edits.
///
/// Handles standard GNU diff output with `--- a/file`, `+++ b/file`,
/// `@@ -start,count +start,count @@` headers.
fn parse_unified_diff(content: &str) -> Result<ParsedEdits, EditParseError> {
    let lines: Vec<&str> = content.lines().collect();
    let mut edits = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Find --- header
        if lines[i].starts_with("--- ") {
            let old_path = extract_diff_path(lines[i]);

            // Expect +++ on next line
            i += 1;
            if i >= lines.len() || !lines[i].starts_with("+++ ") {
                return Err(EditParseError::InvalidDiff(
                    "expected +++ header after ---".to_string(),
                ));
            }
            let new_path = extract_diff_path(lines[i]);
            let file_path = new_path.unwrap_or_else(|| {
                old_path.unwrap_or_else(|| "unknown".to_string())
            });

            i += 1;

            // Collect all hunks for this file
            let mut old_text = String::new();
            let mut new_text = String::new();

            while i < lines.len() {
                if lines[i].starts_with("@@ ") {
                    // Parse hunk
                    i += 1;
                    while i < lines.len() {
                        let line = lines[i];
                        if line.starts_with("--- ")
                            || line.starts_with("@@ ")
                            || line.starts_with("diff ")
                        {
                            break;
                        }
                        if line == "\\ No newline at end of file" {
                            i += 1;
                            continue;
                        }
                        if let Some(stripped) = line.strip_prefix('-') {
                            old_text.push_str(stripped);
                            old_text.push('\n');
                        } else if let Some(stripped) = line.strip_prefix('+') {
                            new_text.push_str(stripped);
                            new_text.push('\n');
                        } else if let Some(stripped) = line.strip_prefix(' ') {
                            old_text.push_str(stripped);
                            old_text.push('\n');
                            new_text.push_str(stripped);
                            new_text.push('\n');
                        } else {
                            // Lines without prefix treated as context
                            old_text.push_str(line);
                            old_text.push('\n');
                            new_text.push_str(line);
                            new_text.push('\n');
                        }
                        i += 1;
                    }
                } else if lines[i].starts_with("--- ") || lines[i].starts_with("diff ") {
                    break;
                } else {
                    i += 1;
                }
            }

            if old_text.is_empty() && new_text.is_empty() {
                return Err(EditParseError::InvalidDiff(format!(
                    "no hunks found for file: {}",
                    file_path
                )));
            }

            edits.push(FileEdit {
                file: file_path,
                old: old_text.trim_end_matches('\n').to_string(),
                new: new_text.trim_end_matches('\n').to_string(),
            });
        } else {
            i += 1;
        }
    }

    if edits.is_empty() {
        return Err(EditParseError::InvalidDiff(
            "no file headers found in diff".to_string(),
        ));
    }

    Ok(ParsedEdits {
        edits,
        format: EditFormat::UnifiedDiff,
        summary: None,
    })
}

/// Extract file path from a diff header line, stripping `a/` or `b/` prefix.
fn extract_diff_path(line: &str) -> Option<String> {
    let path = line
        .strip_prefix("--- ")
        .or_else(|| line.strip_prefix("+++ "))?
        .trim();

    if path == "/dev/null" {
        return None;
    }

    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path);

    Some(path.to_string())
}

// ---------------------------------------------------------------------------
// Full file content parser
// ---------------------------------------------------------------------------

/// Parse full file content into a replacement edit.
///
/// Expects a `File: path` or `--- a/path` header followed by the file content.
fn parse_full_file(content: &str) -> Result<ParsedEdits, EditParseError> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return Err(EditParseError::NoContent);
    }

    // Try to extract path from first line
    let (file_path, content_start) = if let Some(path) = lines[0]
        .strip_prefix("File: ")
        .or_else(|| lines[0].strip_prefix("File:"))
        .or_else(|| lines[0].strip_prefix("file: "))
        .or_else(|| lines[0].strip_prefix("file:"))
    {
        (path.trim().to_string(), 1)
    } else if let Some(path) = lines[0].strip_prefix("--- ") {
        let path = path.trim();
        let path = path
            .strip_prefix("a/")
            .unwrap_or(path);
        (path.to_string(), 1)
    } else {
        return Err(EditParseError::InvalidFormat(
            "full file content requires a 'File: path' or '--- a/path' header on the first line"
                .to_string(),
        ));
    };

    if file_path.is_empty() {
        return Err(EditParseError::InvalidFormat(
            "file path is empty".to_string(),
        ));
    }

    let file_content = if content_start < lines.len() {
        lines[content_start..].join("\n")
    } else {
        String::new()
    };

    Ok(ParsedEdits {
        edits: vec![FileEdit {
            file: file_path,
            old: String::new(),
            new: file_content.trim_end_matches('\n').to_string(),
        }],
        format: EditFormat::FullFile,
        summary: None,
    })
}

// ---------------------------------------------------------------------------
// Normalization helpers
// ---------------------------------------------------------------------------

/// Normalize edits by trimming trailing newlines from old and new fields.
fn normalize_edits(edits: Vec<FileEdit>) -> Vec<FileEdit> {
    edits
        .into_iter()
        .map(|e| FileEdit {
            file: e.file,
            old: e.old.trim_end_matches('\n').to_string(),
            new: e.new.trim_end_matches('\n').to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- JSON tests --

    #[test]
    fn test_json_agentic_output() {
        let input = r#"{"edits": [{"file": "src/main.rs", "old": "fn main() {}", "new": "fn main() {\n    println!(\"hello\");\n}"}], "summary": "Added hello"}"#;
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::JsonOldNew);
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].file, "src/main.rs");
        assert_eq!(result.summary, Some("Added hello".to_string()));
    }

    #[test]
    fn test_json_bare_array() {
        let input = r#"[{"file": "a.rs", "old": "x", "new": "y"}, {"file": "b.rs", "old": "1", "new": "2"}]"#;
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::JsonOldNew);
        assert_eq!(result.edits.len(), 2);
        assert_eq!(result.edits[0].file, "a.rs");
        assert_eq!(result.edits[1].file, "b.rs");
        assert!(result.summary.is_none());
    }

    #[test]
    fn test_json_control_chars() {
        // Simulate LLM outputting literal newlines inside JSON strings
        let input = "{\n\"edits\": [{\n\"file\": \"test.rs\",\n\"old\": \"line1\nline2\",\n\"new\": \"line1\nline2\nline3\"\n}]\n}";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::JsonOldNew);
        assert_eq!(result.edits.len(), 1);
    }

    #[test]
    fn test_json_with_message_field() {
        let input = r#"{"edits": [{"file": "a.rs", "old": "x", "new": "y"}], "message": "Updated a"}"#;
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.summary, Some("Updated a".to_string()));
    }

    #[test]
    fn test_json_empty_edits() {
        let input = r#"{"edits": []}"#;
        let result = EditParser::parse(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_json_malformed() {
        let input = r#"{"edits": [{"file": "a.rs", "old":}]}"#;
        let result = EditParser::parse(input);
        assert!(matches!(result, Err(EditParseError::InvalidJson(_))));
    }

    // -- Unified diff tests --

    #[test]
    fn test_diff_single_file() {
        let input = "\
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"old\");
+    println!(\"new\");
+    println!(\"extra\");
 }";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::UnifiedDiff);
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].file, "src/main.rs");
        assert!(result.edits[0].old.contains("println!(\"old\")"));
        assert!(result.edits[0].new.contains("println!(\"new\")"));
        assert!(result.edits[0].new.contains("println!(\"extra\")"));
    }

    #[test]
    fn test_diff_multi_file() {
        let input = "\
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,2 +1,2 @@
-old_a
+new_a
--- a/src/b.rs
+++ b/src/b.rs
@@ -1,2 +1,2 @@
-old_b
+new_b";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::UnifiedDiff);
        assert_eq!(result.edits.len(), 2);
        assert_eq!(result.edits[0].file, "src/a.rs");
        assert_eq!(result.edits[0].old, "old_a");
        assert_eq!(result.edits[0].new, "new_a");
        assert_eq!(result.edits[1].file, "src/b.rs");
        assert_eq!(result.edits[1].old, "old_b");
        assert_eq!(result.edits[1].new, "new_b");
    }

    #[test]
    fn test_diff_context_lines() {
        let input = "\
--- a/lib.rs
+++ b/lib.rs
@@ -1,5 +1,5 @@
 use std::io;

-fn old_func() {
+fn new_func() {
     // body
 }";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.edits.len(), 1);
        assert!(result.edits[0].old.contains("use std::io;"));
        assert!(result.edits[0].old.contains("fn old_func()"));
        assert!(result.edits[0].new.contains("use std::io;"));
        assert!(result.edits[0].new.contains("fn new_func()"));
        // Context lines present in both
        assert!(result.edits[0].old.contains("// body"));
        assert!(result.edits[0].new.contains("// body"));
    }

    #[test]
    fn test_diff_no_newline_marker() {
        let input = "\
--- a/file.txt
+++ b/file.txt
@@ -1,2 +1,2 @@
-old line
+new line
\\ No newline at end of file";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.edits[0].old, "old line");
        assert_eq!(result.edits[0].new, "new line");
    }

    #[test]
    fn test_diff_strips_ab_prefix() {
        let input = "\
--- a/deep/path/file.rs
+++ b/deep/path/file.rs
@@ -1 +1 @@
-old
+new";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.edits[0].file, "deep/path/file.rs");
    }

    #[test]
    fn test_malformed_diff() {
        // Has --- and +++ but garbage instead of hunks after
        let input = "\
--- a/file.rs
+++ b/file.rs
some random text without @@ hunk header";
        let result = EditParser::parse(input);
        assert!(matches!(result, Err(EditParseError::InvalidDiff(_))));
    }

    #[test]
    fn test_diff_no_hunks() {
        let input = "\
--- a/file.rs
+++ b/file.rs";
        let result = EditParser::parse(input);
        assert!(matches!(result, Err(EditParseError::InvalidDiff(_))));
    }

    // -- Full file content tests --

    #[test]
    fn test_full_file() {
        let input = "\
File: src/new_file.rs
fn main() {
    println!(\"hello\");
}";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::FullFile);
        assert_eq!(result.edits.len(), 1);
        assert_eq!(result.edits[0].file, "src/new_file.rs");
        assert_eq!(result.edits[0].old, "");
        assert!(result.edits[0].new.contains("fn main()"));
    }

    #[test]
    fn test_full_file_with_dash_header() {
        let input = "\
--- a/src/new_file.rs
fn main() {
    println!(\"hello\");
}";
        // This has --- but no +++ or @@, so it falls through diff detection
        // and gets parsed as full file
        let result = EditParser::parse(input).unwrap();
        // Since it has --- but no +++ or @@, detect_format sees "--- " but
        // no "+++ " so it won't match UnifiedDiff. Falls to FullFile.
        assert_eq!(result.edits[0].old, "");
    }

    #[test]
    fn test_full_file_no_path() {
        let input = "just some content\nwithout a header";
        let result = EditParser::parse(input);
        assert!(matches!(result, Err(EditParseError::InvalidFormat(_))));
    }

    #[test]
    fn test_full_file_empty_path() {
        let input = "File: \nsome content";
        let result = EditParser::parse(input);
        assert!(matches!(result, Err(EditParseError::InvalidFormat(_))));
    }

    // -- Markdown extraction tests --

    #[test]
    fn test_markdown_json_block() {
        let input = "Here are the edits:\n\n```json\n{\"edits\": [{\"file\": \"a.rs\", \"old\": \"x\", \"new\": \"y\"}]}\n```\n\nDone!";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::JsonOldNew);
        assert_eq!(result.edits.len(), 1);
    }

    #[test]
    fn test_markdown_diff_block() {
        let input = "Changes:\n\n```diff\n--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new\n```";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::UnifiedDiff);
        assert_eq!(result.edits[0].file, "file.rs");
    }

    #[test]
    fn test_markdown_generic_block() {
        let input = "```\n{\"edits\": [{\"file\": \"a.rs\", \"old\": \"x\", \"new\": \"y\"}]}\n```";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::JsonOldNew);
    }

    #[test]
    fn test_markdown_backticks_in_content() {
        // Backticks inside content should not break fence detection
        // The closing fence must be on its own line
        let input = "```json\n{\"edits\": [{\"file\": \"a.rs\", \"old\": \"code with ``` backticks\", \"new\": \"y\"}]}\n```";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.edits.len(), 1);
    }

    // -- Auto-detection tests --

    #[test]
    fn test_detect_diff() {
        let format = detect_format("--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new", None);
        assert_eq!(format, EditFormat::UnifiedDiff);
    }

    #[test]
    fn test_detect_json_object() {
        let format = detect_format("{\"edits\": []}", None);
        assert_eq!(format, EditFormat::JsonOldNew);
    }

    #[test]
    fn test_detect_json_array() {
        let format = detect_format("[{\"file\": \"a.rs\"}]", None);
        assert_eq!(format, EditFormat::JsonOldNew);
    }

    #[test]
    fn test_detect_full_file() {
        let format = detect_format("File: src/main.rs\nfn main() {}", None);
        assert_eq!(format, EditFormat::FullFile);
    }

    #[test]
    fn test_detect_with_lang_hint_diff() {
        let format = detect_format("{\"looks\": \"like json\"}", Some("diff"));
        assert_eq!(format, EditFormat::UnifiedDiff);
    }

    #[test]
    fn test_detect_with_lang_hint_json() {
        let format = detect_format("--- looks like diff", Some("json"));
        assert_eq!(format, EditFormat::JsonOldNew);
    }

    // -- Error and edge case tests --

    #[test]
    fn test_empty_input() {
        assert!(matches!(
            EditParser::parse(""),
            Err(EditParseError::NoContent)
        ));
    }

    #[test]
    fn test_whitespace_only_input() {
        assert!(matches!(
            EditParser::parse("   \n\t  \n  "),
            Err(EditParseError::NoContent)
        ));
    }

    #[test]
    fn test_input_too_large() {
        let large = "x".repeat(MAX_INPUT_SIZE + 1);
        assert!(matches!(
            EditParser::parse(&large),
            Err(EditParseError::InputTooLarge)
        ));
    }

    #[test]
    fn test_crlf_normalization() {
        let input = "--- a/file.rs\r\n+++ b/file.rs\r\n@@ -1 +1 @@\r\n-old\r\n+new";
        let result = EditParser::parse(input).unwrap();
        assert_eq!(result.format, EditFormat::UnifiedDiff);
        assert_eq!(result.edits[0].old, "old");
        assert_eq!(result.edits[0].new, "new");
    }

    #[test]
    fn test_json_trailing_newlines_normalized() {
        let input = r#"{"edits": [{"file": "a.rs", "old": "old\n\n", "new": "new\n\n"}]}"#;
        let result = EditParser::parse(input).unwrap();
        assert!(!result.edits[0].old.ends_with('\n'));
        assert!(!result.edits[0].new.ends_with('\n'));
    }
}
