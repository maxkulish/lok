# Design: CLO-396 — FR-12c: opencode migration docs + setup guide refresh

## Problem

Users and maintainers of lok who read the setup guide (`docs/guides/lok-setup-guide.md`),
README quickstart, or backend reference tables encounter instructions keyed to
`@google/gemini-cli`, `npx`, and `GEMINI_API_KEY`/`GOOGLE_API_KEY` — all of which
are deprecated by the CLO-394 backend switch and misleading for new users.
The discovery report (`docs/discovery/clo-396.md`) scored the current state at
6/10: the docs are well-structured overall, but roughly 15-20% of the setup guide
and 10% of the README are actively wrong for the opencode era. Users who follow
the stale instructions will install the wrong tool, set unnecessary env vars, and
hit authentication failures that the docs don't explain. CLO-394 (FR-12a) ships
the backend switch in this release; CLO-395 (FR-12b) ships the opencode health
probe alongside. The documentation must follow in the same window so the whole
FR-12 surface is coherent.

## Goals / Non-goals

**Goals**

- Update `docs/guides/lok-setup-guide.md`:
  - Replace the Gemini backend section's install instructions, auth flow,
    default args, and sandbox mapping to reference `opencode`.
  - Replace the sandbox mapping table: `read-only → --agent plan`,
    `workspace-write → --agent build`, `danger-full-access → --agent build
    --dangerously-skip-permissions`.
  - Remove all `@google/gemini-cli`, `npx`, and required `GEMINI_API_KEY`/
    `GOOGLE_API_KEY` references. Mention env vars as optional fallback only.
  - Update the Token Usage observability matrix (Gemini row) to reference
    opencode's JSONL shape.
  - Update Pattern 1 and Pattern 3 examples that show gemini-cli shell commands.
- Update `README.md`:
  - Prerequisites table: replace Gemini row with opencode install + auth.
  - `lok doctor` output example: update to show opencode-ready state.
  - Backend strengths table: keep Gemini row name but note the opencode-driven
    backend in the description.
  - Example workflow: update `backend = "gemini"` step shell examples.
- Add a "Migrating from Gemini CLI" callout in the setup guide's Gemini section.

**Non-goals**

- No new standalone migration documents. The callout in the setup guide is
  sufficient for the FR-12c scope.
- No changes to docs outside `docs/guides/lok-setup-guide.md` and
  `README.md` (e.g., design docs, investigations, specs — those document
  the old state by design).
- No changes to CLO-394/CLO-395 workflow state files or internal docs.
- No changes to `.lok/workflows/` workflow TOML files (their backend
  references like `backend = "gemini"` remain valid and unchanged).
- No headless/CI-CD authentication design or implementation in this task
  (out of scope for FR-12c; covered by the auth section in the migration
  callout at the "acknowledged limitation" level).

## Architecture

This is a docs-only task. No code modules, Rust types, or data flow paths
change. The "architecture" is the set of file-level edits:

```
docs/guides/lok-setup-guide.md   → 6 targeted sections updated
README.md                        → 3 targeted sections updated
```

### Files to modify

**File 1: `docs/guides/lok-setup-guide.md`**

| Section | What changes |
|---------|-------------|
| Prerequisites & install | Add minimum opencode version requirement (e.g., `opencode >= X.Y.Z` — exact version to be confirmed against CLO-394's verified flags). Emphasize Linux install (`curl -fsSL https://opencode.ai/install | bash`) alongside macOS Homebrew. Add PATH troubleshooting tip after install. |
| Minimal config example (~line 20) | `command = "gemini"` + old `args` → keep as-is (user-level example unaffected by default config change) |
| Full config reference (~line 35) | Gemini backend `args` reference — update to show opencode-compatible args |
| Backend types table | Remove "npx" association from CLI backend detection |
| Per-Step Sandbox table | Replace `Gemini --approval-mode default\|auto_edit\|yolo` with `opencode --agent plan\|build` mapping |
| Token Usage observability matrix | Update Gemini source from `stats.promptTokenCount / candidatesTokenCount` to opencode JSONL usage fields |
| Pattern 1 example (~line 200) | Replace `gemini --model ... --sandbox` shell step with opencode equivalent |
| Pattern 3 note (~line 300) | Update "When the backend is Codex or Gemini" sandbox defaulting text |
| Tips section | Remove/replace gemini-cli-specific gotchas |

**File 2: `README.md`**

| Section | What changes |
|---------|-------------|
| Prerequisites table | Gemini row: replace with opencode install showing both macOS (`brew install anomalyco/tap/opencode`) and Linux (`curl -fsSL https://opencode.ai/install | bash`) paths. Add minimum version note. Add auth instruction (`opencode auth login`). |
| `lok doctor` output example | Update to show opencode-ready backend names (if `gemini - ready` is still correct, keep; if the backend label changed, update) |
| Backend strengths table | Gemini row description — update to reference opencode-driven backend |
| Example workflow (~line "gemini" step) | Update shell command if it references old gemini CLI flags |

### No new files

The existing `docs/guides/lok-setup-guide.md` is the canonical onboarding
doc. Adding a separate migration doc would fragment the narrative.

## Public API surface

No Rust trait, struct, or function signatures change. The Backend trait,
BackendConfig schema, QueryOutput, TokenUsage, and StepContext types are
unaffected by this task.

The "API surface" for this task is the *documentation content*: the install
commands, auth flow, sandbox mapping table, and config examples that users
copy-paste into their `lok.toml`. These must match the actual CLO-394
defaults exactly.

## Assumptions

| Text | Confidence | Verification |
|------|-----------|-------------|
| A1: CLO-394 default config ships `opencode` with `args = ["run", "--format", "json"]` and `model = Some("google/gemini-2.5-flash")`. | high | Verified by reading CLO-394 design doc `Config::default` section. Cross-check final CLO-394 PR before merging CLO-396. |
| A2: The sandbox mapping `read-only → --agent plan`, `workspace-write → --agent build`, `danger-full-access → --agent build --dangerously-skip-permissions` matches opencode's release CLI. | medium | CLO-394 design (A3) flags medium confidence and requires verification against opencode `--help`. If the mapping changes, update the docs to match. |
| A3: `opencode auth login` (Google OAuth) is the primary auth flow; `GOOGLE_API_KEY`/`GEMINI_API_KEY` are optional fallbacks. | medium | CLO-394 design (A9) and CLO-395 design (auth detection section) both confirm this. Final verification against CLO-394 implementation. |
| A4: The `gemini` backend name string does not change. | high | CLO-394 design explicitly states "Preserve the backend key `gemini`". No docs change needed for the backend name itself. |
| A5: The README quickstart `lok doctor` output example still shows `gemini - ready`. | high | The backend key is preserved; the doctor check just changes from `npx` to `opencode` binary detection. The "gemini - ready" line remains. |
| A6: CLO-396 must land after or simultaneously with CLO-394 to avoid shipping docs that reference flags/behaviors that don't exist yet. | high | The discovery report captured the blocked-by relation. The implementation phase must order PRs accordingly. |

## Test plan

**Verification steps** (docs-only — manual review, no automated tests):

1. **Line-by-line comparison**: Read every diff in `docs/guides/lok-setup-guide.md` and confirm every `@google/gemini-cli`, `npx`, `GEMINI_API_KEY`, `GOOGLE_API_KEY` reference is either removed or downgraded to "optional fallback". Count remaining stale references; must be zero.

2. **Sandbox mapping coherence**: Confirm the sandbox mapping table, the per-step sandbox docs, and the sandbox defaulting rules section all use the same opencode `--agent` flag values. No dangling `--approval-mode` references.

3. **README prerequisites table**: Confirm the Gemini row shows `brew install anomalyco/tap/opencode` and `opencode auth login` as the install+auth instructions, and that `npm install -g @google/gemini-cli` is removed.

4. **Config example coherence**: Confirm the `lok.toml` minimal/full config examples that show `[backends.gemini]` are consistent with the default config CLO-394 ships (opencode, `google/gemini-2.5-flash` default model).

5. **Migration callout**: Confirm a "Migrating from Gemini CLI" callout appears in the Gemini section and covers: install delta (remove npx, install opencode, minimum version), auth delta (remove API key env, use `opencode auth login`; note headless environments need env-var fallback), sandbox delta (remap `--approval-mode` flags).

6. **Pattern example consistency**: Confirm Pattern 1 and Pattern 3 examples no longer use shell commands with `gemini --model ... -y --sandbox` or other old CLI flags.

7. **Cross-reference check**: Search the entire `docs/` tree for any remaining `@google/gemini-cli` references in user-facing files (exclude design docs, investigations, specs by design). Report count; must be zero.

## Migration / rollout

The change is purely additive at the documentation layer. No code changes,
no config schema changes, no feature flags. Rollout order:

1. CLO-394 (FR-12a) merges — backend switch to opencode.
2. CLO-395 (FR-12b) merges — health probe (may affect `lok doctor` output format;
   docs must reflect the final probe output).
3. **CLO-396 merges** — docs update reflecting both preceding changes.
4. Release cut including all three.

The task is blocked-by CLO-394 and related-to CLO-395 in the Linear
dependency graph. The `design` and `plan` phases are independent of
CLO-394's implementation status; the `implement` phase must wait for
CLO-394 to ship before the docs content can be finalized (to verify the
actual opencode flags and output shape).

## Open questions

- **Does the `lok doctor` output example in the README need changes beyond the
  prerequisites table?** If CLO-395 changes the doctor output format (e.g.,
  adding auth method columns), the example may need updating. Resolved at
  implementation time by reading CLO-395's final doctor output.
- **Should any example shell commands that wrap `gemini` directly (e.g., in
  Pattern 1) be rewritten to use `backend = "gemini"` instead?** The design
  philosophy of the setup guide is to show real patterns. Shell commands that
  call the CLI directly are a teaching tool. If the old shell commands no longer
  work with opencode's positional-prompt semantics, they must be replaced with
  backend-step equivalents. Resolved during implementation by testing each
  example command against opencode.
- **What is the minimum opencode version to pin?** The sandbox mappings and
  `--dangerously-skip-permissions` flag must be verified against `opencode --help`
  after CLO-394 ships. Version must be added to prerequisites.
- **Does the `opencode auth login` flow have a documented headless/CLI-only
  fallback?** If opencode supports `opencode auth login --headless` or
  `GEMINI_API_KEY` env-var auth as a CI fallback, document both paths. If
  headless auth is genuinely unsupported, add a limitations note.
