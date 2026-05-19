# Codex JSONL fixtures

This directory contains scrubbed Codex `exec --json` streams for CLO-373 / FR-40. The fixtures are checked in so the future event-driven Codex parser can be developed and tested without live Codex access.

## Capture environment

- Codex CLI: `codex-cli 0.130.0`
- Capture host: `Darwin 25.5.0 arm64`
- Capture date: `2026-05-19`
- Runtime hermeticity: `cargo test --test codex_fixtures` reads only checked-in files and does not invoke Codex.

## Capture command template

Run captures from a scratch directory or a clean checkout with prompts that avoid local project context:

```bash
codex exec --json --ephemeral -s read-only --model <MODEL> -- "<SCRUBBED_PROMPT>" \
  > tests/fixtures/codex/<SCENARIO>.jsonl
```

Record the exact prompt or a scrubbed equivalent in the table below. Keep streams small and line-delimited; do not pretty-print the JSON.

## Fixture inventory

| Fixture | Terminal event | Prompt/provenance | Edit status |
|---|---|---|---|
| `turn-completed.jsonl` | TBD | TBD | TBD |
| `turn-failed.jsonl` | TBD | TBD | TBD |
| `multi-turn-reasoning.jsonl` | TBD | TBD | TBD |
| `missing-agent-message.jsonl` | TBD | TBD | TBD |

## Scrub checklist

Before committing any fixture, inspect it manually and run a text search for common leakage classes. Replace any hit with a neutral placeholder while preserving valid JSON strings and event order.

Check for:

- Home paths and usernames: `/Users/`, `/home/`, `C:\\Users\\`, `$HOME`, `<USERNAME>`.
- Temporary or project-private paths: `/tmp/`, `/var/folders/`, repo-specific scratch paths.
- Credentials and auth markers: `Bearer `, `api_key`, `api-key`, `apikey`, `token`, `secret`, `password`.
- Token-looking long alphanumeric values.
- Unredacted email addresses.

Suggested gate:

```bash
rg '<HOME>|/Users/|/home/|C:\\Users\\|<USERNAME>|/tmp/|/var/folders/|Bearer |api[_-]?key|apikey|token|secret|password|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+' \
  tests/fixtures/codex/*.jsonl
```

This grep is intentionally conservative. A clean grep is not a proof that no secret exists; manual review is still required.

## JSONL validation

```bash
python3 -c 'import json, sys; [json.loads(l) for f in sys.argv[1:] for l in open(f) if l.strip()]' \
  tests/fixtures/codex/*.jsonl
```

The Rust integration target adds stronger semantic checks:

```bash
cargo test --test codex_fixtures
```

## Missing-agent-message provenance rule

`missing-agent-message.jsonl` must be based on a real `codex exec --json --ephemeral` stream. First try natural prompts or sandbox conditions that complete without a final `agent_message`. If Codex 0.130.0 cannot produce one naturally, it is acceptable to remove only the paired final `agent_message` `item.started` and `item.completed` lines from a real `turn.completed` stream. If that fallback is used, document the natural attempts and exact trim in the fixture inventory.
