# CLO-592: Make the backend library consumable from crates.io with rustdoc, feature docs and a publish dry-run

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-592
**Status**: Finalized
**Finalized**: 2026-08-02
**Approved By**: User

---

## Summary

Make `lokomotiv`'s docs.rs landing page self-sufficient: a reader can wire a backend into their own program from the documentation alone, without opening the repository. This means crate-level rustdoc with a working example, every public type documented with "when a caller reaches for it" context, documented features, a library-focused README section, a passing `cargo publish --dry-run` that validates the packaging shape, and a deliberate decision on the `silence_probe` binary. The publish workflow's `CARGO_TOKEN` assertion is moved after the dry-run step and renamed to `CARGO_REGISTRY_TOKEN` (the variable cargo actually reads) so the dry-run can execute and the assertion accurately guards a future real-publish step.

---

## Background

`lokomotiv` v20260603.0.0 is already on crates.io with `[package.metadata.docs.rs] all-features = true` and a `docsrs` rustdoc cfg in `Cargo.toml`. The docs.rs plumbing exists; the content is what is missing.

CLO-593 extracted the backend into a `[lib]` target, CLO-591 stripped CLI presentation from the library surface and added `#![deny(missing_docs)]`. The public API is settled, but:

- The crate-level rustdoc is a single paragraph with no working example
- The four Cargo features (`default`, `cli`, `bedrock`, `test-support`) are undocumented
- The README covers only CLI usage — no library section exists
- `cargo publish --dry-run` has never executed because `publish.yml` asserts `CARGO_TOKEN` before reaching the dry-run step
- The `BACKEND_CACHE` process-global constraint is documented on the item but not surfaced in crate-level docs
- The pre-publish workspace-split decision is not recorded anywhere a consumer will see it

### Prior Research

The discovery phase (docs/discovery/clo-592.md) identified three approaches and chose **Approach B: Documentation + publish workflow fix**. Key findings:

- **Baseline score**: 7/10 — library target exists, public API settled, `#![deny(missing_docs)]` active. Content is sparse.
- **3 approaches considered**: A (docs-only, S effort), B (docs+publish fix, M effort), C (full sweep, M effort)
- **Chosen**: B — meets all acceptance criteria, moves `CARGO_TOKEN` assertion after the dry-run step

---

## Architecture

### Component Overview

This task touches documentation and CI — no code changes to the backend module itself. The affected files are:

```
src/lib.rs              → Crate-level rustdoc with working example, feature docs
src/backend/*.rs        → Type-level docs: "when a caller reaches for it" context
README.md               → Add library section
Cargo.toml              → Feature doc comments, silence_probe publish decision
.github/workflows/      → publish.yml: reorder CARGO_TOKEN assertion, rename to CARGO_REGISTRY_TOKEN
docs/                   → Workspace-split decision note
```

```mermaid
flowchart LR
    A[docs.rs] --> B[Crate-level rustdoc]
    A --> C[Feature docs]
    A --> D[Type-level docs: when to reach for it]
    B --> E[Working example]
    B --> F[Library-only path]
    G[README.md] --> H[Library section]
    I[publish.yml] --> J[Dry-run step]
    J --> K[CARGO_REGISTRY_TOKEN assertion\n(moved after dry-run, renamed)]
    L[Cargo.toml] --> M[silence_probe: publish = false?]
```

### Affected Components

| Component | Change Type | Description |
|-----------|-------------|-------------|
| `src/lib.rs` | Modified | Add crate-level rustdoc with working example, feature documentation |
| `src/backend/*.rs` | Modified | Add "when a caller reaches for it" doc context to every public type |
| `README.md` | Modified | Add library section distinct from CLI usage |
| `Cargo.toml` | Modified | Decide on `silence_probe` (`publish = false` or keep); feature doc comments |
| `.github/workflows/publish.yml` | Modified | Move token assertion after dry-run; rename `CARGO_TOKEN` → `CARGO_REGISTRY_TOKEN` |
| `docs/` | New file | Record pre-publish workspace-split decision |

### Dependencies

- **Internal**: `src/backend/` module (the public API being documented)
- **External**: docs.rs (rendering target), crates.io (publishing target)
- **Related issues**: CLO-609 (repository metadata), CLO-610 (release attestations)

---

## Detailed Design

### Implementation Approach

**Approach B: Documentation + publish workflow fix.**

The work splits into three workstreams:

1. **Rustdoc** — Write crate-level documentation in `src/lib.rs` with a working example, document all four features, and audit every public type in `src/backend/` to ensure each has a "when a caller reaches for it" doc comment (not just a structural description)
2. **README** — Add a library section with a getting-started example using `default-features = false`
3. **Publish workflow** — Move the token assertion after the dry-run step so the dry-run can execute; rename `CARGO_TOKEN` to `CARGO_REGISTRY_TOKEN` (the variable cargo actually reads) so the assertion accurately guards a future real-publish step; decide on `silence_probe` (`publish = false` or keep)
4. **Workspace-split decision** — Record the pre-publish decision so the next release is a decision, not a default

### Code Structure

No new types or modules. The changes are to documentation strings and workflow YAML.

#### Crate-level rustdoc (`src/lib.rs`)

The existing `src/lib.rs` has a one-paragraph doc comment and a `#![deny(missing_docs)]` attribute. The new doc comment will include:

```rust
//! Multi-backend LLM abstraction extracted from the `lok` orchestrator.
//!
//! # Quick start
//!
//! Add `lokomotiv` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! lokomotiv = { version = "20260603", default-features = false }
//! ```
//!
//! > **Why `default-features = false`?** The `cli` feature (enabled by default)
//! > pulls in terminal dependencies (`clap`, `indicatif`, etc.) that a library
//! > consumer does not need. Disabling default features gives a lean dependency
//! > tree.
//!
//! Then build a backend and run a prompt:
//!
//! ```rust,no_run
//! use lokomotiv::{BackendConfig, create_backend, StepContext, RetryDefaults};
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = BackendConfig {
//!     command: Some("ollama".into()),
//!     model: Some("llama3.2".into()),
//!     ..Default::default()
//! };
//! let backend = create_backend("ollama", &config, RetryDefaults::default())?;
//!
//! let ctx = StepContext::from_prompt("Hello!", Path::new("."), None);
//! let result = backend.query(ctx).await?;
//!
//! println!("Response: {}", result.content);
//! if let Some(usage) = result.usage {
//!     println!("Tokens: {} in, {} out",
//!         usage.input_tokens, usage.output_tokens);
//! }
//! # Ok(())
//! # }
//! ```
```

#### Feature documentation

Each feature gets a doc comment in `Cargo.toml` (already partially done) and a section in the crate-level rustdoc:

```rust
//! ## Cargo features
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `cli` | Yes | Terminal dependencies for the `lok` binaries. Disable with `default-features = false`. |
//! | `bedrock` | No | AWS Bedrock backend via `aws-sdk-bedrockruntime`. |
//! | `test-support` | No | Test helpers (`StubBackend`, `acquire_test_lock`). |
```

#### Publish workflow change

In `.github/workflows/publish.yml`, two changes are made:

1. **Reorder**: The `Assert CARGO_TOKEN is configured` step moves from before the dry-run to after it. The dry-run does not authenticate against crates.io — it packages and compiles locally — so the token is not needed for the dry-run to succeed.

2. **Rename**: The assertion is updated to check `CARGO_REGISTRY_TOKEN` (the environment variable cargo reads automatically) instead of `CARGO_TOKEN` (a non-standard name that cargo ignores). The workflow header already documents this mismatch: *"cargo reads CARGO_REGISTRY_TOKEN from the environment automatically — CARGO_TOKEN is not that variable."* The renamed assertion accurately guards a future real-publish step.

```yaml
# Before (current):
- name: Assert CARGO_TOKEN is configured    # ← blocks dry-run, wrong variable
  env:
    CARGO_TOKEN: ${{ secrets.CARGO_TOKEN }}
  run: |
    if [ -z "${CARGO_TOKEN:-}" ]; then ...
- name: Package and compile                 # ← never reached
  run: cargo publish --dry-run --locked

# After:
- name: Package and compile                 # ← runs first
  run: cargo publish --dry-run --locked
- name: Assert CARGO_REGISTRY_TOKEN is configured  # ← guards future real publish
  env:
    CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
  run: |
    if [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
      echo "::error::secret CARGO_REGISTRY_TOKEN is not set. Add it before any real publish (gh secret set CARGO_REGISTRY_TOKEN)."
      exit 1
    fi
```

> **Note**: The GitHub secret name can stay `CARGO_TOKEN` (internal naming) as long as it is mapped to the `CARGO_REGISTRY_TOKEN` environment variable in the step. Alternatively, rename the secret to `CARGO_REGISTRY_TOKEN` for consistency. The important thing is that the environment variable cargo would read is the one being checked.

### API/Interface Design

No API changes. The public surface remains exactly as settled by CLO-591.

---

## Implementation Plan

### Phase 1: Crate-level rustdoc

- [ ] Write crate-level doc comment with working example
  - [ ] Add `## Quick start` section with `default-features = false` guidance
  - [ ] Add `## Cargo features` table documenting all four features
  - [ ] Add `## Backend cache` note documenting the process-global constraint
  - [ ] Add `## Versioning` note about shared version with binaries
- [ ] Verify `cargo doc --all-features` emits 0 warnings
- [ ] Verify `cargo test --doc` passes

### Phase 2: Public type documentation ("when a caller reaches for it")

Audit every public type re-exported from `src/lib.rs` and ensure each has a doc comment that explains **when a caller would reach for it**, not just what it is structurally. The types to audit:

- [ ] `Backend` trait — when to implement vs when to use a concrete backend
- [ ] `BackendConfig` — when to construct manually vs deserialize from TOML
- [ ] `BackendError` — when to match on variants vs use `is_retryable()`
- [ ] `TokenUsage` — when to read it vs when to aggregate with `saturating_add`
- [ ] `QueryOutput` — when to use `from_text` vs `from_process`, when to check `usage`
- [ ] `StepContext` — when to use `from_prompt` vs construct with full fields
- [ ] `StepOptions` — when to populate vs leave `None`
- [ ] `HealthStatus` — when to call `health_check` vs `is_available`
- [ ] `Message` / `Role` — when to populate history vs single-turn
- [ ] `SandboxMode` — when to set vs leave `None` (backend default)
- [ ] `RetryPolicy` / `RetryExecutor` / `RetryDefaults` — when to customise vs use defaults
- [ ] `create_backend()` — the consumer's entry point; when to use it directly
- [ ] `ClaudeBackend` / `CodexBackend` / `GeminiBackend` / `OllamaBackend` — when to instantiate directly vs via `create_backend`
- [ ] `BedrockBackend` (feature-gated) — when to enable the `bedrock` feature
- [ ] `DEFAULT_TIMEOUT` / `NO_TIMEOUT` / `RETRY_LOG_TARGET` — when to reference these constants
- [ ] Fix the 2 existing private intra-doc link warnings (`is_backend_available`, `HEALTH_TTL_ENV`)
- [ ] Verify `cargo doc --all-features` emits 0 warnings after the audit

### Phase 3: README library section

- [ ] Add `## Using lokomotiv as a library` section to README.md
  - [ ] Getting-started example with `default-features = false`
  - [ ] Link to docs.rs for full API docs
  - [ ] Note about shared versioning

### Phase 4: silence_probe binary decision

- [ ] Decide whether `silence_probe` should be in the published artifact
  - [ ] It requires `test-support` so it won't install by default for end users
  - [ ] It will still be packaged in the `.crate` archive
  - [ ] Recommendation: add `publish = false` to its `[[bin]]` section — it is a test helper, not a consumer-facing binary
- [ ] If `publish = false`: verify `cargo publish --dry-run --locked` no longer includes it
- [ ] If kept: document why in a comment in `Cargo.toml`
- [ ] Verify `cargo build --all-targets` still passes (the binary is still built locally)

### Phase 5: Publish workflow fix

- [ ] Move `CARGO_TOKEN` assertion after the dry-run step in `publish.yml`
- [ ] Rename the assertion from `CARGO_TOKEN` to `CARGO_REGISTRY_TOKEN` (the variable cargo reads)
- [ ] Update the env mapping: `CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}` (or keep secret name `CARGO_TOKEN` but map to the correct env var)
- [ ] Update the error message to reference `CARGO_REGISTRY_TOKEN`
- [ ] Verify `cargo publish --dry-run --locked` succeeds locally
- [ ] Verify `cargo publish --dry-run --locked --features bedrock` succeeds
- [ ] Verify `cargo publish --dry-run --locked --no-default-features` succeeds
- [ ] Address the `src/main.rs` multiple-build-target warning (both `lok` and `lokomotiv` point to `src/main.rs`)
  - [ ] Option A: tolerate the warning (it is cosmetic, not an error)
  - [ ] Option B: move `lok` to `src/bin/lok.rs` and `lokomotiv` to `src/bin/lokomotiv.rs` (or one as a symlink)
  - [ ] Recommendation: tolerate the warning — it is a known Cargo behaviour for alias binaries and does not affect the published package

### Phase 6: Workspace-split decision

- [ ] Record the pre-publish workspace-split decision in a visible location
  - [ ] Add note to crate-level rustdoc or a `docs/` file
  - [ ] State: the lib/bin boundary is convention + CI, not compiler-enforced; a workspace split before publish is a refactor, after publish it is a rename and a yank
  - [ ] Add note to `CONTRIBUTING.md` (if it exists) so internal contributors are aware of the context

### Phase 7: Pre-flight security check

- [ ] Scan for internal-only comments, hardcoded URLs, or other sensitive information in code and documentation before publishing
  - [ ] Check `src/` for any internal-only comments that should not appear on docs.rs
  - [ ] Check `Cargo.toml` metadata (repository, homepage) — CLO-609 must land before any real publish
  - [ ] Verify no hardcoded paths or credentials in documentation examples

### Phase 8: Testing & Validation

- [ ] `cargo test --doc` passes
- [ ] `cargo doc --all-features` emits 0 warnings
- [ ] `cargo publish --dry-run --locked` succeeds (3 variants)
- [ ] `cargo clippy --all-targets` passes
- [ ] `cargo build --all-targets` succeeds

---

## Constraints

**Must**:
- The `#![deny(missing_docs)]` lint must remain active — no `#[allow(missing_docs)]` on any public item
- Every public type re-exported from `src/lib.rs` must have a doc comment explaining **when a caller reaches for it**, not just structural description
- The `library-boundary` CI job must continue to pass unchanged
- The token assertion must still fire before any real publish step (it moves after the dry-run, not after the entire workflow)
- The token assertion must check `CARGO_REGISTRY_TOKEN` (the variable cargo reads), not `CARGO_TOKEN` (which cargo ignores)
- The `silence_probe` binary must get a deliberate `publish = false` decision (not left as a default)
- The working example in rustdoc must compile (use `no_run` if it needs a live backend)

**Must-not**:
- Must not change the public API surface (no new types, no new re-exports, no removed items)
- Must not change backend behaviour
- Must not add new dependencies
- Must not remove or weaken the token assertion — only reorder and rename it

**Prefer**:
- Prefer `no_run` doc-tests over `ignore` so they stay compile-checked
- Prefer documenting the `BACKEND_CACHE` constraint in crate-level docs rather than only on the item itself
- Prefer a single `docs/` file for the workspace-split decision over inline comments

**Escalate when**:
- A doc-test requires a live backend that is not available in CI — use `no_run` and document the limitation
- `cargo doc --all-features` produces warnings that cannot be resolved without changing the public API
- The `silence_probe` `publish = false` change breaks the `library-boundary` CI job (the binary is still built locally, just not published)

---

## Acceptance Criteria

Each criterion must be specific, measurable, and verifiable with a command or exact manual step.

- [ ] **AC1**: `cargo test --doc` passes with at least one example that compiles and runs (even if `no_run` for backends requiring a live process)
- [ ] **AC2**: `cargo publish --dry-run --locked` succeeds, and with `--features bedrock`, and with `--no-default-features`
- [ ] **AC3**: `cargo doc --all-features` emits 0 warnings about missing or broken documentation links
- [ ] **AC4**: A reader can go from the docs.rs landing page to a compiling call site without opening the repository (verified by the crate-level rustdoc containing a complete example)
- [ ] **AC5**: The library-only path (`default-features = false`) is discoverable from the docs, not just from reading `Cargo.toml`
- [ ] **AC6**: Every public type re-exported from `src/lib.rs` has a doc comment explaining when a caller reaches for it (verified by `cargo doc --all-features` producing no "missing documentation" diagnostics)
- [ ] **AC7**: The `silence_probe` binary has a deliberate `publish = false` decision in `Cargo.toml` (or a documented reason to keep it)
- [ ] **AC8**: The publish workflow asserts `CARGO_REGISTRY_TOKEN` (not `CARGO_TOKEN`) and the assertion runs after the dry-run step

**Verification method**: `cargo test --doc && cargo doc --all-features 2>&1 | grep -c "warning:" | xargs test 0 -eq && cargo publish --dry-run --locked && grep -q CARGO_REGISTRY_TOKEN .github/workflows/publish.yml`

---

## Evaluation

| # | Test | Expected Result | Command / Steps |
|---|------|-----------------|-----------------|
| 1 | Doc-tests compile and pass | `cargo test --doc` exits 0 | `cargo test --doc` |
| 2 | No rustdoc warnings | `cargo doc --all-features` emits 0 warnings | `cargo doc --all-features 2>&1 \| grep -c warning` should be 0 |
| 3 | Dry-run with default features | `cargo publish --dry-run --locked` exits 0 | `cargo publish --dry-run --locked` |
| 4 | Dry-run with bedrock | `cargo publish --dry-run --locked --features bedrock` exits 0 | `cargo publish --dry-run --locked --features bedrock` |
| 5 | Dry-run with no default features | `cargo publish --dry-run --locked --no-default-features` exits 0 | `cargo publish --dry-run --locked --no-default-features` |
| 6 | Library builds without CLI features | `cargo build --lib --no-default-features` exits 0 | `cargo build --locked --lib --no-default-features` |
| 7 | All targets build | `cargo build --all-targets` exits 0 | `cargo build --locked --all-targets` |
| 8 | Clippy passes | `cargo clippy --all-targets` exits 0 | `cargo clippy --locked --all-targets -- -D warnings` |
| 9 | silence_probe has publish = false | `Cargo.toml` has `publish = false` on silence_probe `[[bin]]` | `grep -A5 'silence_probe' Cargo.toml \| grep 'publish = false'` |
| 10 | Publish workflow uses CARGO_REGISTRY_TOKEN | `publish.yml` asserts `CARGO_REGISTRY_TOKEN` | `grep CARGO_REGISTRY_TOKEN .github/workflows/publish.yml` |
| 11 | Token assertion runs after dry-run | Dry-run step precedes token assertion | Check step order in `publish.yml` |
| 12 | No missing-doc warnings on public types | `cargo doc --all-features` emits 0 "missing documentation" warnings | `cargo doc --all-features 2>&1 \| grep -i 'missing' \| grep -v 'private'` should be empty |

**Edge cases to cover**:
- The self path-dependency in `[dev-dependencies]` (`lokomotiv = { path = ".", ... }`) was tested and **Cargo handles it correctly** — it strips the path dependency during packaging. No workaround needed.
- `cargo doc --all-features` includes `bedrock` which pulls AWS SDK types — ensure no doc links break under that feature
- The `no_run` doc-test must still be compile-checked; verify with `cargo test --doc --no-run`
- The `src/main.rs` multiple-build-target warning (both `lok` and `lokomotiv` point to it) is a known Cargo cosmetic warning, not an error — it does not affect the published package. Tolerate it.
- The `silence_probe` binary requires `test-support` so it won't install by default, but it will be packaged in the `.crate` archive unless `publish = false` is set.

---

## Testing Strategy

- **Unit Tests**: No new unit tests needed — this is a documentation and CI task
- **Doc Tests**: The crate-level rustdoc example will be a `no_run` doc-test that compiles but does not execute (requires a live backend)
- **Integration Tests**: `cargo publish --dry-run` validates the packaging shape end-to-end
- **Manual Testing**: Open `target/doc/lokomotiv/index.html` in a browser and verify the landing page is self-sufficient

---

## Open Questions

- [x] ~~Should the `silence_probe` binary be excluded from the published artifact?~~ **Resolved in design**: add `publish = false` — it is a test helper, not a consumer-facing binary.
- [x] ~~Does the self path-dependency in `[dev-dependencies]` cause `cargo publish --dry-run` to fail?~~ **Resolved by testing**: `cargo publish --dry-run --allow-dirty` succeeds. Cargo strips the path dependency during packaging. No workaround needed.
- [ ] Should the `src/main.rs` multiple-build-target warning be fixed by moving `lok` to a separate file? **Recommendation**: tolerate it — it is cosmetic and does not affect the published package. Revisit if it causes confusion.

---

## References

- [Linear Task](https://linear.app/cloud-ai/issue/CLO-592)
- [Discovery Report](docs/discovery/clo-592.md)
- [ADR: Backend abstraction exposure shape](docs/adrs/clo-589-backend-library-shape.md)
- [CLO-593 PRD](docs/prds/clo-593-extract-lok-backend-lib.md)
- [CLO-593 Discovery](docs/discovery/clo-593.md)
- [Publish Workflow](.github/workflows/publish.yml)
- [CI Workflow](.github/workflows/ci.yml)
