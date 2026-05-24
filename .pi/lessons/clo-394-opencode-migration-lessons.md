# Lessons: CLO-394 OpenCode Gemini Backend Migration

Durable rules from swapping a deprecated CLI backend to opencode subprocess invocation.

---

## L1 - OpenCode NDJSON streams: joining all `type: "text"` events works for single-turn but pollutes multi-step agentic runs

**Source incident:** CLO-394 implementation. `parse_opencode_output` (gemini.rs:413) accumulates every `type == "text"` event into `response_parts` and joins them. For single-turn `opencode run` invocations this is correct — the fixture emits one text event followed by `step_finish`. For agentic multi-step runs with intermediate tool calls, this would mix tool output and intermediate assistant messages into `QueryOutput.stdout`. Codex flagged this (MEDIUM severity) in pre-PR validation; the resolution was a doc comment documenting the known limitation and deferring final-text semantics to a follow-up.

**Rule:** When parsing NDJSON event streams from LLM subprocess backends, distinguish "final response text" from "all text events." For backends that emit `step_finish` or `stop` events, anchor response text extraction to the terminal event rather than accumulating all text-bearing events. Until a real multi-text fixture is captured, document the current behavior as a known limitation.

**How to apply:** Before writing an NDJSON parser for any backend:
1. Capture a fixture from the actual CLI with a simple prompt (single event expected).
2. Add a second fixture with an agentic prompt that triggers tool calls (multi-event stream).
3. Derive the parser from both fixtures — the multi-event fixture proves the parser doesn't over-collect.
4. If multi-step capture isn't feasible, add a `// KNOWN LIMITATION` comment citing the fixture gap.

---

## L2 - Bot reviewers (gemini-code-assist) catch real defects: lossy integer narrowing via `as`

**Source incident:** CLO-394 PR review. `pick_u32` in gemini.rs:56 used `n as u32` to narrow a `u64` from `serde_json::Value::as_u64()`. The gemini-code-assist bot flagged this as a lossy truncation (MEDIUM) and suggested `.try_into().unwrap_or(u32::MAX)`. The fix was applied immediately; the bot subsequently acknowledged it. This validates the PR review cycle's bot-review step as catching real issues, not just style nits.

**Rule:** Treat bot-review findings as first-class signals. Even MEDIUM-severity structural concerns (lossy casts, unsafe patterns, unbounded allocations) may surface defects the author missed. Do not dismiss them as false positives without a concrete rationale.

**How to apply:** In the PR review cycle, categorize bot comments by severity:
- HIGH: likely to cause runtime failures or security issues → fix before merge
- MEDIUM: structural concerns, potential correctness issues → fix or decline with rationale
- LOW: style/preference → accept or decline

If a MEDIUM comment is declined, record the rationale in both the thread reply and a `.pi/lessons/` entry so sibling tasks don't re-litigate the same call.
