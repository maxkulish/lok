# Roadmap - Lok

**Last Updated**: 2026-08-07 (CLO-653 merged, closing Phase 12 at 6 of 6; Phase 14 at 9 tasks; Phase 13 at 2 of 3)

## Summary

| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 2: Validation Pipeline | 3 | 3 | Complete |
| Phase 2.5: Validation Resilience | 3 | 3 | Complete |
| Phase 3: Failure Classification | 1 | 1 | Complete |
| Phase 4: Backend Error Types & Retry | 3 | 3 | Complete |
| Phase 5: Enrich QueryOutput | 1 | 1 | Complete |
| Phase 6: Config Merging | 1 | 1 | Complete |
| Phase 7: MiniJinja Templates | 2 | 2 | Complete |
| Phase 8: Apply-and-Verify Pipeline | 3 | 3 | Complete |
| Phase 9: Configurable Role Routing | 1 | 1 | Complete |
| Phase 10: Predictable CLI Execution (Phase 2 PRD v5) | 15 | 15 | Complete |
| Phase 11: Health Checks | 1 | 1 | Complete |
| Phase 12: Library Extraction & CI | 6 | 6 | Complete |
| Phase 13: Release Readiness | 3 | 2 | In progress |
| Phase 14: Orchestration Tooling Hardening | 9 | 1 | In progress |
| Phase 15: Security Scan Remediation | 5 | 1 | In Progress |
| Phase 16: Task-Pipeline Cleanup | 2 | 0 | Not started |

## Phase 11: Health Checks

Source: `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §9 step 6 (Health Checks + Warmup)

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-391](https://linear.app/cloud-ai/issue/CLO-391/fr-13a-claude-dual-mode-health-probe-api-vs-cli) | FR-13a: Claude dual-mode health probe (Api vs Cli) | Done | CLO-388 |

## Phase 12: Library Extraction & CI

Source: `docs/adrs/clo-589-backend-library-shape.md`

Makes the `Backend` abstraction consumable by an external crate, and puts a CI gate under the work. CLO-589 fixed the contract, CLO-593 built the target, CLO-591 cleans the surface, CLO-592 ships it; CLO-600 is what stops any of it regressing unnoticed.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-589](https://linear.app/cloud-ai/issue/CLO-589) | Record the crate-shape ADR for extracting the backend abstraction as a library | Done | - |
| [CLO-593](https://linear.app/cloud-ai/issue/CLO-593) | Extract lok's Backend abstraction into a consumable library target | Done | CLO-589 |
| [CLO-600](https://linear.app/cloud-ai/issue/CLO-600) | lok has never run a GitHub Actions workflow: no CI gate on any PR | Done | - |
| [CLO-591](https://linear.app/cloud-ai/issue/CLO-591) | Strip CLI presentation from the library surface so consumers get no terminal chrome | Done | CLO-593 |
| [CLO-592](https://linear.app/cloud-ai/issue/CLO-592) | Make the backend library consumable from crates.io with rustdoc, feature docs and a publish dry-run | Done | CLO-591 |
| [CLO-653](https://linear.app/cloud-ai/issue/CLO-653) | `BACKEND_CACHE` is keyed by name alone, so two library consumers in one process silently share one backend instance | Done | CLO-591 |

CLO-653 reopened this phase on 2026-08-07 and closed it the same day (PR #89, merged as `b413f57`). CLO-591 left it open deliberately and it was recorded as a standing constraint in `docs/DEPENDENCIES.md` rather than as an issue, which is why no backlog view showed it. It belonged here rather than in Phase 13: the two-consumers-one-process case is exactly what extracting the library was for, so the phase was not actually finished while it stood. It was the only known code defect on the library critical path, and the phase is now genuinely complete.

Two bounds replaced the constraint it removed, both documented in `src/lib.rs` where a consumer meets them: `BackendKey` cannot capture ambient construction inputs (the resolved value behind `api_key_env`, Bedrock's AWS environment), and the cache is still process-global rather than owned by the embedding host. Handing a host its own cache handle is the better long-term library shape and was deliberately deferred — it is a superset of this work, not an alternative, since a host-owned cache still needs config-aware keying inside it.

## Phase 13: Release Readiness

What still stands between the crate as it is now and a release someone outside this machine can trust. Both are cheap, and both get more expensive after a publish rather than before it: crate metadata freezes per version, and release provenance is hard to add retroactively once people are already downloading archives.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-609](https://linear.app/cloud-ai/issue/CLO-609) | Point the crate's repository and homepage metadata at maxkulish/lok | Done | - |
| [CLO-638](https://linear.app/cloud-ai/issue/CLO-638) | Verify the declared rust-version = 1.80 in CI, or raise it to what the code actually needs | Done | - |
| [CLO-610](https://linear.app/cloud-ai/issue/CLO-610) | Attest release binaries so their checksums prove origin, not only transfer | Not started | - |

CLO-638 joined this phase on 2026-08-06. It arrived as a CLO-633 follow-up, but it belongs here for the same reason as the other two: the oldest toolchain installed locally is 1.94 and `ci.yml` uses runner-stable, so `rust-version = "1.80"` is a claim nobody has tested. It costs nothing today and becomes a consumer-facing build failure the moment the crate is published.

## Phase 14: Orchestration Tooling Hardening

Nine defects in the markdown-defined orchestration commands, all found by running them rather than reading them, and all sharing one root cause: procedural instructions that nothing executes or tests until they fail in production. CLO-623 is the structural fix; the others are the individual failures that motivated it.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-623](https://linear.app/cloud-ai/issue/CLO-623) | Make pr-review-cycle shell snippets executable and tested | Not started | - |
| [CLO-624](https://linear.app/cloud-ai/issue/CLO-624) | Distinguish a bad reviewer invocation from an empty model response | Not started | - |
| [CLO-627](https://linear.app/cloud-ai/issue/CLO-627) | complete.md edits the aggregation files, then checks out main with them uncommitted | Not started | - |
| [CLO-628](https://linear.app/cloud-ai/issue/CLO-628) | gh pr merge --delete-branch silently skips the remote deletion when its local checkout fails | Not started | - |
| [CLO-637](https://linear.app/cloud-ai/issue/CLO-637) | Make /pr:review's re-review poll recognise a Qodo comment update instead of waiting for a review object | Done | - |
| [CLO-649](https://linear.app/cloud-ai/issue/CLO-649) | spec-review workflow drops the Ollama leg when Linear text contains a single quote | Not started | - |
| [CLO-650](https://linear.app/cloud-ai/issue/CLO-650) | Verify reviewer-bot identity exactly in the PR gates instead of substring matching | Not started | CLO-623 |
| [CLO-651](https://linear.app/cloud-ai/issue/CLO-651) | gh pr checks --watch reports failure instantly when no checks exist yet | Not started | - |
| [CLO-652](https://linear.app/cloud-ai/issue/CLO-652) | Pre-PR validation reports emit absolute machine paths into PR comments | Not started | - |

CLO-637 joined this phase on 2026-08-06, filed out of CLO-633. It is the fourth individual failure of the same root cause: Step 9.5 waited for a review object on the new head SHA, but Qodo submits one only when a pass carries new inline findings. A clean re-review updates its comment in place, so the gate failed precisely on the success case. Observed on PR #80.

Four more joined on 2026-08-07. CLO-649 and CLO-650 came out of the CLO-637 run; CLO-651 and CLO-652 out of the GitHub Actions outage on 2026-08-06 and the pre-PR reports that ran during it. Three of the four are the same shape as CLO-624 and CLO-637 — a gate reading absence of evidence as a verdict, or matching on a substring where an exact identity was meant. CLO-650 is the only task in the phase with a dependency: the bot-identity check belongs inside the script CLO-623 extracts, not in the markdown it replaces.

The phase has grown from four defects to nine without CLO-623 moving. That is the argument for doing CLO-623 next rather than continuing to file against it: every new task here is another copy of shell that the extraction would have to absorb anyway.

## Phase 15: Security Scan Remediation

Five findings from the codex-security scan of `6ac4694` (2026-08-03). They share an origin, not a mechanism, so none of them blocks another and they can land in any order. Two are trust-boundary work that wants a human in the loop (CLO-631, CLO-632); the other three are contained code changes. CLO-633 is first because it is the only one that already fires on ordinary input with no attacker involved.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-633](https://linear.app/cloud-ai/issue/CLO-633) | Fix slice panics on CI log truncation and out-of-range file:line references | Done | - |
| [CLO-631](https://linear.app/cloud-ai/issue/CLO-631) | Escape or remove step output interpolated into workflow shell fields | Not started | - |
| [CLO-632](https://linear.app/cloud-ai/issue/CLO-632) | Gate project-layer lok.toml backend commands behind a trust boundary | Not started | - |
| [CLO-634](https://linear.app/cloud-ai/issue/CLO-634) | Add one path-confinement helper and use it in every worktree writer and reader | Not started | - |
| [CLO-635](https://linear.app/cloud-ai/issue/CLO-635) | Default the Gemini backend to the plan agent when no sandbox is requested | Not started | - |

## Phase 16: Task-Pipeline Cleanup

Two latent defects in `src/tasks/`, both surfaced while CLO-633 was investigated rather than by the security scan that opened Phase 15, and neither of them a security finding. They sit here instead of in Phase 15 because they share that task's origin, not its subject. Both are contained changes and neither blocks anything.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-639](https://linear.app/cloud-ai/issue/CLO-639) | commit_file returns an empty string as a successful SHA when git rev-parse HEAD fails | Not started | - |
| [CLO-640](https://linear.app/cloud-ai/issue/CLO-640) | Deduplicate FILE_REF_RE and extract_file_references across tasks/context.rs and tasks/fix.rs | Not started | - |

## Phase 2: Validation Pipeline

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-182](https://linear.app/cloud-ai/issue/CLO-182) | Extend StepResult with stderr, exit_code, validation fields | Done | CLO-180 |
| [CLO-183](https://linear.app/cloud-ai/issue/CLO-183) | Implement heuristic validators (check field) for step validation | Done | CLO-182 |
| [CLO-184](https://linear.app/cloud-ai/issue/CLO-184) | Implement LLM-based step validation (validate.backend + prompt) | Done | CLO-183 |

## Phase 2.5: Validation Resilience

Driven by Mentis pre-PR validation incident (2026-04-07): Haiku returned unparseable markdown causing fail-closed step errors. See `docs/plans/2026-04-07-clo-214-216-validation-resilience.md`.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-214](https://linear.app/cloud-ai/issue/CLO-214) | Add validate.on_parse_error config (pass/skip/fail) | Done | CLO-184 |
| [CLO-215](https://linear.app/cloud-ai/issue/CLO-215) | Add --explain-validation CLI flag for raw validator response | Done | CLO-184 |
| [CLO-216](https://linear.app/cloud-ai/issue/CLO-216) | Support validate.mode = "lenient" for noise-cleanup validators | Done | CLO-184 |

## Phase 3: Failure Classification

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-185](https://linear.app/cloud-ai/issue/CLO-185) | Implement structured failure data for step errors | Done | CLO-184 |

## Phase 4: Backend Error Types & Retry

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-202](https://linear.app/cloud-ai/issue/CLO-202) | Add BackendError enum with typed variants and is_retryable() | Done | - |
| [CLO-206](https://linear.app/cloud-ai/issue/CLO-206) | Add RetryPolicy with exponential backoff, jitter, retry_after | Done | CLO-202 |
| [CLO-208](https://linear.app/cloud-ai/issue/CLO-208) | Add RetryExecutor decorator wrapping Backend trait | Done | CLO-202, CLO-206 |

## Phase 5: Enrich QueryOutput

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-207](https://linear.app/cloud-ai/issue/CLO-207) | Extend QueryOutput with model, duration, usage, structured, backend | Done | CLO-202 |

## Phase 6: Config Merging

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-203](https://linear.app/cloud-ai/issue/CLO-203) | Implement three-layer config merge with deny_unknown_fields | Done | - |

## Phase 7: MiniJinja Templates

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-204](https://linear.app/cloud-ai/issue/CLO-204) | Add MiniJinja integration with TemplateContext and custom filters | Done | - |
| [CLO-209](https://linear.app/cloud-ai/issue/CLO-209) | Replace regex interpolation in workflow.rs with MiniJinja rendering | Done | CLO-204 |

## Phase 8: Apply-and-Verify Pipeline

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-205](https://linear.app/cloud-ai/issue/CLO-205) | Implement EditParser with 3-format auto-detection | Done | - |
| [CLO-210](https://linear.app/cloud-ai/issue/CLO-210) | Implement DiffApplier, Rollback, Verification, RetryLoop | Done | CLO-205 |
| [CLO-211](https://linear.app/cloud-ai/issue/CLO-211) | Wire apply-verify pipeline into workflow step execution | Done | CLO-205, CLO-210, CLO-202 |

## Phase 9: Configurable Role Routing

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-212](https://linear.app/cloud-ai/issue/CLO-212) | Add configurable role routing with [roles]/[teams] config | Done | CLO-203 |


## Phase 10: Predictable CLI Execution (Phase 2 PRD v5)

Source: `docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §9 release plan.

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-370](https://linear.app/cloud-ai/issue/CLO-370) | Add `usage` field to StepResult for end-to-end token observability (FR-25a) | Done | CLO-207 |
| [CLO-378](https://linear.app/cloud-ai/issue/CLO-378/fr-25b-extend-tokenusage-with-cached-tokens-reasoning-tokens) | FR-25b: extend TokenUsage with cached_tokens + reasoning_tokens | Done | CLO-370 |
| [CLO-382](https://linear.app/cloud-ai/issue/CLO-382/fr-26-gemini-backend-extracts-token-counts-from-json-envelope) | FR-26: Gemini backend extracts token counts from JSON envelope | Done | CLO-378 |
| [CLO-371](https://linear.app/cloud-ai/issue/CLO-371) | Migrate `Backend::query` to `StepContext` + add async `health_check` + sweep Step call sites (FR-19a/19b/20a) | Done | CLO-370 |
| [CLO-372](https://linear.app/cloud-ai/issue/CLO-372) | Thread `StepContext` through non-Step `Backend::query` call sites (FR-20b) | Done | CLO-371 |
| [CLO-373](https://linear.app/cloud-ai/issue/CLO-373/capture-codex-jsonl-fixtures-for-parser-test-corpus-fr-40) | Capture Codex JSONL fixtures for parser test corpus (FR-40) | Done | CLO-372 |
| [CLO-380](https://linear.app/cloud-ai/issue/CLO-380/fr-3b-codex-o-output-last-message-authoritative-result-extraction) | FR-3b: Codex `-o`/`--output-last-message` authoritative result extraction | Done | CLO-381 |
| [CLO-374](https://linear.app/cloud-ai/issue/CLO-374) | Per-step sandbox routing for Codex and Gemini backends (FR-21) | Done | CLO-371 |
| [CLO-383](https://linear.app/cloud-ai/issue/CLO-383/fr-22-apply_editstrue-defaults-codex-sandbox-to-workspace-write) | FR-22: `apply_edits=true` defaults Codex sandbox to `workspace-write` | Done | CLO-374 |
| [CLO-381](https://linear.app/cloud-ai/issue/CLO-381/fr-25-codex-backend-extracts-usage-from-turncompleted-events) | FR-25: Codex backend extracts `usage` from `turn.completed` events | Done | CLO-379, CLO-378 |
| [CLO-384](https://linear.app/cloud-ai/issue/CLO-384/fr-23-per-step-timeout-layered-override-step-backend-global) | FR-23: per-step `timeout` layered override (step > backend > global) | Done | CLO-371 |
| [CLO-388](https://linear.app/cloud-ai/issue/CLO-388) | FR-9a + FR-10 + FR-15: Engine warmup + HealthCache + sync is_available cache-only | Done | CLO-371 |
| [CLO-389](https://linear.app/cloud-ai/issue/CLO-389) | FR-11 + FR-11a: Ollama health probe (/api/version + /api/tags) + ModelInfo + workflow model validation | Done | CLO-388 |
| [CLO-392](https://linear.app/cloud-ai/issue/CLO-392) | FR-13: Codex health probe + version-aware unusable-flag matrix | Done | CLO-388, CLO-371 |
| [CLO-394](https://linear.app/cloud-ai/issue/CLO-394/fr-12a-replace-gemini-cli-backend-with-opencode-subprocess) | FR-12a: Replace Gemini CLI backend with opencode subprocess | Done | CLO-371 |
| [CLO-395](https://linear.app/cloud-ai/issue/CLO-395/fr-12b-opencode-health-probe-google-auth-detection) | FR-12b: opencode health probe + Google auth detection | Done | CLO-394, CLO-388 |
