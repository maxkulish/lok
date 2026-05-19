# Design: CLO-373 - Capture Codex JSONL fixtures for parser test corpus (FR-40)

## Problem

Lok backend/parser contributors have no checked-in Codex `--json` stream corpus to write tests against, so the upcoming event-driven Codex parser (FR-3a) is currently untestable: `src/backend/codex.rs::parse_output` scans stdout with `line.contains("\"type\":\"item.completed\"")` and `line.contains("agent_message")`, and the only sample JSONL anywhere in the repo lives inside review prose (`docs/reviews/CLO-374-codex-validation.md`). The phase-2 PRD `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` makes FR-40 a hard prerequisite for FR-3a in §9, §11.0a, and §11.4: the parser PR cannot land until a real, scrubbed stream corpus exists on `main`. This blocks every Codex usage/parsing FR (FR-3a, FR-3b, FR-25, FR-25b), so it must land first.

## Goals / Non-goals

Goals:
- Add `tests/fixtures/codex/` containing four scrubbed JSONL streams captured from `codex exec --json --ephemeral` against Codex CLI `0.130.0`:
  - `turn-completed.jsonl` - happy path ending in `turn.completed` with a final `item.completed` `agent_message`.
  - `turn-failed.jsonl` - run terminating in `turn.failed` (or, if Codex emits a top-level terminal error for the chosen failure mode, `error`) with structured error details.
  - `multi-turn-reasoning.jsonl` - stream with non-zero `reasoning_output_tokens` in `turn.completed.usage` and at least one intermediate non-`agent_message` `item.completed` event before the final `agent_message` when Codex emits one.
  - `missing-agent-message.jsonl` - terminates in `turn.completed` but has no `item.completed` whose nested `item.type == "agent_message"`; if Codex 0.130.0 cannot naturally produce this, derive it by hand-trimming only the paired final `agent_message` `item.started`/`item.completed` lines from a real `turn.completed` stream and document that provenance in the README.
- Add `tests/fixtures/codex/README.md` documenting the exact capture command, per-fixture prompt/provenance metadata, capture host OS, capture date, scrub checklist, JSONL validation step, pinned Codex version, and hermeticity note (`cargo test --test codex_fixtures` does not require Codex at test time).
- Add `tests/fixtures/codex/version.txt` containing the recorded Codex CLI version for automation-friendly future checks.
- Add `.gitattributes` with `tests/fixtures/codex/*.jsonl text eol=lf` so checked-in JSONL keeps LF line endings across platforms.
- Add one Rust integration test file that loads every `*.jsonl` fixture and asserts each line is valid JSON, has a known Codex event `type`, contains no obvious local paths/secrets, remains under the reviewable size cap, and terminates in one of `{turn.completed, turn.failed, error}`.
- Land the fixture set on `main` independently of any parser change so FR-3a can be opened against a stable test bed.

Non-goals:
- Rewriting `src/backend/codex.rs::parse_output` or introducing a serde event enum - that is FR-3a's PR.
- Adding `--output-last-message` / `-o` support (FR-3b).
- Extending `TokenUsage` with `cached_tokens` / `reasoning_tokens` fields (FR-25b).
- Capturing fixtures for Claude, Gemini, Ollama, or Bedrock backends (separate FRs in the §10 fixture group).
- Building a generic fixture-loading helper for use across backends - introduce only the minimal loader this task needs and let FR-3a evolve it.
- Mocking Codex with wholly synthetic JSONL. The chosen approach is real-stream capture; the only permitted manual edit is the explicitly documented paired-line trim for `missing-agent-message.jsonl` if no current Codex prompt can produce that edge case naturally.

## Architecture

This is an additive, test-focused change. No `src/` modules are modified.

```text
.gitattributes                     # NEW or appended: enforce LF for Codex JSONL fixtures
tests/
├── integration.rs                 # existing - untouched
├── workflows/                     # existing - untouched
└── fixtures/                      # NEW
    └── codex/                     # NEW
        ├── README.md              # capture + scrub + validation procedure, per-fixture metadata
        ├── version.txt            # Codex CLI version used for capture, e.g. codex-cli 0.130.0
        ├── turn-completed.jsonl
        ├── turn-failed.jsonl
        ├── multi-turn-reasoning.jsonl
        └── missing-agent-message.jsonl

tests/codex_fixtures.rs            # NEW - integration test that walks the corpus
```

Capture data flow (one-time, manual, documented in the README):

```text
  developer prompt ──▶ codex exec --json --ephemeral -s read-only -- "<prompt>"
                              │
                              ▼
                         stdout (JSONL)
                              │
                              ▼
              scrub step: paths, usernames, tmp dirs, emails,
              tokens/API keys/bearer secrets, repo-private strings
                              │
                              ▼
              tests/fixtures/codex/<scenario>.jsonl
                              │
                              ▼
              python3 JSONL validator + cargo integration tests
```

Test-time data flow:

```text
  tests/codex_fixtures.rs
        │
        ├── std::fs::read_dir("tests/fixtures/codex")
        ├── filter: path.extension() == Some("jsonl")
        ├── for each *.jsonl:
        │      ├── assert file size <= 20_000 bytes
        │      ├── read_to_string
        │      ├── reject obvious unsanitized local paths/secrets
        │      ├── split by '\n', skip empty
        │      ├── serde_json::from_str::<serde_json::Value>(line)
        │      ├── assert parsed event vector is non-empty
        │      └── assert every event has type in the documented event set
        └── scenario-specific semantic assertions
```

Concrete Rust artefacts:

- New integration test crate `tests/codex_fixtures.rs` with:
  - `fn fixtures_dir() -> std::path::PathBuf` - returns `env!("CARGO_MANIFEST_DIR")/tests/fixtures/codex`.
  - `fn load_fixture(name: &str) -> String` - thin `std::fs::read_to_string` wrapper that panics with the fixture filename, not an absolute machine path.
  - `fn jsonl_fixture_paths() -> Vec<std::path::PathBuf>` - walks `tests/fixtures/codex`, filters to `.jsonl`, sorts by filename for deterministic failures, and ignores co-located `README.md` / `version.txt`.
  - `fn parse_jsonl(name: &str, stream: &str) -> Vec<serde_json::Value>` - splits on `\n`, skips empty lines, deserialises each line, asserts the result is non-empty, and asserts each event's top-level `type` is one of `thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.started`, `item.completed`, or `error`.
  - `fn assert_no_unscrubbed_sensitive_text(name: &str, stream: &str)` - checks for obvious local path and secret markers (`/Users/`, `/home/`, `C:\\Users\\`, `$HOME`, bearer/API-key/token-looking strings, and unredacted emails) over fixture text.
  - Test fns described in §Test plan.
- No new files under `src/`. No new public crate types.
- No new dependencies. `serde_json` is already in `Cargo.toml`.

The fixtures themselves remain line-delimited JSONL, not pretty-printed, so future FR-3a parser tests can exercise line-by-line behavior. If `missing-agent-message.jsonl` requires paired-line trimming, the resulting fixture is still based on a real Codex stream, but the README must label it as hand-trimmed and describe exactly which paired `agent_message` `item.started`/`item.completed` lines were removed.

## Public API surface

No change to the public Rust API of the `lokomotiv` crate. `src/backend/codex.rs`, `Backend`, `CodexBackend`, `StepContext`, and `QueryOutput` are untouched in this PR. The only Rust surface added is private to the new integration test:

```rust
// tests/codex_fixtures.rs
use serde_json::Value;
use std::ffi::OsStr;
use std::path::PathBuf;

const KNOWN_EVENT_TYPES: &[&str] = &[
    "thread.started",
    "turn.started",
    "turn.completed",
    "turn.failed",
    "item.started",
    "item.completed",
    "error",
];
const MAX_FIXTURE_BYTES: u64 = 20_000;
const MAX_CORPUS_BYTES: u64 = 50_000;

fn fixtures_dir() -> PathBuf { /* tests/fixtures/codex */ }
fn jsonl_fixture_paths() -> Vec<PathBuf> { /* read_dir + .jsonl filter + sort */ }
fn load_fixture(name: &str) -> String { /* read file; panic with fixture name only */ }
fn parse_jsonl(name: &str, stream: &str) -> Vec<Value> { /* valid JSONL + non-empty + known type */ }
fn assert_no_unscrubbed_sensitive_text(name: &str, stream: &str) { /* scrub gate */ }

#[test]
fn every_fixture_is_line_valid_jsonl() { /* see Test plan */ }

#[test]
fn fixtures_do_not_exceed_reviewable_size_caps() { /* see Test plan */ }

#[test]
fn fixtures_do_not_contain_obvious_sensitive_text() { /* see Test plan */ }

#[test]
fn turn_completed_fixture_is_valid_jsonl_with_agent_message() { /* see Test plan */ }

#[test]
fn turn_failed_fixture_terminates_in_failure() { /* see Test plan */ }

#[test]
fn multi_turn_reasoning_fixture_reports_reasoning_tokens() { /* see Test plan */ }

#[test]
fn missing_agent_message_fixture_has_no_agent_message_item() { /* see Test plan */ }
```

The fixture README is a human-readable contract. Its capture command is:

```bash
codex exec --json --ephemeral -s read-only --model <MODEL> -- "<SCRUBBED_PROMPT>" \
  > tests/fixtures/codex/<SCENARIO>.jsonl
```

The README must also record, for each fixture, the exact prompt (or a scrubbed equivalent), Codex version, capture-host OS, capture date, terminal event, and whether the file is byte-for-byte captured or hand-trimmed from a real stream.

## Assumptions

- Codex CLI `0.130.0` (already installed locally per discovery) is the recorded version for the captured streams. **Confidence: high.** Verified by `codex --version` during discovery; pinned in both `tests/fixtures/codex/README.md` and `tests/fixtures/codex/version.txt` during implementation.
- Codex `0.130.0` JSONL event names match the set documented in `docs/investigations/codex-quick-ref.md` and PRD §11.4: `thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.started`, `item.completed`, `error`. **Confidence: high.** Verified during discovery against quick-ref + existing CLO-374 review snippets; the integration test asserts this set over every line.
- Codex agent-message item type is represented as `item.type == "agent_message"` inside `item.completed` events, not as a separate `item.item_type` field. **Confidence: high.** Verified from current `src/backend/codex.rs::parse_output` expectations and existing CLO-374 review snippets; integration tests assert this exact nested field name.
- This task remains fixture/test-only and does not introduce new backend carrying structs, public re-exports, or programmatic shell command construction. **Confidence: high.** Verification: final diff contains only `.gitattributes`, `tests/fixtures/codex/`, `tests/codex_fixtures.rs`, and design/status docs; if implementation expands into backend context or shell helpers, re-apply `.pi/lessons/clo-371-stepcontext-migration-lessons.md §L1-L3` and `.pi/lessons/clo-374-sandbox-routing-lessons.md §L1` before proceeding.
- A prompt that asks Codex to "think step by step" and produce a short answer (e.g. a multi-step arithmetic question) reliably yields `reasoning_output_tokens > 0` on a reasoning-capable model. **Confidence: medium.** Reasoning emission depends on Codex's internal routing. Verification path: capture against a reasoning-capable default model (`gpt-5` family) and grep the resulting JSONL for `"reasoning_output_tokens":` and a non-zero value before committing; iterate on the prompt if it lands at zero.
- A no-final-`agent_message` fixture can be produced without inventing synthetic JSONL, either from a natural Codex 0.130.0 `turn.completed` stream or by paired-line trimming of a real `turn.completed` stream. **Confidence: medium.** Verification path: first try natural prompts / sandbox conditions; if none work, remove both paired final `agent_message` `item.started` and `item.completed` lines from a real stream, keep the terminal `turn.completed`, and document the edit in README metadata.
- Scrubbed JSONL remains JSON-valid after replacement. **Confidence: high.** Replacements stay inside JSON string values and preserve quoting; verification path is the README JSONL validator plus `every_fixture_is_line_valid_jsonl`.
- The scrub gate patterns catch common leakage classes for this corpus: POSIX/macOS/Windows home paths, `$HOME`, temp paths, bearer/API-key/token-looking strings, and emails. **Confidence: medium.** Regexes cannot prove the absence of every secret, so README manual inspection remains required; integration tests catch obvious regressions.
- No existing `tests/fixtures/` directory convention exists in lok. **Confidence: high.** Confirmed by discovery (`tests/` contains only `integration.rs` and `workflows/`).
- The four fixtures together stay under a reviewable size target of 50 KB total and 20 KB per fixture. **Confidence: medium.** Codex JSONL can be verbose; verification path is an integration-test metadata assertion plus manual `wc -c tests/fixtures/codex/*.jsonl` before committing.

## Test plan

Unit tests: none - no `src/` code is added in this PR. The existing `src/backend/codex.rs` tests stay green unchanged.

Integration tests (new file `tests/codex_fixtures.rs`):

- `fn every_fixture_is_line_valid_jsonl()` - walks only `tests/fixtures/codex/*.jsonl` (filtering by extension), calls `parse_jsonl` on each, asserts at least one event per file, asserts every event has a string top-level `type`, asserts that type is in the documented event set, and asserts the final event type is one of `turn.completed`, `turn.failed`, or `error`.
- `fn fixtures_do_not_exceed_reviewable_size_caps()` - asserts each JSONL fixture is at most `20_000` bytes and the full corpus is at most `50_000` bytes. If this becomes too noisy in a future parser/perf task, that task can intentionally relax the constants.
- `fn fixtures_do_not_contain_obvious_sensitive_text()` - scans every fixture for obvious unsanitized paths and credentials: `/Users/`, `/home/`, `C:\\Users\\`, `$HOME`, `/tmp/`, bearer tokens, API-key/token markers with long values, and unredacted emails.
- `fn turn_completed_fixture_is_valid_jsonl_with_agent_message()` - loads `turn-completed.jsonl`, asserts the last non-empty event has `type == "turn.completed"`, and asserts at least one event has `type == "item.completed"` with nested `item.type == "agent_message"` and non-empty `item.text`.
- `fn turn_failed_fixture_terminates_in_failure()` - loads `turn-failed.jsonl`, asserts the last event's `type` is in `{ "turn.failed", "error" }`, and asserts the terminal event carries an object/string error payload suitable for future `BackendError` mapping.
- `fn multi_turn_reasoning_fixture_reports_reasoning_tokens()` - loads `multi-turn-reasoning.jsonl`, finds the last `turn.completed` event, asserts its `usage.reasoning_output_tokens` is present and `>= 1`, and separately asserts at least one preceding `item.completed` event whose nested `item.type` is not `agent_message`. These assertions stay split because Codex may report reasoning tokens without a discrete reasoning item in some versions.
- `fn missing_agent_message_fixture_has_no_agent_message_item()` - loads `missing-agent-message.jsonl`, asserts last event `type == "turn.completed"`, and asserts there is no `item.completed` event with nested `item.type == "agent_message"`.

Per-backend test matrix (Codex only - all other backends are explicitly non-goals for this task):

| Scenario | Fixture | Asserted shape | Drives future FR |
|----------|---------|----------------|------------------|
| Happy path | `turn-completed.jsonl` | last event `turn.completed`, final `agent_message` present | FR-3a, FR-25 |
| Failure | `turn-failed.jsonl` | last event `turn.failed`/`error`, terminal error details present | FR-3a |
| Reasoning + tool calls | `multi-turn-reasoning.jsonl` | `reasoning_output_tokens > 0`, intermediate non-message items | FR-3a, FR-25, FR-25b |
| Final message absent | `missing-agent-message.jsonl` | `turn.completed` end, no `agent_message` item | FR-3a fallback |

Manual verification (run by author before pushing, captured in README):

1. `codex --version` reports `codex-cli 0.130.0` and the same value is written to `tests/fixtures/codex/version.txt` and the README.
2. For each scenario, run the capture command in the README into a scratch file, eyeball the JSONL, then move it into `tests/fixtures/codex/`.
3. For `missing-agent-message.jsonl`, try natural Codex 0.130.0 captures first. If none produce `turn.completed` without `agent_message`, hand-trim only the paired final `agent_message` `item.started` and `item.completed` lines from a real `turn.completed` stream and document that exact edit in README metadata.
4. Run the scrub checklist: `rg '<HOME>|/Users/|/home/|C:\\Users\\|<USERNAME>|/tmp/|Bearer |api[_-]?key|token|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+' tests/fixtures/codex/*.jsonl` over every fixture and replace any hit with a neutral placeholder. Re-run until clean.
5. `python3 -c "import json, sys; [json.loads(l) for f in sys.argv[1:] for l in open(f) if l.strip()]" tests/fixtures/codex/*.jsonl` exits 0.
6. Confirm the implementation did not add programmatic shell-command construction for capture; if it adds any helper script/binary, review escaping against `.pi/lessons/clo-374-sandbox-routing-lessons.md §L1` before proceeding.
7. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` (the project's pre-merge gate) passes.
8. `cargo test --test codex_fixtures -- --nocapture` passes on a clean checkout.
9. `wc -c tests/fixtures/codex/*.jsonl` confirms each file is below the 20 KB cap and the total is below 50 KB.

## Migration / rollout

The change is additive and test-only. Runtime behavior, public API, config schema, default arg list, and workflow contracts are unchanged. Existing tests stay green because `tests/codex_fixtures.rs` is a brand-new integration target and the existing `mod tests` inside `src/backend/codex.rs` is untouched.

- Backward compatibility: nothing to migrate. Workflows on `main` continue to invoke Codex exactly as before; `CodexBackend::parse_output` is unchanged.
- Repository metadata: add or append `.gitattributes` with `tests/fixtures/codex/*.jsonl text eol=lf`. This affects line-ending normalization for the new fixture path only.
- Feature flags: none required - fixtures are test-only data with no runtime cost.
- Rollout order: this PR lands first on `main`. FR-3a (the event-driven parser) opens its PR against the corpus that this one delivers, as required by PRD §9 ordering item 5 ("Codex JSONL fixture capture (FR-40) - lands ahead of the parser PR").
- Rollback: revert this PR; deletes the new `tests/fixtures/codex/` tree, `tests/codex_fixtures.rs`, and the fixture-specific `.gitattributes` line. No runtime, config, or workflow regression is possible because nothing under `src/` shipped.

## Open questions

- **Reasoning-token prompt stability.** Whether the chosen prompt for `multi-turn-reasoning.jsonl` will continue to yield `reasoning_output_tokens > 0` across Codex / model upgrades. If a future Codex release stops emitting reasoning tokens for that prompt, the fixture freezes the old shape but the live CLI no longer matches. Tradeoff: re-capturing on every Codex bump (more work) vs. tolerating drift (parser tests stop reflecting current reality). Leaving open for FR-3a's PR to decide whether to re-capture or pin parser tests to `0.130.0`.
- **Natural no-agent-message availability.** The design now permits documented paired-line trimming if Codex 0.130.0 cannot naturally produce the edge case, but implementation should still record which natural attempts were tried before falling back. This is an implementation provenance question, not a design blocker.
- **Failure-mode breadth.** FR-40 asks for a failure stream; one `turn.failed`/`error` fixture is sufficient. FR-3a may later add a second fixture to split `turn.failed` and top-level `error` if parser behavior diverges.
