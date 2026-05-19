# Pre-PR validation: clo-373

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-19
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

HIGH - [design-review.toml](</Users/mk/Code/orchestrator/lok--feat-clo-373-codex/.lok/workflows/design-review.toml:301>) renders untrusted step output into a fixed shell heredoc delimiter. If `steps.claude_fallback.output` contains a line exactly `ENDFALLBACK`, the heredoc terminates and following model-controlled text is executed as shell. This is also outside CLO-373's fixture-only scope.

MEDIUM - [Cargo.toml](/Users/mk/Code/orchestrator/lok--feat-clo-373-codex/Cargo.toml:58) and [src/template/mod.rs](/Users/mk/Code/orchestrator/lok--feat-clo-373-codex/src/template/mod.rs:73) add a runtime dependency feature and change global MiniJinja syntax. The design explicitly says no `src/` modules and no new dependencies. Changing comment delimiters can break existing user/workflow templates that rely on standard `{# ... #}` comments.

MEDIUM - [tests/codex_fixtures.rs](/Users/mk/Code/orchestrator/lok--feat-clo-373-codex/tests/codex_fixtures.rs:112) does not implement the required "token-looking strings" scrub gate, and the README's suggested `rg ... token ...` gate currently matches valid fixture fields like `reasoning_output_tokens`. The security acceptance check is therefore weaker than specified and the documented manual gate cannot pass cleanly as written.

LOW - [tests/codex_fixtures.rs](/Users/mk/Code/orchestrator/lok--feat-clo-373-codex/tests/codex_fixtures.rs:239) only checks that `error` or `message` exists on the terminal failure event. `{"error": null}` would pass, despite the design requiring structured/string error details.

LOW - `git diff main...HEAD --check` reports trailing whitespace in [docs/discovery/clo-373.md](/Users/mk/Code/orchestrator/lok--feat-clo-373-codex/docs/discovery/clo-373.md:58), also at lines 75 and 91.

## Missing Items

- Core fixture files, README, version file, `.gitattributes`, and semantic fixture tests are present.
- Missing/insufficient: robust token-looking secret detection matching the design.
- Pre-merge gate not verified here: `cargo test --test codex_fixtures` could not run because the read-only sandbox cannot create `target/debug/.cargo-lock`.

## Recommendations

- Remove the `Cargo.toml`, `Cargo.lock`, `src/template/mod.rs`, and `.lok/workflows/design-review.toml` changes from this CLO-373 branch, or move them to a separate reviewed fix.
- If keeping the workflow fix elsewhere, do not interpolate untrusted model output into shell source via a fixed heredoc delimiter.
- Tighten fixture scrub tests or adjust the README gate so required secret checks are enforceable without matching legitimate `*_tokens` usage fields.
- Strengthen failure-fixture assertions to require non-null object/string error details.
- Fix trailing whitespace and run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` in a writable checkout.
