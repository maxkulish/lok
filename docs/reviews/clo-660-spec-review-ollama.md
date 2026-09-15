# Spec Review: clo-660

**Reviewer**: Codex via Ollama (glm-5.3:cloud)
**Reviewed**: 2026-09-15
**Pipeline**: lok spec-review

---

## 1. Problem Statement Assessment

The problem statement is **clear, complete, and accurate**.

- It states the corrected ownership fact (`ducks` owns `lokomotiv` on crates.io), the published-version range, the absence of a library target in any published version, and the project's inability to publish without an owner grant or a new crate name.
- It correctly distinguishes:
  - this repository's never-published state,
  - upstream's published binary-only crate,
  - the nonexistence of the pinned `20260603` library version,
  - the practical consequence that downstream crates using a git-only `lokomotiv` dependency cannot themselves publish.
- It matches the Linear task description exactly.
- The "Out of scope: historical records" section is explicit and prevents accidental edits to reviews, designs, plans, discovery notes, status files, and lessons.
- The consequence paragraph is especially strong: it explains why the naming decision already matters to any downstream crate that wants to publish.

## 2. Acceptance Criteria Review

**Strong**

- **AC1-AC3** are unusually precise. They specify the exact dependency block, the exact tag form, the reason `tokio` is needed, and the downstream-publishing limitation. They also preserve the example code bodies, which is the right scope.
- **AC7** fully satisfies the Linear requirement to re-affirm or revise the workspace-split decision without claiming zero consumers.
- **AC13-AC15** are excellent integration-style criteria:
  - a scratch downstream crate,
  - a baseline/after docs build comparison,
  - a real `cargo install` probe.
- **AC12** correctly scopes the allowed change set to exactly the seven target files plus the workflow YAML, the spec, and CLO-660 review files.

**Gaps**

- **Evaluation #3b has a false positive that makes AC10 impossible as written.**
  The command:
  ```bash
  rg -n 'cargo install --git' $LIVING | rg -v -- '--locked'
  ```
  will match `README.md:374`:
  ```bash
  cargo install --git https://github.com/ducks/git-agent
  ```
  That line is unrelated to `lokomotiv` and is correct as-is. It does not contain `--locked`, so it will survive the filter and cause Evaluation #3b to return output, violating AC10.
- The evaluation table does not explicitly cover **AC2** (docs.rs link replacement and Quick Start comment), **AC3** (lib.rs Versioning section), **AC5** (ROADMAP Summary row counts), **AC8** (ADR trigger wording), or **AC9** (PROJECT Up Next row). Some are indirectly covered by the review greps or by reading, but the verification-method paragraph only names AC4, AC6 content, AC7, and AC9 as read-checked.
- There is no explicit evaluation that scans living docs **outside the seven target files** for stale publishing claims. The escalate clause is good, but there is no command to discover such a hit.

## 3. Constraints Check

**Aligned**

- Must/Must-not/Prefer/Escalate are used correctly.
- The spec respects the Linear constraints exactly:
  - no `Cargo.toml`, `Cargo.lock`, `Makefile`, or CI/workflow changes,
  - historical records untouched,
  - no contact with `ducks`,
  - no crate-name decision made inside this task.
- The explicit protection of `Cargo.toml`'s `documentation = "https://docs.rs/lokomotiv"` and `authors = ["ducks"]` fields, while still recording them in the PROJECT.md naming-decision row, is a good example of separating current facts from the future decision.
- The requirement to re-verify crates.io facts before writing them into docs is a strong constraint.

**Concerns**

- The new-text style rule ("regular hyphens, never em dashes") is unusual but explicit, and it does not contradict the task.
- No constraint contradicts existing codebase patterns.

## 4. Decomposition Quality

**Well-scoped**

- Sub-task 1: consumer docs (`README.md`, `src/lib.rs`).
- Sub-task 2: decision records (`docs/decisions/clo-592-workspace-split.md`, `docs/adrs/clo-589-backend-library-shape.md`).
- Sub-task 3: aggregation docs (`docs/ROADMAP.md`, `docs/DEPENDENCIES.md`, `docs/PROJECT.md`).
- Sub-task 4: verification sweep and evidence recording.

This is a clean, mostly independent split. Each sub-task is well under two hours.

**Issues**

- Sub-task 1 currently inherits the Evaluation #3b false-positive problem.
- Sub-task 4 does not include an explicit sweep for living docs outside the seven target files.

## 5. Evaluation Coverage

**Covered**

- **AC1** is covered by Evaluation #10 (tag exists) and partially by #3/#4.
- **AC10** is covered by Evaluation #3 and #3b.
- **AC11** is covered by Evaluation #4 with a sensible classification scheme.
- **AC12** is covered by Evaluation #5 and #6.
- **AC13** is covered by Evaluation #7.
- **AC14** is covered by Evaluation #8.
- **AC15** is covered by Evaluation #11.
- **AC7** is covered by Evaluation #9 and the read-check method.

**Gaps**

- **AC10 is currently unmeetable because Evaluation #3b will always return the unrelated `README.md:374` `cargo install --git` line.**
- **AC2** has no explicit check that:
  - the docs.rs link is gone,
  - the Quick Start command includes `--locked`,
  - the install comment correctly says both `lok` and `lokomotiv` are installed.
- **AC3** has no explicit check that the `# Versioning` section no longer claims publication.
- **AC5** has no explicit check that the ROADMAP Summary row counts CLO-660.
- **AC8** has no explicit check that the ADR trigger line changed and no other ADR line did.
- **AC9** has no explicit check that the PROJECT Up Next row exists with the required notes.
- There is no explicit evaluation for the escalate condition "a living doc outside the seven files states or implies this project publishes `lokomotiv`."

## 6. Codebase Alignment

**Violations**

- None found. This is a documentation-only change, and the spec does not alter the `Backend` trait, `BackendError`, `create_backend`, feature gates, or dependency graph.

**Alignment**

- The example's imports match the public root re-exports in `src/lib.rs:120-125`.
- The `Backend::query` call matches the trait contract in `src/backend/mod.rs:318-343`.
- `create_backend` matches the public signature in `src/backend/mod.rs:362-366`.
- `default-features = false` matches the established library-boundary pattern in `Cargo.toml`.
- The `tokio = { version = "1", features = ["macros", "rt-multi-thread"] }` line is correct for `#[tokio::main]`.
- The git-only dependency/publishing limitation is correctly grounded in Cargo's multiple-locations rule.

## 7. Blind Spots

- **Evaluation #3b's false positive is the main blind spot.** It will fail on the unrelated `git-agent` install line.
- **No explicit sweep of living docs outside the seven target files.** The spec escalates if one is found, but provides no command to find it.
- **No explicit check for the docs.rs link replacement.** A simple strict grep for `docs.rs/lokomotiv` in `README.md` and `src/lib.rs` would close this.
- **No explicit check for the ROADMAP Summary row count.** A read-check is sufficient, but it should be named as such.
- **No explicit check for the ADR trigger and "no other line changes" requirement.** A targeted diff or grep would close this.
- **No explicit check for the PROJECT Up Next row.** Again, a read-check is sufficient, but it should be named.

## 8. Verdict

**APPROVE_WITH_SUGGESTIONS**

## 9. Actionable Feedback

1. **Fix Evaluation #3b so it does not match the unrelated `cargo install --git` line.**
   Scope it to this project's install command, for example:
   ```bash
   rg -n 'cargo install --git https://github.com/maxkulish/lok' $LIVING | rg -v -- '--locked'
   ```
   or:
   ```bash
   rg -n 'cargo install .*lokomotiv' $LIVING | rg -v -- '--locked'
   ```

2. **Add explicit read-check rows for AC2, AC3, AC5, AC8, and AC9**, or extend the verification-method paragraph to name them.
   At minimum, add:
   - a strict grep that `docs.rs/lokomotiv` no longer appears in `README.md` or `src/lib.rs`,
   - a check that the ROADMAP Phase 13 Summary row counts CLO-660,
   - a check that only line 152 of the ADR changed,
   - a check that the PROJECT Up Next naming row exists.

3. **Add an evaluation step to sweep all non-historical living docs outside the seven target files** for stale publishing claims.
   Example:
   ```bash
   rg -n -i 'lokomotiv|crates\.io|docs\.rs|cargo install lokomotiv' README.md docs --glob '*.md' \\
     --glob '!docs/reviews/**' --glob '!docs/designs/**' --glob '!docs/design-docs/**' \\
     --glob '!docs/plans/**' --glob '!docs/discovery/**' --glob '!docs/specs/**' \\
     --glob '!docs/status/**' --glob '!docs/lessons/**' --glob '!docs/prds/**'
   ```

4. **Optionally add `docs.rs/lokomotiv` to the strict grep** so a stale docs.rs link cannot survive AC10.

5. **Minor:** make Evaluation #11's exact `(executables `lok`, `lokomotiv`)` grep less brittle across Cargo versions, or note that the exit code and `--version` output are the authoritative checks.
