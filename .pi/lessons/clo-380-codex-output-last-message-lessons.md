# Lessons: CLO-380 - FR-3b Codex `-o` authoritative result extraction

Rules discovered while completing FR-3b that are likely reusable for later
Codex/backend changes.

---

## L1 - Preserve JSONL as diagnostics, but not as the only success source

**Source incident:** CLO-380 implementation in `src/backend/codex.rs` showed that malformed JSONL can coexist with a valid
`-o` output file. If success text depended solely on parser success, users would see false parse failures while the
actual final answer existed.

**Rule:** Keep JSONL parsing for `turn.failed` / `error` and usage accounting, but prefer `--output-last-message` as the
primary success-text path. Fall back to parsed `agent_message` only when the side-channel file is missing/empty.

**How to apply:**
- Pass `-o <tmpfile>` on each Codex invocation and read it after process exit.
- Consider this source authoritative for success text when non-empty.
- Still retain JSONL-driven terminal-failure semantics and usage extraction from fallback events.

---

## L2 - Preserve CLI failure precedence even when fallback text exists

**Source incident:** In codex error paths with stale/unsupported `-o` behavior, non-zero exits still need to propagate as
execution failures even if the last-message file contains content.

**Rule:** A readable process stderr combined with parser parse failures should remain a hard execution-failure path;
fallback precedence must not mask genuine failures.

**How to apply:**
- If `Command` exits non-zero and stderr is non-empty, return a failed BackendError first.
- Keep `parse_error`/`turn.failed` as authoritative when the invocation itself failed.
