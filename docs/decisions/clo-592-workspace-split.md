# CLO-592: Pre-publish workspace-split decision

**Date**: 2026-08-02
**Context**: CLO-592 makes the `lokomotiv` library crate consumable from crates.io.
The library and binaries currently live in a single `Cargo.toml` with shared
versioning.

## Decision

Do **not** split the workspace before publishing. The lib/bin boundary is
enforced by convention and CI (`library-boundary` job), not by the compiler.
A workspace split is a refactoring that can be done at any time; doing it
before publishing would add scope and risk without immediate benefit.

## Rationale

- The `library-boundary` CI job (`cargo build --locked --lib --no-default-features`)
  already catches accidental CLI-dependency leaks into the library
- The `cli` feature gate on binary-only dependencies (`clap`, `indicatif`, etc.)
  is already in place and verified
- A workspace split would require:
  - Creating a new `lokomotiv-core` or similar crate
  - Moving `src/backend/` into it
  - Updating all internal `use` paths
  - Updating the `lok` binary crate to depend on the new crate
  - Updating CI to build both crates
- This is pure refactoring with no user-facing benefit

## Implications

- **Before publish**: A workspace split is a refactoring — rename the library
  crate, update imports, no yank needed
- **After publish**: A workspace split is a rename and a yank — the old crate
  name on crates.io cannot be reused, and existing consumers must migrate
- **This ticket excludes publishing**, so the window for a clean split remains
  open. If a split is desired, do it before the first real `cargo publish`.

## Related

- CLO-592 design document: `docs/design-docs/clo-592-library-docs-and-publish-dry-run.md`
- CLO-591: Backend library shape (established the lib/bin boundary)
- CLO-609: Repository metadata (must land before any real publish)
