# CLO-592 Implementation Plan: Make the backend library consumable from crates.io

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-592
**Design Document**: docs/design-docs/clo-592-library-docs-and-publish-dry-run.md
**Created**: 2026-08-02
**Overall Progress**: 0% (0/68 tasks completed)

---

## Architecture Context

`lokomotiv` is a single-package Rust crate with a `[lib]` target (the backend library) and three `[[bin]]` targets (`lok`, `lokomotiv`, `silence_probe`). The library surface was settled by CLO-591 and re-exports ~40 public types from `src/backend/`. The crate is already on crates.io with docs.rs plumbing, but the documentation content is sparse: the crate-level rustdoc is a single paragraph, features are undocumented, and the README covers only CLI usage. The publish workflow (`publish.yml`) has a `CARGO_TOKEN` assertion that blocks `cargo publish --dry-run` from ever executing.

This plan covers 8 phases: crate-level rustdoc, public type documentation, README library section, silence_probe binary decision, publish workflow fix, workspace-split decision, pre-flight security check, and final validation.

---

## Tasks

### Phase 1: Crate-level rustdoc (`src/lib.rs`)

- [ ] Task 1.1: Write crate-level doc comment with working example
  - [ ] 1.1.1: Add `## Quick start` section with `default-features = false` guidance
  - [ ] 1.1.2: Add `## Cargo features` table documenting all four features (`default`, `cli`, `bedrock`, `test-support`)
  - [ ] 1.1.3: Add `## Backend cache` note documenting the process-global `BACKEND_CACHE` constraint
  - [ ] 1.1.4: Add `## Versioning` note about shared version with binaries
  - [ ] 1.1.5: Add `no_run` doc-test example: build an Ollama backend, run a prompt, read response and token usage
- [ ] Task 1.2: Verify `cargo doc --all-features` emits 0 warnings
- [ ] Task 1.3: Verify `cargo test --doc` passes

### Phase 2: Public type documentation ("when a caller reaches for it")

Audit every public type re-exported from `src/lib.rs` and ensure each has a doc comment explaining **when a caller would reach for it**, not just what it is structurally.

- [ ] Task 2.1: Document `Backend` trait — when to implement vs when to use a concrete backend (`src/backend/mod.rs`)
- [ ] Task 2.2: Document `BackendConfig` — when to construct manually vs deserialize from TOML (`src/backend/config.rs`)
- [ ] Task 2.3: Document `BackendError` — when to match on variants vs use `is_retryable()` (`src/backend/mod.rs`)
- [ ] Task 2.4: Document `TokenUsage` — when to read it vs when to aggregate with `saturating_add` (`src/backend/mod.rs`)
- [ ] Task 2.5: Document `QueryOutput` — when to use `from_text` vs `from_process`, when to check `usage` (`src/backend/mod.rs`)
- [ ] Task 2.6: Document `StepContext` — when to use `from_prompt` vs construct with full fields (`src/backend/context.rs`)
- [ ] Task 2.7: Document `StepOptions` — when to populate vs leave `None` (`src/backend/context.rs`)
- [ ] Task 2.8: Document `HealthStatus` — when to call `health_check` vs `is_available` (`src/backend/context.rs`)
- [ ] Task 2.9: Document `Message` / `Role` — when to populate history vs single-turn (`src/backend/context.rs`)
- [ ] Task 2.10: Document `SandboxMode` — when to set vs leave `None` (backend default) (`src/backend/context.rs`)
- [ ] Task 2.11: Document `RetryPolicy` / `RetryExecutor` / `RetryDefaults` — when to customise vs use defaults (`src/backend/retry.rs`, `src/backend/config.rs`)
- [ ] Task 2.12: Document `create_backend()` — the consumer's entry point; when to use it directly (`src/backend/mod.rs`)
- [ ] Task 2.13: Document `ClaudeBackend` / `CodexBackend` / `GeminiBackend` / `OllamaBackend` — when to instantiate directly vs via `create_backend` (each provider's file)
- [ ] Task 2.14: Document `BedrockBackend` (feature-gated) — when to enable the `bedrock` feature (`src/backend/bedrock.rs`)
- [ ] Task 2.15: Document `DEFAULT_TIMEOUT` / `NO_TIMEOUT` / `RETRY_LOG_TARGET` — when to reference these constants (`src/backend/mod.rs`)
- [ ] Task 2.16: Fix the 2 existing private intra-doc link warnings (`is_backend_available`, `HEALTH_TTL_ENV`) (`src/backend/mod.rs`)
- [ ] Task 2.17: Verify `cargo doc --all-features` emits 0 warnings after the audit

### Phase 3: README library section (`README.md`)

- [ ] Task 3.1: Add `## Using lokomotiv as a library` section to README.md
  - [ ] 3.1.1: Getting-started example with `default-features = false`
  - [ ] 3.1.2: Link to docs.rs for full API docs
  - [ ] 3.1.3: Note about shared versioning between library and binaries
- [ ] Task 3.2: Verify README renders correctly on GitHub

### Phase 4: silence_probe binary decision (`Cargo.toml`)

- [ ] Task 4.1: Add `publish = false` to the `silence_probe` `[[bin]]` section in `Cargo.toml`
  - [ ] 4.1.1: Add comment explaining why (test helper, not consumer-facing)
- [ ] Task 4.2: Verify `cargo build --all-targets` still passes (binary still built locally)
- [ ] Task 4.3: Verify `cargo publish --dry-run --locked` no longer packages `silence_probe`

### Phase 5: Publish workflow fix (`.github/workflows/publish.yml`)

- [ ] Task 5.1: Move the `Assert CARGO_TOKEN is configured` step after the `Package and compile` step
- [ ] Task 5.2: Rename the assertion from `CARGO_TOKEN` to `CARGO_REGISTRY_TOKEN`
  - [ ] 5.2.1: Update the env mapping: `CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}`
  - [ ] 5.2.2: Update the error message to reference `CARGO_REGISTRY_TOKEN`
  - [ ] 5.2.3: Update the step name and comment
- [ ] Task 5.3: Verify `cargo publish --dry-run --locked` succeeds locally
- [ ] Task 5.4: Verify `cargo publish --dry-run --locked --features bedrock` succeeds
- [ ] Task 5.5: Verify `cargo publish --dry-run --locked --no-default-features` succeeds
- [ ] Task 5.6: Verify the token assertion still fires before any real publish step (step order check)

### Phase 6: Workspace-split decision (`docs/`)

- [ ] Task 6.1: Create `docs/decisions/clo-592-workspace-split.md` recording the pre-publish decision
  - [ ] 6.1.1: State: the lib/bin boundary is convention + CI, not compiler-enforced
  - [ ] 6.1.2: State: a workspace split before publish is a refactor; after publish it is a rename and a yank
  - [ ] 6.1.3: State: this ticket excludes publishing, keeping the window open
- [ ] Task 6.2: Add a note to `CONTRIBUTING.md` (if it exists) about the workspace-split context

### Phase 7: Pre-flight security check

- [ ] Task 7.1: Scan `src/` for internal-only comments that should not appear on docs.rs
- [ ] Task 7.2: Check `Cargo.toml` metadata (`repository`, `homepage`) — note CLO-609 must land before any real publish
- [ ] Task 7.3: Verify no hardcoded paths or credentials in documentation examples
- [ ] Task 7.4: Check for any internal-only URLs or references in the codebase

### Phase 8: Testing & Validation

- [ ] Task 8.1: `cargo test --doc` passes
- [ ] Task 8.2: `cargo doc --all-features` emits 0 warnings
- [ ] Task 8.3: `cargo publish --dry-run --locked` succeeds (3 variants: default, bedrock, no-default-features)
- [ ] Task 8.4: `cargo clippy --all-targets` passes with `-D warnings`
- [ ] Task 8.5: `cargo build --all-targets` succeeds
- [ ] Task 8.6: `cargo build --locked --lib --no-default-features` succeeds (library-boundary check)
- [ ] Task 8.7: Verify `grep CARGO_REGISTRY_TOKEN .github/workflows/publish.yml` finds the assertion
- [ ] Task 8.8: Verify `grep -A5 'silence_probe' Cargo.toml | grep 'publish = false'` finds the setting
- [ ] Task 8.9: Open `target/doc/lokomotiv/index.html` in a browser and verify the landing page is self-sufficient

### Phase 9: Finalization

- [ ] Task 9.1: Commit all changes with conventional commit message
  - [ ] 9.1.1: `git add` all modified files
  - [ ] 9.1.2: `git commit -m "docs(CLO-592): add crate-level rustdoc, feature docs, README library section, and fix publish dry-run"`
- [ ] Task 9.2: Push branch: `git push origin feat/clo-592-crates`
- [ ] Task 9.3: Create PR: `gh pr create --title "docs(CLO-592): make backend library consumable from crates.io" --body "[PR body]"`
- [ ] Task 9.4: Link PR to Linear task CLO-592
- [ ] Task 9.5: Request review

---

## Module Structure

Files that will be created or modified:

- `src/lib.rs` — Modified: crate-level rustdoc with working example, feature docs, versioning note
- `src/backend/mod.rs` — Modified: type-level docs for Backend, BackendError, TokenUsage, QueryOutput, create_backend, constants; fix private intra-doc links
- `src/backend/config.rs` — Modified: type-level docs for BackendConfig, RetryDefaults
- `src/backend/context.rs` — Modified: type-level docs for StepContext, StepOptions, HealthStatus, Message, Role, SandboxMode
- `src/backend/retry.rs` — Modified: type-level docs for RetryPolicy, RetryExecutor
- `src/backend/claude.rs` — Modified: type-level docs for ClaudeBackend
- `src/backend/codex.rs` — Modified: type-level docs for CodexBackend
- `src/backend/gemini.rs` — Modified: type-level docs for GeminiBackend
- `src/backend/ollama.rs` — Modified: type-level docs for OllamaBackend
- `src/backend/bedrock.rs` — Modified: type-level docs for BedrockBackend
- `README.md` — Modified: add library section
- `Cargo.toml` — Modified: silence_probe publish=false
- `.github/workflows/publish.yml` — Modified: reorder + rename CARGO_TOKEN → CARGO_REGISTRY_TOKEN
- `docs/decisions/clo-592-workspace-split.md` — Created: workspace-split decision record

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage will be recalculated based on completed tasks.

---

## Notes

- The self path-dependency in `[dev-dependencies]` was tested and confirmed safe — Cargo strips it during packaging
- The `src/main.rs` multiple-build-target warning is cosmetic and tolerated
- CLO-609 (repository metadata) must land before any real publish, but is out of scope for this ticket
- The `BACKEND_CACHE` process-global constraint is documented on the item; this plan adds it to crate-level docs
- Each doc change should be verified with `cargo doc --all-features` before moving to the next phase
