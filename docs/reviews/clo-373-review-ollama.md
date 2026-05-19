# Design Review: clo-373

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-05-19
**Pipeline**: lok design-review

---

## Design Review: CLO-373 Codex JSONL Fixtures

### Verdict: **APPROVE_WITH_SUGGESTIONS**

The design is well-structured, appropriately scoped, and aligned with project goals. The additive-only approach and clear separation between fixtures and production code is sound. A few improvements would strengthen operational readiness and security.

---

### Strengths

- **Clear problem framing** - Links directly to PRD blocking requirements (FR-3a prerequisite)
- **Tight scope** - Non-goals section prevents scope creep; no src/ changes
- **Good test strategy** - Validates both JSON validity and terminal event semantics
- **Reproducibility focus** - README with pinned version, capture command, and validation steps
- **Scrub awareness** - Explicit checklist for paths, usernames, $HOME

---

### Key Findings

**1. Scrub completeness has gaps** (Security)

The scrub checklist covers filesystem paths and usernames but misses:
- API keys or tokens that might appear in tool call arguments or environment variable captures
- Nested JSON values that `sed` patterns might miss (e.g., `"path": "/Users/foo/.ssh/id_rsa"` inside a tool call argument)

**2. Test doesn't validate fixture semantics** (Code Quality)

The test checks:
- Each line is valid JSON
- Last line is a terminal event

But does NOT verify:
- `multi-turn-reasoning.jsonl` actually contains `reasoning_output_tokens > 0`
- `turn-completed.jsonl` has a nested `agent_message`

This means a malformed fixture could pass tests but fail downstream FR-3a parser work.

**3. No CI gate for scrub verification** (Operational Readiness)

Verification step 4 (`rg '<HOME>|/Users/|...'`) is manual. Nothing prevents unscrubbed fixtures from merging.

---

### Prioritized Action Items

| Priority | Item | Rationale |
|----------|------|-----------|
| P1 | Add assertion in `codex_fixtures.rs` that `multi-turn-reasoning.jsonl` contains `reasoning_output_tokens > 0` and `turn-completed.jsonl` has an `agent_message` item | Ensures fixtures actually exercise the scenarios they claim to |
| P2 | Extend scrub checklist to include: API keys, tokens, file contents in tool arguments, and add a `tests/fixtures/codex/.scrub-patterns.txt` with regex patterns for CI enforcement | Reduces risk of credential leakage |
| P3 | Add a `cargo test --test codex_fixtures` step to the manual verification checklist (step 6 currently runs all tests; make fixture validation explicit) | Clearer verification path |
| P4 | Resolve the `missing-agent-message.jsonl` creation approach before PR merge - the open question flags real uncertainty | Implementation risk |
| P5 | Consider adding `assert!(metadata.len() < 25_000)` in the test to enforce size cap programmatically (or document in README why manual-only is sufficient) | Prevents fixture bloat |

---

### Minor Suggestions

- **Version pinning**: Add a `version.txt` alongside fixtures for automation-friendliness (deferring to README is acceptable but less robust)
- **Open questions**: Consider resolving items 2-4 before merge rather than deferring to FR-3a; they affect fixture correctness

---

### Summary

The design is sound and ready to implement with minor hardening around fixture semantic validation and scrub completeness. The P1 item (asserting fixture internals) is the most important gap - it protects downstream work from silently malformed fixtures.
