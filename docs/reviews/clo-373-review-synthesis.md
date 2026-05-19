# Review Synthesis: clo-373

**Synthesized**: 2026-05-19
**Pipeline**: lok design-review
**Reviewers**: Gemini 3.1 Pro, Codex/Ollama (glm-5:cloud), Claude (fallback if needed)

---

## Reviewer Status
| Reviewer | Status | Detail |
|----------|--------|--------|
| Gemini | OK | APPROVE_WITH_SUGGESTIONS, 4 findings |
| Ollama | OK | APPROVE_WITH_SUGGESTIONS, 5 prioritized items + minor suggestions |
| Claude Fallback | OK | APPROVE_WITH_SUGGESTIONS, 13 actionable items (P0/P1/P2) |

## Agreement (High Confidence)
| # | Finding | Severity |
|---|---------|----------|
| 1 | Filter fixture iteration to `path.extension() == Some("jsonl")` — co-located `README.md` will otherwise break the line-validity test (Gemini, Claude) | P0 |
| 2 | Scrub checklist must cover API keys, bearer tokens, and similar credentials — POSIX path/username regex alone is insufficient (Gemini, Ollama, Claude) | P0 |
| 3 | Tests must validate fixture *semantics*, not just JSON validity — confirm `reasoning_output_tokens > 0` and presence of `agent_message` where claimed (Ollama, Claude) | P0 |
| 4 | Resolve the `missing-agent-message.jsonl` capture strategy before merge rather than deferring to FR-3a (Gemini, Ollama, Claude) | P1 |
| 5 | Add a CI scrub-regex gate over `tests/fixtures/codex/*.jsonl` — README-only enforcement leaks (Ollama, Claude) | P1 |
| 6 | Resolve the 20 KB / 50 KB size cap one way — programmatic `assert!(metadata.len() < N)` or drop from Goals (Ollama, Claude) | P1 |
| 7 | If a hand-edited fixture is used, document it in the README so re-capture scripts/maintainers don't overwrite or partially trim it (Gemini, Claude) | P1 |

## Disagreement (Needs Human Decision)
| # | Topic | Position A (Reviewer) | Position B (Reviewer) |
|---|-------|----------------------|----------------------|
| 1 | Reasoning fixture assertion shape | Ollama: single assertion checks `reasoning_output_tokens > 0` AND nested reasoning item present | Claude: split into two independent tests — Codex can legitimately emit reasoning tokens without a discrete reasoning item, so coupling them turns a valid capture into a false failure |

## Novel Insights (Single Reviewer)
| # | Finding | Source | Severity |
|---|---------|--------|----------|
| 1 | Add `.gitattributes` entry `tests/fixtures/codex/*.jsonl text eol=lf` — Windows `core.autocrlf=true` will silently rewrite the corpus and break byte-for-byte fidelity | Claude | P0 |
| 2 | Strengthen JSONL validation: assert each line has a `type` field whose value is in the documented event set (`thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.started`, `item.completed`, `error`) | Claude | P0 |
| 3 | Assert `!parsed_vec.is_empty()` — a truncated fixture currently passes silently | Gemini | P1 |
| 4 | Record per-fixture metadata in README header: exact prompt, Codex version, capture-host OS (macOS vs Linux sandbox semantics differ), capture date | Claude | P1 |
| 5 | Pin the corpus to Codex 0.130.0 explicitly in the README; CLI bumps require re-capture, not test edits | Claude | P1 |
| 6 | Consider a second failure fixture so `turn.failed` and top-level `error` (two distinct termination modes) are pinned separately for FR-3a | Claude | P2 |
| 7 | Add `version.txt` alongside fixtures for automation-friendly version detection | Ollama | P2 |
| 8 | State in README that `cargo test --test codex_fixtures` is hermetic — no Codex install required | Claude | P2 |
| 9 | Avoid absolute `CARGO_MANIFEST_DIR` paths in test panic messages — leaks into CI logs under `--nocapture` | Claude | P2 |
| 10 | Capture-time hand-trim must remove BOTH `item.started agent_message` and final `item.completed agent_message` to preserve start/complete pairing invariant | Claude | P1 (conditional on hand-trim path) |

## Consolidated Verdict
**Overall: APPROVE_WITH_SUGGESTIONS**

All three reviewers independently arrived at APPROVE_WITH_SUGGESTIONS. The design is sound and additive; the gaps are fixture hardening (extension filtering, semantic assertions, line-ending pinning, credential scrub breadth) rather than architectural concerns.

## Priority Actions

**P0 — resolve before merge:**
1. Filter fixture discovery to `path.extension() == Some(OsStr::new("jsonl"))` so the test ignores `README.md` and backup files. *(Agreement #1)*
2. Extend the scrub checklist + regex to cover API keys, bearer tokens, emails, and Windows-style `C:\Users\` paths. *(Agreement #2)*
3. Add fixture-semantic assertions: `reasoning_output_tokens > 0` in `multi-turn-reasoning.jsonl`, `agent_message` present in `turn-completed.jsonl`. **Decide whether to combine or split** the reasoning-token + reasoning-item checks (Disagreement #1) — recommend splitting per Claude's argument that the two properties are orthogonal in Codex's actual emission. *(Agreement #3 + Disagreement #1)*
4. Add `.gitattributes` entry `tests/fixtures/codex/*.jsonl text eol=lf` to enforce stated byte-for-byte fidelity. *(Novel #1)*
5. Strengthen line validation to assert each entry has a `type` in the documented event set — catches schema drift cheaply. *(Novel #2)*

**P1 — strongly recommended:**
6. Resolve `missing-agent-message.jsonl` strategy in the design (older binary / rename / hand-trim) — if hand-trim, remove both paired `item.*` `agent_message` lines. *(Agreement #4 + Novel #10)*
7. Add a CI scrub-regex gate so the corpus cannot regress on future PRs. *(Agreement #5)*
8. Resolve the size cap — either `assert!(metadata.len() < 20_000)` or drop from Goals. *(Agreement #6)*
9. Add `assert!(!parsed_vec.is_empty())` to protect against silently truncated fixtures. *(Novel #3)*
10. Record per-fixture metadata (prompt, Codex version, capture-host OS, date) in the README and a "tests pinned to Codex 0.130.0; CLI bumps require re-capture" warning. *(Novel #4, #5)*
11. Flag any hand-edited fixture explicitly in the README so re-capture workflows don't overwrite it. *(Agreement #7)*

**P2 — nice to have:**
12. Note hermeticity of `cargo test --test codex_fixtures` in the README. *(Novel #8)*
13. Consider a second failure fixture splitting `turn.failed` and top-level `error`. *(Novel #6)*
14. Add `version.txt` for automation-friendly version pinning. *(Novel #7)*
15. Drop absolute `CARGO_MANIFEST_DIR` paths from panic messages. *(Novel #9)*
