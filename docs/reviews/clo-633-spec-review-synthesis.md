# Spec Review Synthesis: clo-633

**Synthesized**: 2026-08-03
**Pipeline**: lok spec-review

---

## Reviewer Availability

| Source | Status |
|---|---|
| Gemini | ✅ Valid review |
| Ollama (Codex / glm-5.2:cloud) | ❌ `REVIEW_FAILED` — empty output, process died after workdir/model banner |
| Claude fallback | Skipped by policy (an external reviewer succeeded) |

Only one reviewer produced output, so no cross-model agreement is available. Instead I verified each Gemini claim against the codebase directly — the "Verified" column below is evidence, not consensus.

## Verified Findings (checked against the tree)

| # | Finding | Evidence | Severity |
|---|---|---|---|
| 1 | **Two panic sites missed by the spec's "four slicing sites" count.** Both are head-truncation at a fixed byte offset. | `src/main.rs:1902` `&diff[..max_diff_chars]` (50,000); `src/tasks/implement.rs:583` `&code[..code.len().min(150)]` | High |
| 2 | **Test 10's expected output contradicts the spec's own `line_window` math.** The spec asserts a 3-line window; the math yields 2. | Spec line 178 expects `"    2: b\n>>>    3: c\n     4: d\n"`. Running the spec's formula (`end=line+ctx`, `start=line-ctx` clamped) with `line=3, ctx=1, total=5` gives `start=2, end=4` → 2 lines | High (spec-internal contradiction; the eval would fail a correct implementation) |
| 3 | `extract_file_references` is duplicated across the two files being deduplicated. | `src/tasks/context.rs:256` vs `src/tasks/fix.rs:243` — diff is one comment and one blank line | Low |
| 4 | Problem statement, AC1–AC8, decomposition, and codebase alignment are sound. No violations found. | — | — |

## Corrected Reviewer Claim

| Topic | Gemini's position | Verified reality |
|---|---|---|
| `str::floor_char_boundary` stabilization | "Stabilized in Rust 1.77.0, so the spec's claim that it postdates 1.80 is inaccurate" | **Gemini is wrong, the spec is right.** `#[stable(feature = "round_char_boundary", since = "1.91.0")]` in the local sysroot's `core/src/str/mod.rs:423`. `Cargo.toml:5` declares `rust-version = "1.80"`, so the method is genuinely unavailable at MSRV. The spec's constraint (lines 100–107) needs no edit. |

Gemini reached the right conclusion (keep the `is_char_boundary` walk) from a wrong premise — don't let the premise into the spec.

## Correction to Gemini's Own Fix

Gemini proposed replacing Test 10's expectation with `">>>    3: c\n     4: d\n"`. That string is also wrong: the non-marker prefix is `"   "` + space + `{:4}` = 7 leading spaces, not 5. I ran the actual format:

```
">>>    3: c\n       4: d\n"
```

Use that verbatim. The same 5-vs-7-space error is present in the spec's current Test 10 string, so it predates Gemini's suggestion.

## Novel Insights (single reviewer)

Every finding here is single-source by construction. Findings 1 and 2 are the ones that change the work; 3 is optional cleanup.

## Consolidated Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec's diagnosis, decomposition, and constraints are correct and implementable. Two fixes are needed before it's an accurate blueprint: an undercounted panic surface and a self-contradictory test expectation. Neither touches the design.

## Priority Actions

1. **Expand scope to the two missed panic sites** (`src/main.rs:1902`, `src/tasks/implement.rs:583`). Both want the *head*, not the tail, so both are one-line swaps to the existing `crate::utils::truncate_utf8` — no new helper. Update the problem statement's "four slicing sites" to six and add ACs plus eval rows for each.
2. **Fix Test 10** to `">>>    3: c\n       4: d\n"` (2 lines, 7-space non-marker prefix). Cross-check Tests 11 and 12 for the same padding error while you're in there — Test 12's *behavior* is correct (`line=0, ctx=1` → `"       1: a\n"`, no marker), which does preserve today's `unwrap_or(0)` path per AC7.
3. **Leave the `floor_char_boundary` constraint alone.** It is factually correct as written.
4. **Optional:** fold the two `extract_file_references` copies into `src/utils.rs` alongside `render_line_window`. Same drift risk, same cleanup, marginal added cost — but it widens the diff, so it's a judgment call rather than a requirement.
