# Roadmap - Lok

**Last Updated**: 2026-04-09

## Summary

| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 2: Validation Pipeline | 3 | 3 | Complete |
| Phase 2.5: Validation Resilience | 3 | 3 | Complete |
| Phase 3: Failure Classification | 1 | 1 | Complete |
| Phase 4: Backend Error Types & Retry | 3 | 3 | Complete |
| Phase 5: Enrich QueryOutput | 1 | 0 | Planned |
| Phase 6: Config Merging | 1 | 1 | Complete |
| Phase 7: MiniJinja Templates | 2 | 2 | Complete |
| Phase 8: Apply-and-Verify Pipeline | 3 | 2 | In Progress |
| Phase 9: Configurable Role Routing | 1 | 0 | Planned |

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
| [CLO-207](https://linear.app/cloud-ai/issue/CLO-207) | Extend QueryOutput with model, duration, usage, structured, backend | Backlog | CLO-202 |

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
| [CLO-211](https://linear.app/cloud-ai/issue/CLO-211) | Wire apply-verify pipeline into workflow step execution | Backlog | CLO-205, CLO-210, CLO-202 |

## Phase 9: Configurable Role Routing

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-212](https://linear.app/cloud-ai/issue/CLO-212) | Add configurable role routing with [roles]/[teams] config | Backlog | CLO-203 |
