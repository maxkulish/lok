# Pre-PR validation: clo-394

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned FAIL with one MEDIUM and one LOW; cargo run blocked by read-only sandbox |
| Gemini | OK | Returned PASS_WITH_NOTES with one minor style note |
| Claude fallback | SKIPPED | External reviewers succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR
- None of the findings reach Must-Fix. Codex's MEDIUM about `parse_opencode_output` joining all text events is real per the design language ("final response text"), but in lok's actual invocation model (single-turn `opencode run --format json`) the captured fixture (`tests/fixtures/gemini/success-with-stats.json`) emits a single `type:"text"` event followed by `step_finish`, so the join collapses to the final response. Documenting and resolving "final vs. all text" should be tracked as a follow-up once a real multi-text fixture exists.

## Out of Scope / Deferred
- **Final-text-only parsing for multi-step opencode runs** (Codex MEDIUM, gemini.rs:413-468). Tolerant fallback in current fixtures is correct; adding a multi-step fixture and switching to "text associated with final `step_finish`" is a fixture-driven follow-up, not blocking for CLO-394's single-turn flows.
- **Legacy `npx @google/gemini-cli` model/sandbox flag injection** (Codex LOW, gemini.rs:531). The design explicitly limits legacy compatibility to the parser-only path ("pinned users continue to invoke the old CLI…The parser keeps the current Gemini-envelope fallback"). Argv-level back-compat for the deprecated CLI is intentionally not provided.
- **`build_argv` taking `command` as input** (Gemini LOW). Pure stylistic preference; design's `build_argv` signature explicitly takes the binary name so opencode vs. legacy routing is one function. No action.

## False Positives / Tooling Artifacts
- Codex's "cargo failed opening `target/debug/.cargo-lock` with `Operation not permitted`" is a sandbox limitation in the review harness, not a code defect. The pre-merge gate (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`) still needs to run locally before PR.

## Recommendation
PROCEED_WITH_FIXES. The single bounded fix iteration before opening the PR is to (1) run the design's pre-merge gate locally (`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`) to cover the read-only-sandbox gap Codex hit, and (2) add a short doc comment on `parse_opencode_output` (gemini.rs:413) noting that multi-step opencode runs would currently join all text events and that locking final-text semantics is deferred until a real multi-text fixture is captured. No code logic changes are required for the migration to ship.
