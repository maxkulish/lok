# Design: CLO-380 - FR-3b: Codex -o/--output-last-message authoritative result extraction

## Problem

Discovery for CLO-380 found that `CodexBackend::query()` still treats Codex JSONL as both the final-answer source and the diagnostic stream. That leaves Lok workflows using `backend = "codex"` vulnerable to result-extraction failures whenever JSONL output is incomplete, missing a final `agent_message`, or otherwise cannot satisfy the strict FR-3a parser, even though Codex CLI v0.119.0+ can write the final message to a dedicated `-o / --output-last-message` file. FR-3b in `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` requires that file to become the authoritative success-text path while JSONL continues to drive `turn.failed` propagation and token usage extraction.

## Goals / Non-goals

**Goals**

- Create one per-invocation temporary last-message file before spawning Codex and pass it as `-o <path>`.
- On process success, prefer non-empty `-o` contents for `QueryOutput.stdout` over JSONL `agent_message` text.
- Preserve JSONL-driven failure detection: `turn.failed` and top-level `error` still return `BackendError::ExecutionFailed`, even if `-o` contains text.
- Preserve JSONL-driven token usage extraction independently of which text source wins.
- Fall back to the existing strict JSONL `agent_message` extraction when the `-o` file is empty, missing, or unreadable.
- Clean up the temporary file on success and error paths using RAII.
- Add hermetic fixture-backed tests for populated `-o`, malformed/incomplete JSONL with populated `-o`, empty/missing `-o` fallback, and `turn.failed` precedence.

**Non-goals**

- Runtime Codex version probing or cached capability detection; this slice always passes `-o` and relies on the existing non-zero process path if an older CLI rejects the flag.
- `--output-schema` integration or schema validation.
- Changing the public `Backend` trait, `StepContext`, `QueryOutput`, `TokenUsage`, or workflow TOML schema.
- Touching Gemini, Claude, Ollama, or Bedrock behavior.
- Preserving debug artifacts under `.lok/debug/`; FR-39 covers that separately.

## Architecture

The implementation stays inside the Codex backend and parser modules.

```
CodexBackend::query(ctx)
  |
  |-- allocate NamedTempFile
  |-- build_argv_prefix(..., Some(tmp.path()))
  |      exec --json --ephemeral -s <mode> [--model <m>] -o <tmp> -- <prompt>
  |-- Command::output().await
  |-- stdout -> codex_event::parse_jsonl_diagnostics(stdout)
  |      usage: Option<TokenUsage>
  |      agent_message: Option<String>
  |      terminal_error: Option<BackendError>   // turn.failed / error
  |      parse_error: Option<BackendError>      // no turn.completed / no agent_message
  |-- if terminal_error: return Err(terminal_error with exit_code)
  |-- last_message = read_last_message(tmp.path())
  |-- text = last_message.or(agent_message).ok_or(parse_error)
  '-- QueryOutput::from_process(text, stderr, exit_code, "codex", duration)
          .with_model(effective_model)
          .with_usage(usage)
```

Concrete module changes:

- `src/backend/codex.rs`
  - Extend private `build_argv_prefix()` with `output_last_message_path: Option<&Path>` and append `-o <path>` when present.
  - Add private `fn create_last_message_file() -> Result<NamedTempFile, BackendError>` using `tempfile::Builder` with a `lok-codex-last-` prefix.
  - Add private async `fn read_last_message(path: &Path) -> Option<String>` that uses `tokio::fs::read_to_string`, returns non-empty UTF-8 text after trimming only trailing `\r`/`\n`, and treats missing/empty/unreadable files as `None`.
  - Hold the `NamedTempFile` binding for the full duration of `query()` so `Drop` removes it on every return path.
  - Resolve precedence in `query()` after process exit: terminal JSONL errors first, non-empty `-o` text second, strict JSONL text fallback third.

- `src/backend/codex_event.rs`
  - Factor the current parser loop into a diagnostics-producing helper, e.g. `parse_jsonl_diagnostics()`.
  - Keep the existing strict `parse_jsonl_stream()` API and semantics by implementing it as a wrapper over diagnostics. Existing FR-3a tests that expect `BackendError::Parse` for missing `agent_message` remain valid.
  - Diagnostics must continue to skip unknown/unparseable lines just like the current parser, but must preserve usage from `turn.completed` even when `agent_message` is missing and a populated `-o` file will provide the result text.

- `tests/fixtures/codex/`
  - Add last-message companion files and README inventory rows for the FR-3b scenarios.
  - The fixture scrub gate must include `.txt` companions without flagging legitimate `*_tokens` JSONL field names.

## Public API surface

No public Rust or TOML API changes are introduced. The public `Backend` trait remains:

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, ctx: StepContext<'_>) -> Result<QueryOutput, BackendError>;
    fn is_available(&self) -> bool;
}
```

The only signature changes are crate-private or private implementation details.

```rust
// src/backend/codex.rs
use std::path::Path;
use tempfile::NamedTempFile;

impl CodexBackend {
    fn build_argv_prefix(
        base_args: &[String],
        sandbox: Option<super::SandboxMode>,
        model: Option<&str>,
        output_last_message_path: Option<&Path>,
    ) -> Vec<String>;

    fn create_last_message_file() -> Result<NamedTempFile, super::BackendError>;

    async fn read_last_message(path: &Path) -> Option<String>;
}
```

```rust
// src/backend/codex_event.rs
#[derive(Debug, Default)]
pub(crate) struct CodexStreamDiagnostics {
    pub agent_message: Option<String>,
    pub usage: Option<TokenUsage>,
    pub terminal_error: Option<BackendError>,
    pub parse_error: Option<BackendError>,
}

pub(crate) fn parse_jsonl_diagnostics(stream: &str) -> CodexStreamDiagnostics;

// Existing strict API retained for callers/tests that need FR-3a behavior.
pub(crate) fn parse_jsonl_stream(stream: &str) -> Result<ParsedTurn, BackendError>;
```

`CodexBackend::query()` consumes diagnostics rather than cloning errors:

```rust
let diagnostics = super::codex_event::parse_jsonl_diagnostics(&stdout);
if let Some(err) = diagnostics.terminal_error {
    return Err(with_exit_code(err, exit_code));
}

let file_text = Self::read_last_message(last_message_file.path()).await;
let text = file_text
    .or(diagnostics.agent_message)
    .ok_or_else(|| diagnostics.parse_error.unwrap_or_else(|| BackendError::Parse {
        message: "Codex completed without output-last-message or JSONL agent_message".into(),
    }))?;

Ok(QueryOutput::from_process(text, stderr_str, exit_code, "codex", start.elapsed())
    .with_model(effective_model)
    .with_usage(diagnostics.usage))
```

## Assumptions

- **A1 (high)**: Codex CLI v0.119.0+ accepts `-o <path>` for `codex exec` and writes the final assistant message there on successful turns. **Verification**: capture new FR-3b fixtures with the repository's Codex CLI and document the command in `tests/fixtures/codex/README.md`.
- **A2 (medium)**: Always passing `-o` to older Codex versions fails noisily with a non-zero exit rather than silently changing behavior. **Verification**: implementation smoke-test with a stub command that rejects `-o`; no cheap real older-binary check is required for this slice.
- **A3 (high)**: `turn.failed` and top-level JSONL `error` events are authoritative failures even when `-o` contains text. **Verification**: unit test the `turn-failed.jsonl` + populated last-message case and assert `BackendError::ExecutionFailed`.
- **A4 (high)**: `tempfile::NamedTempFile` RAII cleanup is sufficient for success and error paths. **Verification**: unit tests assert the path disappears after the owning `NamedTempFile` is dropped; this follows the existing dependency contract rather than adding manual cleanup code.
- **A5 (high)**: Parser and fixture tests must not assume every `item.completed` has a matching `item.started`. **Verification**: apply `.pi/lessons/clo-373-codex-fixtures-lessons.md § L1`; FR-3b fixtures assert only needed semantics, not invented event pairing.
- **A6 (high)**: Fixture scrub checks must distinguish credential tokens from legitimate usage token fields. **Verification**: apply `.pi/lessons/clo-373-codex-fixtures-lessons.md § L2`; extend the existing scrub gate to `.txt` companions while retaining explicit credential-key/high-entropy checks.
- **A7 (high)**: `build_argv_prefix()` remains the single source of truth for Codex argv construction. **Verification**: apply `.pi/lessons/clo-371-stepcontext-migration-lessons.md § L3`; all query-time flags (`-s`, `--model`, `-o`) are asserted through that helper's tests.
- **A8 (high)**: `tempfile::NamedTempFile` creates the last-message file with private permissions appropriate for temporary LLM output. **Verification**: rely on the crate contract and add a Unix-only unit assertion for owner-read/write mode when available.

## Test plan

**Unit tests in `src/backend/codex.rs`**

- `codex_argv_includes_output_last_message_when_path_given()` asserts `-o` and the path appear exactly once.
- `codex_argv_omits_output_last_message_when_path_none()` protects existing tests that pass `None`.
- `codex_argv_orders_output_last_message_after_sandbox_and_model()` pins argv ordering.
- `read_last_message_returns_none_for_missing_file()`.
- `read_last_message_returns_none_for_empty_or_whitespace_file()`.
- `read_last_message_preserves_leading_whitespace_and_trims_only_trailing_newlines()`.
- `named_tempfile_cleanup_removes_last_message_path()` proves RAII cleanup without invoking Codex.

**Unit tests in `src/backend/codex_event.rs`**

- `diagnostics_preserves_usage_when_agent_message_missing()` uses `missing-agent-message.jsonl` and asserts `usage.is_some()`, `agent_message.is_none()`, and `parse_error.is_some()`.
- `diagnostics_reports_turn_failed_as_terminal_error()` uses `turn-failed.jsonl` and asserts `terminal_error` is `ExecutionFailed`.
- Existing strict parser tests remain unchanged: `parse_jsonl_stream(missing-agent-message)` still returns `BackendError::Parse`.

**Fixture/integration tests**

- Extend `tests/codex_fixtures.rs` to include expected `.last-message.txt` companions in fixture inventory and scrub checks.
- Add hermetic composed tests in `tests/codex_parse_output.rs` or Codex unit tests for:
  - populated `-o` + valid JSONL -> success text from `-o`, usage from JSONL;
  - populated `-o` + JSONL missing final agent message -> success text from `-o`, usage from JSONL;
  - empty `-o` + valid JSONL -> fallback to JSONL agent message;
  - missing `-o` file + valid JSONL -> fallback to JSONL agent message;
  - populated `-o` + `turn.failed` JSONL -> `BackendError::ExecutionFailed`;
  - populated `-o` + no usable JSONL usage -> success with `usage == None`.

**Manual verification**

1. Run a local Codex capture with `codex exec --json --ephemeral -s read-only -o <tmpfile> -- "Reply exactly: fixture happy path."` and confirm the file contains the final message.
2. Run `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.
3. Check `$TMPDIR` for leaked `lok-codex-last-*` files after the unit/integration tests.

## Migration / rollout

This is a purely additive internal Codex backend change. Existing workflow TOML files continue to work, no public structs gain fields, and all non-Codex backends are untouched. Users on Codex CLI v0.119.0+ get more robust final-answer extraction automatically. If an older Codex rejects `-o`, the existing non-zero exit handling reports the CLI error; version-gated fallback can be added later if that proves common. Rollout is a single PR with fixtures, tests, and README fixture-inventory updates.

## Open questions

- **Real older-Codex behavior for unknown `-o`**: the design assumes noisy failure rather than silent success without writing the file. If testing finds silent ignore behavior, no change is needed; if testing finds confusing stderr, the plan should add a clearer error hint.
- **Debug artifact retention**: FR-39 proposes preserving failed-run JSONL and `-o` files under `.lok/debug/<run-id>/`. CLO-380 intentionally deletes tempfiles; preserving them should wait for that separate debug-artifact design.
