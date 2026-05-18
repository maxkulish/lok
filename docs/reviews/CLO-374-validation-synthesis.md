# CLO-374 Validation Synthesis — Per-Step Sandbox Routing

## Review Sources

- **Codex review** (`CLO-374-codex-validation.md`): completed.
- **Gemini review** (`CLO-374-gemini-validation.md`): did not execute — Gemini CLI refused with
  "current folder is not trusted". Re-run from a trusted workspace (or with `--skip-trust`) when a
  second independent pass is required.

## Findings (initial verdict: FAIL)

### Must Fix

1. **`Config::default()` shipped stale Codex args (`-s read-only`).** Because the runtime always
   instantiates `CodexBackend` from `Config::default()`, the constructor took the "custom args"
   branch — dropping `--ephemeral` and double-specifying `-s` at query time. Functional only by
   accident (Codex resolves last `-s` wins).

2. **Gemini `--approval-mode` values unverified.** Marked `untested` in the workflow YAML; design
   contract required matching `gemini --help` flag names.

### Should Fix

3. **Test/production drift.** Sandbox arg builders existed twice — once in `query()`, once in test
   helpers — guaranteeing the Config::default() regression would slip through.

4. **Re-run Gemini independent review** in a trusted directory.

## Remediation (this PR)

| Finding | Resolution | Evidence |
|---|---|---|
| Must-fix 1 | `Config::default()` Codex args changed to `["exec", "--json", "--ephemeral"]` | `src/config.rs:151-155` |
| Must-fix 1 | Regression test asserts default config produces `--ephemeral` and no pinned `-s` | `src/config.rs::test_codex_default_args_use_ephemeral_not_sandbox` |
| Must-fix 1 | Backend-level regression chains `Config::default()` -> `CodexBackend::new()` -> argv builder, asserts `--ephemeral` present and exactly one `-s` flag | `src/backend/codex.rs::codex_default_config_yields_ephemeral_and_one_sandbox_flag` |
| Must-fix 2 | Ran `gemini --help` — confirmed `--approval-mode` choices: `default, auto_edit, yolo, plan`. Backend already uses these strings (`src/backend/gemini.rs:72-77`). YAML updated to `held`. | `docs/status/clo-374-workflow.yaml:84-86` |
| Should-fix 3 | Extracted `CodexBackend::build_argv_prefix` and `GeminiBackend::build_shell_cmd`. Both `query()` and tests now call the same function; drift between test helper and production logic is no longer possible. | `src/backend/codex.rs:40-72`, `src/backend/gemini.rs::build_shell_cmd` |
| Should-fix 4 | Not done in this pass — Gemini CLI trust-prompt remains an environment issue. Codex review + manual `gemini --help` verification used as the sign-off path. | n/a |

## Test Results

- `cargo test --bin lok` — **489 passed**, 0 failed (was 485 before this remediation: +4 new).
- `cargo fmt --all` — clean.
- `cargo check --bin lok` — clean.

## Verdict (post-remediation)

**PASS**. All must-fix items resolved; the production code path now exercises the same arg
builders the tests cover, so the class of bug Codex caught cannot recur silently. Gemini approval
flags verified against live `--help` output. The independent Gemini review remains skipped — Codex
+ direct CLI verification is the substitute audit trail.
