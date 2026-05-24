# PRD: FR-12a — Replace Gemini CLI backend with opencode subprocess

## Overview

`CLO-394` migrates the `gemini` backend off deprecated `@google/gemini-cli` so existing lok workflows remain functional as Google deprecates the old CLI path.

## Problem

Lok currently invokes Gemini through `npx @google/gemini-cli`, with subprocess handling, health checks, and error hints still keyed to the old binary. This migration must preserve the `gemini` backend contract while switching execution to `opencode`.

## Goals

1. Keep backend key `gemini` stable for users and configuration.
2. Invoke `opencode` with the documented invocation pattern, including positional prompt support.
3. Preserve/restore token usage accounting for `StepResult` via `QueryOutput.usage`.
4. Update docs/config/validation paths to reflect opencode dependency and authentication model.

## Functional Requirements

### FR-12a.1 Invocation

- Replace default `GeminiBackend` invocation from:
  `npx @google/gemini-cli --output-format json ... <prompt>`
  to:
  `opencode run --format json --model google/<model> --agent <plan|build> [--dangerously-skip-permissions] -- <prompt>`.
- For each call:
  - `read-only` → `--agent plan`
  - `workspace-write` (default) → `--agent build`
  - `danger-full-access` → `--agent build --dangerously-skip-permissions`

### FR-12a.2 Parsing and usage

- Parse opencode JSON event output and extract final response text.
- Extract usage fields when present into `TokenUsage` (`prompt`, `completion`, `cached`, `reasoning` if available).
- Preserve successful-text fallback if output is not parseable, to avoid regressions on event-shape drift.

### FR-12a.3 Defaults and config

- Update `Config::default` (`src/config.rs`) Gemini backend entries to:
  - `command = Some("opencode")`
  - `args = ["run", "--format", "json"]`
  - `skip_lines` removed/unused
  - `timeout = 600s`
  - model default `google/gemini-2.5-flash` (or approved equivalent)
- Update existing tests:
  - `test_gemini_backend_defaults` should assert new command + args behavior and no Gemini-specific flags.

### FR-12a.4 Availability checks and health hints

- Update doctor checks in `src/main.rs` from (`gemini`, `npx`) to (`gemini`, `opencode`) with updated install hint:
  `brew install anomalyco/tap/opencode` or `curl -fsSL https://opencode.ai/install | bash`.
- Remove hard `GOOGLE_API_KEY` required check for Gemini path; auth is handled through `opencode auth login` (with env fallback noted in docs only).

### FR-12a.5 Documentation and developer ergonomics

- Update any `gemini`/sandbox mapping comments and setup docs to describe the opencode flow.
- Keep `src/backend/context.rs` sandbox comment synchronized with FR mapping.

## Non-goals

- No long-lived opencode daemon/persistent attach mode.
- No protocol migration to any opencode SDK crate in this issue.
- No changes to existing workflow files outside this backend migration.

## Acceptance criteria

- Command path invokes `opencode` with positional prompt + `--agent` mapping.
- `CLO-394` command mapping supports `read-only`, `workspace-write`, and `danger-full-access` sandbox modes.
- `StepResult.usage` is populated from opencode usage output when present.
- `cargo test` and `cargo clippy -- -D warnings` pass.
- Existing configurations using backend `gemini` continue to run without TOML schema changes.

## Risks / mitigation

- Opencode event schema may differ from assumptions; parser should be tolerant (graceful fallback while preserving user-visible output).
- New auth/setup path differs from old `GOOGLE_API_KEY`; update checks/comments accordingly to avoid misleading setup diagnostics.
