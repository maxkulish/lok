# Lessons: CLO-373 Codex JSONL fixtures

Durable rules from capturing the FR-40 Codex JSONL fixture corpus.

---

## L1 - Do not assume every `item.completed` has a paired `item.started`

**Source incident:** CLO-373 implementation needed `missing-agent-message.jsonl`. Codex CLI 0.130.0 produced real streams where final `item.completed` events with nested `item.type == "agent_message"` had no paired `item.started` line. The approved fallback originally assumed paired `item.started`/`item.completed` trimming, but the actual stream only had the completed event.

**Rule:** Fixture capture and parser tests must preserve observed Codex event shapes. Do not invent pairing invariants for `item.started`/`item.completed`; only require pairs for item types where the real captured stream actually emits both.

**How to apply:** When creating missing-message or tool-call fixtures, inspect the source stream first and document exactly which real lines were removed or scrubbed. Parser tests should assert the semantics needed for the task (for example, no final `agent_message`) without assuming every item has a start event.

---

## L2 - Scrub gates must distinguish credential tokens from usage-token fields

**Source incident:** CLO-373 pre-PR validation flagged the README scrub regex because a bare `token` search would match legitimate Codex usage fields such as `input_tokens`, `cached_input_tokens`, `output_tokens`, and `reasoning_output_tokens`.

**Rule:** JSONL secret scanners should not grep bare `token` when the format legitimately contains token-count field names. Match credential-shaped keys/values (`access_token`, `auth_token`, quoted `token`, `Bearer`, API-key markers) and high-entropy value shapes instead.

**How to apply:** For Codex fixture tests, combine explicit credential key markers with a high-entropy-string heuristic. Keep README/manual scrub commands aligned with the automated gate so the documented command can pass on valid fixture files.
