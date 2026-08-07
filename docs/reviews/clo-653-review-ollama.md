# Design Review: CLO-653

**Reviewer**: Codex via Ollama (glm-5.2:cloud)
**Reviewed**: 2026-08-07
**Design Document**: docs/designs/clo-653-backend-cache-key.md
**Review Duration**: 47s
**Pipeline**: run by hand - the design-review workflow is broken (CLO-655, CLO-656)

---

# Design Review: CLO-653 Backend Cache Key

## Verdict: **APPROVE_WITH_SUGGESTIONS**

This is a high-quality design document. The reasoning is tight, the failure-mode analysis (especially the warmup write-back divergence) is exactly the kind of "one site that fails silently" thinking that distinguishes a good design from a plausible one, and the phased plan with a failing-test-first gate is exemplary. The public-break calculus (do it now while unpublished) is sound. I have one concern that could block implementation and a handful of items worth tightening.

---

## Key Findings

### 🟠 Finding 1 — `BackendConfig`/`RetryPolicy` may not be `Hash`-derivable (potential blocker)

The design's linchpin is `#[derive(PartialEq, Eq, Hash)]` on `BackendConfig` and `RetryPolicy`. LLM provider configs very commonly carry floating-point fields (`temperature`, `top_p`, `frequency_penalty`, `max_tokens`-as-f64, etc.). `f64`/`f32` do not implement `Eq` or `Hash`, so the derive will not compile if any such field exists.

The document enumerates `Vec<String> args` but never confirms the full field set of `BackendConfig` or `RetryPolicy` is hashable. This is the single most likely thing to derail Phase 2.

**Action**: Before Phase 2, enumerate every field of `BackendConfig` and `RetryPolicy`. If floats exist, specify the strategy explicitly — `ordered_float::NotNan`/`OrderedFloat` wrapper, rationalized/quantized representation, or a custom `Hash`/`Eq` impl. State the chosen approach in the doc. (Discovery report `clo-653.md` presumably read these structs — surface the field list here.)

### 🟡 Finding 2 — Read-then-write TOCTOU in `create_backend`

```rust
if let Some(entry) = get_backend_cache().read()?.get(&key) { return Ok(...); }
// lock dropped
let inner = /* construct */;
get_backend_cache().write()?.insert(key, ...);
```

Between the read guard release and the write guard acquisition, two callers with the same key both miss, both construct a backend, and the last writer wins. This is pre-existing behavior, but the new keying makes concurrent same-key construction *more* likely (e.g. parallel workflow steps sharing config). It is wasteful, not incorrect — but for an async runtime the cost is a redundant provider construction (HTTP client, TLS setup). Consider either:
- noting this explicitly as accepted, or
- using an `entry().or_insert_with(...)`-style pattern under a single write lock, or
- a `try_insert`/double-check pattern.

Worth a sentence in Constraints either way.

### 🟡 Finding 3 — Unbounded cache growth

With name-only keying the cache was bounded by provider count. With `BackendConfig` in the key, the cache is now bounded by *distinct configurations seen*. For a CLI this is almost certainly fine, but the document should state the bound assumption explicitly and acknowledge the worst case (a caller that mints configs with varying per-request fields). One line in Constraints would close this.

### 🟡 Finding 4 — `is_available` semantics changed for directly-constructed providers

`*Backend::new(config)` now yields `is_available() == false` (no key attached). This is honest, but it *is* a behavior change for any existing code that constructs a provider directly and calls `is_available`. The doc names `engine.rs:24` (`ClaudeBackend::new` direct call) and ~15 unit tests. **Verify none of those call sites call `is_available` on the hand-built instance** and expect `true`. Test #5 covers the new contract; a grep confirming no relying call sites exist would close the gap. Mention the verification in Phase 3.

### 🟢 Finding 5 — Memory duplication of config in keys

`BackendKey` owns a `BackendConfig` clone, stored both in the cache map *and* inside each provider's `Option<BackendKey>`. For configs with large `Vec<String>` args this is duplicative, though negligible for a CLI. Not a blocker; the open question on hashing cost already gestures at the perf axis. No change needed, but you could note that `Arc<BackendConfig>` is a future option if memory shows up — keep the owned design now for simplicity and `Eq`/`Hash` ergonomics.

### 🟢 Finding 6 — `any_cached_health_by_name` "arbitrary match"

Returning *an* arbitrary match under multi-config is well-documented ("must never be used to select an instance"). Good. One small hardening: consider having it return the *first probed healthy* entry if any, rather than a HashMap-iteration-order arbitrary one, so the diagnostic output is at least non-misleading when one config is healthy and another is not. Optional.

---

## What the Doc Gets Right (worth noting)

- **Owned-values-over-digest argument**: exactly correct — a digest collision silently reproduces the defect, and "looks correct by construction" is the hardest failure to detect. Strong reasoning.
- **Warmup write-back analysis**: identifying that `engine.rs:100` (read) and `engine.rs:161` (write-back by `backend.name()`) diverge under config-keying is the single most valuable insight in the doc. The move-key-into-future fix is clean.
- **`Option<BackendKey>` via builder**: preserving `new(config)` public API and ~15 hand-built test constructors is the right tradeoff; `is_available() == false` for uncached instances is the honest answer.
- **Failing-test-first gate** with "must observe failing against `main`" is the correct TDD discipline.
- **Deferred Approach C framing** as a superset rather than alternative is a mature architectural judgment.
- **Edge cases** (cache cleared between read and write-back; `health: None` vs `Some(unavailable)`; bedrock `block_in_place`; differing-only-in-`skip_lines` configs) are genuinely thoughtful.

---

## Minor / Editorial

- The `is_available` snippet `self.key.as_ref().is_some_and(super::is_backend_available)` is correct (`is_some_and` takes `FnOnce(&BackendKey) -> bool` via `Option<&BackendKey>`), but a reader might stumble; a closure form `|k| super::is_backend_available(k)` would read more clearly in the doc.
- API table marks `BACKEND_CACHE` and `get_backend_cache` as the two breaks — confirm `get_backend_cache` is actually `pub` (it's used by `engine.rs`, which is binary-only per the doc's own analysis). If it's `pub(crate)` it's not a public break; the table should be precise since it's the document's semver argument.
- Phase 6 validation lists `cargo test --test backend_public_api` — good, but the Acceptance Criteria verification command omits it. Align the two so the acceptance gate is the superset.
- "Open Questions" has the hashing-cost item open — fine, but consider promoting it to a tracked follow-up issue rather than living indefinitely in this design doc.

---

## Prioritized Action Items

1. **(Blocker-adjacent) Confirm `BackendConfig`/`RetryPolicy` field types are `Eq`+`Hash`-compatible.** If floats exist, specify the handling strategy (NotNan/OrderedFloat/custom impl) and add it to the Detailed Design. *Do this before Phase 2.*
2. **(Verify) Grep all `*Backend::new(config)` call sites for `.is_available()` usage** expecting `true`; record the result in Phase 3.
3. **(Doc) Add a Constraints note** on the read-then-write TOCTOU and on unbounded-growth bound, stating both as accepted.
4. **(Doc) Tighten the API table**: confirm `get_backend_cache` visibility and mark the break accurately.
5. **(Doc) Align** Acceptance Criteria verification command with the Phase 6 suite (include `--test backend_public_api`).
6. **(Optional) `any_cached_health_by_name`**: prefer returning a healthy entry if any exists, for less misleading diagnostics.
7. **(Optional) Promote** the hashing-cost open question to a tracked follow-up issue.

The design is ready to proceed to implementation once item 1 is resolved. None of the other items are blockers.
