# Design Review: clo-180

**Reviewer**: Codex via Ollama (glm-5:cloud)
**Reviewed**: 2026-03-30
**Pipeline**: lok design-review

---

## 1. Completeness Check

| Section | Status | Assessment |
|---------|--------|------------|
| Summary | Present | Clear problem statement and solution overview |
| Background | Present | Good context on current state, prior research cited |
| Architecture | Present | Good component diagram and affected components table |
| Detailed Design | Present | Struct definitions, interface changes, call site patterns |
| Implementation Plan | Present | Phased approach with specific tasks and verification steps |
| Acceptance Criteria | Present | Testable criteria with verification methods |

**Missing/Weak sections:**
- No explicit **Security** section (security review is mentioned in acceptance but not as a design concern)
- No explicit **Concurrency Safety** section (async/await context considerations)
- No **Rollback Plan** (what if the change needs to be reverted?)
- No **Performance Impact** analysis (will struct allocation affect hot paths?)

---

## 2. Architecture Assessment

### Strengths
- **Minimal blast radius**: The `QueryOutput` struct is a clean abstraction that localizes change to one return type
- **Backward compatible**: `QueryResult.output` remains stdout-only, preserving existing consumer contracts
- **Clear separation**: CLI backends populate stderr/exit_code, API backends return `None`/`None` - simple mental model
- **Good constructor pattern**: `from_stdout()` / `from_process()` encourages correct usage over raw struct construction
- **Proper trait design**: Using `async_trait` maintains consistency with existing `Backend` trait

### Concerns

1. **No `Debug` trait derived** - Open question acknowledges this, but `Debug` should be derived by default for debugging/logging.

2. **Stderr capture gap in Gemini** - The design correctly notes `gemini.rs:62-63` pipes stderr, but **line 71 already discards it**. The design says "the fix is returning existing captured data" but Gemini currently does:
   ```rust
   let stderr = String::from_utf8_lossy(&output.stderr);  // captured but...
   if !output.status.success() {
       anyhow::bail!("Gemini failed: {}", stderr);  // only used for errors
   }
   Ok(self.parse_output(&stdout))  // stderr lost
   ```
   The plan correctly identifies this, but it's worth noting that **no code currently preserves stderr on success**.

3. **Missing `Eq`/`PartialEq`** - For test assertions and comparisons, these traits would be useful.

---

## 3. Codebase Alignment

### Alignment with existing patterns

| Pattern | Design Follows? | Notes |
|---------|-----------------|-------|
| `async_trait` usage | Yes | Already used in Backend trait |
| `anyhow::Result` | Yes | Consistent error handling |
| `Arc<dyn Backend>` sharing | Yes | QueryOutput is owned/cloned, no Arc needed |
| `tokio::process::Command` | Yes | Already used in CLI backends |
| `Stdio::piped()` for subprocess | Yes | Already used for both stdout/stderr |
| `QueryResult` wrapper struct | Yes | Design preserves this pattern |

### Violations / Concerns

1. **File Map in Plan is incomplete** - Plan lists `conductor.rs`, `debate.rs`, `team.rs`, `spawn.rs` as needing changes, but **design document's "Affected Components" table says `main.rs` and `tasks/*.rs` instead**. This is a discrepancy. The actual grep shows:
   - `conductor.rs:185` - confirmed
   - `debate.rs:230` - confirmed
   - `team.rs:82,121` - confirmed
   - `spawn.rs:173,282` - confirmed
   
   The design document's affected components table appears to conflate direct `backend.query()` calls with indirect usage via `run_query_with_config`/`QueryResult`.

2. **`QueryResult` vs `QueryOutput` naming** - `QueryResult` is the existing struct that wraps backend output with metadata (backend name, elapsed_ms). `QueryOutput` is the new struct from `query()`. This is confusing naming. Consider `ProcessOutput` or `BackendOutput` instead.

3. **No cache.rs consideration** - `src/cache.rs:17` imports `QueryResult` and has `impl From<&QueryResult> for CachedResult`. The design doesn't mention cache impacts, but since `QueryResult.output` stays the same, this is likely fine.

---

## 4. Security Review

### Security Posture: Acceptable

| Concern | Status | Notes |
|---------|--------|-------|
| API keys in SecretString | Already correct | `claude.rs:4` uses `secrecy::SecretString` |
| No hardcoded secrets | Verified | Keys from env vars |
| Input validation for subprocess | Review | Prompt text is shell-escaped via `sh -c` in Gemini, but direct args in others |
| Path traversal prevention | Not applicable | No file writes in this change |

### Subprocess Argument Safety

The design changes `query()` return type but **does not change subprocess invocation**. However:

- **Gemini** uses `sh -c` with interpolated prompt (escapes single quotes: `replace("'", "'\\''")`)
- **Claude CLI** and **Codex** pass prompt as direct argument with `--` separator

This is existing behavior, not changed by this design. But the current shell-escape approach is basic.

---

## 5. Implementation Concerns

### Plan Quality

The implementation plan in `docs/plans/2026-03-30-clo-180-query-output.md` is **detailed and actionable**, with step-by-step verification (`cargo check` after each phase).

### Concerns

1. **No concurrent capture pattern needed** - The design correctly notes that Gemini/Codex already pipe stderr with `Stdio::piped()`. The canonical `tokio::join!` pattern for concurrent stdout/stderr reads isn't needed because `cmd.output().await` handles this automatically.

2. **Error path unchanged** - The design correctly states that non-zero exit codes continue to `bail!`. This means `QueryOutput` is only returned on success. **Callers never see `QueryOutput.stderr` when the command fails.**

3. **`parse_output()` still applied** - The plan says `parse_output()` (skip_lines filtering) happens before `QueryOutput` construction. This is correct - the filtered stdout goes into `QueryOutput.stdout`.

4. **Bedrock feature flag** - The plan includes bedrock.rs changes but the file is `#[cfg(feature = "bedrock")]`. Tests need `--features bedrock` to compile. Plan task 6 mentions this but verification command uses `cargo build --features bedrock`. Should also test default build.

5. **File count discrepancy** - Design says "6 call sites in workflow.rs", grep shows 6 confirmed:
   - Line 797 (for_each loop)
   - Line 968 (multi-backend consensus)
   - Line 1072 (synthesis backend)
   - Line 1182 (single-backend)
   - Line 1389 (fix retry 1)
   - Line 1437 (fix retry 2)

---

## 6. Blind Spots

### Missing edge cases

1. **Empty stdout with exit code 0** - Design mentions this case (Azure CLI example) but doesn't specify behavior. `QueryOutput { stdout: "", stderr: Some("..."), exit_code: Some(0) }` is valid and should pass through. Should be explicit in acceptance tests.

2. **Stderr on error** - The design says `QueryOutput` is only for successful queries. But stderr is most valuable for debugging **failures**. Consider whether a partial `QueryOutput` should be included in the error payload (via `anyhow::Context` or custom error type).

3. **Exit code -1 fallback** - Code uses `unwrap_or(-1)` for exit codes on errors. `-1` isn't documented. Consider a constant or explicit comment.

### Unstated assumptions

1. **All backends use tokio::process::Command** - True for CLI backends, but Ollama/Bedrock use HTTP/SDK. Design correctly marks these as `stderr: None, exit_code: None`.

2. **No concurrent mutation of backend state** - Backends are `Send + Sync` and called via `Arc`. The `query()` method takes `&self`. Consistent but worth noting for future stateful backends.

3. **`is_available()` check happens once** - At backend creation time, not before each query. Existing behavior.

### Missing cross-cutting concerns

1. **Logging** - No mention of whether stderr should be logged when present. Consider `tracing::debug!` for non-empty stderr on success.

2. **Metrics** - No metrics emission for stderr length or exit codes. Future observability gap.

3. **Timeout behavior** - Existing code wraps `backend.query()` in `tokio::time::timeout`. The timeout returns `Err(_)` and bypasses `QueryOutput` entirely. Correct but should be documented.

### Integration risks

1. **`conductor.rs` uses `backend.query()` directly** (line 185) - not via `run_query_with_config`. Conductor doesn't get timeout wrapping. Design correctly identifies this for update.

2. **`spawn.rs` has two call sites** (lines 173 and 282) - both need `.stdout` extraction.

3. **`debate.rs` match arm destructuring** - Line 230 pattern matches `Ok(response)`. Needs to become `Ok(query_output)` with `.stdout` extraction. Plan covers this.

---

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is well-structured, aligns with existing patterns, and has a clear implementation path. The blast radius is contained, and backward compatibility is maintained. There are minor gaps in documentation and a discrepancy between the design doc's affected components table and the plan's file map.

---

## 8. Actionable Feedback

### Must Fix Before Implementation

| # | Item | Location | Priority |
|---|------|----------|----------|
| 1 | Resolve file map discrepancy between design doc (says `main.rs`, `tasks/*.rs`) and plan (correctly identifies `conductor.rs`, `debate.rs`, `team.rs`, `spawn.rs`) | Design doc "Affected Components" table | High |
| 2 | Add `#[derive(Debug, Clone)]` to `QueryOutput` - don't leave this as an open question | Design "Detailed Design" section | Medium |

### Should Fix (Non-Blocking)

| # | Item | Location | Priority |
|---|------|----------|----------|
| 3 | Consider naming `QueryOutput` -> `ProcessOutput` or `BackendOutput` to avoid confusion with `QueryResult` | Design "Detailed Design" section | Medium |
| 4 | Add acceptance test for empty stdout with exit code 0 | Design "Evaluation" section | Medium |
| 5 | Document that `stderr` on error is discarded (only successful queries return `QueryOutput`) | Design "Constraints" section | Low |
| 6 | Add logging consideration: log non-empty stderr at debug level on success | Design "Open Questions" | Low |

### Suggestions for Future Design Docs

| # | Item | Note |
|---|------|------|
| 7 | Add explicit Security section | Standardize security review in all design docs |
| 8 | Add Rollback Plan section | What if this change needs to be reverted? |
| 9 | Consider adding `Eq`/`PartialEq` to `QueryOutput` for test assertions | Future improvement |
