# Design Review: clo-373

**Reviewer**: Claude (fallback)
**Reviewed**: 2026-05-19
**Pipeline**: lok design-review
**Note**: External reviewers failed; this is the fallback review

---

I have enough context to produce the structured review. Note: `docs/arch/` and `docs/adrs/` do not exist in this repo; ADR compliance will reference the closest equivalents (`docs/design-docs/`, `docs/investigations/`, and the phase-2 PRD).

## 1. Completeness Check

All required sections present:
- Problem (with file/line refs to `src/backend/codex.rs::parse_output` and PRD §9/§11.0a/§11.4)
- Goals / Non-goals (4 explicit non-goals fencing off FR-3a/3b/25b scope)
- Architecture (ASCII diagrams for both capture-time and test-time flows)
- Public API surface (Rust signatures + capture command contract)
- Assumptions (8 items, each with confidence + verification path)
- Test plan (5 named tests + 8-step manual checklist + per-backend matrix)
- Migration / rollout (additive-only with rollback)
- Open questions (4 items, each with tradeoff and deferral target)

Nothing structural is missing. The design is unusually heavy for a test-only PR — but no section is absent.

## 2. Architecture Assessment

**Strengths**:
- Clean additive boundary: no `src/` modules touched, no new deps, no public-API change, rollback is `git revert`.
- Test-time flow is minimal: `read_dir` → `read_to_string` → `lines()` → `serde_json::from_str::<Value>`. No async, no shell invocation, no IO timing concerns.
- Deferring to `serde_json::Value` (rather than a typed event enum) is the right call — schema commitment belongs to FR-3a, not to a corpus PR.
- The four fixtures map cleanly to the four parser code paths FR-3a needs: happy, terminal-error, reasoning-bearing, no-final-message fallback.
- Each assumption ships with a verification path, not just a confidence label.

**Concerns**:
- Test helpers panic via `unwrap_or_else(|e| panic!(...))` with full `path.display()` — `--nocapture` in CI will print absolute `CARGO_MANIFEST_DIR` paths. Minor log leak.
- The design promises *byte-for-byte fidelity to Codex 0.130.0 output* but doesn't enforce it: no `.gitattributes` entry to prevent `core.autocrlf` from silently rewriting `*.jsonl` on a Windows checkout.
- `every_fixture_is_line_valid_jsonl` walks `read_dir` without an extension filter. The same directory contains `README.md`. The test will either fail or silently skip non-JSON files depending on how the filter is written. Spec is ambiguous.
- `multi_turn_reasoning_fixture_reports_reasoning_tokens` couples two orthogonal properties into one assertion: `usage.reasoning_output_tokens > 0` *and* presence of an intermediate non-`agent_message` `item.completed`. Codex can legitimately emit reasoning tokens without exposing a discrete reasoning item — that valid capture would fail the test.
- `parse_jsonl` uses `.lines()` (splits on `\n` and `\r\n`) but the architecture diagram states "split by `\n`". Mismatch is harmless today but the doc is slightly inaccurate.

## 3. ADR Compliance

No `docs/adrs/` or `docs/arch/` directory exists. Cross-checked against the closest equivalents:

- **`docs/investigations/codex-quick-ref.md`** — design cites the documented event set (`thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.started`, `item.completed`, `error`) and places `reasoning_output_tokens` on `turn.completed`. Aligned.
- **`docs/design-docs/clo-374-per-step-sandbox-routing.md`** — capture uses `-s read-only`, matching CLO-374's sandbox model. Assumption #4 cross-references `.pi/lessons/clo-374-sandbox-routing-lessons.md §L1` as a guardrail against shell-construction regressions.
- **`docs/prds/prd-phase-2-predictable-cli-execution-v5.md` §9/§11.0a/§11.4** — design honors the FR-40-before-FR-3a ordering and preserves the documented event set.
- **CLO-371/CLO-372 design docs** — design correctly does not touch `Backend::query`/`StepContext`, which are now stable on `main`.

No conflicts. Deferred decisions (event enum shape, `version.txt`) are punted to FR-3a rather than pre-empted.

## 4. Security Review

**No hardcoded secrets.** The capture command is a template; scrubbing happens via README checklist + `rg '<HOME>|/Users/|/home/|<USERNAME>|/tmp/'`.

**Gaps**:
- Scrub regex covers POSIX paths only. Misses: Windows-style `C:\Users\` paths, API keys / bearer tokens in upstream-provider error payloads (Codex's `error` event can quote `Authorization: Bearer ...` from a 401), email addresses, git remote URLs in tool-call output. Recommend a second-pass regex: `rg -iE 'bearer |api[_-]?key|sk-[a-zA-Z0-9]|ghp_|\b[\w.+-]+@[\w-]+\.[\w.-]+'`.
- Enforcement is README-only. A CI gate that re-runs the scrub regex over `tests/fixtures/codex/*.jsonl` on every PR would prevent future contributors from regressing the corpus. Worth filing as follow-up.
- If open question #2 resolves to hand-trim (option c), removing only the final `item.completed agent_message` line will leave the preceding `item.started agent_message` orphaned — violating Codex's start/complete pairing invariant. Hand-trim must remove both.

No injection vectors: the test harness only reads files; capture is manual and out-of-band.

## 5. Implementation Concerns

- **Determinism vs real-capture tension**: Tests assert specific field names (`item.type`, `usage.reasoning_output_tokens`, `item.text`). A Codex 0.131 schema tweak will silently fail the corpus and block unrelated PRs. Open question #1 surfaces this but defers it. Acceptable, but the README must warn: "tests pinned to Codex 0.130.0 shape; CLI bumps require re-capture, not test edits."
- **Fixture-size cap is half-enforced**: 20 KB / 50 KB targets exist as `wc -c` in the manual checklist but no `cargo test` assertion. This is the worst state — drifts over time and provides no guardrail. Either add `assert!(metadata.len() < 20_000)` (cheap, slightly noisy) or drop from Goals.
- **Reasoning fixture brittleness** (re-emphasizing §2): Split the assertion into two independent tests so partial coverage still passes when one property is missing.
- **`.pi/lessons/` cross-references** in Assumption #4 require those lesson files to exist on disk. Implementation should verify presence — this PR cites them as guardrails.
- **Codex `error` vs `turn.failed`**: The `turn-failed.jsonl` test accepts either as the terminal event. Fine, but these represent two distinct termination modes that FR-3a will eventually need to differentiate. Today's design covers only one of them under a single fixture.

## 6. Blind Spots

1. **Line-ending normalization**. No `.gitattributes` entry. Windows contributors with `core.autocrlf=true` will silently rewrite the corpus on checkout, breaking byte-for-byte fidelity.
2. **Trailing newline convention**. POSIX (final `\n`) vs no-trailing-newline isn't specified. `parse_jsonl` tolerates both via `trim().is_empty()` filter, but the design doesn't say so explicitly — re-captures may diverge.
3. **Reasoning-tokens ≠ reasoning-item**. The design conflates "non-zero `reasoning_output_tokens`" with "intermediate non-`agent_message` item exists." These are independent Codex features.
4. **Corpus-staleness signal**. Nothing prompts maintainers to re-capture when Codex moves. A `Last captured: 2026-05-19` README header + a 6-month recapture note is the cheapest mitigation.
5. **Prompt provenance**. Each fixture's *exact prompt* must be in the README so capture is reproducible. The design mentions this implicitly but doesn't list it in the scrub checklist or capture-command template.
6. **Cross-platform capture parity**. macOS (`sandbox-exec`) vs Linux (Landlock/seccomp) produce structurally different sandbox-violation streams. README must record the capture-host OS.
7. **Test hermeticity not stated**. The integration test reads only committed files — but the design never says so explicitly. Contributors may assume they need a working Codex install to run `cargo test --test codex_fixtures`. One README line ("hermetic; does not invoke Codex CLI") closes this.
8. **JSON validation is too weak**. `serde_json::from_str::<Value>` accepts *anything* that's valid JSON. The corpus is supposed to pin Codex's event vocabulary — recommend asserting each line has a `type` field whose value is in the known set (`thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.started`, `item.completed`, `error`). Catches schema drift earlier and at lower cost than FR-3a's parser would.

## 7. Verdict

**APPROVE_WITH_SUGGESTIONS**

The design correctly executes the PRD-mandated sequencing (FR-40 before FR-3a), keeps the change additive and rollback-trivial, and provides enough scaffolding that implementation should land in a couple of hours. Architectural choices — `serde_json::Value` over typed enum, no shared loader, panic-on-error helpers — are defensible deferrals. Blind spots are non-blocking but several P0 items below should be resolved before merge.

## 8. Actionable Feedback

**P0 — resolve before merge**:
1. Add `.gitattributes` line: `tests/fixtures/codex/*.jsonl text eol=lf` to enforce the stated byte-for-byte fidelity cross-platform.
2. Filter fixture discovery in `every_fixture_is_line_valid_jsonl` to `path.extension() == Some(OsStr::new("jsonl"))` — current spec trips on the co-located `README.md` or any backup file.
3. Split `multi_turn_reasoning_fixture_reports_reasoning_tokens` into two orthogonal tests: (a) `reasoning_output_tokens > 0`, (b) presence of an intermediate non-`agent_message` `item.completed`. Coupling them turns a valid capture into a test failure.
4. Strengthen JSONL validation: assert each line has a `type` field with a value from the documented event set. Cheap, catches schema drift before FR-3a runs.
5. Pick and document the `missing-agent-message.jsonl` capture strategy (older binary / `turn.failed` rename / hand-trim). If hand-trim, remove both the `item.started agent_message` and the final `item.completed agent_message` to preserve start/complete pairing.

**P1 — strongly recommended**:
6. Extend the scrub regex in the README to cover API keys, bearer tokens, emails, and Windows-style `C:\Users\` paths. One-line addition; closes a real leak surface.
7. Record per-fixture metadata in the README header: exact prompt, Codex version, capture-host OS, capture date. Sandbox semantics differ across platforms and re-captures need this to reproduce.
8. Resolve the 20 KB size cap one way or the other — either add `assert!(metadata.len() < 20_000)` or drop from Goals. The half-enforced state is the worst option.
9. Add a README warning: "Tests pinned to Codex 0.130.0 event shape; CLI bumps require re-capture, not test edits."

**P2 — nice to have**:
10. Note in the README that `cargo test --test codex_fixtures` is hermetic (file-only; no Codex install required).
11. Consider a second failure fixture so `turn.failed` and top-level `error` are pinned separately — FR-3a will need to distinguish them.
12. Add a CI scrub-regex gate over `tests/fixtures/codex/*.jsonl` to prevent corpus regression on future PRs.
13. Avoid embedding absolute `CARGO_MANIFEST_DIR` paths in test panic messages — keep CI logs portable.
