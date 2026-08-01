# Spec Review Synthesis: clo-591

**Synthesized**: 2026-07-31
**Pipeline**: lok spec-review

---

## Reviewer Status

| Source | Result | Notes |
|---|---|---|
| Gemini | ✅ Success | Full review, verdict `APPROVE_WITH_SUGGESTIONS` |
| Ollama (Codex / glm-5:cloud) | ❌ REVIEW_FAILED | Empty output — the CLI printed only its banner ("Reading additional input from stdin...") and returned nothing |
| Claude fallback | ⏭️ Skipped | Not triggered because Gemini succeeded |

Single-source synthesis. No cross-reference is possible, so nothing below carries multi-reviewer confirmation — treat every item as one reviewer's opinion, not consensus.

## Findings (Single Reviewer — Gemini)

| # | Finding | Category | Severity |
|---|---------|----------|----------|
| 1 | Sub-task 3 must mandate a custom `env_logger` formatter in `main.rs`. Default `env_logger::init()` emits timestamps, target names, and level prefixes, which would visibly regress CLI output once console printing is replaced by the `log` facade (loss of concise yellow warning prefixes and retry glyphs). | Constraint gap | High |
| 2 | No acceptance criterion covers CLI output parity. Add one asserting the terminal presentation is unchanged after the migration to `log`. | AC gap | High |
| 3 | Internal-only `pub` items in `src/backend/` (`is_cache_entry_fresh`, `parse_health_cache_ttl`, `resolve_health_cache_ttl`, `HEALTH_TTL_LOGGED`, cache structs) leak into the library's public surface. Sub-task 5 should explicitly demote them to `pub(crate)`. | Encapsulation | Medium |
| 4 | AC1 / Test 1 only assert absence of `indicatif`, `colored`, `clap` from `cargo tree --no-default-features`. Should also cover the other binary-only deps gated in sub-task 1: `chrono`, `dirs`, `futures`, `minijinja`, `sha2`. | AC coverage | Medium |
| 5 | Stdout/stderr capture in Tests 5/6/7 (`tests/backend_public_api.rs`) is flaky under Rust's default parallel test runner. Spec should mandate serialization (serial lock) or subprocess invocation. | Test reliability | Medium |
| 6 | No Clippy gate for either feature set. Making deps optional commonly introduces unused-import warnings under `--no-default-features`. | Verification gap | Low |

## What the Review Confirmed as Sound

Not action items, but worth keeping stable through revision:

- Problem statement is complete and goes beyond the Linear ticket by catching stdout-contaminating sandbox warnings and verbose Claude CLI diagnostics.
- `log` facade over `tracing` — correct weight for a library boundary.
- `default = ["cli"]` + `required-features = ["cli"]` on `[[bin]]` — idiomatic Rust feature gating.
- Decomposition is correctly ordered (routing library warnings through `log` before making `colored` optional) and each sub-task fits under ~2 hours.
- The `TestLockGuard` opaque wrapper correctly avoids leaking Tokio lock types across the library boundary.
- All six ADR divergence rows are addressed; no codebase-alignment violations found.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

Derived from a single reviewer. The rule (any `NEEDS_REVISION` → `NEEDS_REVISION`) had no second opinion to apply against, so confidence in this verdict is lower than a normal multi-source pass. Items 1 and 2 are close to the revision threshold on their own — a CLI UX regression would ship silently since no AC currently catches it.

## Priority Actions

1. **Add the custom logger formatter constraint** (findings 1 + 2, treat as one unit). Add a **Must** constraint: the binary logger in `main.rs` implements a custom formatter that suppresses timestamp/target/level headers and renders warnings with the existing yellow prefix style. Pair it with a new AC asserting CLI terminal output retains layout and color parity.
2. **Expand AC1 and Test 1** to assert `chrono`, `dirs`, `futures`, `minijinja`, and `sha2` are also absent from the `--no-default-features` dependency tree.
3. **Extend sub-task 5** to demote internal-only `pub` items in `src/backend/` to `pub(crate)`.
4. **Specify the stream-capture strategy** for Tests 5/6/7 — serial lock or subprocess, not ambient parallel capture.
5. **Add Clippy gates** to the evaluation section: `cargo clippy --all-targets -- -D warnings` and the same with `--no-default-features`. This aligns with the existing repo convention from commit `e37d2c8` (`ci: lint test code by running clippy with --all-targets`).

## Recommendation on the Failed Reviewer

Ollama produced no output at all rather than a substantive failure — the stderr shows the Codex CLI starting normally against `glm-5:cloud` and then returning empty. Given the spec touches feature-gating and public API boundaries where a second opinion has real value, consider re-running the Ollama leg before acting on this synthesis, or falling back to Claude for a genuine second source.
