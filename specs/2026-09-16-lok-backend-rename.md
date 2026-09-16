# Spec: Rename the crate from `lokomotiv` to `lok-backend`

**Created**: 2026-09-16
**Linear**: none (untracked, like PR #113)
**Branch**: `chore/crate-rename`
**Revision**: 1
**Estimated scope**: M (about 25 files; package identity, one binary removed, CI and release steps, living docs. No behavior change beyond the removed binary and the retry log target string)

## 1. Problem Statement

### What is true (verified 2026-09-16, base `4add3b8`)

| Fact | Source |
|------|--------|
| The crates.io name `lokomotiv` belongs to upstream `ducks`, who published all 28 binary-only versions. This project cannot publish under it | `docs/DEPENDENCIES.md` "crates.io publishing", CLO-660 |
| `lok-backend` is unregistered on crates.io | `GET https://crates.io/api/v1/crates/lok-backend` returned 404 |
| The package, the `[lib]` and a second `[[bin]]` are all named `lokomotiv`. The `lok` and `lokomotiv` binaries both build from `src/main.rs` | `Cargo.toml:2`, `:25-33`, `:48-50` |
| The package depends on itself by name as a dev-dependency | `Cargo.toml:113`: `lokomotiv = { path = ".", default-features = false, features = ["test-support"] }` |
| The library exports one module, `backend`: the `Backend` trait and the concrete backends, without lok's orchestration layer | `src/lib.rs:1-6`, `:128` |
| `RETRY_LOG_TARGET` is a public const hard-coded to `"lokomotiv::retry"`. The CLI compares record targets against the const, not the literal | `src/backend/mod.rs:717`, `src/main.rs:458` |
| The CLI log filter is `off,lokomotiv={l},lok={l}`. env_filter 2.0.0 matches a directive with `target.starts_with(name)`, so `lok=` already covers every `lokomotiv::*` target and will cover every `lok_backend::*` target. The CLI's log output does not depend on the `lokomotiv=` entry | `src/main.rs:479`, `env_filter-2.0.0/src/directive.rs:17` |
| Inside the repository the `lokomotiv` binary is smoke-tested and packaged by `release.yml:102` and `:134`, and asserted by `ci.yml:291`. `rg -w lokomotiv` over `~/.config`, `~/.claude/commands`, `~/.claude/skills` and the dotfiles repo finds no caller outside it | `rg` on 2026-09-16 |
| No `Cargo.toml` under `~/Code` or `~/Work` depends on `lokomotiv`. The work laptop was not checked | `rg -l --glob Cargo.toml lokomotiv ~/Code ~/Work` |
| `make release` edits only `Cargo.toml` and `Cargo.lock` and accepts an explicit `VERSION`. The tags pinned at `README.md:36`, `:41`, `:101` and `src/lib.rs:17` are maintained by hand and still read `v20260914.0.0` | `Makefile:18`, `:111-136` |
| The repository has no Actions secrets, so `CARGO_REGISTRY_TOKEN` is unset, and `publish.yml` stops at `cargo publish --dry-run` by design | `gh secret list` (empty), `.github/workflows/publish.yml:1-20` |
| Both `[[bin]]` targets compile `src/main.rs`, so its unit tests run twice today. Dropping one binary lowers the test count by construction | `Cargo.toml:25-33` |

### Decisions (user, 2026-09-16)

1. Keep one crate and name it for the library. A later workspace split keeps that name for the library and adds a CLI crate (`lok-cli`), so library consumers do not change their dependency line a second time.
2. The package is `lok-backend` and the lib is `lok_backend`.
3. Rename now and publish later. The first crates.io publish is a separate task that waits on CLO-635, which fixes a fail-open sandbox default in `src/backend/gemini.rs`, inside the published module.
4. Drop the `lokomotiv` binary.
5. The work is untracked: no Linear issue, branch `chore/crate-rename`.
6. The docs pin `v20260916.0.0`, the release that ships the rename. The user runs `make release VERSION=20260916.0.0` right after merge.

### Why it matters

Until the crate has a name this project can publish, every library consumer is limited to a git dependency and cannot publish itself, and the workspace-split question stays blocked (`docs/decisions/clo-592-workspace-split.md`). Renaming before the first publish costs one dependency-line change per consumer. After a publish it would also cost a registry migration.

### Out of scope

- Historical records keep `lokomotiv` as written: `docs/{reviews,plans,designs,design-docs,prds,lessons,specs,discovery,status}/**`, `specs/**` except this spec, `.pi/lessons/**`, and `architecture/*.html` (a v20260208.0.2 snapshot).
- Every line of `docs/adrs/clo-589-backend-library-shape.md` except its revisit trigger, including its title and the matching row in `docs/adrs/README.md`.
- The existing dated sections of `docs/decisions/clo-592-workspace-split.md`. This change adds a section and edits none.
- Task titles quoted from Linear in aggregation tables, such as CLO-660's.
- The crates.io publish, the crate `description` and `keywords`, and a real publish step in `publish.yml`.
- The workspace split.
- Inaccuracies in the `.pi` prompts other than the crate and binary names (they say edition 2024 against `Cargo.toml`'s 2021, and version `20260412.x.y`). Reported as follow-ups.
- Anything under `~/.cargo` or `/usr/local/bin`.

## 2. Acceptance Criteria

- [ ] **AC1 Manifest.** In `Cargo.toml`:
  - `name = "lok-backend"`, and `[lib] name = "lok_backend"`
  - no `[[bin]]` named `lokomotiv`. The `lok` and `silence_probe` entries are unchanged
  - `documentation = "https://docs.rs/lok-backend"` and `authors = ["ducks", "Max Kulish"]`
  - the `cli` feature comment no longer mentions `cargo install lokomotiv`, and says the default keeps `cargo install` building `lok`
  - the dev-dependency is `lok-backend = { path = ".", default-features = false, features = ["test-support"] }`, with its comment unchanged
  - `version` is not edited

  `Cargo.lock` is regenerated and contains no `lokomotiv`.
- [ ] **AC2 Source.** Every `lokomotiv::` path in `src/` and `tests/` becomes `lok_backend::`. Module docs that name the library say `lok-backend` (`src/engine.rs:3`, `tests/backend_public_api.rs:1-3` and `:84`). `rg lokomotiv src tests Cargo.toml Cargo.lock` returns nothing.
- [ ] **AC3 Log target and filter.** `RETRY_LOG_TARGET` is `"lok_backend::retry"`. `logger_filter` builds `off,lok_backend={l},lok={l}`, and its tests and the `render_record` tests in `src/main.rs` use the new strings with otherwise unchanged expectations. The retry glyph and the `error:`/`warning:` prefixes render as before.
- [ ] **AC4 CI and release.** `release.yml` smoke-tests and packages only `lok`, and the smoke-test step name no longer says "both binaries". In `ci.yml`, "Binaries install with no extra flags" keeps `test -x "$root/bin/lok"` and replaces the `lokomotiv` line with `test ! -e "$root/bin/lokomotiv"`.
- [ ] **AC5 README.** The library section heading is exactly `## Using lok-backend as a library`, and its prose names `lok-backend`. The dependency snippet is exactly:
  ```toml
  [dependencies]
  lok-backend = { git = "https://github.com/maxkulish/lok", tag = "v20260916.0.0", default-features = false }
  tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
  ```
  The example imports `use lok_backend::{`. The section also says:
  - a crate that already depends on `lokomotiv` through git keeps its `lokomotiv::` imports with `lokomotiv = { package = "lok-backend", git = "https://github.com/maxkulish/lok", tag = "v20260916.0.0", default-features = false }`, and a crate pinned to an older tag is unaffected
  - log filters written against the literal `lokomotiv::retry` must use `RETRY_LOG_TARGET` or `lok_backend::retry`

  The Quick Start installs with `cargo install --locked --git https://github.com/maxkulish/lok --tag v20260916.0.0 lok-backend`, and its trailing comment says the install puts `lok` on `PATH`. The note that `cargo install lokomotiv` without `--git` installs the upstream crate stays. The tag-format example uses `v20260916.0.0`. The Versioning note names `lok-backend`. The etymology line (`:571`) is unchanged.
- [ ] **AC6 lib.rs docs.** The quick-start snippet and import match AC5 exactly. The downstream-publishing sentence and `# Versioning` name `lok-backend` and say it is not on crates.io. The `cli` feature row says it is required by the `lok` binary.
- [ ] **AC7 DEPENDENCIES.** The "crates.io publishing" entry records the decision:
  - this project will publish as `lok-backend`, unregistered on 2026-09-16 and not yet published
  - the first publish waits on CLO-635 and on an Actions secret for the registry token
  - the `lokomotiv` owner facts and their re-verify commands stay, because they are why a new name exists
  - one added command re-checks that `lok-backend` is unregistered or owned by this project

  The Standing constraints paragraph that ties the split to "the crates.io naming decision" says the name is decided and the split is revisited before the first `lok-backend` publish. `**Last Updated**` names this change.
- [ ] **AC8 Decision record and ADR.** `docs/decisions/clo-592-workspace-split.md` gains `## Naming decided 2026-09-16`, which says the crate stays one package renamed `lok-backend`, a future split keeps `lok-backend` as the library and adds `lok-cli`, the "do not split now" verdict stands, and the revisit point is before the first crates.io publish of `lok-backend`. No earlier line changes. In `docs/adrs/clo-589-backend-library-shape.md`, only the revisit trigger that says "under a name this project controls" changes, to name `lok-backend`.
- [ ] **AC9 Aggregation.**
  - `docs/PROJECT.md`: the naming-decision Up Next row is replaced by a row for the first crates.io publish of `lok-backend`, listing CLO-635 and the missing token secret, and noting that the crate `description` and README get revisited then. Recently Completed gains an untracked row (task id `-`) for this rename. `**Last Updated**` names it.
  - `docs/ROADMAP.md`: the Phase 13 intro names `lok-backend` and says the publish waits on CLO-635. No table row or Summary count changes. `**Last Updated**` names it.
- [ ] **AC10 Agent and prompt docs.** `.github/copilot-instructions.md`, `.pi/agents/{claude-designer,codex-pre-pr,ollama-rust-reviewer,ops-reviewer,security-reviewer}.md`, `.pi/extensions/orchestrate/README.md`, `.pi/IMPLEMENTATION_SUMMARY.md`, `.lok/prompts/design-draft-prompt.md` and `.lok/workflows/pre-pr-validation.toml:97` describe a single crate `lok-backend` that produces one binary, `lok`. `ops-reviewer.md` checks for the `lok` binary only and keeps its warning that `cargo install lokomotiv` installs the upstream crate.
- [ ] **AC11 Build and tests.** Evaluation #4 and #5 exit 0 with zero failed tests. The test count is not a gate. The evidence names `backend_public_api`, `cli_output`, `project_config_trust` and the `logger_filter` and `render_record` tests as passing.
- [ ] **AC12 Docs and packaging.** Evaluation #6, #7 and #8 pass. Nothing is uploaded to crates.io.
- [ ] **AC13 Consumers before merge.** Evaluation #9 and #10 pass: the README example compiles against the worktree both directly and through the `package = "lok-backend"` alias, and a scratch install holds exactly `bin/lok`.
- [ ] **AC14 Consumers after release.** After the user pushes `v20260916.0.0`, Evaluation #13 and #14 pass against the real tag.
- [ ] **AC15 Living-doc grep.** Evaluation #11 returns only classified hits, and #12 lists only allowed paths.

## 3. Constraints

**Must**:
- Record the Evaluation #6 baseline before the first edit to `src/`.
- Leave `version` to `make release`. Never run `make release`, push a tag, or publish; the user does the release.
- Run install checks with a scratch `--root`. Never write to `~/.cargo` or `/usr/local/bin`.
- Keep the source rename mechanical: path and string changes only, no refactoring, no import reordering beyond `cargo fmt`.
- Replace text in place in living docs. In the dated decision record, add a section instead of editing old ones.
- New text uses regular hyphens, never em dashes. Do not rewrite existing em dashes in lines this change does not otherwise touch.
- Use absolute dates and versions.
- Rebase on `origin/main` before opening the PR. CLO-623 is in progress and touches `.pi/scripts` and docs; if it merges first, re-run Evaluation #11 and #12.

**Prefer**:
- One commit per sub-task in section 4.
- The repository's PR flow: pre-PR validation, then `/pr:create`.

**Escalate** (stop and ask):
- A `lokomotiv` hit in a living file that is a command, a path or a filter rather than prose, and is not listed in section 1.
- `lok-backend` appears on crates.io under another owner before merge.

## 4. Decomposition

1. **Manifest, source and tests.** Record the Evaluation #6 baseline, then make AC1-AC3. In `src/lib.rs` this includes the `use lokomotiv::` line of the `no_run` quick-start example, because `cargo test` compiles doctests; the toml snippet and prose wait for sub-task 3. Run #3-#5. Files: `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/engine.rs`, `src/backend/mod.rs`, `src/bin/silence_probe.rs`, `src/lib.rs` (code paths and doctest imports), `tests/backend_public_api.rs`
2. **CI and release.** AC4. Files: `.github/workflows/ci.yml`, `.github/workflows/release.yml`
3. **Consumer docs.** AC5, AC6. Run #2 and #6-#10. #2 cannot pass before this sub-task, since `src/lib.rs` prose still names `lokomotiv`. Files: `README.md`, `src/lib.rs` (docs)
4. **Records, aggregation and prompts.** AC7-AC10. Run #11 and #12. Files: `docs/DEPENDENCIES.md`, `docs/decisions/clo-592-workspace-split.md`, `docs/adrs/clo-589-backend-library-shape.md`, `docs/PROJECT.md`, `docs/ROADMAP.md`, `.github/copilot-instructions.md`, the five `.pi/agents` files, `.pi/extensions/orchestrate/README.md`, `.pi/IMPLEMENTATION_SUMMARY.md`, `.lok/prompts/design-draft-prompt.md`, `.lok/workflows/pre-pr-validation.toml`
5. **After merge.** The user runs `make release VERSION=20260916.0.0`, then #13 and #14 run.

**Dependency order**: sub-task 1 before 3, because the doc examples compile against the renamed lib. Sub-tasks 2 and 4 are independent of both. Sub-task 5 runs after merge.

## 5. Evaluation

Commands run from the worktree root. `WT` is the worktree's absolute path, `EV` an evidence directory in the session scratchpad, and `UA='User-Agent: lok (github.com/maxkulish/lok)'`.

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Name still free | `404` | `curl -s -o /dev/null -w '%{http_code}' -H "$UA" https://crates.io/api/v1/crates/lok-backend` |
| 2 | No stray name in code (AC1, AC2) | No output, exit 1 | `rg -n lokomotiv src tests Cargo.toml Cargo.lock` |
| 3 | Log strings (AC3) | The const in `src/backend/mod.rs`, the filter format in `src/main.rs` and the `logger_filter` test assertions, nothing else | `rg -n 'lok_backend::retry\|off,lok_backend=' src` |
| 4 | Format and lint, as CI runs them (AC11) | exit 0 each | `cargo fmt -- --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo clippy --locked --all-targets --features bedrock -- -D warnings`; `cargo clippy --locked --lib --tests --no-default-features -- -D warnings` (`ci.yml:46`, `:54`, `:275`) |
| 5 | Tests, as CI runs them (AC11) | `0 failed` in every summary | `cargo test --locked 2>&1 \| rg '^test result\|^     Running\|Doc-tests'`; `cargo test --locked --features bedrock 2>&1 \| rg '^test result'`; `cargo test --locked --lib --no-default-features 2>&1 \| rg '^test result'` (`ci.yml:49`, `:57`, `:272`) |
| 6 | rustdoc warnings unchanged (AC12) | Empty `diff` | Script 6 |
| 7 | Public API documented (AC12) | exit 0 | `RUSTDOCFLAGS='-D missing_docs' cargo doc --locked --no-deps --lib --all-features` |
| 8 | Packaging (AC12) | exit 0, output names `lok-backend` | `cargo publish --dry-run --locked 2>&1 \| tail -5`, on a committed tree |
| 9 | Installed binaries (AC13) | `lok` only | `root=$(mktemp -d); cargo install --locked --path . --root "$root"; ls "$root/bin"` |
| 10 | Consumer examples, path (AC13) | `direct exit=0`, `alias exit=0` | Script 10 |
| 11 | Living-doc grep (AC15) | Every hit classified (a)-(f), discovery lists nothing | Script 11 |
| 12 | Change set (AC15) | Only files named in section 4, this spec and its plan `docs/plans/2026-09-16-lok-backend-rename.md` | `git diff --name-only "$(git merge-base origin/main HEAD)"` |
| 13 | Release tag exists (AC14) | Tag listed | `git ls-remote --tags origin v20260916.0.0` |
| 14 | Consumer and install, git (AC14) | `git exit=0`, `bin/` holds `lok` only, `lok 20260916.0.0` | Script 14 |

### Script 6: rustdoc warnings, compared by message

```bash
# Before the first edit to src/:
cargo doc --locked --no-deps --lib --all-features 2>&1 | rg '^warning: ' | rg -v 'generated [0-9]+ warning' | sort > "$EV/doc-before.txt"
# After sub-task 3:
cargo doc --locked --no-deps --lib --all-features 2>&1 | rg '^warning: ' | rg -v 'generated [0-9]+ warning' | sort > "$EV/doc-after.txt"
diff <(sed 's/lokomotiv/lok_backend/g' "$EV/doc-before.txt") "$EV/doc-after.txt"
```

### Script 10: README example as a downstream crate

```bash
S=$(mktemp -d)
cargo new --quiet "$S/direct"
cargo new --quiet "$S/alias"
# The first rust code block after the library-section heading in README.md
awk '/^## Using lok-backend as a library/{f=1} f&&/^```rust/{p=1;next} p&&/^```/{exit} p' README.md > "$S/direct/src/main.rs"
sed 's/lok_backend::/lokomotiv::/g' "$S/direct/src/main.rs" > "$S/alias/src/main.rs"
printf 'lok-backend = { path = "%s", default-features = false }\ntokio = { version = "1", features = ["macros", "rt-multi-thread"] }\n' "$WT" >> "$S/direct/Cargo.toml"
printf 'lokomotiv = { package = "lok-backend", path = "%s", default-features = false }\ntokio = { version = "1", features = ["macros", "rt-multi-thread"] }\n' "$WT" >> "$S/alias/Cargo.toml"
cargo check --quiet --manifest-path "$S/direct/Cargo.toml"; echo "direct exit=$?"
cargo check --quiet --manifest-path "$S/alias/Cargo.toml"; echo "alias exit=$?"
```

### Script 11: living-doc grep

```bash
LIVING=(README.md src/lib.rs Cargo.toml .github/copilot-instructions.md .github/workflows/ci.yml
  .github/workflows/release.yml .lok/prompts/design-draft-prompt.md .lok/workflows/pre-pr-validation.toml
  .pi/agents/claude-designer.md .pi/agents/codex-pre-pr.md .pi/agents/ollama-rust-reviewer.md
  .pi/agents/ops-reviewer.md .pi/agents/security-reviewer.md .pi/extensions/orchestrate/README.md
  .pi/IMPLEMENTATION_SUMMARY.md docs/DEPENDENCIES.md docs/PROJECT.md docs/ROADMAP.md)
rg -n lokomotiv "${LIVING[@]}"
# Discovery: any file outside LIVING and the out-of-scope records. A listed path means escalate.
rg -l lokomotiv --hidden -g '!.git' -g '!target' \
  -g '!docs/{reviews,plans,designs,design-docs,prds,lessons,specs,discovery,status}/**' \
  -g '!specs/**' -g '!.pi/lessons/**' -g '!architecture/**' -g '!docs/adrs/**' -g '!docs/decisions/**' . \
  | sed 's|^\./||' | rg -v -x -F -f <(printf '%s\n' "${LIVING[@]}")
```

Hit classes: (a) facts about the upstream `lokomotiv` crate, (b) the `package = "lok-backend"` migration alias and its explanation, (c) the README etymology line, (d) Linear task titles in aggregation tables, (e) the `lokomotiv::retry` migration note, (f) the `ci.yml` absence assertion. `docs/decisions/clo-592-workspace-split.md` and `docs/adrs/**` are checked by reading their diff: only the added section and the trigger line change.

### Script 14: after the release

```bash
S=$(mktemp -d)
cargo new --quiet "$S/git"
awk '/^## Using lok-backend as a library/{f=1} f&&/^```rust/{p=1;next} p&&/^```/{exit} p' README.md > "$S/git/src/main.rs"
awk '/^## Using lok-backend as a library/{f=1} f&&/^```toml/{p=1;next} p&&/^```/{exit} p' README.md | rg -v '^\[dependencies\]' >> "$S/git/Cargo.toml"
cargo check --quiet --manifest-path "$S/git/Cargo.toml"; echo "git exit=$?"
cargo install --locked --git https://github.com/maxkulish/lok --tag v20260916.0.0 lok-backend --root "$S/root"
ls "$S/root/bin"; "$S/root/bin/lok" --version
```
