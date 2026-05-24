## Design Document Review: CLO-391

### Verdict: **APPROVE_WITH_SUGGESTIONS**

The design is well-structured and technically sound. It follows existing patterns, addresses the requirements clearly, and includes appropriate safeguards. A few minor enhancements would improve operational robustness.

---

### Key Findings

**Strengths**
- Clean separation of `probe_api()` / `probe_cli()` concerns with mode-based dispatch
- Async-safe: Uses `tokio::process::Command` and `OnceLock<Mutex<HashMap>>` for memoization
- Secure: API key accessed via `Secret<String>::expose_secret()`, no network calls for API probe
- Good timeout discipline: 2s budget for CLI commands is appropriate
- Aligns with Ollama backend patterns (CLO-389)
- Clear acceptance criteria with specific test cases

---

### Actionable Items (Prioritized)

**P1 — Should Add**
1. **Logging strategy missing**: No mention of what to log when probes fail or timeout. Add:
   - Log level for timeout events (warn?)
   - Log for version parse failures (debug?)
   - Log for cache hit/miss on help output (trace?)
   
2. **No diagnostic field in `HealthStatus`**: When `available: false`, users have no visibility into *why*. Consider adding `diagnostic: Option<String>` to surface reasons like "ANTHROPIC_API_KEY not set" or "claude binary not found on PATH".

**P2 — Nice to Have**
3. **Whitespace-only API key**: The empty check `api_key.expose_secret().is_empty()` passes but doesn't catch `"   "` — add `.trim().is_empty()` or document this as intentional.

4. **Partial probe failure behavior**: If `--version` succeeds but `--help` times out, the doc says "treat as unsupported" — clarify whether `version` is still populated or if both fields are cleared.

5. **Help output format brittleness**: If `claude --help` output format changes, the `--output-format json` detection could break. Consider documenting that this is a best-effort check and failures should not block availability.

**P3 — Future Consideration**
6. **Binary update detection**: If `claude` is updated in-place (same path, new version), the help cache key (version-based) handles this correctly — but document that stale processes with old `--help` output aren't a concern.

---

### Minor Nits

- **Open Questions**: The HashMap vs LruCache question is correctly answered — version count is O(1), no need for eviction.

- **Evaluation table**: Test case 4 mocks `claude` binary — ensure the test harness has a strategy for this (e.g., temp PATH override or mock trait).

---

### Summary

Solid design ready for implementation. The P1 items (logging + diagnostics) would significantly improve troubleshooting in production but aren't blockers. The concurrency model, error handling, and alignment with existing patterns are all correct.


**Duration**: 20s
