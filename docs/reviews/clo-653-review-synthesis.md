# Review Synthesis: CLO-653

**Synthesized**: 2026-08-07
**Design Document**: docs/designs/clo-653-backend-cache-key.md
**Pipeline**: run by hand — `lok run .lok/workflows/design-review.toml` is broken

---

## Reviewer Status

| Reviewer | Status | Detail |
|---|---|---|
| Codex via Ollama (`glm-5.2:cloud`) | OK | 47s, 7.7KB, verdict `APPROVE_WITH_SUGGESTIONS`. Run by hand with `-c model_reasoning_effort=high` |
| Claude fallback | SKIPPED | Unreachable — the pipeline dies at `ollama_review`, which is upstream of the fallback |
| Pipeline (`lok run`) | FAILED | Two independent defects, filed as CLO-655 and CLO-656 |

The workflow failed before any reviewer ran. `${#OUTPUT}` at `.lok/workflows/design-review.toml:40` is read by MiniJinja as an unterminated comment (`{#` opens one), so the template never renders. The reported error named `{{ steps.health_check.output }}`, a variable that resolves correctly — `map_template_error` reports every template failure as `UnknownVariable` and, lacking a source range, blames the first `{{ }}` in the template. A second layer sits behind it: `~/.codex/config.toml` sets `model_reasoning_effort = "xhigh"`, which `glm-5.2:cloud` rejects, so fixing line 40 alone would not have produced a review either.

Because only one reviewer produced output, this is a Single Review synthesis.

## Source

Codex via Ollama (`glm-5.2:cloud`). Full text at `docs/reviews/clo-653-review-ollama.md`.

## Key Findings

| # | Finding | Severity | Disposition |
|---|---|---|---|
| 1 | `BackendConfig` / `RetryPolicy` may carry `f64` fields, which would make the `Eq + Hash` derive fail to compile | Blocker-adjacent | **Not applicable — disproven** |
| 2 | Read-then-write TOCTOU in `create_backend`; two callers can both construct on the same key | Medium | Applied — documented as accepted, with the reason it is safe |
| 3 | Cache growth is now bounded by distinct configurations, not provider count | Medium | Applied — documented as accepted |
| 4 | `is_available()` becomes false for directly-constructed providers; verify nothing relies on the old behaviour | Medium | Applied — verified clean, recorded |
| 5 | The config is cloned into both the map and the provider | Low | Applied — noted, with `Arc<BackendConfig>` as the escape hatch |
| 6 | `any_cached_health_by_name` returns a `HashMap`-order arbitrary entry | Low | Applied — now prefers a probed-available entry |

## Verdict

`APPROVE_WITH_SUGGESTIONS`, with the single blocker-adjacent item resolved by evidence rather than by change.

## Finding 1 in detail — why it does not apply

The reviewer reasoned that LLM provider configs commonly carry `temperature`, `top_p` and similar `f64` fields, which implement neither `Eq` nor `Hash`. Sound general reasoning, and the review was deliberately given only the design document, so it could not check. It does not hold for this crate: `BackendConfig` is a process and endpoint config, not a sampling config. Every field of both types is `Eq + Hash` — the enumeration now sits in the design doc.

Confirmed by compiling rather than by inspection. Both derives were added temporarily; `cargo check --all-targets` and `cargo check --all-targets --features bedrock` compiled clean, and the change was reverted with `src/` left identical to `HEAD`.

## Finding 2 in detail — one nuance the review missed

The race is real and pre-existing, and config keying does make same-key contention likelier. But the review treats last-writer-wins as merely wasteful, when under this design it is also *safe* for a reason worth stating: equal keys imply equal `BackendConfig` and equal `RetryPolicy`, so the two racing instances are equivalent and it does not matter which survives. Under name-only keying they could differ arbitrarily, which is the defect being fixed. The race therefore gets strictly less dangerous, not more.

The suggested `entry().or_insert_with(...)` fix was rejected: it would hold the write lock across construction, including the bedrock arm's `block_in_place`, which is a worse trade than one redundant HTTP client.

## Applied Changes

1. Added a compile-verified field-by-field hashability table (Finding 1)
2. Added a "Concurrent construction of the same key" section and a Constraints entry (Finding 2)
3. Added a "Cache growth bound" section and a Constraints entry (Finding 3)
4. Recorded the `is_available` verification and added a re-check to Phase 3 (Finding 4)
5. Noted the double-clone and the `Arc<BackendConfig>` escape hatch (Finding 5)
6. Specified probed-available preference for `any_cached_health_by_name` (Finding 6)
7. Editorial: closure form in the `is_available` snippet; `cargo test --test backend_public_api` added to the AC verification command so it matches Phase 6
8. Confirmed `get_backend_cache` is genuinely `pub` (`backend/mod.rs:433`), so the API table's semver claim is accurate as written

## Flagged for the user

None. No suggestion contradicted a prior decision recorded in `docs/adrs/`, `docs/context/`, or `CLAUDE.md`.

## Follow-ups filed

- **CLO-655** (High) — `${#VAR}` in a workflow step body parses as an unterminated Jinja comment, killing the design-review pipeline. Includes a comment documenting the second `model_reasoning_effort` failure layer
- **CLO-656** (Medium) — every template failure is reported as `UnknownVariable` naming the first `{{ }}` in the template

Neither will land on this branch.
