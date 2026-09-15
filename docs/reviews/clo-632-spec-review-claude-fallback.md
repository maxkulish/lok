# Spec Review: clo-632

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-09-15
**Pipeline**: lok spec-review
**Note**: The external reviewer failed; this is the fallback review

---

I checked the spec's claims against the code and am writing up the review now.

# Spec Review: CLO-632 Project-config trust boundary

I checked the spec's claims against the worktree. Almost all of them hold:
- `load_config_from_paths` is at `src/config.rs:533-564`, and the only caller of `load_config` is `src/main.rs:498`.
- `deep_merge` replaces arrays and scalars rather than merging them.
- `BackendConfig` has no `endpoint` field and no serde aliases, so the key names are fixed.
- Health checks run `which` and then `--version` (`src/backend/codex.rs:355`). `which` resolves `./scripts/...` against the current directory, so the attack works as described.
- The existing `test_load_config_from_paths_*` tests only set non-gated keys in the project file, so AC5's "unchanged" promise holds.
- The rollout note is right that only `loker/lok.toml` breaks. The other local repos keep their config at `.lok/lok.toml`, which lok never loads.

## 1. Problem Statement

This section is strong. It gives the attack as a concrete file, traces the call chain (config loads before dispatch, then warmup, then the health check spawns the binary) and compares it to direnv and git config. The key audit is mostly correct. Three gaps:

- **The `model` reasoning only covers shell injection, not flag injection.** "Never through a shell" does not rule out a value starting with `-` being read as a flag by the backend CLI. In practice the risk is small:
  - gemini adds a `google/` prefix to any model without a `/` (`gemini.rs:82-88`) and puts the prompt after `--`.
  - codex uses clap, which should reject a value starting with a dash after `--model`.
  
  The audit should say this in one line so a later reader does not reopen the question.
- **The `enabled` entry misses Bedrock.** In a `--features bedrock` build, a project can add `[backends.bedrock] enabled = true`. That spends the user's AWS credentials, because `BedrockBackend::new` reads only `model` (`bedrock.rs:102-116`). This is cost, not exfiltration, but it should be listed as an accepted risk.
- **Project workflows are not covered.** `find_workflow` checks `.lok/workflows/<name>.toml` before the global and built-in workflows (`workflow.rs:3642-3646`). The built-in `hunt.toml`, `audit.toml` and similar files even tell users to override them there. So a cloned repo can still replace any workflow the user runs by name with arbitrary `shell` steps. That is the same authority `defaults.command_wrapper` grants, reached another way.
  - Gating `command_wrapper` is still right.
  - But the docs update (AC8) must not suggest that the project layer can no longer run commands.
  - The spec should name this as a known gap outside the boundary.

## 2. Acceptance Criteria

The criteria are specific and testable, and AC1 (report every violation, say how to fix it) is well written. Issues:

- **AC6 can pass without testing anything.** It only asserts "non-zero exit and no marker file". If `ask` failed for some unrelated reason before warmup, the test would still pass.
  - Add a positive control: run the same hostile file through `--config` and assert that the marker file **does** appear. That proves the harness really reaches the spawn step.
  - It also depends on setting `current_dir` to the temp directory, because `which` resolves the relative path against the current directory.
- **AC6 could make real, paid backend calls if the gate ever breaks.** The test keeps the inherited `PATH` and runs `ask "hi"`. On a developer machine with `claude` and `opencode` installed, a regression would send real queries.
  - Put `enabled = false` for gemini, claude and ollama in the test's `lok.toml`. `enabled` is not gated, so the file stays valid.
  - Or point `PATH` at an empty directory.
- **Evaluation row 12 will not run as written.** `HOME=$(mktemp -d) cargo run -- backends` gives cargo's rustup proxy an empty home, so it cannot find `~/.rustup` or `~/.cargo` and the toolchain fails to resolve. Use `cargo build && HOME=$(mktemp -d) ./target/debug/lok backends` instead.
- **Nothing states that a rejected file blocks every subcommand.** Config loads before dispatch (`main.rs:498`), so a rejected project file breaks `lok backends`, `lok doctor` and `lok init` in that directory, not just `ask`. That is the intended fail-closed behaviour, but an AC or the rollout note should say it.
- **The "pass `--config`" fix hint is vague.** `--config` skips the user config as well as the project file (`config.rs:543-548`). The hint should say which file to pass (for example `--config ~/.config/lok/lok.toml`), or state that passing the project file means fully trusting it.

## 3. Constraints & Assumptions

These are tight and match the code:
- Presence is detected from the raw `toml::Value`, while values are compared after typed parsing.
- Hostile values are printed through `{:?}`.
- The minimum Rust version and feature-gating rules follow `tests/cli_output.rs`.
- The gate stays in the binary.
- The escalation triggers are the right ones.

Two points:

- **The "accepted cost" of clause (a) is a security downgrade, not only a lost setting.** A project can reset `command` to a bare `codex` (looked up on `PATH`) even when the user pinned an absolute path. It can also reset `args` and so drop any hardening `-c` flags the user added. lok always adds `-s <mode>` itself (`codex.rs:218-219`), which limits the worst sandbox case, but the rest of the downgrade is real.
  - There is an alternative that keeps committed `lok init` files loading with no downgrade. When a project gated value equals the built-in default but differs from what the user config resolved, keep the user's value and skip that key. That does not break the "no silent drop of rejected keys" rule, because the key is not rejected.
  - This changes a decision you already made with the user (HITL), so take it back to them rather than switching quietly.
  - If adopted, Evaluation row 6 would expect `/opt/bin/codex` instead of `codex`.
- **Forward-looking guard.** Session note F3 (`.lok/lok.toml` is never loaded) is correctly out of scope. Add one line saying any future project config location must go through the same check, so a later F3 fix does not bypass the gate.

## 4. Decomposition / Phases

- Four sub-tasks with a correct dependency order and file lists that match the "M" size.
- Sub-task 1's pure-function design is good for testing.
- **Test count does not match the stated rule.** The Evaluation table lists 9 new unit tests for AC1-AC5, but the Prefer rule says "one focused test per acceptance criterion". Rows 1-4 (one gated key each) fit naturally into one table-driven test. Pick one approach and make the spec consistent.
- **One doc example is wrong.** Sub-task 4 marks a "bedrock `api_key_env`" example as user-config-only, but Bedrock never reads `api_key_env`. Check that example before re-labelling it. The fix itself stays out of scope.

## 5. Risks & Open Questions

| # | Risk / question | Severity |
|---|---|---|
| R1 | AC6 can pass without testing anything, and can reach real paid backends if the gate breaks | Medium |
| R2 | Clause (a) lets a project undo a user's hardening (pinned path, extra args) | Medium (needs a HITL call) |
| R3 | Project `.lok/workflows/` can still replace workflows with shell steps, so the docs could overstate the boundary | Medium (docs accuracy plus a follow-up ticket) |
| R4 | Evaluation row 12 breaks cargo/rustup | Low |
| R5 | `model` flag injection and Bedrock `enabled` cost are not written into the audit | Low |
| R6 | A rejected file blocks every subcommand, and the `--config` hint is vague | Low |
| Q1 | Should the error also say that only `./lok.toml` in the exact current directory is checked, not parent directories? That matches today's loader. | Open |

## Verdict

**APPROVE_WITH_SUGGESTIONS**

The boundary, the enforcement point and the key audit are sound, and everything important was confirmed in the code. None of the issues blocks implementation. R1 and R4 are test and verification bugs that are cheap to fix in the spec now. R2 and R3 need a sentence or a user decision before the docs are written.

## Priority Actions

1. **Harden AC6:**
   - Disable the other three backends in the hostile `lok.toml`.
   - Set `current_dir` to the temp directory.
   - Add a `--config` positive control that asserts the marker file is created.
2. **Take clause (a) back to the user:** keep the user's value when the project repeats the built-in default, instead of applying the project's value. If they keep the current rule, rewrite the "accepted cost" to name the downgrade (pinned path back to a `PATH` lookup, hardening args dropped).
3. **Record the `.lok/workflows/` gap:**
   - Add it as out of scope in the Must-not list.
   - Require the AC8 docs to say the boundary covers config keys, not the contents of project workflows.
   - Open a follow-up ticket.
4. **Fix Evaluation row 12:** build first, then run `./target/debug/lok backends` with `HOME` pointed at a temp directory.
5. **Make the error text precise:** say that `--config` also skips the user config, and state that a rejected file blocks all subcommands.
6. **Extend the audit:** add one line each on `model` flag injection (safe per backend, with the reasons above) and Bedrock `enabled` (cost only). Also add the line that future project config locations must go through the gate.
7. **Tidy up:** reconcile the unit-test count with the one-test-per-AC rule, and check the Bedrock `api_key_env` doc example.
