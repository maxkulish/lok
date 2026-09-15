# Spec Review: clo-660

**Reviewer**: Codex via Ollama (glm-5.3:cloud)
**Reviewed**: 2026-09-15
**Pipeline**: lok spec-review

---

I've read the spec, all seven target files, the workflow YAML pre-decisions, the referenced ADR/decision record, `Cargo.toml`, the publish/release workflows, and validated the spec's verifiable claims against the repo (`git show v20260914.0.0:Cargo.toml` confirms the `[lib]` target; local tags exist; grep hit-counts for Evaluation #9 are 12 files; the `create_backend` root re-export and `default-features = false` pattern check out). Here is the review.

## 1. Problem Statement Assessment

**Strong.** This is one of the better-grounded problem statements I've reviewed in this repo. It is self-contained, states its facts in a verifiable table with the checking method for each, and correctly identifies the two failure modes: the README tells consumers a dependency line that cannot resolve, and the planning docs describe a publish window ("pre-publish", "first library-carrying release") that does not exist because this project is not an owner of the crates.io name. The consequence paragraph ("either an owner grant or a new crate name; neither chosen; git dependency is the only consumption path") is the correct decision-theoretic framing, and it matches the Linear description exactly, including the explicit requirement that the workspace-split decision be re-affirmed or revised (AC7 + escalation clause).

The "Why it matters" section is unusually honest about the meta-failure (CLO-654/CLO-660 "corrected" one false premise into a second one). The out-of-scope list is precise and correctly protects historical records.

One accuracy nit that a precision-focused spec should not have: item 2 cites `README.md:62-63` for the docs.rs link; it is actually at lines 71-72 (all other line citations in the spec check out — I verified ROADMAP:55/63, DEPENDENCIES:31/50/61-65/78-82/84, ADR:152, lib.rs:109-113, README:25-31/74-77/82).

## 2. Acceptance Criteria Review

**Strong**:
- AC1-AC9 are unusually specific and measurable: exact dependency forms, exact install commands, literal verdict lines, exact line-anchored statements, task counts ("Tasks 4, Completed 2").
- AC12 (change-set confinement) and AC10/AC11 (grep-based) are directly testable; the grep patterns are well-chosen, and anchoring `^cargo install lokomotiv` to line start to permit a quoted prose warning is a thoughtful edge-case handling.
- AC13 is a real integration test with a concrete scratch-crate recipe, and `use lokomotiv::create_backend;` is valid — `create_backend` is re-exported at the crate root and is not feature-gated. The chosen tag `v20260914.0.0` contains the `[lib]` target (verified).
- AC7 handles the Linear "re-affirm or revise" requirement with an explicit verdict line plus an escalation path for the revision case.

**Gaps**:
- **AC11 is unsatisfiable as written** (highest-priority finding). Evaluation #4 searches eight files including `docs/ROADMAP.md`, `docs/PROJECT.md`, and the ADR. It will hit: (a) ROADMAP:46 / PROJECT's CLO-592 recently-completed rows, whose Linear-quoted title "Make the backend library consumable from crates.io..." the spec's own out-of-scope rule says must stay; (b) ADR lines 32, 106 ("consumers use the existing `lokomotiv` crate artifact"), 117, 127, and 200 ("Revisit before CLO-592 publishes") — all unchanged by AC8's "no other line in that ADR changes". Those hits are neither corrected nor correctable under this spec. AC11 says "Every hit ... is a corrected statement", which no implementation can satisfy. Reword to "every hit is either corrected or deliberately preserved as historical, with a one-line justification."
- **AC9 lets the stale premise in PROJECT.md:9 survive.** The Active Work row's title still says "the real deadline is the first release that ships a library target" — the exact second false claim the spec's "Why it matters" criticizes in DEPENDENCIES:78-82. AC9 only requires the row to "show the current phase". Evaluation #4 would surface the hit, but no AC tells the implementer to rewrite the row's premise. Add "the Active Work row no longer states the first-library-release deadline" to AC9.
- **AC1 would mandate writing a new false claim.** The required sentence says "release tags follow `vYYYYMMDD.N.0`", but the repo has tags `v20260520.0.1`, `v20260524.0.1`, `v20260524.0.2`, `v20260524.0.3`. The scheme is `vYYYYMMDD.N.P`, with `P` usually 0 but not always. A spec whose entire purpose is removing false factual claims should not hard-code a versioning statement the tag list disproves. Use `vYYYYMMDD.N[.P]` or say the date component is the release identity.
- **The AC2 install command is never tested.** AC13 validates the library git dependency; nothing validates `cargo install --git ... --tag v<TAG> lokomotiv`, not even a package/tag resolution check. Also, the edge-case claim "the installed binary is `lok`" is incomplete: the package at the tag declares **two** `[[bin]]` targets (`lok` and `lokomotiv`, both from `src/main.rs`), and `cargo install <package>` installs all binaries of a package. The existing comment "Package is 'lokomotiv', binary is 'lok'" is already imprecise; the spec's edge case should acknowledge both binaries are installed rather than require the comment "stay true" of a statement that is only half true.
- **AC14's "no new warnings" has no defined baseline.** Record the pre-change `cargo doc` output in the evidence (or compare against `main`) so "new" is measurable.

## 3. Constraints Check

**Aligned**:
- Must-not on `Cargo.toml`/`Cargo.lock`/`Makefile`/`.github/` matches the Linear task exactly, and explicitly protecting the `documentation = "https://docs.rs/lokomotiv"` and `authors = ["ducks"]` fields shows the constraint was written by someone who read the manifest.
- "Re-run Evaluation #1/#2 before writing any fact" is the right guard for a spec whose facts are live-API-dependent and now 0-1 days old.
- The escalation triggers are concrete and checkable (owner ≠ `ducks` or includes `maxkulish`, scratch-build failure, out-of-scope living doc found, re-evaluation points to revision).
- The historical-record freeze with the one sanctioned exception (the decision record's dated re-evaluation) is consistent with how this repo treats decision records as living (CLO-591 amended ADR rows in place).

**Concerns**:
- **Internal tension in "Replace false text in place ... The one sanctioned exception is the decision record, which keeps its original Decision date and gains a dated re-evaluation section."** AC7 requires more than that: retitling (removing "Pre-publish"), rewriting the Implications section, and rewriting the Related line. The constraint's phrasing ("gains a dated re-evaluation section") undersells the sanctioned edits and could be read as forbidding the title/Implications rewrites AC7 mandates. State the exception fully.
- **"Match the surrounding style of each file" vs "never em dashes" contradict each other** for DEPENDENCIES.md and PROJECT.md, whose existing prose is full of em dashes. The no-em-dash rule is presumably for grep-ability; say so, and acknowledge it overrides style-matching in those two files.
- **The re-affirm verdict is pre-decided.** AC7 mandates the literal line before the re-evaluation is performed, while Linear only requires "re-affirmed or revised". The workflow YAML shows the user pre-decided this ("The workspace-split decision is re-evaluated on that premise" with an escalation path for the revision outcome), so this is defensible — but it should be acknowledged as a pre-decision pinned into an AC, not presented as the output of the re-evaluation sub-task.

## 4. Decomposition Quality

**Well-scoped**:
- Sub-tasks 1-3 are genuinely independent (disjoint file sets, no shared state), sub-task 4 correctly sequenced last, and the fact base (Evaluation #1/#2) is identified as the only cross-cutting dependency.
- Each sub-task is well under 2 hours of human effort. Consumer docs (1) and decision records (2) are each a single afternoon of careful editing.

**Issues**:
- Sub-task 1 carries the heaviest evaluation load (Evaluation #7 scratch git-dependency build, which fetches the full repo from GitHub, plus #8 full docs + doctests). Still fine as one sub-task, but it is the one most likely to blow the 2-hour wall-clock budget on a slow network; the escalation clause for AC13 failure partially covers this.
- No sub-task explicitly owns correcting the PROJECT.md Active Work row's premise text (see AC9 gap above) — as written, an implementer following only the sub-task descriptions could leave the stale title in place and still believe they satisfied AC9.
- "Estimated scope: M (7 files edited, ..., docs only)" — `src/lib.rs` is a source file. The edit is rustdoc-only, but the scope line should say "docs and rustdoc" for precision, especially since AC14 runs cargo.

## 5. Evaluation Coverage

**Covered**:
- Every AC maps to at least one evaluation or an explicit read-and-check method; the spec even names the verification method per-AC. Evaluation #3/#4 with a strict tier and a justified-hits tier is the right two-level grep design, and the line-start anchor for `^cargo install lokomotiv` shows the patterns were actually thought through against the intended new text.
- Evaluation #9 is a real, reproducible historical-docs sweep (I ran it: 12 files), and AC7 correctly requires the decision record to carry that list.
- Edge cases (package vs binary name, `default-features = false` rationale, the stale `CI Gate` line at DEPENDENCIES:84 explicitly deferred as a follow-up, the ROADMAP:63 rust-version justification to keep) show the spec anticipated collateral damage.

**Gaps**:
- No evaluation for the AC2 install command (see above); even `cargo install --git ... --tag v<TAG> --list`-style verification or a documented manual check would close it.
- Evaluation #2 assumes `versions[0]` is the newest; crates.io currently sorts newest-first, but the check would silently report the wrong "newest" if ordering ever changes. Add a `sort_by` or assert the ordering.
- Evaluation #8's "no new warnings" lacks a baseline (see AC14).
- Evaluation #1/#2 require network; the spec doesn't say what to do if crates.io is unreachable at implement time (presumably escalate, but it's implicit).

## 6. Codebase Alignment

**Violations**: None found.
- No code behavior changes; the Backend trait contract, `BackendErrorKind`/anyhow patterns, and timeout layering are untouched, correctly.
- `#![deny(missing_docs)]` means the `src/lib.rs` rustdoc edits must stay well-formed; AC14's `cargo doc` gate covers this.
- AC13's `use lokomotiv::create_backend;` is valid against the current root re-exports; `default-features = false` is the exact pattern the `library-boundary` CI job already enforces (`cargo build --locked --lib --no-default-features`).
- I verified `git show v20260914.0.0:Cargo.toml` contains `[lib] name = "lokomotiv"`, matching the spec's fact table.
- The out-of-scope list matches the repo's established living-vs-historical doc split (e.g., CLO-609's precedent of leaving `ducks/lok` strings in discovery/status as historical).

**Alignment**:
- The standing-entry pattern for DEPENDENCIES.md (one re-verifiable place, aggregation docs point at it) matches how CLO-653's resolved constraint is tracked there.
- Keeping Linear-quoted task titles untouched in aggregation tables follows the repo's own precedent (CLO-609's handling).
- Adding a dated re-evaluation section to a decision record while preserving the original date follows the ADR-amendment style already used in clo-589 (CLO-591's in-place row amendments).

## 7. Blind Spots

1. **AC11 vs preserved historical titles is an internal contradiction** (detailed above) — this is the one issue that will bite during implementation or verification.
2. **PROJECT.md:9's Active Work premise** — the spec criticizes DEPENDENCIES:78-82 for repeating CLO-660's mistaken correction but overlooks that the PROJECT.md CLO-660 row does the same thing, and no AC explicitly fixes it.
3. **The `vYYYYMMDD.N.0` sentence** would introduce a fresh inaccuracy contradicted by four existing tags.
4. **Two-binary install**: `cargo install` of the package installs both `lok` and `lokomotiv` binaries; the edge case asserts a single binary. Minor, but it's a factual claim in a spec about factual claims.
5. **`Cargo.toml`'s stale `documentation = "https://docs.rs/lokomotiv"` field** points at the upstream binary's docs. The spec correctly forbids changing it (Linear constraint) and even names it in Must-not, but — unlike the DEPENDENCIES:84 `CI Gate` staleness, which it explicitly defers with a "report it as a follow-up" instruction — it doesn't record this as a known-stale field / follow-up for the eventual naming decision. Same for `authors = ["ducks"]`, which will matter if the crate name is ever transferred or a new name is chosen. A one-line follow-up note in the PROJECT.md row (AC9) or the DEPENDENCIES standing entry would close this.
6. **Offline/network failure modes** for Evaluations #1/#2/#7/#13 are only implicitly covered by escalation; a sentence stating "if crates.io or GitHub is unreachable, stop and escalate" would make the failure path explicit.
7. No cross-cutting code concerns (error handling, logging, timeouts) apply — this is genuinely docs/rustdoc-only, and the spec correctly avoids inventing any.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

The spec is rigorously grounded, correctly scoped, and matches the Linear task faithfully. All findings are wording-level precision issues — but given that this spec's entire thesis is "docs must stop making unverified factual claims", the ones that would mandate or permit new inaccuracies deserve fixing before implementation.

## 9. Actionable Feedback

1. **P1 — Fix AC11's wording.** Change "Every hit ... is a corrected statement" to "every hit is either a corrected statement or deliberately preserved historical content (Linear-quoted task titles, unchanged ADR context lines), each with a one-line justification." As written it is unsatisfiable given ROADMAP:46, the PROJECT.md CLO-592 row, and ADR lines 32/106/127/200 that AC8 freezes.
2. **P1 — Extend AC9** to require the PROJECT.md CLO-660 Active Work row to drop "the real deadline is the first release that ships a library target" and state the corrected premise; currently the spec leaves its own task row carrying the false claim it exists to remove.
3. **P2 — Correct the versioning sentence in AC1.** Release tags do not all follow `vYYYYMMDD.N.0` (`v20260520.0.1`, `v20260524.0.1-.3` exist). Mandate `vYYYYMMDD.N[.P]` or an equivalent true statement, in both README and lib.rs.
4. **P2 — Add an evaluation for the AC2 install command** (at minimum tag/package resolution; ideally `cargo install --git ... --tag v<TAG> --bin lok --list` or equivalent), and fix the edge case: the install produces both `lok` and `lokomotiv` binaries; "the installed binary is `lok`" and the required comment are only half true.
5. **P3 — Reconcile the "replace in place" constraint with AC7**: the decision-record exception also covers the retitle, the Implications rewrite, and the Related-line rewrite, not just the dated section.
6. **P3 — Acknowledge the em-dash exception**: "match surrounding style" and "never em dashes" conflict in DEPENDENCIES.md/PROJECT.md; state that the no-em-dash rule wins for new text.
7. **P3 — Fix the README citation** `:62-63` → `:71-72` for the docs.rs link.
8. **P3 — Record the stale `Cargo.toml` fields as a follow-up** (`documentation = "https://docs.rs/lokomotiv"`, `authors = ["ducks"]`) the way the DEPENDENCIES:84 `CI Gate` staleness is handled, rather than only listing them under Must-not.
9. **P3 — Make Evaluation #2 order-independent** (sort by `created_at` or assert descending order) and define the "no new warnings" baseline for Evaluation #8; state explicitly that unreachable network during Evaluations #1/#2/#7/#13 means stop-and-escalate.
