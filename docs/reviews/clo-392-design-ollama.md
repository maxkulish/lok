# Design Review: CLO-392

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-05-24
**Pipeline**: lok design-review (manual fallback)

---

## Review: CLO-392 Codex Health Probe Design

### Verdict: **APPROVE_WITH_SUGGESTIONS**

The design is well-structured and addresses a real pain point. The approach is sound, but there are a few gaps and one code issue that should be addressed before implementation.

---

### **Key Findings**

**Strengths**

- Clear problem statement with user impact (token waste, opaque failures)
- Well-scoped goals/non-goals prevent scope creep
- Const-based flag matrix avoids unnecessary dependencies
- 2-second timeout is appropriate for a version probe
- Cache integration prevents redundant subprocess spawns
- Rollback strategy is clean (trait contract unchanged)
- Assumptions include confidence levels — good practice

**Issues**

1. **Code snippet has truncation artifact** — The "After" code shows `.m…15 tokens truncated…` which breaks the example. Should show the complete error transformation.

2. **`compare_versions` signature is malformed** — Written as `(u32, u32, u32) -> (u32, u32, u32) -> bool` but should be `fn compare_versions(installed: (u32,u32,u32), required: (u32,u32,u32)) -> bool`.

3. **Exit code not checked** — The proposal spawns `codex --version` and parses stdout, but doesn't verify the process exited with code 0. A crashing binary might still produce partial output.

4. **stderr ignored** — No mention of capturing or logging stderr from the version probe. Useful for diagnostics if the binary exists but fails.

---

### **Blind Spots / Edge Cases**

| Gap | Risk | Recommendation |
|-----|------|----------------|
| Binary exists but crashes on `--version` | Could parse garbage and fail closed (acceptable), but error message will be confusing | Check `output.status.success()` before parsing |
| Multiple version strings in output | Parser grabs first triplet — good, but not documented | Add to assumptions or test case |
| `BACKEND_CACHE` poisoned during warmup | Document claims "high confidence" but doesn't handle the case | Add `.unwrap_or_else(|_| fallback)` or document why safe |
| Flag matrix source ownership | `docs/investigations/codex-quick-ref.md` is authoritative but who maintains it? | Add a note about keeping matrix in sync during release |

---

### **Actionable Items** (prioritized)

1. **Fix code snippets** — Restore truncated "After" example and correct `compare_versions` signature
2. **Check exit code** — Validate `output.status.success()` before parsing version
3. **Log stderr on failure** — Include stderr in error context for debugging
4. **Document matrix sync** — Add note about flag matrix maintenance responsibility
5. **Add test case** — Binary exists but returns non-zero exit code from `--version`
6. **Consider: poison handling** — Either add defensive code or strengthen the assumption

---

### **Minor Nits**

- Test table uses `0.117.5` as "ancient" — ensure this version actually exists or use a clear placeholder
- Open question #1 (warning location) — recommend the workflow layer to keep backend focused
- Open question #2 (prefix variance) — design already addresses this with "extract first triplet" approach

---

The design is ready to implement after addressing items 1-3 (code accuracy, exit code check, stderr logging). The remaining items are improvements that could be addressed during implementation or tracked separately.
