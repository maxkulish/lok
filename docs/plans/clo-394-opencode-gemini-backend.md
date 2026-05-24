# Plan: CLO-394 FR-12a: Replace Gemini CLI backend with opencode subprocess

## Context
- Design: docs/designs/clo-394-opencode-gemini-backend.md
- Discovery: docs/discovery/clo-394.md
- PRD: docs/prds/clo-394-opencode-migration.md
- Linear: https://linear.app/cloud-ai/issue/CLO-394/fr-12a-replace-gemini-cli-backend-with-opencode-subprocess

## Sub-tasks

### ST1 Capture canonical opencode fixtures and lock parser input expectations
**Files:** tests/fixtures/gemini/
**Acceptance:** `cargo test --test gemini_fixtures` (passes with `malformed.json` malformed and all other fixture JSON valid)
**Estimate:** M

- Replace/add opencode-shaped fixtures in `tests/fixtures/gemini/` so parser shape is evidence-driven.
- Preserve scrubbed/sensitive-data guarantees in `tests/gemini_fixtures.rs`.
- Keep compatibility fixture names for existing envelope tests (`success-no-stats.json`, `success-with-stats.json`) if reused by the new parser path.
- Decide/record exact opencode shape assumptions in the parser doc comments before implementation.

### ST2 Implement Command-based Gemini backend execution path
**Files:** src/backend/gemini.rs
**Acceptance:** `cargo test --bin lok --test gemini_build_argv_`
**Estimate:** L

- Replace shell-string construction with `tokio::process::Command::new("opencode")` + `.args()` using a new internal `build_argv(...)` helper.
- Map `SandboxMode` to opencode `--agent` flags and default-to-build behavior for `None`/`WorkspaceWrite`.
- Normalize model into `google/<id>` only when no provider prefix exists; do not double-prefix provider-qualified values.
- Remove `skip_lines`-based line-skipping behavior from query execution flow.
- Return existing `BackendError::Unavailable` / `ExecutionFailed` semantics on spawn/exit failures.

### ST3 Implement tolerant parser fallback
**Files:** src/backend/gemini.rs
**Acceptance:** `cargo test --bin lok --test gemini_parse_backend_output`
**Estimate:** M

- Introduce/finish `parse_backend_output(stdout: &str) -> (String, Option<TokenUsage>)`.
- Try opencode JSON/NDJSON extraction first, including usage extraction fallback to prompt/completion/cached/reasoning fields when present.
- Keep legacy `GeminiEnvelope` parsing fallback for users with custom `command = "npx"` configs so pinned legacy CLI output still works.
- On parse failure, return `(stdout.to_string(), None)` to preserve user-visible output.

### ST4 Update runtime defaults and diagnostics
**Files:** src/config.rs, src/main.rs, src/backend/context.rs
**Acceptance:** `cargo test --bin lok --test default_gemini_config_uses_opencode && cargo run --bin lok -- doctor`
**Estimate:** S

- Update `Config::default` gemini backend to `command = Some("opencode")`, `args = ["run", "--format", "json"]`, `default_model = Some("google/gemini-2.5-flash")`, `skip_lines = 0`.
- Keep field names/paths stable for backward compatibility (backend key and TOML shape).
- Replace `lok doctor` gemini checks with `opencode` entry and install hint.
- Remove hard `GOOGLE_API_KEY` requirement from doctor key checks.
- Update `SandboxMode` comment in `src/backend/context.rs` to document opencode agent routing semantics.

### ST5 Add/refresh test matrix and close-loop validation
**Files:** src/backend/gemini.rs, tests/gemini_fixtures.rs, tests/fixtures/gemini/
**Acceptance:** `cargo test` (or at minimum `cargo test --bin lok -- --test-threads=1 && cargo test --test gemini_fixtures`)
**Estimate:** M

- Add/adjust unit tests for:
  - argv construction (`run --format json`, `--`, prompt positioning)
  - sandbox mapping
  - apply-edits warnings/behavior
  - model prefixing/non-prefixing
  - legacy envelope + opencode parsing + malformed fallback
- Ensure all backend unit tests + fixture tests are green together.

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## Risks
- CLI output shape may differ from fixture assumptions and force a parser adjustment before broad implementation; this is expected and handled by ST1.
- If `opencode run` requires explicit `stdin(Stdio::null())`, add it in ST2 without restoring shell-based execution.
- Removing `GOOGLE_API_KEY` check in doctor may reduce onboarding cues for env-var-only users, so docs/comments should call out `opencode auth login` as primary path.
