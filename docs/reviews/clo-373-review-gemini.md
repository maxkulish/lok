# Design Review: clo-373

**Reviewer**: Gemini 3.1 Pro
**Reviewed**: 2026-05-19
**Pipeline**: lok design-review

---

**VERDICT: APPROVE_WITH_SUGGESTIONS**

This is an excellently scoped and well-reasoned design document. It correctly identifies the need for test fixtures before building the parser, limits the scope entirely to test infrastructure, and proactively addresses security (PII scrubbing) and maintainability (integration tests for the fixtures themselves).

Here are the key findings and prioritized actionable items to refine the implementation.

### Key Findings & Suggestions

**1. Fixture Directory Iteration (Code Quality / Edge Case)**
The design proposes iterating over the `tests/fixtures/codex` directory using `std::fs::read_dir` in `every_fixture_is_line_valid_jsonl()`.
*   **Suggestion:** Ensure the iteration explicitly filters for `path.extension() == Some("jsonl")`. The architecture lists a `README.md` in the same directory; without an extension filter, the test will attempt to parse the markdown file as JSON and panic.

**2. Empty Fixture Protection (Completeness)**
The `parse_jsonl` function correctly filters empty lines (`.filter(|l| !l.trim().is_empty())`). However, if a fixture file is accidentally truncated to 0 bytes (or contains only newlines), it will silently yield an empty `Vec`.
*   **Suggestion:** Update the design for `parse_jsonl` (or the tests that call it) to explicitly assert `!parsed_vec.is_empty()`. A valid Codex JSONL stream must contain at least one event.

**3. Documenting Hand-Edited Fixtures (Operational Readiness)**
In the "Open questions" section, you mention that you might need to hand-trim a real stream to produce `missing-agent-message.jsonl` (Option C).
*   **Suggestion:** If you go with Option C, the `tests/fixtures/codex/README.md` must explicitly flag this file as hand-crafted. Future maintainers running the "re-capture all fixtures" script need to know *not* to overwrite this specific file, or how to manually reproduce the trim if they do.

**4. Comprehensive Secret Scrubbing (Security Posture)**
The manual verification checklist defines a solid regex for paths and usernames (`rg '<HOME>|/Users/|...'`).
*   **Suggestion:** Add a step to the README's scrub checklist to manually inspect the JSONL for leaked API keys, bearer tokens, or sensitive repository names. While the `codex` CLI might not natively log these, LLM tool-call inputs/outputs captured in the stream often echo environment variables or file contents.

### Summary
The architecture is completely sound and ready for implementation. The suggestions above are minor implementation details to ensure the tests are robust against directory contents and human error during future fixture updates. Please proceed with implementation!
