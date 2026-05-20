# Design: CLO-382 - FR-26: Gemini backend extracts token counts from JSON envelope

## Problem

Workflow authors who run `lok` with `backends.gemini` as a primary or consensus backend see `step.usage = null` in `lok run --output json` even though the Gemini CLI returns a structured JSON envelope containing `stats.promptTokenCount`, `stats.candidatesTokenCount`, and optionally `stats.cachedContentTokenCount`. `GeminiBackend::query` in `src/backend/gemini.rs:81` calls `parse_output` (lines 33-39) which blindly drops the first `skip_lines` lines and joins the remainder, then constructs `QueryOutput::from_process(...).with_model(...)` with **no** `.with_usage(...)` call (lines 140-148). The envelope's `stats` block is discarded byte-for-byte. PRD v5 FR-26 lists this as the last must-do extraction work in the FR-25 family; CLO-370 already wired `StepResult.usage` through the conductor and CLO-378 already extended `TokenUsage` with `cached_tokens` / `reasoning_tokens`, so Gemini is the only major CLI backend still silently dropping usage. Until this lands, downstream cost/observability summaries are incomplete for any workflow whose hot path touches Gemini.

## Goals / Non-goals

**Goals**

- Add `--output-format json` to the Gemini CLI invocation produced by `build_shell_cmd` so the envelope is emitted reliably.
- Parse the JSON envelope after a successful child exit, extract the visible response text into `QueryOutput.stdout`, and populate `QueryOutput.usage` from `stats.promptTokenCount` / `stats.candidatesTokenCount` (and `stats.cachedContentTokenCount` when present) via the existing `TokenUsage` builder.
- On any JSON parse failure or missing `response` / `stats` field, fall back to the current `parse_output(...)` text-skip behaviour and leave `usage = None`. The step must still succeed if the child exited 0.
- Land a `tests/fixtures/gemini/` directory with at least four scrubbed envelopes (success with `stats`, success without `stats`, error envelope from a failed child, malformed JSON) and a per-fixture parser unit test, mirroring the `tests/fixtures/codex/` layout.
- Keep the existing argv-snapshot tests in `src/backend/gemini.rs` green and extend them so the new flag, the JSON parser, and the fallback are all exercised.

**Non-goals**

- No changes to the `Backend` trait, to `TokenUsage`, or to `QueryOutput` (CLO-378 already finished the type-side work).
- No conductor-side change: `src/workflow.rs` already copies `qo.usage` into `step_usage` (see the FR-25a path around the single-backend, consensus, and synthesis branches), so populating `QueryOutput.usage` is sufficient for end-to-end observability.
- No new top-level dependencies. `serde_json` is already in the dependency set used by `src/backend/mod.rs` and `src/workflow.rs`.
- No streaming / persistent-daemon work, no `session_export`/`session_import`, no `--include-directories` promotion (FR-24a is its own ticket).
- No changes to `GeminiBackend::is_available()` or to the FR-12 health-probe semantics.
- No tightening of the contract to "JSON always". Text-mode fallback is explicitly preserved so workflows that override `args` to drop `--output-format json` keep working.

## Architecture

The change is isolated to `src/backend/gemini.rs` plus a new fixture directory under `tests/fixtures/gemini/` and an integration target under `tests/`. No new modules are introduced.

```
src/backend/gemini.rs
   GeminiBackend
   ├── build_shell_cmd          (append `--output-format json` unless args already set it)
   ├── parse_output             (kept as text-mode fallback)
   ├── parse_gemini_envelope    (NEW: serde_json::from_str -> GeminiEnvelope)
   ├── envelope_to_usage        (NEW: GeminiStats -> TokenUsage)
   └── query                    (after child exit: try envelope parse, else fall back)

tests/fixtures/gemini/          (NEW)
   ├── README.md                (capture command, scrub checklist, fixture inventory)
   ├── version.txt              (gemini-cli version that produced the captures)
   ├── success-with-stats.json
   ├── success-no-stats.json
   ├── error-envelope.json
   └── malformed.json           (intentionally invalid JSON)

tests/gemini_fixtures.rs        (NEW: integration target, parser-only, no child process)
```

Data flow inside `GeminiBackend::query`:

```
ctx ──► build_shell_cmd ──► sh -c "echo '' | npx ... --output-format json '<PROMPT>'"
                                 │
                                 ▼
                          tokio::process::Command::output().await
                                 │
                                 ▼
                    ┌────────────┴────────────┐
                    │ output.status.success() │
                    └────────────┬────────────┘
                                 │ yes
                                 ▼
              parse_gemini_envelope(&stdout)        ──── err / missing fields ────┐
                                 │ ok                                              │
                                 ▼                                                 ▼
                  QueryOutput::from_process(env.response, …)        QueryOutput::from_process(parse_output(&stdout), …)
                       .with_model(effective_model)                       .with_model(effective_model)
                       .with_usage(Some(env.usage))                       (usage = None)
```

New private types (kept inside `src/backend/gemini.rs`, not re-exported):

```rust
#[derive(serde::Deserialize)]
struct GeminiEnvelope {
    response: String,
    stats: Option<GeminiStats>,
    // Error envelopes carry { "error": { "message": "..." } } here; we don't
    // need a typed field because we only consume the envelope on exit-success.
}

#[derive(serde::Deserialize)]
struct GeminiStats {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "cachedContentTokenCount")]
    cached_content_token_count: Option<u32>,
}
```

`envelope_to_usage` maps a `GeminiStats` to `Option<TokenUsage>`: if both `promptTokenCount` and `candidatesTokenCount` are present, it builds `TokenUsage::new(prompt, completion).with_cached(cached_content_token_count)`. If either count is missing, it returns `None` (we do not invent zeros).

`build_shell_cmd` gains a single change: append `--output-format json` to the command unless the caller-provided `args` already contains that flag (defensive, so power-users overriding `args` are not double-flagged). This is a literal substring check on `args.iter().any(|a| a == "--output-format")`; the flag matches Gemini CLI's documented form (PRD v5 §FR-4 line 143). The shell-escape behaviour is unchanged.

## Public API surface

No public trait, struct, or function signature changes. The change is purely internal to `GeminiBackend`. The `Backend::query` contract continues to return `std::result::Result<QueryOutput, BackendError>`; `QueryOutput.usage` is `Option<TokenUsage>` already.

Before (in `src/backend/gemini.rs:140-148`):

```rust
let parsed_stdout = self.parse_output(&stdout);
Ok(super::QueryOutput::from_process(
    parsed_stdout,
    stderr_str,
    exit_code,
    "gemini",
    start.elapsed(),
)
.with_model(effective_model))
```

After:

```rust
let (response_text, usage) = match Self::parse_gemini_envelope(&stdout) {
    Some(env) => (env.response, Self::envelope_to_usage(env.stats)),
    None => (self.parse_output(&stdout), None),
};
Ok(super::QueryOutput::from_process(
    response_text,
    stderr_str,
    exit_code,
    "gemini",
    start.elapsed(),
)
.with_model(effective_model)
.with_usage(usage))
```

New private associated items (signatures only):

```rust
impl GeminiBackend {
    fn parse_gemini_envelope(stdout: &str) -> Option<GeminiEnvelope> { /* serde_json::from_str */ }
    fn envelope_to_usage(stats: Option<GeminiStats>) -> Option<super::TokenUsage> { /* … */ }
}
```

`build_shell_cmd` keeps its existing signature; only the body changes:

```rust
fn build_shell_cmd(
    command: &str,
    args: &[String],
    model: Option<&str>,
    sandbox: Option<super::SandboxMode>,
    prompt: &str,
) -> String
```

## Assumptions

- **high** - The Gemini CLI emits a single JSON document on stdout when invoked with `--output-format json`, with shape `{ "response": "...", "stats": { "promptTokenCount": <u32>, "candidatesTokenCount": <u32>, "cachedContentTokenCount"?: <u32> } }`. Verify by capturing a real fixture during implementation (the discovery report flags this as the only outstanding debt) and inspecting the actual key names; if upstream uses different casing, only the `serde(rename = ...)` attributes change.
- **high** - `serde_json` is already a workable dependency for `src/backend/gemini.rs`. Verify by `grep -n "serde_json" Cargo.toml`; the codebase already uses it in `src/backend/mod.rs::QueryOutput.structured` and across `src/workflow.rs`.
- **medium** - Adding `--output-format json` to the default `args` does not regress existing power-users who pass custom `args` via `BackendConfig`. Verify by inspecting `GeminiBackend::new` (lines 16-31): when `config.args` is non-empty we keep their value; when it is empty we now append `--output-format json` to the default `vec!["@google/gemini-cli".to_string()]`. Document this in the implementation PR so anyone overriding `args` knows to include the flag.
- **medium** - Counts fit in `u32`. Gemini context windows top out well below `u32::MAX`; `TokenUsage` is already `u32` for all backends and uses saturating addition. Verify in `src/backend/mod.rs:109-126`.
- **low** - The envelope's `response` field carries the textual answer in the same shape the existing text-mode path returned. Some Gemini CLI versions may wrap the answer in markdown fences or include trailing whitespace. Verify against the captured fixture; if the shape differs materially, add a small trim step and call it out in the PR description.

## Test plan

**Unit tests** (`src/backend/gemini.rs`, `#[cfg(test)] mod tests`):

- `gemini_build_shell_cmd_appends_output_format_json` - argv snapshot asserts `--output-format json` is present when `args` does not already contain `--output-format`.
- `gemini_build_shell_cmd_does_not_double_add_output_format` - when caller supplies `args = vec!["@google/gemini-cli", "--output-format", "json"]`, the shell command contains the flag exactly once.
- `gemini_parse_envelope_extracts_usage` - feed the `success-with-stats.json` fixture content into `parse_gemini_envelope` + `envelope_to_usage`; assert `prompt_tokens`, `completion_tokens`, `total_tokens`, and `cached_tokens` match the fixture.
- `gemini_parse_envelope_without_stats_returns_no_usage` - feed `success-no-stats.json`; assert `envelope.response` is non-empty and `envelope_to_usage(stats)` returns `None`.
- `gemini_parse_envelope_malformed_returns_none` - feed `malformed.json`; assert `parse_gemini_envelope` returns `None` so the caller falls back to text mode.
- `gemini_parse_envelope_missing_response_returns_none` - hand-built JSON `{"stats": {...}}` (no `response`); assert parse fails.
- Existing sandbox / model-flag / shell-escape tests (`gemini_sandbox_*`, `gemini_model_flag_*`, `gemini_sandbox_prompt_is_escaped`) must keep passing without modification.

**Integration tests** (`tests/gemini_fixtures.rs`, new file, mirrors `tests/codex_fixtures.rs`):

- Enumerate every `*.json` under `tests/fixtures/gemini/`, bound each file at `MAX_FIXTURE_BYTES` (~20_000), and ensure the corpus stays under a corpus cap (~50_000) so the suite remains hermetic and tiny.
- For each fixture name, assert the parser's outcome matches an expected `Outcome { has_response: bool, has_usage: bool, prompt: Option<u32>, completion: Option<u32>, cached: Option<u32> }` table-of-truth in the test file.
- A separate sanity test asserts every fixture file is either valid JSON (success / no-stats / error) or in `malformed.json`'s allow-list.

**Per-backend matrix.** Because FR-26 touches only the Gemini backend and the conductor copy is already exercised by FR-25a's tests, the matrix here is a regression matrix rather than a trait-level one:

| Backend         | Coverage                                                                                          |
|-----------------|---------------------------------------------------------------------------------------------------|
| Gemini (this PR)| Unit + fixtures above. `QueryOutput.usage` is `Some(...)` when envelope has `stats`, else `None`. |
| Codex           | `tests/codex_fixtures.rs` regression - must remain green. No code change here.                    |
| Claude API      | Existing usage tests in `src/backend/claude.rs` - must remain green.                              |
| Claude CLI      | Out of scope for FR-26.                                                                           |
| Ollama          | `src/backend/ollama.rs` extraction test - regression-pin per PRD FR-27.                           |
| Bedrock         | `src/backend/bedrock.rs` usage test (feature-gated) - regression-pin.                             |

**Manual verification steps** (run by assignee MK, who has Gemini CLI access):

1. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` - clean on `feat/clo-382-gemini`.
2. With `gemini-cli` installed, run `echo '' | npx @google/gemini-cli --output-format json 'Reply exactly: ok.' > /tmp/gemini-capture.json` and verify the envelope shape against `GeminiEnvelope` / `GeminiStats`. Save a scrubbed copy as `tests/fixtures/gemini/success-with-stats.json`.
3. Run an existing workflow that uses Gemini as the primary backend; confirm `lok run --output json` now shows `step.usage.prompt_tokens` and `step.usage.completion_tokens` for Gemini steps (and `step.usage.cached_tokens` if the envelope reports `cachedContentTokenCount`).
4. Temporarily override `BackendConfig.args` to drop `--output-format json`, run the same workflow, and confirm the step still succeeds with `step.usage = null` (text-mode fallback).

## Migration / rollout

This change is **purely additive at the API boundary** and **observably additive at the workflow boundary** (a previously-null field gains data).

- No `Backend` trait change; no `TokenUsage` / `QueryOutput` shape change.
- No feature flag is required. `--output-format json` is a Gemini CLI flag, not a lok config flag, and the text-mode fallback covers every failure case the discovery report enumerated.
- Rollout order is a single PR: Gemini backend changes + fixtures + tests land together. There is no upstream consumer to coordinate with; rs-wisper already deserializes `StepResult.usage` (CLO-370 sequence).
- Backward compatibility for workflows that pin a custom `args` list is preserved via the duplicate-flag check in `build_shell_cmd`. Document this in the PR description so anyone overriding `args` knows to include `--output-format json` themselves if they want usage.
- If a user is on a `gemini-cli` version older than v0.42 (PRD v5 §FR-4) and `--output-format json` is not recognised, the child will exit non-zero and we return `BackendError::ExecutionFailed` with the CLI's stderr, exactly as today. This matches the PRD's stated risk row ("older gemini-cli installs"); FR-12 / capability registry will eventually warn before the call, but that is out of scope for FR-26.
- Pre-merge gate per `AI-AGENTS.md`: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Open questions

- **Fixture capture environment.** The discovery report (`docs/discovery/clo-382.md` §"Discovery Debt") flags that no `tests/fixtures/gemini/` exists yet and that capture requires either installing `gemini-cli` locally or asking MK to run the capture. The exact `cachedContentTokenCount` shape is therefore unverified - this design assumes it is a sibling of `promptTokenCount` inside `stats`, but it could instead live at envelope top level. Tradeoff: capture before implementation locks the schema but blocks the PR on environment access; capture during implementation lets coding start now but risks one revision of `GeminiStats` field names after the fixture lands. Recommended (not yet decided): capture first, even if it costs a day, because the parser shape and the unit-test assertions both depend on it.
- **What to do when `stats` is present but only one of `promptTokenCount` / `candidatesTokenCount` is set.** This design returns `None` in `envelope_to_usage` if either field is missing, to avoid writing misleading zeros. The PRD's acceptance criteria (FR-26) only mandates that `QueryOutput.usage` be populated when stats are present; it does not specify partial-stats behaviour. Tradeoff: returning `None` is conservative and matches "no data is better than wrong data"; returning a half-populated `TokenUsage` would match the upstream envelope literally but could mislead summary aggregation. Leave open for review.
- **Whether to also extract a Gemini `model` echo from the envelope.** Some Gemini CLI versions emit the resolved model name inside the envelope; today we set `with_model(effective_model)` from the request side. Out of scope for FR-26 as written but worth flagging once the real fixture is captured.
