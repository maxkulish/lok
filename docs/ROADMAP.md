# Roadmap - Lok

**Last Updated**: 2026-04-03

## Summary

| Phase | Tasks | Completed | Status |
|-------|-------|-----------|--------|
| Phase 2: Validation Pipeline | 3 | 3 | Complete |
| Phase 3: Failure Classification | 1 | 0 | In Progress |

## Phase 2: Validation Pipeline

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-182](https://linear.app/cloud-ai/issue/CLO-182) | Extend StepResult with stderr, exit_code, validation fields | Done | CLO-180 |
| [CLO-183](https://linear.app/cloud-ai/issue/CLO-183) | Implement heuristic validators (check field) for step validation | Done | CLO-182 |
| [CLO-184](https://linear.app/cloud-ai/issue/CLO-184) | Implement LLM-based step validation (validate.backend + prompt) | Done | CLO-183 |

## Phase 3: Failure Classification

| Task | Title | Status | Dependencies |
|------|-------|--------|--------------|
| [CLO-185](https://linear.app/cloud-ai/issue/CLO-185) | Implement structured failure data for step errors | In Progress | CLO-184 |
