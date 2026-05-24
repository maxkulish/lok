# Design: CLO-394 - FR-12a: Replace Gemini CLI backend with opencode subprocess

## Problem

Lok users and automation pipelines that select the `gemini` backend - `lok ask`, `lok run`, the `audit` task in `Config::default`, and rs-wisper review/fix flows - currently shell out to deprecated `@google/gemini-cli` via `sh -c "echo '' | npx @google/gemini-cli --output-format json ... '<prompt>'"`. Google has marked the Gemini CLI as deprecated; the backend's command construction (`src/backend/gemini.rs:142-173`), default config (`src/config.rs:410-427`), doctor checks (`src/main.rs:715-747`), and JSON envelope parser (`GeminiEnvelope`) are all keyed to the old binary and its `stats.models.<model>.tokens` shape. Discovery scored this 6/10: the shape of the migration is clear, but the existing shell-string execution path is incompatible with opencode's positional-prompt + JSON event semantics, so the work is more than a token swap. The migration must land before the deprecated CLI loses upstream maintenance and breaks existing `gemini` workflows.

## Goals / Non-goals

**Goals**

- Preserve the backend key `gemini`, the `Backend` trait surface, and the `BackendConfig` schema so existing TOML configurations and step definitions keep working without edits.
- Replace shell-string execution in `GeminiBackend::query` with a direct `tokio::process::Command::new("opencode")` argv invocation that mirrors the pattern used in `src/backend/codex.rs`.
- Map `SandboxMode` -> opencode `--agent` flags: `ReadOnly` -> `--agent plan`, `WorkspaceWrite` -> `--agent build`, `DangerFullAccess` -> `--agent build --dangerously-skip-permissions`. Keep the existing `apply_edits` -> `WorkspaceWrite` defaulting rule.
- Parse opencode JSON output into `QueryOutput.stdout` (final response text) and `QueryOutput.usage` (`TokenUsage`), tolerating event-shape drift via a successful-text fallback that mirrors the current parser's behavior.
- Update `Config::default` Gemini entry to `command = Some("opencode")`, `args = ["run", "--format", "json"]`, drop `skip_lines`, keep the 600 s timeout, default `model = Some("google/gemini-2.5-flash")`.
- Replace doctor check `("gemini", "npx", ...)` with `("gemini", "opencode", "brew install anomalyco/tap/opencode  |  curl -fsSL https://opencode.ai/install | bash")` and drop the strict `GOOGLE_API_KEY` requirement (opencode handles auth via `opencode auth login`).
- Update the `// Maps to Codex -s and Gemini --approval-mode` comment in `src/backend/context.rs:100` to describe opencode agent routing.

**Non-goals**

- No long-lived opencode daemon, persistent attach mode, or session resumption.
- No migration to an opencode SDK crate; subprocess invocation only.
- No changes to user-authored workflow `.toml` files, the `Backend` trait signature, or `QueryOutput` / `TokenUsage` shape.
- No new dependencies in `Cargo.toml` - reuse `tokio::process::Command`, `serde`, `serde_json`, `which`, and the existing `async_trait` stack.
- No re-architecting of other backends (Claude, Codex, Ollama, Bedrock) and no shared "subprocess backend" abstraction in this task.
- No streaming-event consumer (Approach C); the design parses aggregated stdout only.

## Architecture

All changes are localized to four existing files; no new modules are introduced.

```
                  +-------------------------+
StepContext  -->  | GeminiBackend::query    |   src/backend/gemini.rs
                  |  - build_argv()         |
                  |  - run Command          |
                  |  - parse output         |
                  +-----------+-------------+
                              |
                              v
                  +-------------------------+
                  | tokio::process::Command |
                  |  "opencode"             |
                  |  run --format json      |
                  |  --model google/<M>     |
                  |  --agent <plan|build>   |
                  |  [--dangerously-...]    |
                  |  -- <prompt>            |
                  +-----------+-------------+
                              |
                stdout (JSON) v
                  +-------------------------+
                  | parse_opencode_output() |
                  |  -> (text, usage?)      |
                  | fallback: raw stdout    |
                  +-----------+-------------+
                              |
                              v
                       QueryOutput { stdout, stderr, exit_code,
                                     model, duration, usage, ... }
```

**Data flow**

1. `GeminiBackend::query(ctx: StepContext)` resolves the effective model (`ctx.model` -> `self.default_model` -> `"google/gemini-2.5-flash"` from defaults) and normalizes it for opencode: if the model already contains a provider prefix (`provider/model`) pass it through unchanged; otherwise prefix with `google/`.
2. `GeminiBackend::query(ctx: StepContext)` resolves the effective sandbox with the existing `apply_edits=true + sandbox=None => WorkspaceWrite` rule, then applies opencode's default routing explicitly: `None` and `WorkspaceWrite` both emit `--agent build`; `ReadOnly` emits `--agent plan`; `DangerFullAccess` emits `--agent build --dangerously-skip-permissions`.
3. `build_argv(&self.args, model, sandbox, apply_edits, prompt)` returns `Vec<String>` containing the opencode subcommand, format flag, `--model <provider/model>`, `--agent <plan|build>`, optional `--dangerously-skip-permissions`, then `--`, then the positional `prompt`. No shell quoting; argv is passed directly to `Command::args`.
3. `Command::new(&self.command)` with `.args(&argv)`, `.current_dir(ctx.cwd)`, `.kill_on_drop(true)`, `.stdout(Stdio::piped())`, `.stderr(Stdio::piped())`. No `sh -c`, no `echo '' |` stdin pipe (opencode does not require stdin).
4. On non-zero exit, return `BackendError::ExecutionFailed { message, exit_code }`. On spawn failure, return `BackendError::Unavailable`. These mirror the existing error mapping.
5. `parse_backend_output(stdout: &str) -> (String, Option<TokenUsage>)` tries opencode JSON/NDJSON extraction first. If that fails, it tries the existing Gemini CLI envelope shape as a compatibility fallback for users who pinned `command = "npx"` in custom config. If both parsers fail, it returns `(stdout.to_string(), None)` so the user still sees raw output instead of a hard parse failure.
6. Assemble `QueryOutput::from_process(response_text, stderr_str, exit_code, "gemini", elapsed).with_model(effective_model).with_usage(usage)` - identical to the current return path.

**Types**

- `GeminiBackend` struct in `src/backend/gemini.rs` keeps fields `command: String`, `args: Vec<String>`, `default_model: Option<String>`. The `skip_lines: usize` field is removed (no longer meaningful for opencode output).
- A new private `OpencodeOutput`/event deserialization helper is added next to the existing Gemini CLI envelope parser. Exact opencode field set is TBD pending fixture capture (see Open questions); both helpers live in `src/backend/gemini.rs` and are `pub(crate)` only so parser tests can exercise them without widening the public backend API.
- `SandboxMode` in `src/backend/context.rs` is unchanged; only its doc comment is updated to reference opencode agent routing.

**Out-of-scope source paths** (intentionally untouched): `src/backend/mod.rs`, `src/backend/claude.rs`, `src/backend/codex.rs`, `src/backend/ollama.rs`, `src/backend/bedrock.rs`, `src/workflow.rs`, `src/conductor.rs`, `src/apply_verify/*`.

## Public API surface

The `Backend` trait, `QueryOutput`, `TokenUsage`, `BackendError`, `StepContext`, and `BackendConfig` schemas are unchanged. The only public-ish surface that changes is the `GeminiBackend` struct internals and one doc comment.

**Unchanged trait (for reference)**

```rust
#[async_trait]
impl super::Backend for GeminiBackend {
    fn name(&self) -> &str { "gemini" }

    async fn query(
        &self,
        ctx: super::StepContext<'_>,
    ) -> std::result::Result<super::QueryOutput, super::BackendError>;

    fn is_available(&self) -> bool;

    async fn health_check(&self) -> std::result::Result<super::HealthStatus, super::BackendError>;
}
```

**Changed: `GeminiBackend` struct and helpers** (`src/backend/gemini.rs`)

Before:

```rust
pub struct GeminiBackend {
    command: String,
    args: Vec<String>,
    skip_lines: usize,
    default_model: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct GeminiEnvelope {
    response: String,
    #[serde(default)]
    stats: Option<serde_json::Value>,
}

fn build_shell_cmd(
    command: &str,
    args: &[String],
    model: Option<&str>,
    sandbox: Option<super::SandboxMode>,
    apply_edits: bool,
    prompt: &str,
) -> String;

pub(crate) fn parse_gemini_envelope(stdout: &str) -> Option<GeminiEnvelope>;
pub(crate) fn envelope_to_usage(stats: Option<serde_json::Value>) -> Option<super::TokenUsage>;
```

After:

```rust
pub struct GeminiBackend {
    command: String,
    args: Vec<String>,
    default_model: Option<String>,
}

/// Tolerant view over opencode's `run --format json` stdout. Exact field
/// names confirmed against captured fixtures in tests/fixtures/gemini/.
#[derive(serde::Deserialize)]
pub(crate) struct OpencodeOutput {
    // Field set finalized when capturing fixtures; see Open questions.
}

/// Build the argv vector passed to `opencode`. Centralises sandbox -> agent
/// mapping and model injection so tests exercise the same code path as
/// production. Returns the full argv WITHOUT the leading binary name.
pub(crate) fn build_argv(
    base_args: &[String],
    model: Option<&str>,
    sandbox: Option<super::SandboxMode>,
    apply_edits: bool,
    prompt: &str,
) -> Vec<String>;

/// Parse opencode stdout into (response_text, usage). On any parse failure,
/// returns (stdout.to_string(), None) so the caller still surfaces raw
/// output to the user.
pub(crate) fn parse_backend_output(stdout: &str) -> (String, Option<super::TokenUsage>);
```

The existing `resolve_effective_sandbox`, `Backend` impl shape, `QueryOutput::from_process(...).with_model(...).with_usage(...)` return path, and `BackendError` mapping are preserved exactly.

**Changed: `Config::default` Gemini entry** (`src/config.rs:410-427`)

Before:

```rust
backends.insert(
    "gemini".to_string(),
    BackendConfig {
        enabled: true,
        command: Some("npx".to_string()),
        args: vec![
            "@google/gemini-cli".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
        ],
        skip_lines: 1,
        api_key_env: None,
        model: None,
        timeout: Some(Duration::from_secs(600)),
        max_retries: None,
        retry_delay_ms: None,
    },
);
```

After:

```rust
backends.insert(
    "gemini".to_string(),
    BackendConfig {
        enabled: true,
        command: Some("opencode".to_string()),
        args: vec!["run".to_string(), "--format".to_string(), "json".to_string()],
        skip_lines: 0,
        api_key_env: None,
        model: Some("google/gemini-2.5-flash".to_string()),
        timeout: Some(Duration::from_secs(600)),
        max_retries: None,
        retry_delay_ms: None,
    },
);
```

`skip_lines` is left as `0` because the field still exists in `BackendConfig`; it is simply not consumed by the new `GeminiBackend`. Removing the field from `BackendConfig` is out of scope for this task (would touch all backends).

**Changed: doctor checks** (`src/main.rs:715-747`)

```rust
let checks = vec![
    ("codex", "codex", "npm install -g @openai/codex"),
    (
        "gemini",
        "opencode",
        "brew install anomalyco/tap/opencode  |  curl -fsSL https://opencode.ai/install | bash",
    ),
    (
        "claude",
        "claude",
        "Install Claude Code: https://claude.ai/claude-code",
    ),
];

let keys = vec![
    ("ANTHROPIC_API_KEY", "claude backend"),
    ("AWS_PROFILE", "bedrock backend (or AWS_ACCESS_KEY_ID)"),
];
```

The `GOOGLE_API_KEY` entry is removed; opencode auth status is not surfaced by `lok doctor` in this task (caller should run `opencode auth status` separately).

## Assumptions

| # | Assumption | Confidence | Verification |
|---|---|---|---|
| A1 | `opencode run --format json --model <id> --agent <plan\|build> [--dangerously-skip-permissions] -- <prompt>` is the supported invocation contract for non-interactive runs. | medium | Cross-check against current opencode CLI docs and `opencode run --help` on the target install before implementation merge; lock the chosen argv in `build_argv` unit tests. |
| A2 | opencode writes JSON or NDJSON stdout that includes the final assistant message and token usage fields. | medium | Capture real opencode fixtures before writing the parser, per `.pi/lessons/clo-382-gemini-lessons.md § L1`; fixtures must drive `OpencodeOutput`/event parsing and include a usage-bearing response. |
| A3 | The sandbox mapping `read-only -> plan`, `workspace-write/default -> build`, and `danger-full-access -> build + --dangerously-skip-permissions` matches opencode's agent semantics. | medium | Verify against opencode agent docs and add one argv unit test per mapping; preserve `apply_edits=true` defaulting behavior from `.pi/lessons/clo-383-apply-edits-sandbox-default-lessons.md § L3`. |
| A4 | Direct `Command::new("opencode")` argv invocation is safer than the existing shell-string path and avoids shell escaping regressions. | high | This deliberately removes the class of shell escaping issue recorded in `.pi/lessons/clo-374-sandbox-routing-lessons.md § L1`; add a hostile-prompt argv test proving prompt text is passed as one argv element. |
| A5 | opencode does not require a piped stdin; removing `echo '' | sh -c ...` is safe. | medium | Smoke-test `opencode run --format json -- "ping"` with stdin attached to `/dev/null`; if it hangs, set `stdin(Stdio::null())` explicitly without restoring a shell wrapper. |
| A6 | `google/gemini-2.5-flash` is an accepted default model identifier for opencode's Google provider. | low | Verify with `opencode models list` or a smoke run before merging; if the identifier differs, update `Config::default` and tests before implementation completes. |
| A7 | Backend default argv changes must be mirrored in both `GeminiBackend::new` and `Config::default`. | high | Apply `.pi/lessons/clo-382-gemini-lessons.md § L3`; tests must construct from `Config::default()` and from an empty `BackendConfig` to prove the defaults stay synchronized. |
| A8 | Token usage includes prompt/completion plus optional cached/reasoning fields when opencode emits them. | medium | Fixture tests must assert both optional fields explicitly; mirror complete cached/reasoning assertions per `.pi/lessons/clo-378-tokenusage-extension-lessons.md § L2`. |
| A9 | Auth is delegated to `opencode auth login`; removing the hard `GOOGLE_API_KEY` doctor requirement does not regress env-var-auth users. | medium | Confirm opencode accepts `GOOGLE_API_KEY` or `GEMINI_API_KEY` fallback for `google/*`; document this as optional fallback rather than a `lok doctor` failure. |
| A10 | Existing in-tree workflows and tasks that select `gemini` continue to work with the migrated defaults. | high | Run `cargo test` and one manual `lok run`/`lok ask --backend gemini` smoke on a host with opencode auth. |

## Test plan

**Unit tests** (in `src/backend/gemini.rs`, replacing the current `tests` module)

- `gemini_default_config_uses_opencode_command_and_run_args` - asserts `Config::default` `gemini` backend has `command = Some("opencode")`, `args = ["run", "--format", "json"]`.
- `gemini_build_argv_includes_format_json` - asserts `["run", "--format", "json", ...]` is a prefix of the argv.
- `gemini_build_argv_passes_prompt_as_positional_after_dash_dash` - asserts the last two argv elements are `"--", "<prompt>"`.
- `gemini_build_argv_no_shell_metacharacter_escaping` - passes a prompt containing `'`, `"`, `$`, `;`, and confirms it appears verbatim in argv (no shell quoting).
- `gemini_sandbox_read_only_maps_to_agent_plan` - asserts `--agent plan` appears, `--dangerously-skip-permissions` does not.
- `gemini_sandbox_workspace_write_maps_to_agent_build` - asserts `--agent build` appears, `--dangerously-skip-permissions` does not.
- `gemini_sandbox_danger_full_access_maps_to_agent_build_plus_skip_permissions` - asserts both flags appear.
- `gemini_sandbox_none_defaults_to_agent_build` - asserts `--agent build` appears because lok's migrated Gemini path defaults to opencode's build agent.
- `gemini_apply_edits_true_no_sandbox_defaults_to_agent_build` - mirrors current `gemini_apply_edits_true_no_sandbox_emits_auto_edit`.
- `gemini_apply_edits_true_read_only_preserved_with_warning` - mirrors current `gemini_apply_edits_true_explicit_plan_preserved`.
- `gemini_model_flag_preserves_provider_prefixed_models` - asserts `--model google/gemini-2.5-flash` for the default config and no double-prefixing when `ctx.model = "google/gemini-3-pro-preview"`.
- `gemini_model_flag_prefixes_bare_gemini_models` - asserts `ctx.model = "gemini-2.5-flash"` becomes `--model google/gemini-2.5-flash`.
- `gemini_parse_backend_output_extracts_opencode_response_text` - fixture-driven.
- `gemini_parse_backend_output_extracts_opencode_usage_when_present` - fixture-driven; asserts `TokenUsage { prompt, completion, total, cached, reasoning }` populated.
- `gemini_parse_backend_output_preserves_legacy_gemini_envelope` - feeds the existing Gemini CLI fixture and asserts response/usage still extract for pinned legacy configs.
- `gemini_parse_backend_output_fallback_on_malformed_json` - asserts `(stdout, None)` so user still sees raw output.
- `gemini_parse_backend_output_handles_missing_usage_gracefully` - asserts `(text, None)`.

The current shell-string tests (`gemini_build_shell_cmd_*`, `gemini_sandbox_*_approval_mode`, `gemini_parse_envelope_*`) are deleted in the same change - they assert behavior that no longer exists.

**Integration tests** (`tests/gemini_fixtures.rs` + `tests/fixtures/gemini/`)

- Add `tests/fixtures/gemini/opencode-success.json` (minimal response, no usage).
- Add `tests/fixtures/gemini/opencode-success-with-usage.json` (response + usage object).
- Keep `tests/fixtures/gemini/malformed.json` and extend the fallback test to confirm `parse_backend_output` returns `(raw_stdout, None)` instead of panicking.
- Replace `tests/fixtures/gemini/success-no-stats.json` and `success-with-stats.json` content with opencode-shaped JSON; keep file names if the corpus size cap (`MAX_CORPUS_BYTES = 50_000` in `tests/gemini_fixtures.rs`) is preserved.
- `every_fixture_is_valid_json_or_known_malformed` and `fixtures_under_size_cap` continue to pass unchanged.
- Add `tests/gemini_argv.rs` (or extend `tests/gemini_fixtures.rs`) with one integration test that constructs `GeminiBackend` from `Config::default`, invokes `build_argv` through a small `pub(crate)` shim, and asserts the full argv vector for a representative `StepContext`.

**Per-backend test matrix** (smoke for the `Backend` trait surface, regression guard for sibling backends)

| Backend | Tests touched in this task | Expectation |
|---|---|---|
| `claude` | none | green |
| `codex` | none | green |
| `gemini` | full replacement (above) | green |
| `ollama` | none | green |
| `bedrock` (feature) | none | green |

**Manual verification** (executed on a host with `opencode` installed and `opencode auth login` completed)

1. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` - pre-merge gate.
2. `lok doctor` - confirm `gemini` row shows `ready` when `opencode` is on PATH, and the install hint matches the PRD strings.
3. `lok ask --backend gemini "What is 2+2?"` - confirm a non-empty response and a populated `usage` line in any verbose output.
4. `lok run` with a workflow that includes a `gemini` step under `sandbox = "workspace-write"` and `apply_edits = true` - confirm the step completes and produces a `StepResult` with `usage` populated.
5. `lok run` with `sandbox = "danger-full-access"` - confirm `--dangerously-skip-permissions` appears in any captured argv logging and the step completes.
6. Run an existing `audit` task (`Config::default`, `src/config.rs:483-487`) end-to-end to confirm no regression for the in-tree workflow that selects `gemini`.

## Migration / rollout

The change is **backward-compatible at the configuration layer** and **breaking at the execution layer for users who relied on the old default command**:

- TOML schema is unchanged. Workflows referencing `backend = "gemini"` continue to load and resolve.
- Users with a custom `BackendConfig` for `gemini` that explicitly sets `command = "npx"` and `args = ["@google/gemini-cli", ...]` will continue to invoke the legacy CLI because user overrides take precedence over lok defaults. The parser keeps the current Gemini-envelope fallback so these pinned users still receive extracted response/usage while they migrate on their own schedule.
- Users on the default config will silently start invoking `opencode` after upgrade. If `opencode` is not on PATH, queries fail with `BackendError::Unavailable { message: "Failed to execute gemini command: ..." }`; `lok doctor` surfaces this with the new install hint.
- `GOOGLE_API_KEY` is no longer checked by `lok doctor`. Users who had it set keep it set; opencode picks it up as an env fallback (assumption above). No env-var migration is required.
- No feature flag is added. The migration is direct: one PR replaces the gemini path; the deprecation timeline does not warrant maintaining two code paths.
- No database migrations, no on-disk format changes, no API consumers to coordinate with.
- Rollout order: (1) merge this PR behind the standard pre-merge gate; (2) update `README.md` and any setup docs in the same PR to point at `opencode auth login`; (3) cut a normal lok release; (4) downstream tasks `CLO-395` and `CLO-396` (blocked-by relations in the workflow YAML) unblock automatically.

## Open questions

All design-time questions have an implementation or follow-up disposition:

- **Exact opencode JSON output shape** — resolved for planning as implementation ST1: capture real opencode fixtures before parser code and lock `OpencodeOutput`/event fields to the observed shape.
- **Single JSON document vs. NDJSON event stream** — resolved for planning as part of ST1/ST2: parser must support whichever shape the captured fixture shows; `parse_backend_output(&str)` remains broad enough for both.
- **Correct default model identifier** — resolved for planning as implementation verification: confirm `google/gemini-2.5-flash` by `opencode models list` or a smoke run before changing `Config::default`.
- **stdin handling** — resolved for planning as manual verification: smoke-test `opencode run --format json -- "ping"`; if it hangs, use `stdin(Stdio::null())` without returning to `sh -c`.
- **Auth surfacing in `lok doctor`** — resolved: follow PRD FR-12a.4 and remove hard `GOOGLE_API_KEY` checking; docs can mention env-var auth as optional fallback.
- **Retiring `skip_lines` from `BackendConfig`** — deferred out of scope. Keep `skip_lines = 0` in config defaults and ignore it in `GeminiBackend`; schema cleanup can be a later dedicated config-migration issue if needed.
