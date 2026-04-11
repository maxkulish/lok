# Spec Review Synthesis: clo-211

**Synthesized**: 2026-04-11
**Pipeline**: lok spec-review

---

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| 1 | `format` command semantics between apply and verify are unclear/problematic given `RetryLoop` couples them tightly (Gemini: architecturally impossible; Ollama: failure behavior undefined when `apply_edits=true`) | High |
| 2 | git-agent checkpoint behavior change lacks coverage (Gemini: per-attempt -> per-step granularity loss not acknowledged; Ollama: no test that rollback uses `Rollback::rollback` when git-agent is *available*) | Medium |
| 3 | Re-query prompt / error-message formatting vs legacy is under-specified (Gemini: `previous_raw` context dropped; Ollama: exact format strings not preserved for backward compat) | Medium |
| 4 | Timeout case formatting in step result message is ambiguous (Gemini: `{exit_or_timeout}` risks `Option::None`; Ollama: `step.timeout = 0` handling undefined) | Medium |

## Disagreement (Needs Human Decision)
| # | Topic | Gemini Position | Ollama Position |
|---|-------|-----------------|-----------------|
| 1 | Severity of `format`/`RetryLoop` coupling | Hard blocker - C-6 directly contradicts MN-7; demands API change (`pre_verify` hook or `format` field) | Soft concern - just clarify failure semantics; no API change proposed |
| 2 | Overall verdict | NEEDS_REVISION | APPROVE_WITH_SUGGESTIONS |
| 3 | ST-2 scope | Acceptable as-is | Should be split into ST-2a..2d (~250-line diff is risky) |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | ST-1 templates omit `context.previous_raw`, forcing LLM to guess its own failing output | Gemini | High |
| 2 | Apply-only path (`apply_edits=true, verify=None`) has no AC or test; `apply_once` helper location unspecified | Ollama | High |
| 3 | `StepResult.elapsed_ms` instrumentation unclear - does `RetryLoop` expose timing or does workflow keep outer `start` instant? | Ollama | Medium |
| 4 | CLI output parity not addressed - legacy prints `"Fix attempt N/M"`, `"Re-querying LLM..."`; `RetryLoop` lacks a `Reporter` hook | Ollama | Medium |
| 5 | `stop_on_parse_error=false` default needs explicit regression test | Ollama | Medium |
| 6 | ST-6 should clarify `FileEdit` stays (used by `EditParser`); only `AgenticOutput` + helpers are deletable | Ollama | Low |
| 7 | `max_output_bytes` (1 MiB) should cross-reference `EditParser::MAX_INPUT_SIZE` to stay in sync | Ollama | Low |
| 8 | Validation phase (post-fix-loop, legacy ~2100-2150) scope vs CLO-211 unclear | Ollama | Low |

## Consolidated Verdict
**NEEDS_REVISION** (Gemini blocker on format/RetryLoop conflict; any NEEDS_REVISION -> NEEDS_REVISION)

## Priority Actions

**P0 - Blockers (resolve before implementation)**
1. Resolve C-6 vs MN-7 conflict: either amend MN-7 to allow a `pre_verify`/`format` hook on `RetryLoop`, or explicitly move `format` outside the loop and document the linter-as-verifier risk. (Agreement #1, Disagreement #1)
2. Inject `previous_raw` into ST-1 re-query prompt templates; spec exact format strings to preserve legacy behavior. (Agreement #3, Novel #1)
3. Add AC + test for apply-only path (`apply_edits=true, verify=None`) and specify `apply_once` helper location. (Novel #2)

**P1 - Spec gaps (fill before merge)**
4. Document git-agent checkpoint granularity change (per-attempt -> per-step) in AC-7/C-7, or add a per-attempt callback to `EditRequester`; add test for "git-agent available but rollback still uses `Rollback::rollback`". (Agreement #2)
5. Specify timeout formatting in step message - distinct "Timeout" vs "Exit code N" strings - and define `step.timeout=0` -> `Verification` mapping. (Agreement #4)
6. Clarify `StepResult.elapsed_ms` source (outer `start` vs `RetryLoopOutcome`). (Novel #3)
7. Decide CLI output parity: add `Reporter` trait to `RetryLoop`, or mark as explicit non-goal with follow-up task. (Novel #4)

**P2 - Polish**
8. Add regression test for `stop_on_parse_error=false` default. (Novel #5)
9. Clarify in ST-6 that `FileEdit` is retained (used by `EditParser`). (Novel #6)
10. Consider splitting ST-2 into verify-fail / verify-timeout / cleanup sub-tasks. (Disagreement #3)
11. Cross-reference `max_output_bytes` with `EditParser::MAX_INPUT_SIZE`. (Novel #7)
12. Confirm validation phase scope vs CLO-211. (Novel #8)
