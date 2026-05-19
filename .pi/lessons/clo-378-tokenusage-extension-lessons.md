# Lessons: CLO-378 FR-25b TokenUsage Extension

Durable rules from extending a public struct with `Option<u32>` fields in a binary crate.

---

## L1 - When removing `#[allow(dead_code)]` from public API methods, confirm the binary crate actually calls them first

**Source incident:** CLO-378 pre-PR validation fix iteration. The synthesis reviewer suggested removing `#[allow(dead_code)]` from `with_cached` and `with_reasoning` builder methods on `TokenUsage`, arguing that `pub` items on a public struct should not need suppression. After removing both attributes, `cargo clippy --all-targets -- -D warnings` failed with dead_code warnings because `lok` is a binary crate (`bin` targets `lok` and `lokomotiv`) and these methods have no callers yet — they are intentionally public API surface for downstream FRs (CLO-381, CLO-382) that will wire data later.

**Rule:** In a Rust binary crate (not `lib`), `pub` methods that are part of the crate-internal public API but not yet consumed by any binary target will trigger dead_code lints at `-D warnings`. Before removing a dead_code suppression, verify the method is actually called by a binary target or integration test, not merely that it is `pub`.

**How to apply:** For structural extensions that add builder methods whose wiring is deferred to downstream tasks, keep `#[allow(dead_code)]` (or `#[expect(dead_code)]` on newer Rust) and document the downstream issue in a comment. Remove the suppression only when the first caller lands. Do not rely on `pub` visibility alone to silence dead_code in binary crates.

---

## L2 - Mirror-complete test matrices across symmetric fields; "same logic" comments don't satisfy the gate

**Source incident:** CLO-378 pre-PR validation fix iteration. `test_token_usage_saturating_add_folds_optionals` had a complete 4-combination matrix (Some/None × 4) for `cached_tokens` but only 1 assertion (`Some + Some`) for `reasoning_tokens`, followed by a comment `// reasoning_tokens follows same logic`. The plan ST4 explicitly required "All 4 Some/None combinations for both fields". Codex validation flagged this as a plan violation even though `sum_opt` is shared.

**Rule:** When a test plan specifies an exhaustive combinatorial matrix for a struct with symmetric fields, copy the complete assertion list for each field. A comment claiming "same logic" is not sufficient — the test must prove it. Pre-PR reviewers (and future maintainers) won't assume shared helper correctness without executing the assertions.

**How to apply:** Write the first field's assertions, then literally copy-paste the assertion block and change only the field accessor (e.g. `cached_tokens` → `reasoning_tokens`). The near-duplication is intentional; do not DRY the assertions into a helper — the point of the test is to exercise the public API surface, not the internal helper. This applies to any symmetric field pair on a public struct (e.g. `x`/`y` coordinates, `width`/`height`, `start`/`end`).

