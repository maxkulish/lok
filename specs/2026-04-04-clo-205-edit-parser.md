# Spec: Implement EditParser with 3-format auto-detection

**Created**: 2026-04-04
**Estimated scope**: M (3 new files + 1 edit, ~6 sub-tasks, target: 25+ tests)

## 1. Problem Statement

lok's edit pipeline (`src/workflow.rs:2844-2855`) only parses one format: JSON old/new pairs deserialized into `AgenticOutput` (`src/workflow.rs:174-184`). When LLMs output unified diffs or full file contents instead, `parse_edits()` fails because `extract_json_from_text()` (`src/workflow.rs:2789-2841`) finds no JSON, or the JSON doesn't match the `AgenticOutput` schema.

This causes silent failures for any LLM that prefers diff-style or full-file responses over the specific JSON schema lok requires.

This task creates `src/apply_verify/edit_parser.rs` with auto-detection of 3 edit formats, plus markdown code block extraction for all formats. It produces a normalized `Vec<FileEdit>` regardless of input format. The existing `FileEdit` struct (`src/workflow.rs:167-172`) with `file`, `old`, `new` fields remains the canonical output type.

**Key types involved**:
- `FileEdit` (`src/workflow.rs:167-172`) - `file: String`, `old: String`, `new: String`
- `AgenticOutput` (`src/workflow.rs:174-184`) - `edits: Vec<FileEdit>`, `summary`, `message`
- `parse_edits()` (`src/workflow.rs:2844-2855`) - current JSON-only parser
- `extract_json_from_text()` (`src/workflow.rs:2789-2841`) - 3-strategy JSON extraction from markdown
- `sanitize_json_strings()` (`src/workflow.rs:2747-2771`) - escapes control chars in JSON strings

**Note**: This task creates the parser infrastructure. CLO-210 (DiffApplier, Rollback, Verification) and CLO-211 (pipeline wiring) integrate it into the workflow.

## 2. Acceptance Criteria

- [ ] `mod apply_verify;` declared in `src/main.rs`
- [ ] `src/apply_verify/mod.rs` re-exports `EditParser`, `EditFormat`, `ParsedEdits`, `EditParseError`
- [ ] `EditFormat` enum with 3 variants: `UnifiedDiff`, `JsonOldNew`, `FullFile`
- [ ] `EditParser::parse(input: &str) -> Result<ParsedEdits, EditParseError>` auto-detects format and returns normalized edits
- [ ] `ParsedEdits` struct contains `edits: Vec<FileEdit>`, `format: EditFormat`, `summary: Option<String>`
- [ ] Unified diff parsing: single-file and multi-file diffs with `---/+++` headers and `@@ -L,C +L,C @@` hunks
- [ ] JSON old/new pairs parsing: backward-compatible with existing `AgenticOutput` format (including `summary` and `message` fields)
- [ ] Full file content parsing: extracts file path from `--- a/path` header or `File: path` annotation, content is the `new` field with empty `old` (full replacement)
- [ ] Markdown code block extraction: strips ```` ```json ````, ```` ```diff ````, ```` ``` ```` fences before parsing
- [ ] Auto-detection is heuristic-first: `detect_format()` inspects content structure and returns `EditFormat`, then the corresponding parser is called once. No fallthrough cascade - if the heuristic picks wrong, the parser returns an error
- [ ] Detection heuristics: unified diff (presence of `--- a/` or `@@ -`), JSON (starts with `{` or `[`), full file (fallback with `File:` path header)
- [ ] JSON parsing reuses `sanitize_json_strings()` pattern for LLM control character quirks
- [ ] `EditParseError` enum with `thiserror`: `NoContent`, `InvalidDiff`, `InvalidJson`, `InvalidFormat`, `AmbiguousFormat`, `InputTooLarge`
- [ ] Input size limit: reject inputs >1MB with `EditParseError::InputTooLarge`
- [ ] `ParsedEdits` derives `Debug, Clone`
- [ ] Unit tests cover all 3 formats, markdown extraction, auto-detection, and edge cases (target: 25+ tests)
- [ ] `cargo test` passes, `cargo clippy` clean

**Verification method**: `cargo test -p lokomotiv -- apply_verify && cargo clippy -p lokomotiv -- -D warnings`

## 3. Constraints

**Must**:
- Output `Vec<FileEdit>` using the existing `FileEdit` struct from `src/workflow.rs:167-172` - do not define a new struct
- Unified diff parser must handle standard GNU diff output: `--- a/file`, `+++ b/file`, `@@ -start,count +start,count @@`, context/add/remove lines
- JSON parser must be backward-compatible: `{"edits": [{"file": "...", "old": "...", "new": "..."}]}` and bare `[{"file": "...", "old": "...", "new": "..."}]` (array without wrapper)
- For full file content format, `old` must be empty string `""` (signals full file replacement to the applier)
- Auto-detection must not require explicit format hints from the user - it inspects the content
- All `pub` items must have doc comments
- Use `thiserror` for errors, consistent with `TemplateError` and `BackendError`

**Must-not**:
- Do NOT modify `src/workflow.rs` - the existing `parse_edits()` stays untouched (CLO-211 scope)
- Do NOT add the `apply_verify` module to any execution path yet - infrastructure only
- Do NOT use external crates for diff parsing - implement with regex and string operations
- Do NOT use `unsafe` code

**Prefer**:
- Reuse `find_closing_fence()` pattern from `src/workflow.rs:2776-2786` for markdown fence detection (re-implement, don't import private fn)
- Keep format-specific parsers as separate `fn` items for testability
- Normalize line endings: convert `\r\n` to `\n` early, then strip trailing `\n` from both `old` and `new` fields for consistency

**Escalate when**:
- Unified diff format has non-standard extensions (e.g., git binary patches, rename headers) that would require significant parser complexity
- LLM output contains mixed formats in a single response (e.g., diff + JSON)

## 4. Decomposition

1. **Add dependency and module skeleton**: Create `src/apply_verify/mod.rs` with `mod edit_parser;` and type re-exports. Create stub `src/apply_verify/edit_parser.rs` with `EditFormat`, `ParsedEdits`, `EditParseError` types. Add `mod apply_verify;` to `src/main.rs`. - files: `src/main.rs`, `src/apply_verify/mod.rs`, `src/apply_verify/edit_parser.rs`

2. **Implement markdown extraction and auto-detection**: In `edit_parser.rs`, implement `extract_code_block(input: &str) -> (String, Option<&str>)` that returns (content, language_hint) after stripping fences. Implement `detect_format(content: &str) -> EditFormat` that inspects content structure. - files: `src/apply_verify/edit_parser.rs`

3. **Implement JSON old/new pairs parser**: Implement `parse_json_edits(content: &str) -> Result<ParsedEdits, EditParseError>`. Handle both `AgenticOutput` wrapper and bare array. Sanitize control characters. Unit tests for valid JSON, bare array, malformed JSON, control chars. - files: `src/apply_verify/edit_parser.rs`

4. **Implement unified diff parser**: Implement `parse_unified_diff(content: &str) -> Result<ParsedEdits, EditParseError>`. Parse `---/+++` headers, `@@` hunks, context/add/remove lines. Reconstruct `old` and `new` strings from hunks for each file. Handle single-file and multi-file diffs. Unit tests. - files: `src/apply_verify/edit_parser.rs`

5. **Implement full file content parser**: Implement `parse_full_file(content: &str) -> Result<ParsedEdits, EditParseError>`. Extract file path from `File: path` header or `--- a/path` header. Content becomes `new` with empty `old`. Unit tests. - files: `src/apply_verify/edit_parser.rs`

6. **Wire up EditParser::parse() and integration tests**: Implement `EditParser::parse(input: &str)` that chains: size check -> extraction -> detection -> format parser. Add input size limit (1MB). Integration tests for markdown-wrapped content, auto-detection across formats, error paths. - files: `src/apply_verify/edit_parser.rs`

**Dependency order**: 1 -> 2 -> 3, 4, 5 (parallel after 2) -> 6

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | JSON with `AgenticOutput` wrapper | Returns `EditFormat::JsonOldNew`, correct FileEdits | `cargo test -- apply_verify::edit_parser::tests::test_json_agentic_output` |
| 2 | JSON bare array `[{"file":..., "old":..., "new":...}]` | Returns `EditFormat::JsonOldNew`, correct FileEdits | `cargo test -- apply_verify::edit_parser::tests::test_json_bare_array` |
| 3 | JSON with control characters (literal newlines in strings) | Sanitized and parsed correctly | `cargo test -- apply_verify::edit_parser::tests::test_json_control_chars` |
| 4 | Single-file unified diff | Returns `EditFormat::UnifiedDiff`, one FileEdit with correct old/new | `cargo test -- apply_verify::edit_parser::tests::test_diff_single_file` |
| 5 | Multi-file unified diff | Returns `EditFormat::UnifiedDiff`, multiple FileEdits | `cargo test -- apply_verify::edit_parser::tests::test_diff_multi_file` |
| 6 | Full file content with `File: path` header | Returns `EditFormat::FullFile`, old="" | `cargo test -- apply_verify::edit_parser::tests::test_full_file` |
| 7 | Content wrapped in ````json` code block | Extracted and parsed as JSON | `cargo test -- apply_verify::edit_parser::tests::test_markdown_json_block` |
| 8 | Content wrapped in ````diff` code block | Extracted and parsed as diff | `cargo test -- apply_verify::edit_parser::tests::test_markdown_diff_block` |
| 9 | Auto-detection of diff (has `--- a/`) | Detected as `UnifiedDiff` | `cargo test -- apply_verify::edit_parser::tests::test_detect_diff` |
| 10 | Auto-detection of JSON (starts with `{`) | Detected as `JsonOldNew` | `cargo test -- apply_verify::edit_parser::tests::test_detect_json` |
| 11 | Empty/whitespace input | Returns `EditParseError::NoContent` | `cargo test -- apply_verify::edit_parser::tests::test_empty_input` |
| 12 | Malformed diff (missing hunks) | Returns `EditParseError::InvalidDiff` | `cargo test -- apply_verify::edit_parser::tests::test_malformed_diff` |
| 13 | Input >1MB | Returns `EditParseError::InputTooLarge` | `cargo test -- apply_verify::edit_parser::tests::test_input_too_large` |
| 14 | Diff with `\ No newline at end of file` marker | Handled gracefully (marker stripped) | `cargo test -- apply_verify::edit_parser::tests::test_diff_no_newline_marker` |
| 15 | Diff with context lines (space prefix) | old/new reconstructed correctly | `cargo test -- apply_verify::edit_parser::tests::test_diff_context_lines` |
| 16 | Full file with no path header | Returns `EditParseError::InvalidFormat` | `cargo test -- apply_verify::edit_parser::tests::test_full_file_no_path` |
| 17 | Input with `\r\n` line endings | Normalized to `\n` | `cargo test -- apply_verify::edit_parser::tests::test_crlf_normalization` |
| 18 | `cargo clippy` clean | No warnings | `cargo clippy -p lokomotiv -- -D warnings` |

**Edge cases to verify**:
- Diff with context lines (lines starting with space) reconstructs old/new correctly
- Diff file paths strip `a/` and `b/` prefixes
- JSON with missing `file` field returns error
- Backticks inside code block content don't break fence detection
- Multiple code blocks in input - first one wins
- Diff with `\ No newline at end of file` marker
- Full file content with no explicit path header returns error
