# Rename the crate from `lokomotiv` to `lok-backend` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the package to `lok-backend` (lib `lok_backend`), drop the `lokomotiv` binary, and bring CI, release, consumer docs, records and agent prompts in line, without publishing anything.

**Architecture:** One package keeps its layout. The rename is mechanical: the manifest identity, every `lokomotiv::` path, the retry log target string and the CLI log filter change. CI and release stop building a second binary. Living docs replace the name in place; the dated decision record gains a section instead of edits.

**Tech Stack:** Rust 2021 (MSRV 1.83), Cargo, env_logger/env_filter 2.0.0, GitHub Actions, `actionlint`.

**Spec:** `specs/2026-09-16-lok-backend-rename.md` (read it with this plan; AC and Evaluation numbers below refer to it).

## Global Constraints

- Package `name = "lok-backend"`, `[lib] name = "lok_backend"`. Never edit `version`; `make release` owns it.
- Docs pin tag `v20260916.0.0`. The user runs `make release VERSION=20260916.0.0` after merge. Never run `make release`, push a tag, or publish.
- Install checks use a scratch `--root`. Never write to `~/.cargo` or `/usr/local/bin`.
- Source rename is path and string changes only. No refactoring, no import reordering beyond `cargo fmt`.
- New text uses regular hyphens, never em dashes. Existing em dashes on untouched lines stay.
- Absolute dates and versions only.
- Out of scope, left as written: `docs/{reviews,plans,designs,design-docs,prds,lessons,specs,discovery,status}/**`, `specs/**` except the spec, `.pi/lessons/**`, `architecture/*.html`, `docs/adrs/README.md`, every line of `docs/adrs/clo-589-backend-library-shape.md` except its revisit trigger, existing sections of `docs/decisions/clo-592-workspace-split.md`, Linear task titles in aggregation tables, and the `.pi` prompts' edition and version inaccuracies.
- Commits carry no attribution trailers.
- Escalate (stop and ask) on a `lokomotiv` hit in a living file that is a command, path or filter and is not listed in the spec, or if `lok-backend` appears on crates.io under another owner.

**Shell conventions for this repository's sessions:** run from the worktree root, pass absolute paths, never `cd`, never `git -C`. `WT` below is `/Users/mk/Code/orchestrator/lok--chore-crate-rename`. `EV` is a `crate-rename/` evidence directory in the executor's session scratchpad; it must survive across tasks because Task 1 records a baseline that Task 3 compares against.

---

### Task 1: Manifest, source and tests

Covers AC1, AC2, AC3 and Evaluation #3, #4, #5.

**Files:**
- Modify: `Cargo.toml:2`, `:7`, `:11`, `:30-33`, `:49`, `:54-56`, `:113`
- Modify: `Cargo.lock` (root package entry only)
- Modify: `src/main.rs:458`, `:479`, `:2448-2505`
- Modify: `src/engine.rs:3`, `:8`
- Modify: `src/backend/mod.rs:717`
- Modify: `src/bin/silence_probe.rs:17`, `:120`
- Modify: `src/lib.rs:31` (doctest import only; the rest of `src/lib.rs` waits for Task 3)
- Test: `tests/backend_public_api.rs:1-6`, `:27`, `:84`, `:90-91`, `:147`, `:208`

**Interfaces:**
- Consumes: nothing.
- Produces: library crate path `lok_backend`; `pub const RETRY_LOG_TARGET: &str = "lok_backend::retry"`; `fn logger_filter(verbose: bool, rust_log: Option<String>) -> String` returning `off,lok_backend={level},lok={level}` for bare levels. Tasks 3 and 4 document these exact strings.

- [ ] **Step 1: Confirm the name is still free (Evaluation #1)**

```bash
curl -s -o /dev/null -w '%{http_code}\n' -H 'User-Agent: lok (github.com/maxkulish/lok)' https://crates.io/api/v1/crates/lok-backend
```

Expected: `404`. Anything else: stop and escalate.

- [ ] **Step 2: Record the rustdoc warning baseline (Script 6, first half)**

Must run before any edit under `src/`.

```bash
mkdir -p "$EV"
cargo doc --locked --no-deps --lib --all-features 2>&1 | rg '^warning: ' | rg -v 'generated [0-9]+ warning' | sort > "$EV/doc-before.txt"
wc -l "$EV/doc-before.txt"
```

Expected: the command exits 0; the line count is recorded (it may be 0).

- [ ] **Step 3: Edit `Cargo.toml`**

Replace, in order:

```toml
name = "lokomotiv"
```
with
```toml
name = "lok-backend"
```

```toml
authors = ["ducks"]
```
with
```toml
authors = ["ducks", "Max Kulish"]
```

```toml
documentation = "https://docs.rs/lokomotiv"
```
with
```toml
documentation = "https://docs.rs/lok-backend"
```

Delete this block and the blank line after it:
```toml
[[bin]]
name = "lokomotiv"
path = "src/main.rs"
required-features = ["cli"]

```

In `[lib]`:
```toml
name = "lokomotiv"
path = "src/lib.rs"
```
with
```toml
name = "lok_backend"
path = "src/lib.rs"
```

The `cli` feature comment:
```toml
# Everything the `lok` binaries need and the library does not. Enabled by
# default so `cargo install lokomotiv` keeps working; library consumers opt out
# with `default-features = false` to get a tree with no CLI dependencies.
```
with
```toml
# Everything the `lok` binary needs and the library does not. Enabled by
# default so `cargo install` keeps building `lok`; library consumers opt out
# with `default-features = false` to get a tree with no CLI dependencies.
```

The dev-dependency (its three comment lines above stay unchanged):
```toml
lokomotiv = { path = ".", default-features = false, features = ["test-support"] }
```
with
```toml
lok-backend = { path = ".", default-features = false, features = ["test-support"] }
```

- [ ] **Step 4: Regenerate the root entry in `Cargo.lock`**

```bash
cargo update --workspace
git diff --stat Cargo.lock
git diff Cargo.lock | rg '^[-+][^-+]'
```

Expected: `Cargo.lock | 2 +-` (or a small move of the same entry), and the changed lines are only `-name = "lokomotiv"` / `+name = "lok-backend"`. Any dependency version change: stop, run `git checkout Cargo.lock`, and investigate before continuing.

- [ ] **Step 5: Rename every `lokomotiv::` path and target string**

```bash
sed -i '' 's/lokomotiv::/lok_backend::/g' src/main.rs src/engine.rs src/backend/mod.rs src/bin/silence_probe.rs src/lib.rs tests/backend_public_api.rs
```

This covers `src/main.rs:458`, `:2486`, `:2499`, `:2502`, `:2505`; `src/engine.rs:8`; `src/backend/mod.rs:717` (the const becomes `"lok_backend::retry"`); `src/bin/silence_probe.rs:17`, `:120`; `src/lib.rs:31` (the doctest `use` line; no other line of `src/lib.rs` contains `lokomotiv::`); and `tests/backend_public_api.rs:3`, `:6`, `:27`, `:84`, `:90`, `:91`, `:147`, `:208`.

- [ ] **Step 6: Rename the two module docs that name the library**

`src/engine.rs:3`:
```rust
//! This module wraps the `lokomotiv` backend library with lok-specific
```
with
```rust
//! This module wraps the `lok-backend` library with lok-specific
```

`tests/backend_public_api.rs:1`:
```rust
//! External-consumer integration test for the `lokomotiv` library target.
```
with
```rust
//! External-consumer integration test for the `lok-backend` library target.
```

- [ ] **Step 7: Confirm what is left**

```bash
rg -n lokomotiv src tests Cargo.toml Cargo.lock
```

Expected: exactly these hits, all handled in later steps or tasks:
- `src/main.rs:479` `format!("off,lokomotiv={l},lok={l}")` (Step 10)
- `src/main.rs:2448`, `:2452`, `:2469`, `:2473`, `:2477` `"off,lokomotiv=..."` (Step 8)
- `src/lib.rs:17`, `:22`, `:24`, `:78`, `:118` (Task 3)

Any other hit: classify it against the spec before continuing.

- [ ] **Step 8: Change the `logger_filter` test expectations first**

```bash
sed -E -i '' 's/"off,lokomotiv=(debug|trace|warn),lok=/"off,lok_backend=\1,lok=/' src/main.rs
```

The five assertions in `bare_rust_log_level_is_scoped_to_lok_targets` and `default_filter_keeps_dependencies_off` now read, for example:

```rust
        assert_eq!(
            super::logger_filter(false, Some("debug".to_string())),
            "off,lok_backend=debug,lok=debug"
        );
```

- [ ] **Step 9: Run the two tests and watch them fail**

```bash
cargo test --locked --bin lok -- bare_rust_log_level_is_scoped_to_lok_targets default_filter_keeps_dependencies_off 2>&1 | tail -15
```

Expected: both FAIL; each panic shows `left: "off,lokomotiv=...` against `right: "off,lok_backend=...`. `test result: FAILED. 0 passed; 2 failed`.

- [ ] **Step 10: Change the filter**

`src/main.rs:479`:
```rust
    let level = |l: &str| format!("off,lokomotiv={l},lok={l}");
```
with
```rust
    let level = |l: &str| format!("off,lok_backend={l},lok={l}");
```

- [ ] **Step 11: Run the two tests and watch them pass**

```bash
cargo test --locked --bin lok -- bare_rust_log_level_is_scoped_to_lok_targets default_filter_keeps_dependencies_off 2>&1 | tail -5
```

Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Step 12: Format, then check that the code no longer names the old crate (Evaluation #3)**

```bash
cargo fmt
git diff --stat
rg -n lokomotiv src tests Cargo.toml Cargo.lock | rg -v '^src/lib.rs:'
rg -n 'lok_backend::retry|off,lok_backend=' src
```

Expected: `git diff --stat` lists only the files in this task. The second command prints nothing. The third prints `src/backend/mod.rs:717` (the const), `src/main.rs:479` (the filter) and the five `logger_filter` assertions, nothing else. If `cargo fmt` rewrapped a line, that is allowed; if it reordered imports in an unrelated file, revert that file.

- [ ] **Step 13: Lint as CI runs it (Evaluation #4)**

```bash
cargo fmt -- --check; echo "fmt exit=$?"
cargo clippy --locked --all-targets -- -D warnings > "$EV/clippy.log" 2>&1; echo "clippy exit=$?"
cargo clippy --locked --all-targets --features bedrock -- -D warnings > "$EV/clippy-bedrock.log" 2>&1; echo "clippy bedrock exit=$?"
cargo clippy --locked --lib --tests --no-default-features -- -D warnings > "$EV/clippy-lib.log" 2>&1; echo "clippy lib exit=$?"
```

Expected: every exit is 0. On a non-zero exit, read the matching log in `$EV`.

- [ ] **Step 14: Test as CI runs it (Evaluation #5)**

```bash
cargo test --locked 2>&1 | rg '^test result|^     Running|Doc-tests' | tee "$EV/test-default.txt"
cargo test --locked --features bedrock 2>&1 | rg '^test result' | tee "$EV/test-bedrock.txt"
cargo test --locked --lib --no-default-features 2>&1 | rg '^test result' | tee "$EV/test-lib.txt"
rg -c ' 0 failed' "$EV"/test-*.txt; rg 'FAILED| [1-9][0-9]* failed' "$EV"/test-*.txt
cargo test --locked --bin lok -- retry_records_keep_their_glyph ordinary_warnings_keep_their_prefix bare_rust_log_level_is_scoped_to_lok_targets default_filter_keeps_dependencies_off explicit_module_directives_pass_through 2>&1 | rg '^test ' | tee "$EV/test-named.txt"
```

Expected: every `test result:` line says `ok` and `0 failed`; the `FAILED` `rg` prints nothing. `test-named.txt` lists the two `render_record` tests and the three `logger_filter` tests as `ok`, then `test result: ok. 5 passed; 0 failed`. `test-default.txt` shows `Running` lines for `tests/backend_public_api.rs`, `tests/cli_output.rs`, `tests/project_config_trust.rs` and `Doc-tests lok_backend`. There is one `src/main.rs` unit-test binary instead of two, so the total test count drops; that is expected and not a gate.

- [ ] **Step 15: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/engine.rs src/backend/mod.rs src/bin/silence_probe.rs src/lib.rs tests/backend_public_api.rs
git commit -m "chore(crate): rename the package to lok-backend and drop the lokomotiv binary

Upstream ducks owns lokomotiv on crates.io, so the library takes a name
this project can publish. The retry log target becomes lok_backend::retry
and the CLI filter names lok_backend; env_filter matches by prefix, so the
lok= entry already covered the library's targets."
```

---

### Task 2: CI and release

Covers AC4.

**Files:**
- Modify: `.github/workflows/ci.yml:291`
- Modify: `.github/workflows/release.yml:98-106`, `:134`

**Interfaces:**
- Consumes: Task 1's manifest (no `lokomotiv` `[[bin]]`).
- Produces: nothing later tasks call.

- [ ] **Step 1: Assert the removed binary stays gone in CI**

`.github/workflows/ci.yml`, step "Binaries install with no extra flags":
```yaml
          test -x "$root/bin/lok"
          test -x "$root/bin/lokomotiv"
```
with
```yaml
          test -x "$root/bin/lok"
          test ! -e "$root/bin/lokomotiv"
```

- [ ] **Step 2: Smoke-test only `lok` in the release**

`.github/workflows/release.yml`:
```yaml
      - name: Smoke test both binaries on their native runner
        shell: bash
        run: |
          set -euo pipefail
          for bin in lok lokomotiv; do
            BIN="target/${{ matrix.target }}/release/$bin"
            "$BIN" --version
            "$BIN" --help >/dev/null
          done
```
with
```yaml
      - name: Smoke test the binary on its native runner
        shell: bash
        run: |
          set -euo pipefail
          BIN="target/${{ matrix.target }}/release/lok"
          "$BIN" --version
          "$BIN" --help >/dev/null
```

- [ ] **Step 3: Package only `lok`**

`.github/workflows/release.yml`, step "Package", delete this line:
```yaml
          cp "target/${{ matrix.target }}/release/lokomotiv" "$STAGE/"
```

- [ ] **Step 4: Lint the workflows and run the install check locally**

```bash
actionlint .github/workflows/ci.yml .github/workflows/release.yml; echo "actionlint exit=$?"
rg -n lokomotiv .github/workflows
```

Expected: `actionlint exit=0` (it was 0 before the edit). The `rg` prints only `ci.yml:291: test ! -e "$root/bin/lokomotiv"`.

The CI install step itself runs in Task 3 Step 9 (Evaluation #9), which runs the same `cargo install --locked --path . --root` against a scratch root.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml .github/workflows/release.yml
git commit -m "ci: smoke-test and package only the lok binary

The lokomotiv binary is gone. CI now asserts it stays absent from a
default install."
```

---

### Task 3: Consumer docs

Covers AC5, AC6 and Evaluation #2, #6, #7, #8, #9, #10.

**Files:**
- Modify: `README.md:23-46`, `:48-52`, `:89`, `:97-102`
- Modify: `src/lib.rs:10-25`, `:78`, `:118`

**Interfaces:**
- Consumes: `lok_backend` crate path, `RETRY_LOG_TARGET = "lok_backend::retry"` (Task 1).
- Produces: README heading exactly `## Using lok-backend as a library`, whose first ```` ```toml ```` block is the dependency snippet and whose first ```` ```rust ```` block is the example. Scripts 10 and 14 extract both with `awk`, so the migration snippet must come after the dependency snippet.

- [ ] **Step 1: Rewrite the README library section head**

Replace `README.md` from `## Using lokomotiv as a library` through the paragraph ending `...multiple-locations)).` (lines 23-46) with:

````markdown
## Using lok-backend as a library

The `lok-backend` library is not on crates.io. Before `v20260916.0.0` its crate
was named `lokomotiv`. The crate called `lokomotiv` on
[crates.io](https://crates.io/crates/lokomotiv) is published by the upstream
project, [ducks/lok](https://github.com/ducks/lok), is binary-only, and does not
contain this repository's changes.

Depend on this repository through git instead, with **no default features** to
avoid pulling in CLI-only dependencies. The example below also needs `tokio`,
because it uses `#[tokio::main]`:

```toml
[dependencies]
lok-backend = { git = "https://github.com/maxkulish/lok", tag = "v20260916.0.0", default-features = false }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The `tag` selects the source revision to build. Each release is tagged `v` plus
the crate version (`vYYYYMMDD.N.P`, for example `v20260916.0.0`).

A crate that depends on `lok-backend` through git cannot itself be published to
crates.io: crates.io needs a registry version for every dependency, and
`lok-backend` is not on crates.io (see
[specifying dependencies from multiple locations](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations)).

A crate that already depends on this repository as `lokomotiv` keeps its
`lokomotiv::` imports by renaming the dependency when it moves to
`v20260916.0.0` or later:

```toml
lokomotiv = { package = "lok-backend", git = "https://github.com/maxkulish/lok", tag = "v20260916.0.0", default-features = false }
```

A crate pinned to an older tag is unaffected. The retry notice now logs under
the target `lok_backend::retry`, so a log filter written against the literal
`lokomotiv::retry` must use `RETRY_LOG_TARGET` or `lok_backend::retry`.
````

- [ ] **Step 2: Rename the example import**

`README.md`, in the ```` ```rust,no_run ```` block:
```rust
use lokomotiv::{
```
with
```rust
use lok_backend::{
```

- [ ] **Step 3: Rename the versioning note**

```markdown
> **Note**: `lokomotiv` shares its version number with the `lok` binary. Both are
```
with
```markdown
> **Note**: `lok-backend` shares its version number with the `lok` binary. Both are
```

- [ ] **Step 4: Update the Quick Start install**

Keep the paragraph above the block unchanged (it still warns that `cargo install lokomotiv` without `--git` installs the upstream crate). Replace:
```bash
cargo install --locked --git https://github.com/maxkulish/lok --tag v20260914.0.0 lokomotiv
                             # Package "lokomotiv"; installs the "lok" and "lokomotiv" binaries
```
with
```bash
cargo install --locked --git https://github.com/maxkulish/lok --tag v20260916.0.0 lok-backend
                             # Package "lok-backend"; puts the "lok" binary on PATH
```

Leave `## Why "Lok"?` (`README.md:571`) unchanged.

- [ ] **Step 5: Rewrite the `src/lib.rs` quick start**

Replace `src/lib.rs:15-25`:
```rust
//! ```toml
//! [dependencies]
//! lokomotiv = { git = "https://github.com/maxkulish/lok", tag = "v20260914.0.0", default-features = false }
//! tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
//! ```
//!
//! The `tag` selects the source revision to build. A crate that depends on
//! `lokomotiv` through git cannot itself be published to crates.io:
//! crates.io needs a registry version for every dependency, and no library
//! version of `lokomotiv` exists there (see
//! [specifying dependencies from multiple locations](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations)).
```
with
```rust
//! ```toml
//! [dependencies]
//! lok-backend = { git = "https://github.com/maxkulish/lok", tag = "v20260916.0.0", default-features = false }
//! tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
//! ```
//!
//! The `tag` selects the source revision to build. A crate that depends on
//! `lok-backend` through git cannot itself be published to crates.io:
//! crates.io needs a registry version for every dependency, and
//! `lok-backend` is not on crates.io (see
//! [specifying dependencies from multiple locations](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations)).
```

The `use lok_backend::{` line at `:31` is already done (Task 1).

- [ ] **Step 6: Update the feature row and versioning in `src/lib.rs`**

`:78`:
```rust
//! | `cli` | yes | CLI-only dependencies (`clap`, `indicatif`, `colored`, …). Required by the `lok` and `lokomotiv` binaries. |
```
with
```rust
//! | `cli` | yes | CLI-only dependencies (`clap`, `indicatif`, `colored`, …). Required by the `lok` binary. |
```

`:118`:
```rust
//! `lokomotiv` shares its version number with the `lok` binary. Both are
```
with
```rust
//! `lok-backend` shares its version number with the `lok` binary. Both are
```

The rest of `# Versioning` already says the library is not on crates.io; leave it.

- [ ] **Step 7: No stray name in code (Evaluation #2), rustdoc (Evaluation #6, #7)**

```bash
rg -n lokomotiv src tests Cargo.toml Cargo.lock; echo "rg exit=$?"
cargo doc --locked --no-deps --lib --all-features 2>&1 | rg '^warning: ' | rg -v 'generated [0-9]+ warning' | sort > "$EV/doc-after.txt"
diff <(sed 's/lokomotiv/lok_backend/g' "$EV/doc-before.txt") "$EV/doc-after.txt"; echo "diff exit=$?"
RUSTDOCFLAGS='-D missing_docs' cargo doc --locked --no-deps --lib --all-features > "$EV/doc-strict.log" 2>&1; echo "doc exit=$?"
cargo test --locked --doc 2>&1 | rg '^test result'
```

Expected: `rg exit=1` with no output; `diff exit=0` with no output; `doc exit=0`; the doctest summary says `0 failed`.

- [ ] **Step 8: Commit, then rehearse packaging (Evaluation #8)**

`cargo publish --dry-run` refuses a dirty tree, so commit first.

```bash
git add README.md src/lib.rs
git commit -m "docs: point library consumers at lok-backend

README and the crate docs pin v20260916.0.0, install the lok-backend
package, and show the package alias that keeps lokomotiv:: imports
compiling."
cargo publish --dry-run --locked > "$EV/publish-dry-run.log" 2>&1; echo "publish exit=$?"
tail -5 "$EV/publish-dry-run.log"
```

Expected: `publish exit=0`; the output includes `Packaging lok-backend v20260915.0.0` and `aborting upload due to dry run`. Nothing is uploaded.

- [ ] **Step 9: Install into a scratch root (Evaluation #9)**

```bash
cargo install --locked --path "$WT" --root "$EV/install-root" 2>&1 | tail -3
ls "$EV/install-root/bin"
```

Expected: `ls` prints exactly `lok`.

- [ ] **Step 10: Compile the README example as a downstream crate (Script 10)**

```bash
S="$EV/consumers"
cargo new --quiet "$S/direct"
cargo new --quiet "$S/alias"
awk '/^## Using lok-backend as a library/{f=1} f&&/^```rust/{p=1;next} p&&/^```/{exit} p' "$WT/README.md" > "$S/direct/src/main.rs"
sed 's/lok_backend::/lokomotiv::/g' "$S/direct/src/main.rs" > "$S/alias/src/main.rs"
printf 'lok-backend = { path = "%s", default-features = false }\ntokio = { version = "1", features = ["macros", "rt-multi-thread"] }\n' "$WT" >> "$S/direct/Cargo.toml"
printf 'lokomotiv = { package = "lok-backend", path = "%s", default-features = false }\ntokio = { version = "1", features = ["macros", "rt-multi-thread"] }\n' "$WT" >> "$S/alias/Cargo.toml"
cargo check --quiet --manifest-path "$S/direct/Cargo.toml"; echo "direct exit=$?"
cargo check --quiet --manifest-path "$S/alias/Cargo.toml"; echo "alias exit=$?"
```

Expected: `direct exit=0`, `alias exit=0`. Before trusting the result, `head -3 "$S/direct/src/main.rs"` shows `use std::path::Path;` and `use lok_backend::{`, which proves `awk` extracted the right block.

---

### Task 4: Records, aggregation and prompts

Covers AC7, AC8, AC9, AC10 and Evaluation #11, #12.

**Files:**
- Modify: `docs/DEPENDENCIES.md:2`, `:59`, `:66-67`, `:80-95`
- Modify: `docs/decisions/clo-592-workspace-split.md` (insert before `## Related`)
- Modify: `docs/adrs/clo-589-backend-library-shape.md:152`
- Modify: `docs/PROJECT.md:3`, `:16`, Recently Completed top row
- Modify: `docs/ROADMAP.md:3`, `:56`
- Modify: `.github/copilot-instructions.md:7`
- Modify: `.pi/agents/claude-designer.md:4-5`, `.pi/agents/codex-pre-pr.md:13-15`, `.pi/agents/ollama-rust-reviewer.md:11-13`, `.pi/agents/ops-reviewer.md:4-5`, `:22-23`, `:49`, `.pi/agents/security-reviewer.md:17-18`
- Modify: `.pi/extensions/orchestrate/README.md:32`, `.pi/IMPLEMENTATION_SUMMARY.md:133`
- Modify: `.lok/prompts/design-draft-prompt.md:15-18`, `.lok/workflows/pre-pr-validation.toml:97`

**Interfaces:**
- Consumes: the names from Tasks 1-3 (`lok-backend`, `lok_backend::retry`, `v20260916.0.0`) and the verification results recorded in `$EV`.
- Produces: nothing later tasks call.

**`**Last Updated**` rule** (used in Steps 1, 5 and 6): replace the date line's opening `(` text with `(<new lead>. Earlier: `, and change the first ` Earlier: ` already in that line to `; `, so the line keeps a single `Earlier:`. Line numbers above can move if `main` changed; match on text.

- [ ] **Step 1: `docs/DEPENDENCIES.md` Last Updated**

New lead: `crate renamed from lokomotiv to lok-backend, untracked; the crates.io publishing entry records the decision and the standing constraint ties the split to the first lok-backend publish`

- [ ] **Step 2: `docs/DEPENDENCIES.md` standing constraints**

```markdown
One item CLO-591 left open deliberately. It blocks no task, and its cost depends on the crates.io naming decision described below, not on a date:
```
with
```markdown
One item CLO-591 left open deliberately. It blocks no task, and its cost depends on when the library is first published to crates.io as `lok-backend` (see below), not on a date:
```

```markdown
  migration. `docs/decisions/clo-592-workspace-split.md` re-affirmed not
  splitting on 2026-09-15 and ties the question to the naming decision.
```
with
```markdown
  migration. `docs/decisions/clo-592-workspace-split.md` re-affirmed not
  splitting on 2026-09-15 and again on 2026-09-16, when the name `lok-backend`
  was decided. The split is revisited before the first `lok-backend` publish.
```

- [ ] **Step 3: `docs/DEPENDENCIES.md` crates.io publishing entry**

Replace from `**crates.io publishing** (last verified 2026-09-15):` through `[CLO-660](https://linear.app/cloud-ai/issue/CLO-660).` with:

````markdown
**crates.io publishing** (last verified 2026-09-16): the crates.io name `lokomotiv` is owned
by `ducks`, the upstream author, who published all 28 versions. The last one is `20260208.0.2`
(2026-02-08). Every version is binary-only and none is yanked. This project has never published,
and its `[lib]` target reached main later, in `ee28f3c` (2026-07-27). Because of that ownership,
the crate was renamed on 2026-09-16 to `lok-backend` (lib `lok_backend`), a name unregistered on
crates.io that day, and the `lokomotiv` binary was dropped. Nothing is published under the new
name yet. The first publish waits on [CLO-635](https://linear.app/cloud-ai/issue/CLO-635), which
fixes a fail-open sandbox default in the published `backend` module, and on an Actions secret
holding the registry token, which this repository does not have (see `docs/PROJECT.md` Up Next).
Until then the library is consumed as a git dependency on `github.com/maxkulish/lok`, and a
downstream crate that depends on it that way cannot itself be published to crates.io. Re-verify
the upstream owner and latest version, and the state of `lok-backend`, before relying on them:

```bash
curl -s -H 'User-Agent: lok (github.com/maxkulish/lok)' https://crates.io/api/v1/crates/lokomotiv/owners | jq -r '.users[].login'
curl -s -H 'User-Agent: lok (github.com/maxkulish/lok)' https://crates.io/api/v1/crates/lokomotiv/versions | jq -r '.versions | sort_by(.created_at) | [length, (map(select(.yanked)) | length), last.num, last.created_at[:10]] | @tsv'
curl -s -o /dev/null -w '%{http_code}\n' -H 'User-Agent: lok (github.com/maxkulish/lok)' https://crates.io/api/v1/crates/lok-backend
```

The first prints the owners (`ducks`). The second prints the version count, the yanked count, and
the newest version with its date (`28  0  20260208.0.2  2026-02-08`). The third prints `404` while
`lok-backend` is unregistered. Once it prints `200`, the first command pointed at `lok-backend`
must list this project's owner, or the name was taken by someone else. Recorded by
[CLO-660](https://linear.app/cloud-ai/issue/CLO-660) and the rename
(`specs/2026-09-16-lok-backend-rename.md`).
````

- [ ] **Step 4: Decision record section and ADR trigger**

In `docs/decisions/clo-592-workspace-split.md`, insert immediately before `## Related` (no other line changes):

```markdown
## Naming decided 2026-09-16

The naming question the re-evaluation above left open is settled. The crate
stays one package, renamed from `lokomotiv` to `lok-backend` (lib
`lok_backend`), a name unregistered on crates.io on 2026-09-16. The `lokomotiv`
binary was dropped, leaving `lok`. The change is specified in
`specs/2026-09-16-lok-backend-rename.md`.

- The package is named for the library, not the CLI. A future split keeps
  `lok-backend` as the library crate and adds a CLI crate, `lok-cli`, so library
  consumers do not change their dependency line a second time
- A git consumer that keeps its `lokomotiv::` imports depends with
  `package = "lok-backend"`. A consumer pinned to an older tag is unaffected
- The first crates.io publish of `lok-backend` waits on CLO-635 and on an
  Actions secret for the registry token

**Verdict: stands.** Do not split the workspace now. The rename changed the
package name, not the layout, so none of the reasons above changed. Revisit the
split before the first crates.io publish of `lok-backend`.

```

In `docs/adrs/clo-589-backend-library-shape.md`:
```markdown
- The library is published to crates.io under a name this project controls, and consumer feedback asks for a lighter dependency tree.
```
with
```markdown
- The library is published to crates.io as `lok-backend`, and consumer feedback asks for a lighter dependency tree.
```

- [ ] **Step 5: `docs/PROJECT.md`**

Last Updated new lead: `crate renamed from lokomotiv to lok-backend, untracked, and moved to Recently Completed; the naming-decision row in Up Next became the first lok-backend publish`

Replace the Up Next row that begins `| Medium | - | Decide how the library reaches crates.io:` (whole line) with:

```markdown
| Medium | - | First crates.io publish of the library as `lok-backend`, the name the crate took on 2026-09-16. Waits on CLO-635, which fixes a fail-open sandbox default in the published `backend` module, and on an Actions secret for the registry token: the repository has none, so `publish.yml` stays a dry run. Metadata freezes per published version, so revisit the crate `description` and `keywords`, the README library section, and the workspace split (`docs/decisions/clo-592-workspace-split.md`) first. Facts in `docs/DEPENDENCIES.md` under crates.io publishing | CLO-635; registry token secret | Next (HITL) |
```

Insert as the first data row of Recently Completed (directly under `|------|-------|-----------|---------|`). Every claim in it must match a result recorded in `$EV` by Tasks 1-3; correct the row if one differs:

```markdown
| - | Rename the crate from lokomotiv to lok-backend | 2026-09-16 | Untracked, no CLO task. Upstream `ducks` owns `lokomotiv` on crates.io, so the package and lib are now `lok-backend` and `lok_backend`, and the `lokomotiv` binary is gone: installs and release archives carry `lok` only, and CI asserts `bin/lokomotiv` stays absent. `RETRY_LOG_TARGET` became `lok_backend::retry` and the CLI filter `off,lok_backend={l},lok={l}`; env_filter matches targets by prefix, so `lok=` already covered the library and verbose output is unchanged. README and `src/lib.rs` pin `v20260916.0.0`, released right after merge, and show a `package = "lok-backend"` alias that keeps `lokomotiv::` imports compiling. DEPENDENCIES, the workspace-split record (new dated section, verdict stands), the ADR trigger, ROADMAP Phase 13 and the agent prompts name `lok-backend`. Verified by CI-equivalent clippy and tests (0 failed), unchanged rustdoc warnings, `cargo publish --dry-run`, a scratch install holding only `bin/lok`, and the README example compiling directly and through the alias. Spec `specs/2026-09-16-lok-backend-rename.md`. |
```

- [ ] **Step 6: `docs/ROADMAP.md`**

Last Updated new lead: `crate renamed from lokomotiv to lok-backend, untracked; Phase 13's intro names lok-backend and ties its first publish to CLO-635, with no row or count change`

In the Phase 13 intro:
```markdown
This project has never published to crates.io and cannot publish `lokomotiv`, because upstream `ducks` owns that name (see "crates.io publishing" in `docs/DEPENDENCIES.md`).
```
with
```markdown
This project has never published to crates.io. Upstream `ducks` owns the name `lokomotiv`, so on 2026-09-16 the crate was renamed to `lok-backend`, and its first publish waits on CLO-635, which fixes a fail-open sandbox default in the published `backend` module (see "crates.io publishing" in `docs/DEPENDENCIES.md`).
```

and
```markdown
CLO-609's crate metadata starts to matter at the first crates.io publish under a name this project controls,
```
with
```markdown
CLO-609's crate metadata starts to matter at the first crates.io publish of `lok-backend`,
```

- [ ] **Step 7: Agent and prompt docs**

`.github/copilot-instructions.md:7`:
```markdown
Package name: `lokomotiv`. Binary name: `lok`.
```
with
```markdown
Package name: `lok-backend`. Binary name: `lok`.
```

`.pi/agents/claude-designer.md:4-5`:
```markdown
for the lok repository. lok is a single Rust crate (`lokomotiv`, version
`20260412.x.y`) that produces two binaries - `lok` (the CLI) and `lokomotiv` - and
```
with
```markdown
for the lok repository. lok is a single Rust crate (`lok-backend`, version
`20260412.x.y`) that produces one binary - `lok` (the CLI) - and
```

`.pi/agents/codex-pre-pr.md:13-15`:
```markdown
- Single Rust crate at the repo root: `lokomotiv` (version
  `20260412.x.y`), producing two binaries - `lok` (CLI) and
  `lokomotiv`. Edition 2024.
```
with
```markdown
- Single Rust crate at the repo root: `lok-backend` (version
  `20260412.x.y`), producing one binary - `lok` (CLI). Edition 2024.
```

`.pi/agents/ollama-rust-reviewer.md:11-13`:
```markdown
- Single Rust crate at the repo root: `lokomotiv` (version
  `20260412.x.y`), producing binaries `lok` and `lokomotiv`. Edition
  2024.
```
with
```markdown
- Single Rust crate at the repo root: `lok-backend` (version
  `20260412.x.y`), producing one binary, `lok`. Edition 2024.
```

`.pi/agents/ops-reviewer.md:3-5`:
```markdown
You are an operations-focused reviewer for the lok project - a single
Rust crate (`lokomotiv`) shipped as two CLI binaries (`lok` and
`lokomotiv`). lok is a local developer tool, not a hosted service, so
```
with
```markdown
You are an operations-focused reviewer for the lok project - a single
Rust crate (`lok-backend`) shipped as one CLI binary, `lok`. lok is a
local developer tool, not a hosted service, so
```

`.pi/agents/ops-reviewer.md:22-23`:
```markdown
- Single Rust crate `lokomotiv` (version `20260412.x.y`), edition 2024,
  producing two binaries - `lok` (CLI) and `lokomotiv`.
```
with
```markdown
- Single Rust crate `lok-backend` (version `20260412.x.y`), edition 2024,
  producing one binary - `lok` (CLI).
```

`.pi/agents/ops-reviewer.md:49`:
```markdown
   - Binary names (`lok`, `lokomotiv`) and `[[bin]]` entries are
```
with
```markdown
   - The binary name (`lok`) and `[[bin]]` entries are
```

Lines 54-55 (`cargo install lokomotiv` installs the upstream binary) stay.

`.pi/agents/security-reviewer.md:17-18`:
```markdown
- Single Rust crate `lokomotiv` producing `lok` (CLI) and `lokomotiv`
  binaries. Edition 2024.
```
with
```markdown
- Single Rust crate `lok-backend` producing the `lok` (CLI) binary.
  Edition 2024.
```

`.pi/extensions/orchestrate/README.md:32`:
```markdown
| Stack | Tauri v2 + Rust + React | Single Rust crate (`lokomotiv`) - bins `lok` and `lokomotiv` |
```
with
```markdown
| Stack | Tauri v2 + Rust + React | Single Rust crate (`lok-backend`) - bin `lok` |
```

`.pi/IMPLEMENTATION_SUMMARY.md:133`:
```markdown
| Stack | Tauri v2 + Rust + React | Single Rust crate `lokomotiv` (bins `lok`, `lokomotiv`) - modules `backend`, `workflow`, `conductor`, `tasks`, `apply_verify`, `role`, etc. |
```
with
```markdown
| Stack | Tauri v2 + Rust + React | Single Rust crate `lok-backend` (bin `lok`) - modules `backend`, `workflow`, `conductor`, `tasks`, `apply_verify`, `role`, etc. |
```

`.lok/prompts/design-draft-prompt.md:15-18`:
```markdown
lok is a single-crate Rust project (`lokomotiv`, edition 2021) with two binaries:

- `lok` - operator CLI for running multi-LLM workflows defined in TOML
- `lokomotiv` - orchestrator daemon / supporting binary
```
with
```markdown
lok is a single-crate Rust project (`lok-backend`, edition 2021) with one binary:

- `lok` - operator CLI for running multi-LLM workflows defined in TOML
```

`.lok/workflows/pre-pr-validation.toml:97`:
```text
The Codex reviewer failed. You are the fallback pre-PR validator for **lok** - a Rust CLI workflow orchestrator (single crate `lokomotiv`, edition 2024, binaries `lok` + `lokomotiv`).
```
with
```text
The Codex reviewer failed. You are the fallback pre-PR validator for **lok** - a Rust CLI workflow orchestrator (single crate `lok-backend`, edition 2024, binary `lok`).
```

- [ ] **Step 8: Validate the edited TOML workflow and check for em dashes**

```bash
python3 -c 'import tomllib; tomllib.load(open(".lok/workflows/pre-pr-validation.toml","rb")); print("toml ok")'
git diff -U0 | rg '^\+' | rg -c '\x{2014}'; echo "em-dash rg exit=$?"
```

Expected: `toml ok`; the em-dash `rg` prints nothing and exits 1.

- [ ] **Step 9: Living-doc grep (Script 11)**

```bash
LIVING=(README.md src/lib.rs Cargo.toml .github/copilot-instructions.md .github/workflows/ci.yml
  .github/workflows/release.yml .lok/prompts/design-draft-prompt.md .lok/workflows/pre-pr-validation.toml
  .pi/agents/claude-designer.md .pi/agents/codex-pre-pr.md .pi/agents/ollama-rust-reviewer.md
  .pi/agents/ops-reviewer.md .pi/agents/security-reviewer.md .pi/extensions/orchestrate/README.md
  .pi/IMPLEMENTATION_SUMMARY.md docs/DEPENDENCIES.md docs/PROJECT.md docs/ROADMAP.md)
rg -n lokomotiv "${LIVING[@]}"
rg -l lokomotiv --hidden -g '!.git' -g '!target' \
  -g '!docs/{reviews,plans,designs,design-docs,prds,lessons,specs,discovery,status}/**' \
  -g '!specs/**' -g '!.pi/lessons/**' -g '!architecture/**' -g '!docs/adrs/**' -g '!docs/decisions/**' . \
  | sed 's|^\./||' | rg -v -x -F -f <(printf '%s\n' "${LIVING[@]}")
git diff "$(git merge-base origin/main HEAD)" -- docs/decisions docs/adrs
```

Expected: every hit from the first `rg` falls into a class (a)-(f) from the spec, as follows:
- (a) upstream facts: README library intro and Quick Start note, DEPENDENCIES crates.io entry and its commands, PROJECT/ROADMAP rename prose, `ops-reviewer.md:54`
- (b) alias: README migration paragraph and snippet
- (c) etymology: `README.md` `## Why "Lok"?`
- (d) Linear titles: PROJECT CLO-660 row, ROADMAP CLO-660 row
- (e) retry note: README `lokomotiv::retry` sentence
- (f) `ci.yml` `test ! -e "$root/bin/lokomotiv"`

Also allowed: the untracked Recently Completed rows in PROJECT (the new one and the 2026-09-15 row, whose text predates this change). The discovery command prints nothing. The last diff shows only the added `## Naming decided 2026-09-16` section and the one ADR trigger line.

- [ ] **Step 10: Change set (Evaluation #12)**

```bash
git add -A docs/DEPENDENCIES.md docs/decisions/clo-592-workspace-split.md docs/adrs/clo-589-backend-library-shape.md docs/PROJECT.md docs/ROADMAP.md .github/copilot-instructions.md .pi/agents .pi/extensions/orchestrate/README.md .pi/IMPLEMENTATION_SUMMARY.md .lok/prompts/design-draft-prompt.md .lok/workflows/pre-pr-validation.toml
git status --porcelain
git diff --cached --name-only "$(git merge-base origin/main HEAD)"
```

Expected: `git status --porcelain` shows nothing unstaged or untracked. The name list holds only the files named in spec section 4, the spec itself, and this plan.

- [ ] **Step 11: Commit**

```bash
git commit -m "docs: record the lok-backend naming decision

DEPENDENCIES, the workspace-split record, the ADR trigger, PROJECT and
ROADMAP name lok-backend and tie its first publish to CLO-635. Agent
and prompt docs describe one crate with one binary."
```

---

### Task 5: Pull request

**Files:** none changed unless the rebase needs it.

**Interfaces:**
- Consumes: Tasks 1-4 committed on `chore/crate-rename`.
- Produces: an open PR against `main`.

- [ ] **Step 1: Rebase on current main**

```bash
git fetch origin
git rebase origin/main
git log --oneline origin/main..HEAD
```

Expected: six commits (spec, plan, Tasks 1-4). If the rebase conflicts in an aggregation file, keep both sides' content and reapply the `**Last Updated**` rule from Task 4.

- [ ] **Step 2: Re-verify after the rebase**

If `git diff --stat ORIG_HEAD HEAD -- src tests Cargo.toml Cargo.lock` is non-empty, rerun Task 1 Steps 13-14. Always rerun Task 3 Step 7's first command (Evaluation #2), and Task 4 Steps 9-10 (Evaluation #11, #12).

- [ ] **Step 3: Push and open the PR**

```bash
git push -u origin chore/crate-rename
gh pr create --base main --title "chore(crate): rename the package from lokomotiv to lok-backend" --body-file "$EV/pr-body.md"
```

`$EV/pr-body.md` holds `## Summary` (why the name changed, what consumers change, the dropped binary), `## Verification` (Evaluation #1-#12 with their results from `$EV`), and `## After merge` with these steps for the user:
1. `make release VERSION=20260916.0.0`, which tags and pushes `v20260916.0.0`; the README and `src/lib.rs` tags do not resolve until then.
2. Before the next `make install` or `cargo install --path .`, run `cargo uninstall lokomotiv`: `~/.cargo/bin/lok` is registered under package `lokomotiv`, so installing `lok-backend` over it fails. `make release` copies to `/usr/local/bin` and is not affected.

No attribution trailers or generated-by lines in the body.

---

### Task 6: After release (user-gated)

Covers AC14. Runs only after the user reports that `v20260916.0.0` is pushed.

- [ ] **Step 1: Tag exists (Evaluation #13)**

```bash
git ls-remote --tags origin v20260916.0.0
```

Expected: one line ending `refs/tags/v20260916.0.0`.

- [ ] **Step 2: Consumer and install against the real tag (Script 14)**

Run from an up-to-date `main` checkout so `README.md` is the merged version.

```bash
S="$EV/after-release"
cargo new --quiet "$S/git"
awk '/^## Using lok-backend as a library/{f=1} f&&/^```rust/{p=1;next} p&&/^```/{exit} p' README.md > "$S/git/src/main.rs"
awk '/^## Using lok-backend as a library/{f=1} f&&/^```toml/{p=1;next} p&&/^```/{exit} p' README.md | rg -v '^\[dependencies\]' >> "$S/git/Cargo.toml"
cargo check --quiet --manifest-path "$S/git/Cargo.toml"; echo "git exit=$?"
cargo install --locked --git https://github.com/maxkulish/lok --tag v20260916.0.0 lok-backend --root "$S/root"
ls "$S/root/bin"; "$S/root/bin/lok" --version
```

Expected: `git exit=0`; `ls` prints only `lok`; `lok 20260916.0.0`.
