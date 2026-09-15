# Spec Review Synthesis: clo-660

**Synthesized**: 2026-09-15
**Pipeline**: lok spec-review

---

Synthesizing from the single valid review. The Ollama review succeeded; the Claude fallback was skipped, so there is no second source to cross-reference. All findings below come from Ollama alone.

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| - | No cross-reference possible: only one reviewer produced output | n/a |

## Disagreement (Needs Human Decision)
| # | Topic | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|
| - | None | n/a | Fallback skipped, no position |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | Evaluation #3b has a false positive that makes AC10 unmeetable as written: the `rg 'cargo install --git' \| rg -v -- '--locked'` sweep matches the unrelated `cargo install --git https://github.com/ducks/git-agent` line at `README.md:374`, which correctly lacks `--locked` | Ollama | High |
| 2 | No evaluation sweeps living docs outside the seven target files for stale publishing claims. The escalate clause exists but has no command to discover a hit | Ollama | Medium |
| 3 | AC2 (docs.rs link removed, Quick Start `--locked`, install comment naming both `lok` and `lokomotiv`) has no explicit evaluation row | Ollama | Medium |
| 4 | AC3 (lib.rs `# Versioning` no longer claims publication) has no explicit evaluation row | Ollama | Medium |
| 5 | AC5 (ROADMAP Phase 13 Summary row counts CLO-660), AC8 (ADR trigger line changed and nothing else), AC9 (PROJECT Up Next row with required notes) are only covered by an unnamed read-check | Ollama | Low |
| 6 | `docs.rs/lokomotiv` could be added to the strict grep so a stale docs.rs link cannot survive AC10 | Ollama | Low |
| 7 | Evaluation #11's exact `(executables \`lok\`, \`lokomotiv\`)` grep is brittle across Cargo versions; exit code and `--version` output should be named as the authoritative checks | Ollama | Low |

Ollama also confirmed, with no findings: problem statement matches the Linear task, constraints match (no Cargo/Makefile/CI edits, historical records untouched, no contact with `ducks`, no crate-name decision in this task), the four-sub-task decomposition is clean and independent, and the example code aligns with `src/lib.rs:120-125` re-exports, the `Backend::query` contract in `src/backend/mod.rs:318-343`, and the `create_backend` signature in `src/backend/mod.rs:362-366`.

## Consolidated Verdict
**APPROVE_WITH_SUGGESTIONS**

Single reviewer verdict was APPROVE_WITH_SUGGESTIONS. The Claude fallback was skipped, not failed, so this is not a degraded result. Finding 1 is the one item that should be fixed before implementation starts, since the spec cannot pass its own AC10 gate otherwise.

## Priority Actions
1. **Fix Evaluation #3b** so it targets this project's install command only, for example `rg -n 'cargo install .*lokomotiv' $LIVING | rg -v -- '--locked'` or scoping the pattern to `https://github.com/maxkulish/lok`. Without this AC10 fails on `README.md:374` every time.
2. **Add a sweep of living docs outside the seven target files** for `lokomotiv|crates\.io|docs\.rs|cargo install lokomotiv`, excluding the historical directories (reviews, designs, design-docs, plans, discovery, specs, status, lessons, prds). Wire its result to the existing escalate clause.
3. **Add explicit evaluation rows for AC2 and AC3**: a strict grep that `docs.rs/lokomotiv` is absent from `README.md` and `src/lib.rs`, a check that Quick Start includes `--locked`, and a check that the `# Versioning` section no longer claims publication.
4. **Name the read-checks for AC5, AC8 and AC9** in the verification-method paragraph, and for AC8 add a targeted diff that only line 152 of the ADR changed.
5. **Add `docs.rs/lokomotiv` to the strict grep** in Evaluation #3.
6. **Soften Evaluation #11's executables grep** or note that exit code plus `--version` output are authoritative.
