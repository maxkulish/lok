# Review Synthesis: CLO-380 Design

**Synthesized:** 2026-05-20
**Pipeline:** manual synthesis after direct Gemini architect review (lok design-review workflow variable interpolation failed)
**Source review:** `docs/reviews/clo-380-design-gemini.md`

---

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | OK | Produced structured review with verdict `APPROVE_WITH_SUGGESTIONS`. |
| Lok design-review workflow | REVIEW_FAILED | Workflow failed before reviewers due unsupported interpolation of `steps.health_check.output` / `arg.1` in shell steps. |

## Source

Gemini was the only successful external reviewer. This synthesis uses Gemini as the sole source and records the workflow failure separately as orchestration telemetry.

## Key Findings

| # | Finding | Severity |
|---|---------|----------|
| 1 | The design's precedence hierarchy is sound: terminal JSONL failures must win over `-o`, then non-empty `-o`, then JSONL fallback. | Positive |
| 2 | Reusing `build_argv_prefix` and adding diagnostics beside the strict parser aligns with existing code. | Positive |
| 3 | Trimming leading whitespace from the `-o` file could corrupt code, Markdown, YAML, or other indentation-sensitive output. | Critical |
| 4 | Reading the last-message file with `std::fs` inside async `query()` would block the Tokio executor. | Important |
| 5 | Always passing `-o` may break older Codex versions; document the tradeoff or consider fallback if unsupported versions matter. | Moderate |
| 6 | Temporary LLM output can contain sensitive text; note that `NamedTempFile` uses private permissions. | Minor |

## Verdict

`APPROVE_WITH_SUGGESTIONS`.

## Priority Actions

1. **Applied:** update design to preserve leading whitespace and trim only trailing CR/LF.
2. **Applied:** make `read_last_message` async and specify `tokio::fs::read_to_string`.
3. **Applied:** add an assumption/test note for `NamedTempFile` private permissions.
4. **Flagged/deferred:** do not add automatic retry without `-o` for older Codex in this slice; keep the non-goal and validate noisy failure via stub test. Runtime version/capability gating remains future work if unsupported Codex versions prove common.
