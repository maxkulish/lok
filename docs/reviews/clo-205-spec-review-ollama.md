# Spec Review: clo-205

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-04-04
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

**Strong**: The problem is clearly defined with specific file/line references that I verified as accurate. The root cause (single-format parser limitation) is well-articulated, and the scope boundary (infrastructure only, CLO-210/211 handle integration) is explicit.

**Gap**: The problem statement doesn't explain *why* LLMs might output different formats - is this model-dependent, prompt-dependent, or provider-dependent? This context would help future maintainers understand whether the 3 formats are exhaustive or might need extension.

## 2. Acceptance Criteria Review

**Strong**:
- Clear module structure (`mod apply_verify;`, re-exports)
- Specific format detection logic order (diff -> JSON -> full file)
- Explicit backward compatibility requirement for existing JSON format
- Concrete `EditParseError` variants enumerated
- Clippy/test verification method specified

**Gaps**:
- **Missing**: What happens when auto-detection fails between formats (e.g., content that starts with `{` but isn't valid JSON)? The criterion says "tries unified diff first... then JSON... then full file" but doesn't specify error handling between attempts.
- **Missing**: Trailing newline normalization says "strip trailing `\n`" but should specify: what about `\r\n`? What about multiple trailing newlines?
- **Missing**: For multi-file diffs, should the order of `FileEdit` in the output match the order in the diff? Not specified.
- **Vague**: "Unit tests cover all 3 formats, markdown extraction, auto-detection, and edge cases" - should have specific test count targets like other specs (e.g., CLO-204 had "48 tests").

## 3. Constraints Check

**Aligned**:
- `thiserror::Error` pattern matches `BackendError`, `TemplateError`, `WorkflowError`
- Module structure (`mod.rs` with re-exports) follows `src/backend/` and `src/template/` patterns
- Doc comments on `pub` items matches project style
- "Do NOT use external crates for diff parsing" aligns with minimal dependencies philosophy

**Concerns**:
- **Reuse of `sanitize_json_strings()`**: The spec says "reuses pattern" but the function is in `src/workflow.rs` and is private. The constraint says "re-implement, don't import private fn" but this duplicates logic. Should consider making it `pub(crate)` in a shared utility for consistency, or explicitly note the duplication as intentional.
- **Reuse of `find_closing_fence()`**: Same issue - explicitly re-implementing private function. This is documented but creates maintenance burden if fence logic ever changes.
- **Implicit constraint**: The `FileEdit` struct uses `#[derive(Deserialize)]` but the new `ParsedEdits` doesn't mention if it should also derive useful traits (`Serialize`, `Clone`, `Debug`). The spec shows `edits: Vec<FileEdit>` but `FileEdit` already derives `Clone` - should `ParsedEdits`?

## 4. Decomposition Quality

**Well-scoped**:
- Each sub-task is ~2 hours or less
- Clear file boundaries
- Logical sequencing with dependency graph

**Issues**:
- **Sub-task 5 scope creep**: "Implement full file content parser and EditParser::parse()" combines two different concerns. The integration (`EditParser::parse()`) could be split into a separate sub-task.
- **Missing sub-task**: No explicit sub-task for the `EditParseError` enum implementation beyond mentioning it in sub-task 1. Should be explicit about deriving traits and implementing `std::error::Error`.
- **Parallel execution concern**: Sub-tasks 3, 4, 5 are marked parallel but sub-task 5 depends on 3 and 4 completing (needs all format parsers). The dependency diagram correctly shows this but the numbering suggests otherwise.

## 5. Evaluation Coverage

**Covered**:
- All 3 format parsing paths
- Markdown extraction
- Auto-detection logic
- Error cases (empty, malformed)
- 13 test cases enumerated

**Gaps**:
- **Missing**: No test for ambiguous format (e.g., content starting with `{` that has diff-like patterns inside a string)
- **Missing**: No test for `\ No newline at end of file` diff marker (mentioned in edge cases but not in test table)
- **Missing**: No integration test simulating realistic LLM output with mixed markdown formatting
- **Missing**: No benchmarking or performance consideration for large files (unified diff of 10k+ lines)
- **Vague**: Edge cases list mentions "Backticks inside code block content don't break fence detection" but no test for this in the table

## 6. Codebase Alignment

**Alignment**:
- `thiserror::Error` derive with named fields matches `BackendError` pattern
- `#[allow(dead_code)]` pattern used appropriately for work-in-progress infrastructure
- Module re-exports follow `src/backend/mod.rs` pattern exactly
- Error variants carry `String` messages like `BackendError`

**Violations**:
- **None detected** - The spec follows established patterns well

**Minor suggestions**:
- Consider `pub(crate)` visibility for internal helpers vs `pub` for API surface
- The spec says "Output `Vec<FileEdit>` using existing struct" but doesn't address whether to import from `crate::workflow` or define a type alias. Should be explicit.

## 7. Blind Spots

1. **No logging/observability**: How will parsing failures be diagnosed? The spec mentions `EditParseError` but not logging at debug/trace levels for auto-detection decisions.

2. **No parsing timeout consideration**: Large LLM outputs could cause regex operations to take significant time. Should there be a size limit or timeout?

3. **No validation of file paths**: `FileEdit.file` paths from parsed diffs could contain path traversal (`../../../etc/passwd`). The spec defers to CLO-210/211 but the parser should probably have a "validate paths are relative and within cwd" option.

4. **Missing test category**: Fuzz testing for malformed/malicious input. Unified diff parsing with regex on untrusted LLM output is a potential attack surface.

5. **No mention of streaming**: For very large diffs, should this support streaming parsing? Probably out of scope but worth documenting as a future consideration.

6. **No metric/counters**: How will we know if auto-detection is working well? Should emit metrics about which format is detected.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The specification is well-structured, aligns with existing codebase patterns, and has clear acceptance criteria. The decomposition is reasonable and the evaluation covers the main paths. However, there are several gaps around edge case handling, test completeness, and cross-cutting concerns that should be addressed before implementation.

## 9. Actionable Feedback

**High Priority** (address before implementation):

1. **Add explicit error handling strategy for auto-detection cascade**: Specify whether each format parser should return `Result<Option<ParsedEdits>, EditParseError>` (allowing fallthrough) or if detection is "one-shot" based on heuristics. Current spec is ambiguous.

2. **Clarify newline normalization**: Change "strip trailing `\n` from both `old` and `new`" to something like "normalize line endings to `\n` and strip all trailing newlines (both `\n` and `\r\n`)". Or use `.trim_end()` semantics.

3. **Add test count target**: Replace vague "Unit tests cover..." with specific target like "Minimum 25 unit tests" matching other specs.

4. **Split sub-task 5**: Separate into "5a. Implement full file content parser" and "6. Wire EditParser::parse() with all format parsers". This makes dependencies clearer.

**Medium Priority** (address during implementation):

5. **Add size limit constraint**: Add `Must`: "Reject inputs larger than 1MB to prevent regex DoS" with `EditParseError::InputTooLarge`.

6. **Make sanitization function shared**: Instead of duplicating `sanitize_json_strings()`, move it to `src/utils.rs` as `pub(crate)` and reuse. Or document explicitly why duplication is preferred.

7. **Add logging at debug level**: Include `log::debug!` statements for format detection decisions, parsing successes/failures.

**Low Priority** (document as future work):

8. **Add security note**: Document that file path validation (preventing path traversal) is deferred to CLO-210.

9. **Add extensibility note**: Document that adding new formats (e.g., `EditFormat` variants) should be straightforward - parser is designed for extension.
