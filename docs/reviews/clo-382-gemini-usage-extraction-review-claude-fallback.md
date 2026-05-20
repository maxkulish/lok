# Design Review: CLO-382 — Gemini Backend Token Count Extraction

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-05-20
**Pipeline**: lok design-review (fallback — external reviewers failed)
**Design Document**: `docs/designs/clo-382-gemini-usage-extraction.md`

---

## 1. Completeness Check

| Section | Status | Notes |
|---------|--------|-------|
| Problem | ✓ Present | Clear, with concrete line references (gemini.rs:81, :140-148) |
| Goals / Non-goals | ✓ Present | Well-scoped; explicitly excludes trait/type changes, conductor-side work, and future FRs |
| Architecture | ✓ Present | Data-flow diagram correct; new types are private and non-exported |
| Public API Surface | ✓ Present | Correctly notes zero surface change |
| Assumptions | ✓ Present | Explicitly ranked (high/medium/low); capture-before-implement assumption is flagged as open |
| Test Plan | ✓ Present | Unit + fixture + regression matrix + manual verification steps |
| Migration / Rollout | ✓ Present | Backward-compat reasoning solid (additive field, text-mode fallback) |
| Open Questions | ✓ Present | Three substantive questions that do not block the core design |

## 2. Architecture Assessment

**Strengths**:
- The fallback design (approach A from discovery) is exactly the right choice: attempt JSON parse, fail closed to text mode, preserve backward compatibility.
- Private `GeminiEnvelope` / `GeminiStats` types scoped inside `gemini.rs` avoid polluting `backend/mod.rs` with backend-specific schema types. This mirrors what Codex does with its event types.
- The duplicate-flag guard in `build_shell_cmd` (`args.iter().any(|a| a == "--output-format")`) is lightweight and correct — no regex or parsing overhead.

**Concerns**:
- The design does not explicitly address what happens if `--output-format json` causes the Gemini CLI to emit non-zero exit code (older CLI version that doesn't recognise the flag). The PRD risk table notes this, but the design doc only mentions it in the migration section. Adding a pre-emptive note in the "Assumptions" or "Architecture" section would make the fallback strategy clearer: the child process still exits 0/1 as before; the JSON parse is only attempted on exit-success.

## 3. ADR Compliance

No active ADRs exist in `docs/adrs/` for this project. The design aligns with the existing codebase conventions:
- `TokenUsage` builder pattern (`with_cached`) is already established in `src/backend/mod.rs`.
- `QueryOutput::from_process(...).with_model(...).with_usage(...)` matches the Codex backend pattern.
- No unsafe code, no new external deps.

## 4. Security Review

No security concerns. The change:
- Parses stdout from a subprocess that lok itself spawned (trusted input boundary).
- Uses `serde_json::from_str` on a string already in memory (no new file reads for untrusted input).
- Does not introduce new shell-quoting paths beyond the existing `build_shell_cmd`.
- Does not touch auth, secrets, or network code.

## 5. Implementation Concerns

1. **Fixture dependency**: The design correctly flags that `GeminiStats` field names are unverified until a real fixture is captured. This is the #1 schedule risk. Recommend adding a 24-hour timebox for fixture capture; if it doesn't happen, proceed with the documented field names and file a follow-up Linear issue for schema verification.

2. **Partial-stats behaviour**: The open question on missing `promptTokenCount` vs `candidatesTokenCount` is well-framed. Returning `None` when either is missing is the conservative choice and matches the PRD AC "Missing stats block: usage = None".

3. **`skip_lines` backward compatibility**: The existing `skip_lines` config field becomes a no-op in JSON mode (no lines to skip) but is still respected in text-mode fallback. This is correct and not called out as a concern, which is fine.

## 6. Blind Spots

- **Envelope size**: Gemini CLI — unlike Codex JSONL — emits a single JSON envelope. If a long response produces a very large stdout, `serde_json::from_str` will allocate the entire envelope as a `serde_json::Value` and then again as a `String` in `GeminiEnvelope.response`. For responses above a few megabytes, this doubles memory briefly. In practice, Gemini CLI is not used for multi-megabyte codegen today, but a brief note on `#[serde(borrow)]` or streaming alternatives would be a nice-to-have.
- **Error envelopes**: The design notes that error envelopes carry `{ "error": { ... } }` but does not define a typed parse for them. The child exit-code check happens *before* JSON parsing, so an error envelope from a non-zero exit is already handled by the existing `BackendError::ExecutionFailed` path. This is correct — the JSON parse only runs on success. No action needed, but worth an inline comment in the code.
- **Double-parse**: stdout is parsed as JSON once in `parse_gemini_envelope`, and if that fails, `parse_output` performs line skipping on the same string. This is fine — the JSON parse is fast and the fallback only happens when JSON fails.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is sound, the scope is correctly bounded, and the fallback strategy preserves backward compatibility. The only open item is fixture capture, which is tracked as a discovery debt and does not block approval of the design itself.

## 8. Actionable Feedback

1. **[RECOMMENDED]** In the `GeminiEnvelope` struct, add a `#[serde(deny_unknown_fields)]` guard? No — `deny_unknown_fields` would break forward compatibility if Gemini CLI adds new envelope fields. Keep the default lenient deserialization.
2. **[RECOMMENDED]** Add an inline code comment noting: "Error envelopes are handled by the existing exit-code check before this parse runs."
3. **[OPTIONAL]** Consider adding a `MAX_ENVELOPE_BYTES` constant (e.g., 10 MB) and skip JSON parsing if stdout exceeds it, falling back to text mode. This is a belt-and-suspenders defence against pathological CLI output.
4. **[OPTIONAL]** Document in the implementation PR that `--output-format json` may cause older gemini-cli versions to exit non-zero, and that the existing `BackendError::ExecutionFailed` path handles this gracefully.
