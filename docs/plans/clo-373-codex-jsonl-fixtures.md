# Plan: CLO-373 Capture Codex JSONL fixtures for parser test corpus (FR-40)

## Context
- Design: docs/designs/clo-373-codex-jsonl-fixtures.md
- Discovery: docs/discovery/clo-373.md
- Linear: https://linear.app/cloud-ai/issue/CLO-373/capture-codex-jsonl-fixtures-for-parser-test-corpus-fr-40

## Sub-tasks

### ST1 Add Codex fixture directory metadata
**Files:** `.gitattributes`, `tests/fixtures/codex/README.md`, `tests/fixtures/codex/version.txt`
**Acceptance:** `test -f tests/fixtures/codex/README.md && test -f tests/fixtures/codex/version.txt && rg '^tests/fixtures/codex/\*.jsonl text eol=lf$' .gitattributes`
**Estimate:** S

Create the fixture directory and non-JSONL metadata first. The README must document the capture command template, Codex version, capture host/date fields, per-fixture provenance fields, scrub checklist, JSONL validation command, and hermetic test note. `version.txt` records the Codex CLI version used for capture, expected from discovery as `codex-cli 0.130.0`. Add or append the `.gitattributes` rule without disturbing unrelated attributes.

### ST2 Capture and scrub happy-path and failure fixtures
**Files:** `tests/fixtures/codex/turn-completed.jsonl`, `tests/fixtures/codex/turn-failed.jsonl`, `tests/fixtures/codex/README.md`
**Acceptance:** `python3 -c 'import json,sys; [json.loads(l) for f in sys.argv[1:] for l in open(f) if l.strip()]' tests/fixtures/codex/turn-completed.jsonl tests/fixtures/codex/turn-failed.jsonl && rg '"type":"turn.completed"|"type": "turn.completed"' tests/fixtures/codex/turn-completed.jsonl && rg '"type":"(turn.failed|error)"|"type": "(turn.failed|error)"' tests/fixtures/codex/turn-failed.jsonl`
**Estimate:** M

Capture real `codex exec --json --ephemeral` streams for the happy path and failure path using short non-sensitive prompts. Scrub any usernames, host paths, temp paths, emails, bearer tokens, API keys, or token-looking values while preserving valid JSON strings and Codex event ordering. Record prompt/provenance and terminal event metadata for both fixtures in the README.

### ST3 Capture and scrub reasoning and missing-agent-message fixtures
**Files:** `tests/fixtures/codex/multi-turn-reasoning.jsonl`, `tests/fixtures/codex/missing-agent-message.jsonl`, `tests/fixtures/codex/README.md`
**Acceptance:** `python3 -c 'import json,pathlib; r=[json.loads(l) for l in pathlib.Path("tests/fixtures/codex/multi-turn-reasoning.jsonl").read_text().splitlines() if l.strip()]; m=[json.loads(l) for l in pathlib.Path("tests/fixtures/codex/missing-agent-message.jsonl").read_text().splitlines() if l.strip()]; assert any((e.get("usage") or {}).get("reasoning_output_tokens",0)>0 for e in r if e.get("type")=="turn.completed"); assert m and m[-1].get("type")=="turn.completed"; assert not any(e.get("type")=="item.completed" and (e.get("item") or {}).get("type")=="agent_message" for e in m)'`
**Estimate:** M

Capture a reasoning-capable stream with non-zero `reasoning_output_tokens`. For `missing-agent-message.jsonl`, first try natural Codex 0.130.0 captures. If none produce `turn.completed` without a final `agent_message`, hand-trim only the paired final `agent_message` `item.started`/`item.completed` lines from a real `turn.completed` stream, then document the exact edit and natural attempts in README provenance.

### ST4 Add Rust integration tests for the Codex fixture corpus
**Files:** `tests/codex_fixtures.rs`, `tests/fixtures/codex/*.jsonl`, `tests/fixtures/codex/README.md`
**Acceptance:** `cargo test --test codex_fixtures`
**Estimate:** M

Implement the private test loader and semantic checks from the finalized design. The loader must walk only `*.jsonl`, sort paths deterministically, parse every non-empty line as `serde_json::Value`, assert known event types, assert non-empty event vectors, assert terminal event shape, enforce the 20 KB per-fixture and 50 KB corpus caps, and reject obvious unsanitized paths/secrets. Add scenario-specific tests for happy-path `agent_message`, terminal failure payload, reasoning tokens plus intermediate non-message items, and missing-agent-message absence.

### ST5 Run final corpus validation and full pre-merge gate
**Files:** `.gitattributes`, `tests/fixtures/codex/README.md`, `tests/fixtures/codex/version.txt`, `tests/fixtures/codex/*.jsonl`, `tests/codex_fixtures.rs`
**Acceptance:** `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
**Estimate:** S

Run the final manual and automated gates after all fixture and test files exist. Confirm `wc -c tests/fixtures/codex/*.jsonl` is within the documented caps, the README provenance matches the checked-in fixture contents, and no `src/backend/codex.rs` parser changes slipped into scope.

## Pre-merge gate
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` (fmt + clippy + test)

## Risks
- The reasoning fixture may need prompt/model iteration to produce `reasoning_output_tokens > 0`; keep captures short and validate before committing.
- Codex 0.130.0 may not naturally emit a completed stream without an `agent_message`; the approved fallback is documented paired-line trimming from a real stream, not synthetic JSONL.
- Real Codex streams may contain local paths or environment details; combine manual review, README scrub commands, and the Rust scrub gate before treating fixtures as safe.
- Existing unrelated modified files are present in the worktree; implementation should avoid broad rewrites and keep this task's final product focused on the fixture/test files plus orchestrator docs.
