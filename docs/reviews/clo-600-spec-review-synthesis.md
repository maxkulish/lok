# Spec Review Synthesis: clo-600

**Synthesized**: 2026-07-31
**Pipeline**: lok spec-review

---

Only one of three reviewers produced output. Synthesizing from Gemini alone, with a verification pass on its highest-severity claim.

## Reviewer Status

| Source | Result |
|---|---|
| Gemini | ✅ Valid — APPROVE_WITH_SUGGESTIONS |
| Ollama (Codex / glm-5:cloud) | ❌ REVIEW_FAILED — empty output, process started but returned nothing |
| Claude fallback | ⏭️ Skipped (Gemini succeeded) |

No cross-referencing was possible. Everything below rests on a single reviewer, so confidence is correspondingly lower than a normal multi-model synthesis.

## Verified Contradiction (Reject This Finding)

Gemini's #1 priority action is **factually wrong** and would break the release workflow if applied:

> "GitHub does not provide a `macos-15-intel` runner. Public Intel macOS runners are hosted exclusively on `macos-13`."

This is inverted. `macos-13` was deprecated starting 2025-09-22 and **fully retired on 2025-12-04**; a workflow targeting it today fails with "The macOS-13 based runner images are now retired." GitHub introduced `macos-15-intel` specifically as the migration target for x86_64 macOS, available until **August 2027**, after which Actions drops x86_64 entirely.

The spec's choice of `macos-15-intel` is correct. **Do not change it to `macos-13`.** This looks like a knowledge-cutoff artifact in the reviewer.

Worth adding to the spec instead: a note that `x86_64-apple-darwin` builds have a hard end-of-life in August 2027 on GitHub-hosted runners.

## Findings Worth Acting On

| # | Finding | Severity | Where |
|---|---|---|---|
| 1 | `acquire_test_lock()` is used only in `gemini_health_check_bad_exit` and `gemini_health_check_no_auth`. Extend it to `gemini_health_check_version_timeout` plus the four `codex.rs` health-check tests, rather than changing `write_exec_script` itself. The mutex is the established codebase pattern; concurrent `fork` during the script-creation window inherits the descriptor and yields `ETXTBSY`. | High | Sub-task 2 |
| 2 | AC-5/6/7 mandate `--locked` on dependency resolution but not on the release compile itself. Add `cargo build --release --locked` explicitly so the build can't drift from `Cargo.lock`. | Medium | AC-5, AC-6, AC-7 |
| 3 | `cargo publish --dry-run` compiles and packages locally; it does not authenticate against crates.io. AC-8 should state that the token check is a static environment assertion, not credential validation. | Medium | AC-8 |
| 4 | Test 8 doesn't say whether `lok` and `lokomotiv` ship in one archive or two. Specify a single `lok-<version>-<target>.tar.gz` containing both. | Low | Sub-task 5, Test 8 |
| 5 | `release.yml` needs `permissions: contents: write`; `publish.yml` should be scoped to minimum privileges given it consumes the crates.io secret. | Low | Sub-tasks 5/6 |

## Blind Spot Noted

`ubuntu-24.04-arm` requires public-beta ARM runner access. Gemini assessed this as satisfied for a public repo — reasonable, but unverified here.

## Sections Gemini Found Clean

Problem statement (self-contained, cites real run IDs `30222596040`–`30223254125`), decomposition (7 sub-tasks, dependency chain 1→2, 3→4, (2,3)→7), and codebase alignment against the `Backend` trait, `HealthStatus`, and `BackendErrorKind` in `src/utils.rs`. The native-glibc-over-musl decision was called out as a good call that avoids a custom OpenSSL toolchain.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS** — the four surviving findings are refinements, not blockers.

Caveat: this is a single-reviewer verdict where that reviewer's top-ranked item turned out to be wrong on a checkable fact. Consider a re-run once the Ollama path is fixed before treating the "no violations, exemplary decomposition" assessments as settled.

## Priority Actions

1. **Ignore** the `macos-13` change. Keep `macos-15-intel`; optionally document the Aug-2027 x86_64 sunset.
2. Add `acquire_test_lock()` to the five remaining probe tests (Sub-task 2).
3. Add `--locked` to the release build commands in AC-5/6/7.
4. Annotate AC-8 with the dry-run authentication caveat.
5. Define the single-archive packaging strategy in Sub-task 5.
6. Pin workflow token permissions in both `release.yml` and `publish.yml`.
7. Investigate the Ollama reviewer failure — Codex v0.146.0 with `glm-5:cloud` started and returned empty stdout.

Sources: [GitHub Changelog — macOS 13 runner image is closing down](https://github.blog/changelog/2025-09-19-github-actions-macos-13-runner-image-is-closing-down/), [actions/runner-images #13045 — macOS 15 Intel-based image](https://github.com/actions/runner-images/issues/13045), [actions/runner-images #13046 — macOS 13 deprecation timeline](https://github.com/actions/runner-images/issues/13046), [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
