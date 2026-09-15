# Spec: Correct the docs that misstate who publishes lokomotiv on crates.io

**Created**: 2026-09-15
**Linear**: [CLO-660](https://linear.app/cloud-ai/issue/CLO-660) (absorbs canceled CLO-654)
**Revision**: 4 (spec review r1, user review, spec review r2 applied)
**Estimated scope**: M (7 files edited, 4 sub-tasks; docs and rustdoc, no behavior change)

## 1. Problem Statement

### What is true (verified 2026-09-15)

| Fact | Value | How it was checked |
|------|-------|--------------------|
| Owner of the crates.io name `lokomotiv` | `ducks` (Jake Goldsborough, the upstream author), the only owner. Rechecked independently by the user on 2026-09-15 | `crates.io/api/v1/crates/lokomotiv/owners` |
| Published versions | 28, `20260125.0.0` (2026-01-25) to `20260208.0.2` (2026-02-08), none yanked, all `published_by: ducks` | `crates.io/api/v1/crates/lokomotiv/versions` |
| Downloads | 531 total, all of the upstream binary | `crates.io/api/v1/crates/lokomotiv` |
| Library target in a published version | None. docs.rs reports `lokomotiv-20260208.0.2 is not a library` | docs.rs |
| Published `repository` field | `https://github.com/ducks/lok` | crates.io crate API |
| This repository's history | Upstream commits through `f8852fc` (2026-02-08, the same day as the last publish). The first Max Kulish commit is `ccb8cca` (2026-03-29). `maxkulish/lok` is not a GitHub fork (`parent: none`), and `ducks/lok` was still being pushed to on 2026-08-08 | `git log`, `gh repo view` |
| `[lib]` target | Added in `d828890` (2026-07-26), merged to main in `ee28f3c` (2026-07-27, PR #61, CLO-593) | `git log -S'[lib]' -- Cargo.toml` |
| This project's publish path | `.github/workflows/publish.yml` stops at `cargo publish --dry-run`, and `CARGO_REGISTRY_TOKEN` is not set. This project has never run `cargo publish` | `publish.yml:1-21` |
| Known consumers | None found in the inspected locations: Cargo manifests at depth 1 and 2 under `~/Code`, outside lok itself. This does not rule out git consumers elsewhere, deeper workspaces, or dependencies renamed with `package = "lokomotiv"` | `rg lokomotiv ~/Code/*/Cargo.toml ~/Code/*/*/Cargo.toml` |
| Release tags | `v20260914.0.0` exists on origin, and its `Cargo.toml` has `[lib] name = "lokomotiv"`. Tags do not all end in `.0`: `v20260124.0.1` and `v20260125.0.1` through `.0.8` exist | `git ls-remote --tags origin`, `git show v20260914.0.0:Cargo.toml`, `git tag -l` |
| Binaries in the package | Two installable binaries, `lok` and `lokomotiv`, both built from `src/main.rs` with `required-features = ["cli"]`, plus `silence_probe`, which needs `test-support`. `cargo install` of the package installs `lok` and `lokomotiv` | `git show v20260914.0.0:Cargo.toml` lines 24-47 |
| The quick-start examples need Tokio | Both examples (`README.md:36-69`, `src/lib.rs:20-62`) use `#[tokio::main]`, but neither dependency snippet declares `tokio`. In a scratch downstream crate at tag `v20260914.0.0`, each example failed `cargo check` with exit 101 and 3 errors (unresolved `tokio`, async `main`). Both passed after adding `tokio = { version = "1", features = ["macros", "rt-multi-thread"] }`. Repository doctests do not catch this, because `tokio` is already a dependency of this package. Reproduced independently by the user against the working checkout | Scratch probes, 2026-09-15 |
| The git dependency drops the CLI tree | With `default-features = false`, `cargo tree -i clap` in the scratch consumer matched no package | Scratch probe, 2026-09-15 |
| A git-only dependency blocks downstream publishing | crates.io rejects a package whose dependency has only a `git` source. Cargo allows `git` plus `version`, using the git source locally and the registry version on publish, but no library version of `lokomotiv` exists on crates.io to pair with it | [Cargo reference: multiple locations](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations) |
| `cargo install` ignores the lockfile by default | Without `--locked`, `cargo install` resolves dependencies afresh rather than using the package's committed `Cargo.lock`, so a tag alone does not pin an installation's dependencies. `Cargo.lock` is tracked in this repository. `cargo install --locked --git https://github.com/maxkulish/lok --tag v20260914.0.0 lokomotiv` succeeded on 2026-09-15 and installed `lok` and `lokomotiv` (`lok 20260914.0.0`) | [cargo install: dealing with the lockfile](https://doc.rust-lang.org/cargo/commands/cargo-install.html#dealing-with-the-lockfile), `git ls-files Cargo.lock`, scratch probe |

**Consequence.** This project cannot publish `lokomotiv`, because crates.io rejects a publish from anyone who is not an owner. The library can reach crates.io in only two ways. Either `ducks` adds a co-owner or transfers the name, or the library ships under a new crate name. Neither route has been chosen. Until one is, a git dependency on `github.com/maxkulish/lok` is the only way to consume the library. An application can use that, but a downstream crate that depends on it this way cannot itself be published to crates.io. So the naming decision already matters to any downstream crate that wants to publish.

### What the living docs say instead

Consumer-facing:

1. `README.md:25-31`: *"`lokomotiv` is also published on crates.io as a library crate"*, then `lokomotiv = { version = "20260603", default-features = false }`. No such version exists. The requirement fails to resolve, and the only crate under that name is the upstream binary. The example that follows at `README.md:36-69` would not compile downstream even with a working dependency line, because the snippet leaves out `tokio`.
2. `README.md:71-72`: links to docs.rs for "full API documentation". The docs.rs page is the upstream binary's, which is "not a library".
3. `README.md:74-77`: *"Both are published from the same repository under a single `Cargo.toml`"*, and *"Pin to a specific version in your `Cargo.toml`"*.
4. `README.md:82`: Quick Start `cargo install lokomotiv      # Package is "lokomotiv", binary is "lok"`. This installs upstream's `20260208.0.2` binary, not this repository. The comment is also half true, because a package install gets both the `lok` and `lokomotiv` binaries.
5. `src/lib.rs:10-16`: the same `version = "20260603"` snippet in crate-level rustdoc. The example at `src/lib.rs:20-62` is missing `tokio` in the same way.
6. `src/lib.rs:109-113`: *"Both are published from the same repository"*.

Planning and decisions:

7. `docs/ROADMAP.md:55` (Phase 13 intro): *"between the crate as it is now and a release someone outside this machine can trust. Both are cheap, and both get more expensive after a publish rather than before it"*. It also says "Both" when the phase has three tasks.
8. `docs/ROADMAP.md:63`: *"becomes a consumer-facing build failure the moment the crate is published"*.
9. `docs/DEPENDENCIES.md:31`: *"the pre-publish metadata deadline is cleared"*.
10. `docs/DEPENDENCIES.md:50`: *"It starts costing something once Phase 13 publishes to crates.io"*.
11. `docs/DEPENDENCIES.md:61-65`: the standing constraint *"gets more expensive once a release carries the library surface ... A workspace split before that release is a refactor; after it is a rename and a yank"*.
12. `docs/DEPENDENCIES.md:78-82`: a note that says `lokomotiv` "*is* published" and that the deadline is "the first release that ships a library target". It never says who published it, so it repeats CLO-660's own mistaken correction.
13. `docs/decisions/clo-592-workspace-split.md`:
    - the title *"Pre-publish workspace-split decision"*
    - the Context at line 4: *"CLO-592 makes the `lokomotiv` library crate consumable from crates.io"*
    - the Decision: *"before publishing"*
    - Implications lines 31-36: *"Before publish ... no yank needed / After publish ... a rename and a yank ... do it before the first real `cargo publish`"*
    - Related line 42: *"must land before any real publish"*
14. `docs/adrs/clo-589-backend-library-shape.md:152`: the revisit trigger *"`lokomotiv` is published to crates.io and consumer feedback asks for a lighter dependency tree"*. It assumes a publish this project can make.
15. `docs/PROJECT.md:9`: the CLO-660 Active Work row, as first written during project sync, repeated "the real deadline is the first release that ships a library target". It was corrected to the ownership premise on 2026-09-15, before revision 2.

### Why it matters

- A reader who follows the README hits a failed resolution, and even with a valid line the example does not compile. `cargo install lokomotiv` silently installs a February upstream binary that has none of this repository's work.
- The CLO-653 design has already built an argument on the wrong premise. It claimed "never published", was caught in review and rewritten. CLO-654 and CLO-660 then "corrected" the premise into a second false claim ("the name is ours", "the deadline is the first library-carrying release"), because nobody checked the owner.
- Whoever plans the next public-surface change, or picks up the workspace-split question, will read these docs. They currently describe a publish window that does not exist.

### Out of scope: historical records

These docs keep the old premise, because they record what was believed at the time. Do not edit them: `docs/reviews/**`, `docs/designs/**`, `docs/design-docs/**`, `docs/plans/**`, `docs/discovery/**`, `docs/specs/**`, `specs/**` (except this spec), `docs/status/**` (except `docs/status/clo-660-workflow.yaml`), `.pi/lessons/**`.

Inside living docs, three kinds of text are also preserved:
- Task titles quoted from Linear in aggregation tables, for example CLO-592's "consumable from crates.io" at `docs/ROADMAP.md:46` and `docs/PROJECT.md:51`.
- Every line of `docs/adrs/clo-589-backend-library-shape.md` except the revisit trigger, including lines 32, 106, 117, 127 and 200. They record the shape decision and the CLO-591 resolutions as of their dates, and CLO-653 later superseded line 200's "Revisit before CLO-592 publishes".
- The Decision and Rationale sections of `docs/decisions/clo-592-workspace-split.md`, kept as recorded on 2026-08-02 and explicitly marked historical (AC7).

## 2. Acceptance Criteria

- [ ] **AC1 README library section.** `README.md` no longer claims that `lokomotiv` is published on crates.io as a library, or by this project. The dependency snippet is exactly:
  ```toml
  [dependencies]
  lokomotiv = { git = "https://github.com/maxkulish/lok", tag = "v<TAG>", default-features = false }
  tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
  ```
  `<TAG>` is a release tag that exists on origin. The surrounding prose says three things:
  - The tag selects the source revision to build, and each release is tagged `v` plus the crate version (`vYYYYMMDD.N.P`, for example `v20260914.0.0`). It does not call a tag "the equivalent of pinning a version", and it does not claim the last component is always `0`.
  - `tokio` is needed because the example uses `#[tokio::main]`.
  - A crate that depends on `lokomotiv` through git cannot itself be published to crates.io, with a link to the Cargo reference section on multiple locations.
- [ ] **AC2 README install and versioning.**
  - The Quick Start installs with `cargo install --locked --git https://github.com/maxkulish/lok --tag v<TAG> lokomotiv`. One sentence says `--locked` builds with the repository's `Cargo.lock`, which `cargo install` otherwise ignores. The trailing comment is true: the package is `lokomotiv`, the install puts both `lok` and `lokomotiv` on `PATH`, and the commands that follow use `lok`.
  - One sentence says the crates.io `lokomotiv` crate is published by the upstream project (`ducks/lok`), is binary-only, and does not contain this repository's changes.
  - The docs.rs link is replaced with a local build: `cargo doc --no-default-features --open`.
  - The Versioning note says the library and binary are *built* from one `Cargo.toml`, not *published*, and says to pin by tag, not by a crates.io version.
- [ ] **AC3 lib.rs.** The `src/lib.rs` quick-start snippet is exactly the AC1 block, with the same tag, and carries the same Tokio and downstream-publishing sentences. The `# Versioning` section makes no claim that the crate is published, says the library is not on crates.io, and says to pin by tag.
- [ ] **AC4 ROADMAP intro.** The Phase 13 intro in `docs/ROADMAP.md` covers three things:
  - this project has never published to crates.io and cannot publish `lokomotiv` (upstream `ducks` owns it)
  - what each Phase 13 task protects today: CLO-610 protects the GitHub release archives `release.yml` already publishes, and CLO-638 protects anyone building from source through a git dependency or `cargo install --git`
  - crate metadata starts to matter at the first crates.io publish under a name this project controls

  Line 63 no longer says "the moment the crate is published". The intro's task count matches the table.
- [ ] **AC5 ROADMAP placement.** Phase 13 has a CLO-660 row, and the Summary row for Phase 13 counts it: Tasks 4, Completed 2 until CLO-660 completes. `**Last Updated**` names CLO-660.
- [ ] **AC6 DEPENDENCIES.** `docs/DEPENDENCIES.md:31` and `:50` no longer use pre-publish wording. The note at `:78-82` is **replaced**, not joined by a second note, with a standing entry "crates.io publishing" that states five things:
  - the owner (`ducks`)
  - the last published version and date (`20260208.0.2`, 2026-02-08), with "last verified 2026-09-15"
  - the two commands that re-verify both values (Evaluation #1 and #2)
  - that a library publish needs either an owner grant from `ducks` or a new crate name, and that this decision is open
  - that until the decision is made, downstream crates depending on the library through git cannot publish to crates.io

  The lib/bin standing constraint at `:61-65` is restated against that decision, not against a publish date. `**Last Updated**` names CLO-660, and the header no longer says CLO-660 "is still not placed".
- [ ] **AC7 Decision record.** `docs/decisions/clo-592-workspace-split.md`:
  - Its title no longer says "Pre-publish".
  - `**Context**` at line 4 is corrected: CLO-592 prepared the library for crates.io (rustdoc, feature docs, a publish dry run) and published nothing, and this project cannot publish `lokomotiv`.
  - Directly under `## Decision`, one line says that the Decision and Rationale sections are kept as recorded on 2026-08-02, that they are historical, and that their publish framing is superseded by the re-evaluation below. Neither section is otherwise changed.
  - The Implications section is rewritten to the corrected premises.
  - A dated section, `## Re-evaluated 2026-09-15 (CLO-660)`, covers five things:
    - the corrected premises
    - what a split costs today: the refactor list already in Rationale plus consumer migration. No yank is needed, because nothing on crates.io carries the library. Consumer migration cost is unknown: the section says "no known consumers in the inspected locations", names those locations, and does not claim zero consumers.
    - how the crates.io naming decision interacts with a split
    - that whichever route is chosen should keep the existing `lokomotiv` package name and `lokomotiv::` import paths compatible where practical
    - that migration cost for known and unknown git consumers is reassessed when the route is chosen
  - It contains the literal verdict line `**Verdict: re-affirmed.**` and says the split question is revisited together with the naming decision, before any library publish. The reasoning: no crates.io route exists today; git consumers already avoid the CLI dependency tree with `default-features = false`; a split today costs the full refactor and could change what git consumers write, without a benefit that `default-features = false` does not already give; and choosing the naming route is the natural point to settle the crate layout once. The verdict is the spec author's proposal and counts as settled only when the user approves this spec.
  - It lists the historical docs that carry the old premise, found by Evaluation #9.
- [ ] **AC8 ADR trigger.** The revisit trigger at `docs/adrs/clo-589-backend-library-shape.md:152` names a crates.io publish of the library under a name this project controls. No other line in that ADR changes.
- [ ] **AC9 PROJECT.** `docs/PROJECT.md` Up Next gains a row with task id `-`: decide how the library reaches crates.io (co-ownership of `lokomotiv` from `ducks`, or a new crate name). The row notes three things: it is a prerequisite of any library publish, of downstream crates publishing with a `lokomotiv` dependency, and of revisiting the workspace split; `Cargo.toml`'s `documentation = "https://docs.rs/lokomotiv"`, `authors = ["ducks"]` and the `:55` comment "default so `cargo install lokomotiv` keeps working" must be revisited with it; and migration cost for git consumers is reassessed at that point. The CLO-660 Active Work row shows the current phase and states the ownership premise, not the "first release that ships a library target" deadline.
- [ ] **AC10 Strict greps return nothing.** Evaluation #3 and #3b return no output.
- [ ] **AC11 Review greps return only accurate or deliberately preserved text.** Every hit from Evaluation #4 is classified as one of:
  - (a) a current statement that is accurate after this change
  - (b) a Linear-quoted task title
  - (c) an unchanged ADR clo-589 line listed under "Out of scope"
  - (d) text in the 2026-08-02 Decision or Rationale sections of the workspace-split record, under its historical marker

  Each hit is listed in the implementation evidence with its class and a one-line justification. No class (a) hit says that this project has published, will publish, or can publish `lokomotiv`, or that the library has no consumers.
- [ ] **AC12 Change set is confined.** Evaluation #5 lists only the seven edited files, `docs/status/clo-660-workflow.yaml`, this spec, and `docs/reviews/clo-660-*`. It checks committed, staged, unstaged and untracked changes. Evaluation #6 returns no output.
- [ ] **AC13 The documented examples compile downstream.** For both README.md and src/lib.rs, a scratch binary crate whose `[dependencies]` holds exactly the documented snippet lines, and whose `src/main.rs` holds the documented example verbatim, passes `cargo check` against the selected tag. The exit code is captured directly and is 0, and the logs are kept (Evaluation #7).
- [ ] **AC14 The build is unaffected.** Evaluation #8 shows three things: `cargo doc --no-default-features --no-deps` exits 0 before and after the edits; the sorted `warning` header lines after the edits are identical to the baseline taken on the unedited tree (a message comparison, not a count); and `cargo test --doc` exits 0. The logs are kept.
- [ ] **AC15 The documented install works.** The AC2 install command, including `--locked`, installs both `lok` and `lokomotiv` into a scratch `--root` with exit code 0, and `lok --version` prints the tag's version (Evaluation #11).

**Verification method**: run Evaluation #1-#15. Record each command's exit code, its result and its log path under `phases.implement.evidence` in `docs/status/clo-660-workflow.yaml`. Keep the logs in the evidence directory until the PR merges.

Some criteria also need a read-check, and each is recorded in the evidence against the listed elements:
- AC2: the install comment names both binaries, and the upstream-crate sentence is present
- AC3: the `# Versioning` section says the library is not on crates.io and to pin by tag
- AC4: the three intro elements, and a task count that matches the table
- AC6: the five standing-entry elements
- AC7: each listed element, including the historical marker and the verdict line
- AC9: the three notes in the naming-decision row

AC5 and AC8 have command checks, #13 and #14.

## 3. Constraints

**Must**:
- Re-run Evaluation #1 and #2 before writing any fact into a doc. If the owner, version count or newest version has changed, use the new values and record the change.
- Use absolute dates, version numbers and commit SHAs. Never write "recently", "months ago" or "the next release".
- Replace false text in place, and never add a correction note next to text that stays false. The workspace-split decision record is the one sanctioned exception:
  - keep `**Date**: 2026-08-02` and the Decision and Rationale sections verbatim, under the one-line historical marker from AC7
  - retitle it
  - correct the Context line
  - rewrite the Implications section and the Related line
  - add the dated `## Re-evaluated 2026-09-15 (CLO-660)` section
- Describe the co-ownership-or-rename choice as an open decision, with both routes named.
- Match the surrounding style of each file: heading levels, table shapes, line wrapping and link form. New text uses regular hyphens, never em dashes, even in DEPENDENCIES.md and PROJECT.md, where existing prose uses em dashes. This is the repository owner's writing rule, and it overrides style matching. Do not rewrite existing em dashes in lines this change does not otherwise touch.
- If crates.io or GitHub is unreachable during Evaluation #1, #2, #7 or #11, stop and escalate. Never write a fact that was not re-verified in this run.
- Use the same `<TAG>` in README.md and src/lib.rs, and choose it from `git ls-remote --tags origin` at implement time.
- Capture every Cargo exit code directly (`cmd > log 2>&1; echo "exit=$?"`), never through a pipe, and keep the full logs.

**Must-not**:
- Edit any historical record listed under "Out of scope", or any preserved text listed there.
- Change `Cargo.toml` (including its `documentation = "https://docs.rs/lokomotiv"` and `authors = ["ducks"]` fields), `Cargo.lock`, `Makefile` or any `.github/workflows/*` file, comments included.
- Run `cargo publish` (dry run included), contact `ducks`, or create or set any registry token.
- Choose between co-ownership and renaming, or suggest in any doc that either has been requested.
- Describe `ducks` or the upstream project as a problem. State ownership as a fact.
- Claim that the library has no consumers, or that a split or rename forces no migration. State only what was inspected.
- Change the example code bodies in README.md or src/lib.rs. Only the dependency snippets and the surrounding prose change.

**Prefer**:
- The shortest correction that makes each statement true.
- Pointing the aggregation docs at the DEPENDENCIES standing entry instead of repeating the facts in several places, so there is one place to re-verify.

**Escalate when**:
- Evaluation #1 lists any owner other than `ducks`, or includes `maxkulish`.
- Evaluation #7 or #11 fails at the chosen tag.
- Evaluation #12 finds a hit beyond its four known hits: a living doc outside the seven files that states or implies that this project publishes `lokomotiv`, or gives a `lokomotiv` dependency line or install command.
- The re-evaluation points to *revising* the workspace-split decision rather than re-affirming it.

## 4. Decomposition

1. **Consumer docs**: record the Evaluation #8 baseline on the unedited tree first. Then rewrite the library section, Versioning note and Quick Start in `README.md`, and the quick-start snippet and `# Versioning` section in `src/lib.rs`. Pick `<TAG>` and run Evaluation #7, #8, #10 and #11. Covers AC1-AC3 and AC13-AC15. Files: `README.md`, `src/lib.rs`
2. **Decision records**: retitle the workspace-split record, correct its Context, add the historical marker, rewrite Implications and the Related line, and add the re-evaluation section with the verdict and the historical-docs list (Evaluation #9). Correct the ADR revisit trigger. Covers AC7 and AC8. Files: `docs/decisions/clo-592-workspace-split.md`, `docs/adrs/clo-589-backend-library-shape.md`
3. **Aggregation docs**: rewrite the Phase 13 intro and line 63 and add the CLO-660 row. Rewrite DEPENDENCIES `:31`, `:50` and `:61-65`, and replace `:78-82` with the "crates.io publishing" standing entry. Add the PROJECT.md naming-decision row, and confirm the CLO-660 Active Work row states the ownership premise. Covers AC4-AC6 and AC9. Files: `docs/ROADMAP.md`, `docs/DEPENDENCIES.md`, `docs/PROJECT.md`
4. **Verification sweep**: run Evaluation #3-#6 and #12-#15, classify each review-grep hit, do the read-checks named in the verification method, and record the evidence in the workflow YAML. Covers AC10-AC12 and the command checks for AC5, AC8 and AC9. Files: `docs/status/clo-660-workflow.yaml`

**Dependency order**: the Evaluation #8 baseline in sub-task 1 runs before any edit to `README.md` or `src/lib.rs`. Apart from that, sub-tasks 1-3 are independent. The DEPENDENCIES entry and the decision record only need to agree on the same facts, which Evaluation #1-#2 fix. Sub-task 4 runs after 1-3.

## 5. Evaluation

Commands run from the repository root. `EV` is an evidence directory in the session scratchpad, `TAG` is the chosen tag (for example `v20260914.0.0`), and `VERSION=${TAG#v}`. `LIVING` is an array, which works in both bash and zsh: `LIVING=(README.md src/lib.rs docs/ROADMAP.md docs/DEPENDENCIES.md docs/PROJECT.md docs/decisions/clo-592-workspace-split.md docs/adrs/clo-589-backend-library-shape.md)`, used as `"${LIVING[@]}"`. A plain string does not word-split in zsh.

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | crates.io owner | `ducks` and nothing else | `curl -s -H 'User-Agent: lok-clo-660' https://crates.io/api/v1/crates/lokomotiv/owners \| jq -r '.users[].login'` |
| 2 | Published versions | `28  0  20260208.0.2  2026-02-08` (count, yanked, newest, date) | `curl -s -H 'User-Agent: lok-clo-660' https://crates.io/api/v1/crates/lokomotiv/versions \| jq -r '.versions \| sort_by(.created_at) \| [length, (map(select(.yanked)) \| length), last.num, last.created_at[:10]] \| @tsv'` |
| 3 | Strict greps (AC10) | No output, exit 1 | `rg -n -i 'version = "20260603"\|also published on\|as a library crate\|pre-publish\|once the crate is published\|the moment the crate is published\|once Phase 13 publishes\|Both are published\|equivalent of pinning\|^cargo install lokomotiv' "${LIVING[@]}"` |
| 3b | Every documented install of this project uses `--locked` (AC2, AC10) | No output | `rg -n 'cargo install .*github\.com/maxkulish/lok' "${LIVING[@]}" \| rg -v -- '--locked'`. The pattern is scoped to this repository's URL, because `README.md:374` correctly installs the unrelated `ducks/git-agent` tool without `--locked` |
| 3c | No docs.rs link for the library remains in the consumer docs (AC2, AC3, AC10) | No output, exit 1 | `rg -n 'docs\.rs/(crate/)?lokomotiv' README.md src/lib.rs` |
| 3d | The Quick Start uses the git install (AC2) | One hit in the Quick Start block | `rg -n '^cargo install --locked --git https://github\.com/maxkulish/lok --tag v[0-9]' README.md` |
| 4 | Review greps (AC11) | Every hit is classified (a)-(d) with a justification | `rg -n -i 'unpublished\|never (been )?published\|before (the first \|any )?(real )?publish\|after (a )?publish\|is published\|crates\.io\|docs\.rs\|cargo install lokomotiv\|yank\|no (known )?consumers' "${LIVING[@]}"` |
| 5 | Change set (AC12) | Only the allowed paths | `{ git diff --name-only "$(git merge-base main HEAD)"; git ls-files --others --exclude-standard; } \| sort -u`. `git diff <merge-base>` compares the branch point with the working tree, so later commits on main do not leak in, and it covers committed, staged and unstaged changes, and `ls-files --others` adds untracked files |
| 6 | No build files touched (AC12) | No output | `{ git diff --name-only "$(git merge-base main HEAD)" -- Cargo.toml Cargo.lock Makefile .github/; git ls-files --others --exclude-standard -- .github/; }` |
| 7 | Documented examples compile downstream (AC13) | `readme check exit=0` and `librs check exit=0` | Script 7 below |
| 8 | Docs and doctests (AC14) | `doc exit=0` before and after, empty `diff`, `doctest exit=0` | Script 8 below |
| 9 | Historical docs list (AC7) | The list in the re-evaluation section matches this output | `rg -l -i 'never been published\|pre-publish\|before publish\|after publish\|first real .?cargo publish\|once the crate is published\|publishes to crates' docs/reviews docs/designs docs/design-docs docs/plans docs/discovery docs/specs docs/status specs .pi/lessons \| rg -v clo-660` |
| 10 | Tag exists (AC1) | The chosen tag is listed | `git ls-remote --tags origin "$TAG"` |
| 11 | Documented install works (AC15) | `install exit=0`, `bin/` holds `lok` and `lokomotiv`, `lok $VERSION` | Script 11 below |
| 12 | No other living doc carries the old premise (Escalate clause) | Exactly the four known hits listed under Script 12. Any other hit means stop and escalate | Script 12 below |
| 13 | ROADMAP Summary counts CLO-660 (AC5) | One hit | `rg -n -F '\| Phase 13: Release Readiness \| 4 \| 2 \|' docs/ROADMAP.md` |
| 14 | Only the ADR revisit trigger changed (AC8) | Exactly one hunk header, and it starts at line 152 on both sides | `git diff -U0 "$(git merge-base main HEAD)" -- docs/adrs/clo-589-backend-library-shape.md \| rg '^@@'` |
| 15 | PROJECT naming-decision row exists (AC9) | One Up Next row with task id `-` that names both routes | `rg -n '^\| [A-Za-z]+ \| - \|.*co-own.*new crate name' docs/PROJECT.md`, then read the row for the three required notes |

### Script 7: documented examples as a downstream crate

```bash
fence() { awk -v open="$1" '$0==open && !done {f=1; next} f && $0=="```" {f=0; done=1} f'; }
for ex in readme librs; do
  d="$EV/probe-$ex"; rm -rf "$d"; cargo new --bin --quiet --name probe "$d"
  if [ "$ex" = readme ]; then
    fence '```toml' < README.md | grep -v '^\[dependencies\]$' >> "$d/Cargo.toml"
    fence '```rust,no_run' < README.md > "$d/src/main.rs"
  else
    sed -nE 's#^//! ?##p' src/lib.rs | fence '```toml' | grep -v '^\[dependencies\]$' >> "$d/Cargo.toml"
    sed -nE 's#^//! ?##p' src/lib.rs | fence '```no_run' > "$d/src/main.rs"
  fi
  cargo check --manifest-path "$d/Cargo.toml" > "$d/check.log" 2>&1; echo "$ex check exit=$?"
  grep -F "tag=$TAG" "$d/Cargo.lock" > /dev/null; echo "$ex tag pinned exit=$?"
done
```

The extraction was tested on 2026-09-15 against the current files, where the first `toml` and `rust,no_run`/`no_run` fences are the quick-start blocks. As a negative control, the same examples with the dependency line alone and no `tokio` failed with exit 101.

### Script 8: docs and doctests, compared by message

```bash
# Before any edit to README.md or src/lib.rs:
cargo doc --no-default-features --no-deps > "$EV/doc-before.log" 2>&1; echo "doc before exit=$?"
grep '^warning' "$EV/doc-before.log" | sort > "$EV/doc-warnings-before.txt"
# After the edits:
cargo doc --no-default-features --no-deps > "$EV/doc-after.log" 2>&1; echo "doc after exit=$?"
grep '^warning' "$EV/doc-after.log" | sort > "$EV/doc-warnings-after.txt"
diff "$EV/doc-warnings-before.txt" "$EV/doc-warnings-after.txt"; echo "warning diff exit=$?"
cargo test --doc > "$EV/doctest.log" 2>&1; echo "doctest exit=$?"
```

The sorted list keeps duplicates, so a second copy of an existing warning also shows up in the diff. On 2026-09-15 the unedited tree emitted three `warning` header lines, including `unresolved link to backend::BackendKey::for_test`.

### Script 11: documented install

```bash
cargo install --locked --git https://github.com/maxkulish/lok --tag "$TAG" lokomotiv --root "$EV/installprobe" --debug > "$EV/install.log" 2>&1; echo "install exit=$?"
ls "$EV/installprobe/bin"
"$EV/installprobe/bin/lok" --version
```

The authoritative checks are the exit code, a `bin/` listing that contains `lok` and `lokomotiv`, and `lok --version` printing `lok $VERSION`. The `Installed package ... (executables ...)` summary line in `install.log` is informational only, because its wording can change between Cargo versions. `--debug` only shortens the build. Package resolution and the `--locked` lockfile use are the same as in the documented command.

### Script 12: living docs outside the seven files

```bash
rg -n -i 'cargo install lokomotiv|lokomotiv = \{|docs\.rs/lokomotiv|crates\.io/crates/lokomotiv|published (on|to) crates|is published|unpublished|pre-publish|once (the crate is|it is) published' . \
  --hidden --glob '!.git/**' --glob '!target/**' --glob '!Cargo.lock' \
  --glob '!docs/reviews/**' --glob '!docs/designs/**' --glob '!docs/design-docs/**' --glob '!docs/plans/**' \
  --glob '!docs/discovery/**' --glob '!docs/specs/**' --glob '!specs/**' --glob '!docs/status/**' --glob '!.pi/lessons/**' \
  --glob '!README.md' --glob '!src/lib.rs' --glob '!docs/ROADMAP.md' --glob '!docs/DEPENDENCIES.md' --glob '!docs/PROJECT.md' \
  --glob '!docs/decisions/clo-592-workspace-split.md' --glob '!docs/adrs/clo-589-backend-library-shape.md'
```

On 2026-09-15 this returned exactly four hits. All four are known, and none is edited in this change:

| Hit | Why it stays |
|-----|--------------|
| `.pi/agents/ops-reviewer.md:52`: "`cargo install --path .` (or `cargo install lokomotiv`) produces a" | A reviewer-agent prompt, outside the seven-file scope the user set. `cargo install lokomotiv` installs the upstream binary, so this is reported as a follow-up |
| `Cargo.toml:11`: `documentation = "https://docs.rs/lokomotiv"` | Frozen by Must-not; recorded in the AC9 naming-decision row |
| `Cargo.toml:55`: comment "default so `cargo install lokomotiv` keeps working" | Frozen by Must-not (comments included); recorded in the AC9 naming-decision row |
| `Cargo.toml:113`: `lokomotiv = { path = ".", default-features = false, features = ["test-support"] }` | A path dev-dependency on this package. Accurate |

**Edge cases to verify**:
- The README's `cargo install --locked --git ... lokomotiv` names the *package*. It installs two binaries, `lok` and `lokomotiv`, both from `src/main.rs`. The old comment "binary is "lok"" left out the second one, and the new comment must not repeat that.
- `default-features = false` stays in the git snippet. Without it, a consumer compiles `clap`, `indicatif` and the other CLI dependencies (`Cargo.toml:53-56`).
- The `tokio` features in the snippet are exactly what `#[tokio::main]` needs (`macros`, `rt-multi-thread`). Do not use `full`, which pulls far more into a consumer build than the example requires.
- DEPENDENCIES.md `:84` ("nobody can push to `main`") is stale for a different reason: the `CI Gate` required check was removed on 2026-08-07. It is out of scope. Leave it and report it as a follow-up.
- `Cargo.toml`'s `documentation = "https://docs.rs/lokomotiv"` points at the upstream binary's docs.rs page, and `authors = ["ducks"]` names the upstream author. Both are frozen by Must-not and recorded in the PROJECT.md naming-decision row (AC9), because either route (owner grant or new name) is when they change. The `Cargo.toml:55` comment "default so `cargo install lokomotiv` keeps working" belongs in the same row.
- `.pi/agents/ops-reviewer.md:52` asks the ops reviewer to check that `cargo install lokomotiv` produces a working binary. That command installs the upstream crate. It is out of scope; report it as a follow-up.
- ROADMAP.md `:63` keeps its historical `rust-version = "1.80"` justification. Only the publish clause changes.
- If Evaluation #4 matches the new DEPENDENCIES entry's own warning about `cargo install lokomotiv`, that hit counts as class (a). Test #3 anchors `^cargo install lokomotiv` to line start, so a quoted mention in prose does not fail it.
