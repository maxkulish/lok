# Plan: CLO-382 — FR-26: Gemini Backend Token Count Extraction

## Context
- **Design**: `docs/designs/clo-382-gemini-usage-extraction.md`
- **Discovery**: `docs/discovery/clo-382.md`
- **Linear**: https://linear.app/cloud-ai/issue/clo-382/gemini-backend-extracts-token-counts-from-json-envelope

## Sub-tasks

### ST1 Append `--output-format json` to Gemini CLI invocation
**Files:** `src/backend/gemini.rs` (`build_shell_cmd`)
**What:** Add the flag to the default command line. Guard against double-adding when the caller passes custom `args`.
**Acceptance:** `cargo test gemini_build_shell_cmd` passes (new snapshot tests show the flag is present and not duplicated).
**Estimate:** S

### ST2 Add JSON envelope types and extraction logic
**Files:** `src/backend/gemini.rs`
**What:** Add private `GeminiEnvelope` and `GeminiStats` structs with `serde::Deserialize`. Add `parse_gemini_envelope` and `envelope_to_usage` associated methods. Modify `query` to try JSON envelope first and fall back to existing `parse_output`.
**Acceptance:** `cargo test gemini_parse_envelope` passes.
**Estimate:** S

### ST3 Create `tests/fixtures/gemini/` directory with synthetic fixtures
**Files:** `tests/fixtures/gemini/*` (NEW)
**What:** Add four scrubbed JSON files mirroring the Codex fixture layout — success with stats, success without stats, error envelope, malformed JSON — plus `README.md` and `version.txt`.
**Acceptance:** `cargo test gemini_fixtures` loads each fixture and asserts expected outcomes via a table-of-truth.
**Estimate:** S

### ST4 Capture a real Gemini envelope and reconcile `GeminiStats` field names
**Files:** `tests/fixtures/gemini/success-with-stats.json`, `src/backend/gemini.rs`
**What:** Run `gemini-cli` with `--output-format json` to capture a real envelope. Verify `promptTokenCount`, `candidatesTokenCount`, and optional `cachedContentTokenCount` match the assumed schema. Adjust `GeminiStats` field names / serde rename attributes if upstream uses different casing.
**Acceptance:** Real-capture fixture is accepted by `parse_gemini_envelope` and populates all three token fields.
**Estimate:** S

### ST5 Regression validation across all backends
**Files:** N/A (read-only validation)
**What:** Run the full pre-merge gate. Verify Codex, Claude API, Ollama, and Bedrock tests remain green; only Gemini-related tests change.
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` is clean.
**Estimate:** S

## Pre-merge gate
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`

## Risks
- **Real envelope schema drift**: If `gemini-cli` uses different field casing or nests `stats` differently, ST4 will require a field rename. Mitigation: synthetic fixtures already exercise code paths; field-name fix is a one-line change.
- **Older `gemini-cli` rejects `--output-format json`**: The child exits non-zero and `BackendError::ExecutionFailed` is returned, exactly as today. No regression.
- **User-supplied custom `args` drops the flag**: The `skip_lines` text fallback is preserved; `usage` remains `None`. Documented in PR.
