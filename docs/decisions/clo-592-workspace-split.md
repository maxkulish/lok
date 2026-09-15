# CLO-592: Workspace-split decision

**Date**: 2026-08-02
**Context**: CLO-592 prepared the `lokomotiv` library for crates.io with rustdoc,
feature docs and a `cargo publish --dry-run` workflow, and published nothing. This
project cannot publish `lokomotiv`: the crates.io name belongs to the upstream
author `ducks` (see "crates.io publishing" in `docs/DEPENDENCIES.md`).
The library and binaries currently live in a single `Cargo.toml` with shared
versioning.

## Decision

*Kept as recorded on 2026-08-02: this section and Rationale are historical, and their publish framing is superseded by the 2026-09-15 re-evaluation below.*

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

- **Today**: No crates.io route exists for the library, so a workspace split
  is the refactor listed under Rationale. No yank is involved, because nothing
  on crates.io carries this library. A split could still change what git
  consumers write in their dependency line
- **At the crates.io naming decision**: A library publish needs either an owner
  grant from `ducks` or a new crate name. A new name changes every consumer's
  dependency line once, which is the natural point to settle the crate layout
- **After a first library publish**: Changing the layout means a new crate and
  a migration for registry consumers. Published versions cannot be deleted; a
  yank only stops new dependency resolutions from choosing them

## Re-evaluated 2026-09-15 (CLO-660)

**Corrected premises.** The original record assumed this project would publish
`lokomotiv` and still had a clean window before that publish. In fact:

- crates.io lists `ducks`, the upstream author, as the sole owner and publisher
  of all 28 `lokomotiv` versions, from `20260125.0.0` (2026-01-25) to
  `20260208.0.2` (2026-02-08). None is yanked and all are binary-only
- This project's `[lib]` target reached main later, in `ee28f3c` (2026-07-27,
  CLO-593), and `.github/workflows/publish.yml` stops at a dry run. This project
  has never published, and it cannot publish `lokomotiv` without an owner grant
- The library can reach crates.io only through an owner grant from `ducks` or
  under a new crate name. That decision is open and tracked in `docs/PROJECT.md`
  Up Next. The facts and the commands that re-verify them are under "crates.io
  publishing" in `docs/DEPENDENCIES.md`
- Until then the library is consumed as a git dependency on
  `github.com/maxkulish/lok`, and a downstream crate that depends on it that way
  cannot itself be published to crates.io

**What a split costs today.**

- The refactor listed under Rationale
- No yank, because nothing on crates.io carries this library
- Consumer migration of unknown size. There are no known consumers in the
  inspected locations (Cargo manifests at depth 1 and 2 under `~/Code`, checked
  2026-09-15), but that search cannot rule out git consumers elsewhere, deeper
  workspaces, or dependencies renamed with `package = "lokomotiv"`. A split could
  change what those consumers write

**How the naming decision interacts with a split.**

- An owner grant keeps the `lokomotiv` name, and a later split would add a second
  crate name next to it
- A new crate name changes every consumer's dependency line once, so settling the
  crate layout at the same time avoids a second change
- Whichever route is chosen should keep the existing `lokomotiv` package name and
  `lokomotiv::` import paths compatible where practical. Migration cost for known
  and unknown git consumers is reassessed when the route is chosen

**Verdict: re-affirmed.** Do not split the workspace now. No crates.io route
exists, git consumers already avoid the CLI dependency tree with
`default-features = false`, and a split today costs the full refactor and could
change what git consumers write, without a benefit that `default-features = false`
does not already give. Revisit the split together with the crates.io naming
decision, before any library publish.

**Historical docs that carry the old premise.** These are left as written,
because they record what was believed at the time:

- `docs/design-docs/clo-592-library-docs-and-publish-dry-run.md`
- `docs/designs/clo-653-backend-cache-key.md`
- `docs/discovery/clo-592.md`
- `docs/discovery/clo-653.md`
- `docs/plans/clo-592-library-docs-and-publish-dry-run.md`
- `docs/reviews/clo-592-review-gemini-raw.md`
- `docs/reviews/clo-592-review-gemini.md`
- `docs/reviews/clo-592-review-ollama-raw.md`
- `docs/reviews/clo-592-review-ollama.md`
- `docs/reviews/clo-592-review-synthesis.md`
- `docs/specs/2026-08-03-clo-633-slice-panics.md`
- `docs/status/clo-653-workflow.yaml`

## Related

- CLO-592 design document: `docs/design-docs/clo-592-library-docs-and-publish-dry-run.md`
- CLO-591: Backend library shape (established the lib/bin boundary)
- CLO-609: Repository metadata (landed 2026-08-03, PR #78; it matters again at
  the first crates.io publish under a name this project controls)
- CLO-660: This re-evaluation (`specs/2026-09-15-clo-660-crate-ownership.md`)
