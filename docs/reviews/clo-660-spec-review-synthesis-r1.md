# Spec Review Synthesis: clo-660

**Synthesized**: 2026-09-15
**Pipeline**: lok spec-review

---

Only one reviewer produced output. The Ollama review succeeded and the Claude fallback was skipped, so this synthesis draws on a single source. The cross-reference tables below reflect that: nothing can be marked as agreed or disputed between reviewers.

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| - | Not applicable. Only one valid review, so no cross-reviewer agreement is possible. | - |

## Disagreement (Needs Human Decision)
| # | Topic | Ollama Position | Claude Position |
|---|-------|-----------------|-----------------|
| - | None. The Claude fallback did not run. | - | - |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | AC11 is unsatisfiable as written. Evaluation #4 will hit Linear-quoted task titles in ROADMAP:46 and the PROJECT.md CLO-592 row, plus ADR lines 32, 106, 117, 127 and 200 that AC8 freezes. None of those hits can be "corrected". | Ollama | P1 |
| 2 | AC9 lets the stale premise in PROJECT.md:9 survive. The CLO-660 Active Work row still says "the real deadline is the first release that ships a library target", the exact claim the spec exists to remove, and no AC requires rewriting it. | Ollama | P1 |
| 3 | AC1 mandates a new false claim. The sentence "release tags follow `vYYYYMMDD.N.0`" is contradicted by existing tags `v20260520.0.1` and `v20260524.0.1` through `.0.3`. | Ollama | P2 |
| 4 | The AC2 install command is never tested, and the edge case "the installed binary is `lok`" is only half true. The package at the tag declares two `[[bin]]` targets, `lok` and `lokomotiv`, and `cargo install` installs both. | Ollama | P2 |
| 5 | The "replace in place" constraint undersells the decision-record exception. AC7 also requires the retitle, Implications rewrite and Related-line rewrite, not just a dated re-evaluation section. | Ollama | P3 |
| 6 | "Match surrounding style" and "never em dashes" conflict in DEPENDENCIES.md and PROJECT.md, whose prose uses em dashes. The spec should state which rule wins. | Ollama | P3 |
| 7 | README citation error. The docs.rs link is at lines 71-72, not 62-63. All other line citations checked out. | Ollama | P3 |
| 8 | Stale `Cargo.toml` fields (`documentation`, `authors`) are listed under Must-not but not recorded as a follow-up the way the DEPENDENCIES:84 `CI Gate` staleness is. | Ollama | P3 |
| 9 | Evaluation #2 silently depends on crates.io returning versions newest-first. Evaluation #8 has no baseline for "no new warnings". Network failure during Evaluations #1, #2, #7 and #13 has no explicit stop-and-escalate instruction. | Ollama | P3 |
| 10 | The AC7 re-affirm verdict is pinned before the re-evaluation runs. Defensible because the workflow YAML shows the user pre-decided it, but the spec should say so rather than present it as a sub-task output. | Ollama | P3 |
| 11 | Scope line says "docs only" but `src/lib.rs` is a source file with a rustdoc edit gated by `cargo doc` under `#![deny(missing_docs)]`. | Ollama | P3 |

The reviewer also confirmed several claims directly against the repo, which raises confidence in the parts of the spec it did not flag. The `[lib]` target exists at tag `v20260914.0.0`, the `create_backend` root re-export is not feature-gated, the `default-features = false` pattern matches the existing `library-boundary` CI job, and the Evaluation #9 historical sweep returns 12 files as stated. No constraint or codebase-alignment violations were found.

## Consolidated Verdict
**APPROVE_WITH_SUGGESTIONS**

The single reviewer returned APPROVE_WITH_SUGGESTIONS. The Claude fallback was skipped, so this verdict rests on one source. The reviewer's reasoning is that every finding is wording-level, but the two P1 items would make the spec either unsatisfiable during verification or leave a false claim in place that the task exists to remove.

## Priority Actions
1. **P1, AC11.** Reword "Every hit is a corrected statement" to allow deliberately preserved historical content, each hit carrying a one-line justification. Without this, verification cannot pass.
2. **P1, AC9.** Require the PROJECT.md CLO-660 Active Work row to drop the "first release that ships a library target" premise and state the corrected one.
3. **P2, AC1.** Replace `vYYYYMMDD.N.0` with `vYYYYMMDD.N[.P]` or an equivalent true statement, in both README and lib.rs.
4. **P2, AC2.** Add an evaluation for the install command and correct the edge case to say both `lok` and `lokomotiv` binaries are installed.
5. **P3, constraints.** State the decision-record exception fully so it covers the retitle, Implications and Related-line rewrites AC7 mandates.
6. **P3, constraints.** Declare that the no-em-dash rule overrides style-matching for new text in DEPENDENCIES.md and PROJECT.md.
7. **P3, problem statement.** Fix the README citation to lines 71-72.
8. **P3, follow-ups.** Record the stale `Cargo.toml` `documentation` and `authors` fields as a follow-up alongside the `CI Gate` item.
9. **P3, evaluations.** Sort versions in Evaluation #2, define the warnings baseline for Evaluation #8, and add an explicit stop-and-escalate rule for unreachable crates.io or GitHub.
10. **P3, wording.** Note that the AC7 verdict is a pre-decision from the workflow YAML, and change the scope line to "docs and rustdoc".
