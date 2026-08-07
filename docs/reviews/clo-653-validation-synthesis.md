# Pre-PR validation: clo-653

**Reviewer**: Synthesis (Claude)
**Validated**: 2026-08-07
**Pipeline**: lok pre-pr-validation
---

All Phase 6 legs re-run on HEAD (`a98b4b1`). Results below feed the synthesis.

## Reviewer Status

| Reviewer | Status | Detail |
|----------|--------|--------|
| Codex | OK | Returned success=true with a FAIL verdict and 4 findings. Could not run cargo (sandbox forbids `target/debug/.cargo-lock`); its build/test claims were unverified by it. |
| Claude fallback | SKIPPED | Not invoked — the Codex review succeeded. |
| Synthesis (this pass) | OK | Read design + plan + `git diff main...HEAD`; independently re-ran the full Phase 6 matrix on `a98b4b1`. |

**Independent validation on current HEAD (`a98b4b1`, only `docs/status/clo-653-workflow.yaml` uncommitted):**

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --locked --all-targets -- -D warnings` | pass |
| `cargo test --locked` | pass |
| `cargo clippy --locked --all-targets --features bedrock -- -D warnings` | pass |
| `cargo test --locked --features bedrock` | pass (569 bin) |
| `cargo build --locked --lib --no-default-features` | pass |
| `cargo test --locked --lib --no-default-features` | pass (156) |
| `cargo clippy --locked --lib --tests --no-default-features -- -D warnings` | pass |
| `RUSTDOCFLAGS='-D missing_docs' cargo doc --locked --no-deps --lib --all-features` | pass |
| `cargo +1.83.0 check --locked --all-targets` | pass |
| `cargo +1.88.0 check --locked --all-targets --features bedrock` | pass |
| `cargo test --locked --test backend_public_api` | pass (14) |

## Verdict
PASS_WITH_NOTES

Production wiring matches the design: `create_backend` builds one key and uses it for read, provider attachment and the double-checked insert (`src/backend/mod.rs:368-437`); `warmup_backends` moves that same key into the future so the write-back cannot diverge (`src/engine.rs:99-176`); `unambiguous_cached_health` refuses on ambiguity and both `workflow.rs` callers skip on `None` (`src/engine.rs:233`, `src/workflow.rs:108`, `src/workflow.rs:229`). I found no correctness defect either, and Codex found none. Every issue is a proof-obligation or record-accuracy gap.

## Must Fix Before PR

- **Test 10 absent — the highest-consequence path is unproven end-to-end.** No test seeds two Ollama identities and calls `Workflow::validate`. AC5's second half ("both `workflow.rs` callers skip their check on `None`") rests on code reading alone, on the path that returns `WorkflowError::UnknownModel` and rejects a user's workflow. The helper is well covered (`src/engine.rs:2136`, 64 rounds against `HashMap` ordering); the caller is not. Pattern to copy: `src/workflow.rs:5595-5625`.
- **Test 3 absent — retry participation is proven only by key inequality.** `backend_key_equality_follows_configuration_and_retry` (`tests/backend_public_api.rs:146`) asserts distinct keys; the design requires observably different attempt counts against a 500-returning server. The `RecordingServer` fixture at `src/backend/mod.rs:1043` already gives you the harness.
- **`warmup_writeback_survives_a_cache_clear` does not test its stated edge case.** It clears *before* `warmup_backends`, which then re-runs `create_backend` itself, so the clear never lands between construction and write-back (`src/engine.rs:2220-2259`). Either add a barrier so the clear falls inside that window, or rename the test to what it actually proves and re-open subtask 21.1.
- **Plan checkboxes overstate what shipped.** Tasks 20.2, 20.9, 21.1 and 21.3 are `[x]` for work that is absent, weaker than specified, or deliberately declined. 21.3 in particular was declined on sound grounds (`src/engine.rs:2182-2190` — mutating process-global env under a parallel suite is a race, not a test); record it as an accepted deviation with the `distinct_api_key_env_names_are_distinct_identities` substitute, not as a completion. Same for the `losing_racer_does_not_erase_a_probe` substitution.
- **Update the validation record to name the final revision.** `ci_contract_verified` was committed in `ea4e1dd`, before `84d3096` and `a98b4b1`. The matrix above closes the substance; commit it against `a98b4b1` and flip `codex_validated` / `validation_gate.status`.

## Out of Scope / Deferred

- Codex's subprocess harness for the ambient-`ANTHROPIC_API_KEY` bound. The design accepts that bound as documented-not-fixed; building a subprocess test fixture is new scope on a PR that already carries three signature breaks.
- Strict cardinality for Test 5 ("exactly one entry per configured backend"). `test_warmup_populates_unified_cache` and `assert_probed` cover presence and probed-ness; the exactly-one assertion is a nice-to-have.
- Test 6 via a real warmup probe. `test_is_available_cache_only_no_syscalls` (`src/engine.rs:1175-1222`) proves the provider-key → cache-entry wiring through `MockSyscallBackend`, just seeded by `set_mock_health` rather than by a live probe.
- Threading a real `BackendKey` into workflow validation, and Approach C's host-owned cache handle — both explicitly deferred in the design.
- `BackendKey::for_test` is an additive public item under `test-support` that post-dates the design's API table. Documented in `src/lib.rs:71-75`; worth one line in the PR body alongside the three breaks.

## False Positives / Tooling Artifacts

- **"Full validation is not proven for current HEAD" — resolved, and the code was never at risk.** Every Phase 6 leg passes on `a98b4b1`. The gap was in the written record, which is why the residue above is bookkeeping rather than repair.
- **"The race test deviates from the specified public-path proof" is not a defect.** The two-thread version through `create_backend` is the one that *cannot* work: the second call returns on the cache read hit and never reaches the insert. The status YAML records that an earlier version did exactly this and passed against the unconditional `insert` it was written to rule out. Driving `cache_or_keep_incumbent` directly is strictly more discriminative, and it was verified failing against a reverted unconditional insert. Only the doc note is owed.
- **"Vacuous" overstates the cache-clear test.** It does prove the write-back lands under the same key warmup read — the exact divergence the design flagged as the silent-failure mode. It is mislabelled for the edge case, not empty.
- **Codex's inability to run cargo** was a sandbox limit, not a signal about the branch.

## Recommendation

PROCEED_WITH_FIXES. The implementation is sound, matches the design, and passes the complete CI contract on the exact revision under review — nothing here needs rework or a user decision. One bounded iteration closes the gap: add Test 3 (attempt counts against a 500-returning `RecordingServer`) and Test 10 (two Ollama identities through `Workflow::validate`, both insertion orders), either strengthen `warmup_writeback_survives_a_cache_clear` with a barrier or rename it to what it proves, correct the four overstated plan checkboxes and record the two intentional test substitutions as deviations, then commit the Phase 6 evidence against `a98b4b1`. Test 10 is the one that carries real risk if skipped — it guards the only path in this change that rejects a user's workflow.
