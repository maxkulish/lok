# Project Dashboard - Lok

**Last Updated**: 2026-04-07

## Active Work (WIP Limit: 3)

| Task | Title | Status | Phase | Blocked By |
|------|-------|--------|-------|------------|
| - | - | - | - | - |

## Up Next (Prioritized Backlog)

| Priority | Task | Title | Dependencies |
|----------|------|-------|--------------|
| - | - | - | - |

## Recently Completed

| Task | Title | Completed | Summary |
|------|-------|-----------|---------|
| [CLO-205](https://linear.app/cloud-ai/issue/CLO-205) | Implement EditParser with 3-format auto-detection | 2026-04-07 | EditParser auto-detects unified diff/JSON/full-file formats with markdown extraction, CRLF normalization, 1MB limit, 32 tests |
| [CLO-204](https://linear.app/cloud-ai/issue/CLO-204) | Add MiniJinja integration with TemplateContext and custom filters | 2026-04-04 | MiniJinja 2.0 template engine with TemplateContext, LazyEnv, 8 custom filters, TemplateError enum, 48 tests |
| [CLO-203](https://linear.app/cloud-ai/issue/CLO-203) | Implement three-layer config merge with deny_unknown_fields | 2026-04-04 | Three-layer config merge (defaults -> user -> project), toml::Value deep merge, deny_unknown_fields on all structs |
| [CLO-202](https://linear.app/cloud-ai/issue/CLO-202) | Add BackendError enum with typed variants and is_retryable() | 2026-04-04 | Added BackendError enum (8 variants), is_retryable(), typed errors on Backend::query(), QueryResult.error field |
| [CLO-185](https://linear.app/cloud-ai/issue/CLO-185) | Implement structured failure data for step errors | 2026-04-03 | Added StepFailure struct with StepFailureKind enum (6 variants) for execution-level failure classification |
| [CLO-184](https://linear.app/cloud-ai/issue/CLO-184) | Implement LLM-based step validation (validate.backend + prompt) | 2026-04-03 | Added LLM validation via validate.backend + validate.prompt with JSON structured output, fail-closed parsing, on_error policy |
| [CLO-183](https://linear.app/cloud-ai/issue/CLO-183) | Implement heuristic validators (check field) for step validation | 2026-04-03 | Added not_empty, min_length, contains validators via [steps.validate] TOML config |
| [CLO-182](https://linear.app/cloud-ai/issue/CLO-182) | Extend StepResult with stderr, exit_code, validation fields | 2026-03-31 | Added raw_output, stderr, exit_code, and ValidationResult fields to StepResult |
| [CLO-181](https://linear.app/cloud-ai/issue/CLO-181) | - | - | - |
| [CLO-180](https://linear.app/cloud-ai/issue/CLO-180) | - | - | - |

## Blocked

| Task | Title | Blocked By | Notes |
|------|-------|------------|-------|
| - | - | - | - |
