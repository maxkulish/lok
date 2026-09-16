# Review Synthesis: clo-623

**Synthesized**: 2026-09-16
**Pipeline**: lok design-review
**Reviewers**: Codex/Ollama (glm-5.3:cloud), Claude (fallback if needed)

---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Ollama | OK | Verdict APPROVE_WITH_SUGGESTIONS, 9 findings |
| Claude fallback | SKIPPED | Not needed because the Ollama review succeeded |

## Source
Only the Ollama review produced output, so every finding below comes from that one reviewer. I checked the main claims against `docs/designs/clo-623-pr-review-cycle-tested.md`, and the Finding column says where the design doc confirms or weakens each one.

## Key Findings
| # | Finding | Severity |
|---|---------|----------|
| 1 | **The fake `gh` can't run `unresolved-threads`.** Its allowlist (design line 116) covers only `api <path>`, `--paginate`, `--slurp`, `-X POST` and `-f body=<text>`. There is no `graphql` path and no `-f query=<text>`. Because the fake exits 1 on any unknown flag, `test_unresolved_threads_fails_closed_when_graphql_errors` (line 248) can't run as written. **Confirmed.** | P1 |
| 2 | **A hung `gh api` call can block a gate forever.** `--timeout` only limits the poll loop. Nothing in the design limits a single request, and no test covers a hung request. Note that macOS has no `timeout` binary by default, so a POSIX fix needs a background process plus `kill`, or a documented dependency. **Confirmed that the design says nothing about this.** | P1 |
| 3 | **The Goals section counts the subcommands wrong.** Line 11 says "five subcommands", but lines 54 and 304 add `unresolved-threads` as a sixth. **Confirmed.** | P2 |
| 4 | **The CI grep guard is written as prose, not as a tested pattern list.** Decision 5 (line 307) lists the banned shapes but gives no exact regexes, no anchoring, no allowlist format, and no self-test. Broad patterns like `jq -r` and `submitted_at` could match legitimate one-liners. **Partly valid:** the shapes are listed, but the precision and the self-test are missing. | P2 |
| 5 | **The design doesn't list its POSIX shell rules** (no `local`, `[[ ]]`, `pipefail`, or GNU-only flags, and a position on `set -e`). **Partly covered already:** `shellcheck --shell=sh` plus the Ubuntu (dash) and macOS test matrix (lines 192 and 270) catch most bashisms. The one real gap is saying whether `set -e` is used, given the intentional non-zero exits. | P2 |
| 6 | **GraphQL error handling isn't spelled out in the architecture.** Only the test name covers it. The architecture should say that a non-zero exit, a non-empty `errors` array, missing or null `data`, or a malformed thread each exit 3. | P2 |
| 7 | **`require_iso8601z` may be too strict** because it rejects fractional seconds (line 86). **Low risk in practice:** GitHub REST and GraphQL return whole-second `Z` timestamps. A one-line note in the design that all consumed endpoints use this shape would be enough. | P3 |
| 8 | **The design doesn't say how the JSON output is built.** `new-comments` and `unresolved-threads` print fields that users control (`latest_body`, `path`). The design should require `jq -c`, not shell string interpolation. | P3 |
| 9 | **Three edge-case tests are missing:** `unresolved-threads` with several threads across pages, `created_at` exactly equal to `--since`, and a head change between the head lookup and the first poll. | P3 |

## Verdict
APPROVE_WITH_SUGGESTIONS. The reviewer said the design is ready to implement once findings 1 and 2 are fixed.

## Priority Actions
1. **(P1)** Add the `graphql` path and `-f query=<text>` to the fake `gh` allowlist and fixture resolver. Add a test that checks the fake accepts the GraphQL call shape.
2. **(P1)** Choose a way to cap each request that works on POSIX and macOS, or document why the risk is accepted, and add a test for a hung request.
3. **(P2)** Change line 11 to "six subcommands" and add `unresolved-threads` to the list.
4. **(P2)** Write the guard's exact regexes, the file scope and the allowlist format into decision 5, and add a self-test that fails on each banned shape.
5. **(P2)** Add the fail-closed GraphQL rules to the architecture section, next to the other exit-3 rules.
6. **(P2)** State the `set -e` / `set -u` choice. The other POSIX rules can rely on `shellcheck --shell=sh`.
7. **(P3)** Require `jq -c` for all JSON output.
8. **(P3)** Note that GitHub returns whole-second timestamps.
9. **(P3)** Add the three boundary tests.
