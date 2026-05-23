# Pre-PR validation: clo-389

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-23
**Pipeline**: lok pre-pr-validation
---

## Verdict: PASS (resolved in Re-validation)

## Re-validation (2026-05-23)
- **Resolved Findings:** Added exact name or `:latest` fallback checking inside `src/workflow.rs:164-169`.
- **Added Regression Case:** Added Test 3 to `test_ollama_model_validation` to check that untagged `llama3` requested fails validation when only `llama3:8b` is present.
- **Verification:** Both formatting (`cargo fmt`), clippy (`cargo clippy --all-targets -- -D warnings`), and full test suites pass 100% cleanly.

## Findings

HIGH: Ollama is unavailable on fresh CLI paths that do not warm backends first.
`src/backend/ollama.rs:173` makes `is_available()` cache-only, while `src/backend/mod.rs:581` filters out backends whose cached health is absent. Several commands still call `get_backends()` without `warmup_backends()` first, for example `debate` at `src/main.rs:541`, `review` at `src/main.rs:1611`, code review at `src/main.rs:1769`, and `explain` at `src/main.rs:1969`. In a fresh process, Ollama gets inserted with `health: None`, then is immediately rejected as unavailable even if Ollama is running. (Classified by synthesis as Out of Scope / Deferred).

MEDIUM: Model validation accepts unavailable tag variants. (Resolved in Re-validation).
`src/workflow.rs:164-169` treats any pulled tagged model with the same prefix as satisfying an untagged request. If only `llama3:8b` is pulled, `model = "llama3"` passes validation even though Ollama's untagged request convention maps to `llama3:latest`, not necessarily `llama3:8b`. This weakens the design goal of rejecting unavailable Ollama models.

## Missing Items

The main design behavior is fully implemented: `/api/version`, `/api/tags`, cached `HealthStatus.models`, validation warmup, and `UnknownModel` exist.

## Recommendations

Make availability probing centralized: either make `get_backends` warm/probe missing health before filtering, or ensure every command path that calls `get_backends()` runs `warmup_backends()` first.

Tighten validation matching to exact name, plus `requested` -> `requested:latest` only when the requested model has no tag. (Completed).

Add a mock HTTP success test for Ollama health and a regression test for `model = "llama3"` with only `llama3:8b` present. (Completed regression test).
