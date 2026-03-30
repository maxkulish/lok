# Review Synthesis: CLO-180 - Extend Backend::query() to return QueryOutput struct

**Synthesized**: 2026-03-30
**Reviewers**: Claude (code-validated), Codex/Ollama (glm-5:cloud)
**Failed Reviewers**: Gemini 3.1 Pro (produced MCP initialization noise, no review content)
**Design Document**: docs/design-docs/clo-180-query-output-struct.md

---

## Agreement (High Confidence)

Items where 2+ reviewers independently identified the same concern.

| # | Finding | Reviewers | Severity |
|---|---------|-----------|----------|
| 1 | **Incomplete call site count**: Design doc lists only workflow.rs (6 sites) and backend/mod.rs as needing changes. Actual count is 13 direct `backend.query()` call sites across 6 files: workflow.rs (6), conductor.rs (1), spawn.rs (2), debate.rs (1), team.rs (2), backend/mod.rs (1). conductor.rs, spawn.rs, debate.rs, and team.rs are missing from the Affected Components table. | Claude, Ollama | HIGH |
| 2 | **Missing derive traits**: `QueryOutput` should derive `Debug, Clone` at minimum - this should not be left as an open question. | Claude, Ollama | MEDIUM |
| 3 | **parse_output() ordering**: Code examples for CLI backends show raw stdout going into `QueryOutput`, but Gemini and Codex apply `parse_output()` filtering first. The design notes this correctly in text but code examples don't reflect it. | Claude, Ollama | MEDIUM |
| 4 | **Stderr lost on error path**: Only successful queries return `QueryOutput`. When a command fails (non-zero exit), stderr goes into the `bail!` error message. This is a deliberate design choice but should be explicitly documented. | Claude, Ollama | MEDIUM |
| 5 | **Empty stdout edge case**: Design mentions exit code 0 with empty output as an edge case but no acceptance test covers it. | Claude, Ollama | MEDIUM |

---

## Disagreement (Needs Human Decision)

| # | Topic | Position A (Reviewer) | Position B (Reviewer) |
|---|-------|----------------------|----------------------|
| 1 | **Naming**: `QueryOutput` vs alternative | Keep `QueryOutput` - consistent with `QueryResult` pattern (Claude) | Rename to `ProcessOutput` or `BackendOutput` to reduce confusion with `QueryResult` (Ollama) |

---

## Novel Insights (Single Reviewer)

| # | Finding | Reviewer | Severity |
|---|---------|----------|----------|
| 1 | `conductor.rs:191` calls `result.len()` on the `backend.query()` return - will fail to compile since `QueryOutput` has no `.len()` | Claude | HIGH |
| 2 | `query_with_system()` (claude.rs:181) returns `Result<String>` and is called internally by `Backend::query()` - the wrapping to `QueryOutput` needs explicit specification at the impl level | Claude | MEDIUM |
| 3 | Cache serialization (cache.rs) is unaffected but should be noted for implementer confidence | Claude | LOW |
| 4 | Thread safety: `QueryOutput` must be `Send + Sync` for tokio::spawn boundaries - it is, but the design should note this requirement | Claude | LOW |
| 5 | Consider `Some(stderr).filter(|s: &String| !s.is_empty())` to normalize empty stderr to `None` for cleaner downstream matching | Claude | LOW |
| 6 | Non-empty stderr on successful CLI commands should be logged at debug level | Ollama | LOW |
| 7 | `Eq`/`PartialEq` traits would be useful for test assertions | Ollama | LOW |
| 8 | No explicit Rollback Plan or Performance Impact sections in design doc | Ollama | LOW |

---

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Both completing reviewers (Claude, Ollama) independently reached the same verdict: APPROVE_WITH_SUGGESTIONS. The design is architecturally sound, well-researched, and correctly identifies the core change needed. The primary gap is an incomplete blast radius analysis that misses 6 of 13 direct call sites.

Note: Gemini's failure (producing MCP initialization noise instead of a review) is itself a demonstration of the exact problem CLO-180 aims to solve - unstructured process output being indistinguishable from useful content.

---

## Priority Actions

Ordered by severity, agreement items first:

1. **[HIGH]** Fix the Affected Components table and architecture diagram: add conductor.rs (1 site), spawn.rs (2 sites), debate.rs (1 site), team.rs (2 sites). Total direct `backend.query()` sites: 13. (agreed by: Claude, Ollama)
2. **[HIGH]** Fix `conductor.rs:191` - `result.len()` will fail to compile when return type changes from `String` to `QueryOutput`. (source: Claude)
3. **[MEDIUM]** Add `#[derive(Debug, Clone)]` to `QueryOutput` struct definition. Resolve Open Question #1 now. (agreed by: Claude, Ollama)
4. **[MEDIUM]** Clarify `query_with_system()` handling - specify that wrapping happens at the `Backend::query()` impl level, not in the internal methods. (source: Claude)
5. **[MEDIUM]** Update CLI backend code examples to show `parse_output()` applied before `QueryOutput::from_process()`. (agreed by: Claude, Ollama)
6. **[MEDIUM]** Add acceptance test for empty stdout with exit code 0. (agreed by: Claude, Ollama)
7. **[MEDIUM]** Document that stderr is lost when commands fail (bail path). (agreed by: Claude, Ollama)
8. **[LOW]** Add `cargo test --features bedrock` to evaluation table alongside default build test. (source: Ollama)
9. **[LOW]** Consider debug-level logging for non-empty stderr on successful CLI commands. (source: Ollama)
