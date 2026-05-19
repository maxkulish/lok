# Pre-PR validation: clo-373

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-05-19
**Pipeline**: lok pre-pr-validation
---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned FAIL with 1 HIGH / 2 MEDIUM / 2 LOW findings; flagged cargo test could not run in read-only sandbox |
| Gemini | OK | Returned PASS_WITH_NOTES with 1 MEDIUM / 1 LOW finding |
| Claude fallback | SKIPPED | At least one external reviewer succeeded |

## Verdict
PASS_WITH_NOTES

## Must Fix Before PR

- **Remove out-of-scope changes from this branch.** `Cargo.toml:58`, `Cargo.lock`, `src/template/mod.rs:73`, and `.lok/workflows/design-review.toml` were modified despite the design's explicit "No `src/` modules" and "No new dependencies" guarantees (design §31, §97, §165). Both Codex and Gemini independently flagged this. The MiniJinja `comment_delimiters("{#%", "%#}")` change globally rewrites comment syntax for every workflow template, which could break any in-tree or user workflow using standard `{# … #}` comments. Either split these into a separate, design-doc'd PR or drop them from this branch.

- **Tighten the secret-scrub gate to match design §94 ("token-looking strings").** `tests/codex_fixtures.rs:112` enumerates specific credential markers (`api_key`, `access_token`, `auth_token`, `secret=`, `password=`, `Bearer `) but does not flag generic token-looking long alphanumeric values. The README's suggested `rg ... token ...` gate also matches legitimate fixture fields (`reasoning_output_tokens`, `input_tokens`, `cached_input_tokens`), so the documented manual gate cannot pass cleanly as written. Either narrow the README pattern (e.g. `\btoken\b` excluding the JSON key suffix, or `[\"']token[\"']`) or add a heuristic for high-entropy strings in the Rust test.

- **Fix trailing whitespace in `docs/discovery/clo-373.md` lines 58, 75, 91.** Confirmed via `git diff main...HEAD --check`. Cheap; do it in the same fix iteration.

## Out of Scope / Deferred

- **HIGH heredoc-delimiter injection in `.lok/workflows/design-review.toml:301`** (Codex). If `steps.claude_fallback.output` contains a line exactly `ENDFALLBACK`, the heredoc closes and following model-controlled text is executed as shell. Real issue, but the workflow change itself is out-of-scope for CLO-373 — fold this into the separate workflow-fix PR when the Cargo/template changes are split out, and use a randomized delimiter or `printf` + base64 staging there.

- **LOW failure-event assertion accepts `{"error": null}`** (`tests/codex_fixtures.rs:239`). The current `turn-failed.jsonl` carries a structured `error` object so this is theoretical; design §183 calls for "object/string error payload". Tighten in the FR-3a parser PR when failure semantics are nailed down.

## False Positives / Tooling Artifacts

- **Codex "cargo test could not run in read-only sandbox."** Tooling artifact, not a code issue — Codex itself flagged this as a sandbox limitation, not a test failure.

- **Gemini "Cargo.lock dependency noise."** Self-described as "not an issue", purely informational.

## Recommendation

PROCEED_WITH_FIXES. Bounded fix iteration (single pass):
1. Revert `Cargo.toml`, `Cargo.lock`, `src/template/mod.rs`, and `.lok/workflows/design-review.toml` from this branch (move to a separate workflow-fix PR that also addresses Codex's HIGH heredoc finding).
2. Either narrow the README scrub regex so it no longer matches `*_tokens` fields, or add a token-looking-string heuristic to `assert_no_unscrubbed_sensitive_text` in `tests/codex_fixtures.rs:94`.
3. Strip trailing whitespace from `docs/discovery/clo-373.md:58,75,91`.
4. Re-run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --test codex_fixtures` in a writable checkout to confirm the gate is clean.

Core CLO-373 deliverables (4 fixtures, README, version.txt, .gitattributes, semantic test suite) are present and correct per both reviewers; the fixes above are bounded and do not touch the fixture corpus itself.

## Re-validation

Single bounded fix iteration applied:

1. Reverted the out-of-scope `Cargo.toml`, `Cargo.lock`, `src/template/mod.rs`, and `.lok/workflows/design-review.toml` branch diff back to `main` so CLO-373 contains only fixture/test/docs scope.
2. Tightened the fixture scrub gate by adding a high-entropy token-looking string heuristic in `tests/codex_fixtures.rs` and narrowing the README grep so legitimate `*_tokens` usage fields are not flagged as credential markers.
3. Stripped trailing whitespace from `docs/discovery/clo-373.md`.
4. Re-ran `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --test codex_fixtures` in a writable checkout; it passed.
