# Design Review: CLO-180 - Extend Backend::query() to return QueryOutput struct

**Reviewed**: 2026-03-30
**Reviewer**: Claude (code-level validation against src/)
**Design Document**: docs/design-docs/clo-180-query-output-struct.md

---

## 1. Completeness Check

| Section | Present | Assessment |
|---------|---------|------------|
| Summary | Yes | Clear problem statement - changing `Result<String>` to `Result<QueryOutput>`. Links to PRD and blocking tasks. |
| Background | Yes | Strong. Discovery report findings are specific and grounded (line numbers, file references). |
| Architecture | Yes | Component diagram is accurate. Affected components table is mostly correct (see concerns below). |
| Detailed Design | Yes | `QueryOutput` struct, trait change, and caller updates are well-specified with code examples. |
| Implementation Plan | Yes | 4 phases, clearly ordered. |
| Acceptance Criteria | Yes | 7 testable items with a verification command. |
| Evaluation | Yes | 6 tests with edge cases listed. |
| Testing Strategy | Yes | Unit + integration + manual. |

**Assessment**: All sections are present and substantive. This is a well-structured design document.

## 2. Architecture Assessment

**Strengths**:
- The `from_text()` / `from_process()` constructor pattern is clean and makes the API/CLI distinction explicit at the type level.
- The decision to extract `.stdout` at the `run_query_with_config()` boundary is correct - it preserves full backward compatibility for all `QueryResult` consumers (main.rs, tasks/*.rs, cache.rs, output.rs).
- The "Must-not" constraints are well thought out - especially "must not merge stderr into stdout" and "must not change QueryResult struct."
- Error path preservation (bail on non-zero exit, QueryOutput only for success) maintains behavioral compatibility.

**Concerns**:

1. **Incomplete call site count**: The design document states "6 direct backend.query() call sites in workflow.rs" and "~10 consumers in main.rs/tasks/*.rs." The actual count from source is:
   - `workflow.rs`: 6 sites (lines 797, 968, 1072, 1182, 1389, 1437) - correct
   - `backend/mod.rs`: 1 site (line 143, `run_query_with_config`) - mentioned
   - `conductor.rs`: 1 site (line 185) - **MISSING from design**
   - `spawn.rs`: 2 sites (lines 173, 282) - **MISSING from design**
   - `debate.rs`: 1 site (line 230) - **MISSING from design**
   - `team.rs`: 2 sites (lines 82, 121) - **MISSING from design**

   The design document says "debate.rs: No change" and "tasks/*.rs: No change" because they use `QueryResult.output`. This is partially wrong - `debate.rs` calls `backend.query()` directly (not through `run_query`), so it IS affected. Similarly, `team.rs`, `spawn.rs`, and `conductor.rs` all call `backend.query()` directly and will break at compile time.

   **Total direct call sites that need `.stdout` extraction**: 13 (not 7 as implied by the design).

2. **`conductor.rs` uses `result.len()`**: At line 191, the conductor calls `result.len()` on the return value of `backend.query()`. When the return type changes from `String` to `QueryOutput`, this will fail to compile. The fix is `result.stdout.len()`, but this call site is not listed in the design.

3. **`query_with_system()` is not addressed**: `ClaudeBackend::query_with_system()` (claude.rs:181) returns `Result<String>` and is called by `query()` internally. Since `query()` returns `Result<QueryOutput>`, the internal plumbing in claude.rs needs updating. The `query_api()` and `query_cli()` methods both return `Result<String>` - the wrapping to `QueryOutput` should happen at the `Backend::query()` impl level. This is likely what the design intends but it should be explicit about the internal method chain.

## 3. Codebase Alignment

**Follows existing patterns**:
- Using `anyhow::Result` for all returns - consistent
- `async_trait` usage on `Backend` trait - consistent
- Constructor pattern (`new()` on backends) - `QueryOutput::from_text()`/`from_process()` is analogous
- `Stdio::piped()` already used for stderr in all CLI backends (gemini.rs:63, codex.rs:70, claude.rs:165)

**Pattern observations**:
- Gemini backend applies `parse_output()` (skip_lines) before returning stdout. The design correctly notes this should happen before `QueryOutput` construction, but the code example in "CLI backends" section shows raw `String::from_utf8_lossy` going into `QueryOutput::from_process()`. For Gemini, it should be `self.parse_output(&stdout_str)` going into the `stdout` field. Same for Codex's `self.parse_output()`.
- The open question about `parse_output()` placement is answered correctly ("before") but the code example doesn't reflect it.

**No violations found** against the existing Backend trait contract.

## 4. Security Review

- No security concerns with this change. It is a pure internal refactoring of return types.
- No new subprocess arguments, no new file I/O, no new network calls.
- Stderr capture does not introduce any secret leakage risk - stderr from LLM CLI tools contains diagnostic info, not credentials.

## 5. Implementation Concerns

1. **Phase ordering is correct but Phase 3 scope is incomplete**: Phase 3 says "Update 6 backend.query() call sites in workflow.rs to use .stdout." The actual scope is 13 call sites across 5 files (workflow.rs, conductor.rs, spawn.rs, debate.rs, team.rs). Missing these will cause compile errors.

2. **`query_with_system` return type**: The `ClaudeBackend::query_with_system()` method is public (used by `conductor.rs` indirectly via `ClaudeBackend` downcast). If `query()` returns `QueryOutput` but `query_with_system()` still returns `String`, the internal call chain needs careful handling. The design should specify whether `query_with_system` changes signature or remains `String`.

3. **Derive traits (Open Question #1)**: The design asks whether `QueryOutput` should derive `Debug`, `Clone`. Based on usage patterns:
   - `Debug` is needed (error messages, logging)
   - `Clone` is likely needed if any caller stores the output
   - Recommendation: `#[derive(Debug, Clone)]` at minimum

4. **Empty stderr semantics**: For CLI backends that succeed, stderr can be empty or contain warnings. The design handles this correctly (capture as `Some("")` for CLI, `None` for API). But the code example uses `String::from_utf8_lossy(&output.stderr).to_string()` which will produce `Some("")` for empty stderr. Consider using `Some(stderr_str).filter(|s| !s.is_empty())` to normalize empty stderr to `None` - or document that `Some("")` and `None` have different semantics.

## 6. Blind Spots

1. **Missing call sites** (critical): `conductor.rs`, `spawn.rs`, `debate.rs`, and `team.rs` all call `backend.query()` directly and are not listed in the affected components table. These will fail to compile after the trait change.

2. **Cache serialization**: `src/cache.rs` has `From<&QueryResult>` and `From<CachedResult> for QueryResult`. Since `QueryResult` is unchanged, the cache is safe. But this dependency is not mentioned - worth noting for confidence.

3. **No derive macros specified**: The `QueryOutput` struct in the code example has no derive macros. At minimum `Debug` is needed for error contexts. `Clone` is likely needed for the `conductor.rs` tool-call pattern. `Serialize`/`Deserialize` may be needed if `QueryOutput` is ever cached.

4. **Gemini's parse_output filtering destroys stderr context**: The Gemini backend's `parse_output()` strips `skip_lines` from stdout. If Gemini prefixes stdout with MCP noise (the exact problem motivating this PRD), and `skip_lines` is configured to remove it, then the filtered noise should arguably go into stderr rather than being silently dropped. The design doesn't address this nuance - it's a future concern but worth flagging.

5. **Thread safety**: `QueryOutput` needs to be `Send + Sync` since it passes through `tokio::spawn` boundaries in `workflow.rs` and `spawn.rs`. Plain `String` and `Option<String>` and `Option<i32>` are `Send + Sync`, so this is safe, but the design should note this requirement.

6. **Feature-gated Bedrock**: The design mentions Bedrock but doesn't address the `#[cfg(feature = "bedrock")]` gating. The `QueryOutput` struct itself is not feature-gated (correct), but the design should note that testing requires both `cargo test` and `cargo test --features bedrock`.

7. **`query_with_system` callers**: The conductor module uses `ClaudeBackend` directly (not through the `Backend` trait) for tool-call orchestration. If `query_with_system` signature changes, `conductor.rs` is affected beyond just the `backend.query()` call site.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design is architecturally sound, well-researched, and correctly identifies the core change needed. The `QueryOutput` struct design is clean. The decision to extract `.stdout` at the `run_query_with_config` boundary preserves full backward compatibility for the most common path.

The primary issue is an **incomplete blast radius analysis** - the design lists only workflow.rs and backend/mod.rs as callers needing changes, but 4 additional files (conductor.rs, spawn.rs, debate.rs, team.rs) also call `backend.query()` directly. This is a compile-time error, not a runtime risk, so it will be caught immediately, but the implementation plan should be updated to avoid surprises.

## 8. Actionable Feedback

**P0 (Must fix before implementation)**:
1. Add `conductor.rs` (1 site), `spawn.rs` (2 sites), `debate.rs` (1 site), and `team.rs` (2 sites) to the Affected Components table and Implementation Plan Phase 3. Total direct `backend.query()` call sites: 13.
2. Update the architecture diagram to show conductor.rs, spawn.rs, debate.rs, and team.rs as direct `backend.query()` consumers.

**P1 (Should fix)**:
3. Add `#[derive(Debug, Clone)]` to the `QueryOutput` struct definition.
4. Clarify whether `query_with_system()` (claude.rs:181) changes signature or remains `Result<String>` with wrapping happening only at the `Backend::query()` impl level.
5. Update the CLI backend code example to show `parse_output()` being applied before `QueryOutput::from_process()` construction (for Gemini and Codex).
6. Resolve Open Question #1 (derive traits) - recommend `Debug, Clone` at minimum.

**P2 (Nice to have)**:
7. Document empty stderr semantics: `Some("")` vs `None` for CLI backends with no stderr output.
8. Note that cache.rs is unaffected (for reviewer confidence).
9. Add `cargo test --features bedrock` to the evaluation table.
10. Consider `Some(stderr).filter(|s| !s.is_empty())` normalization for cleaner downstream matching.

---

*This review was produced by reading the actual source files in src/backend/, src/workflow.rs, src/conductor.rs, src/spawn.rs, src/debate.rs, and src/team.rs to validate the design document's claims against the codebase.*
