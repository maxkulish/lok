# lok pi Orchestrator - Implementation Summary

This `.pi/` tree adds a pi-CLI surface for the lok task lifecycle that
mirrors the existing Claude flow at `.claude/commands/task/`. The two
sides share the same YAML schema, same phase set, same allowed
transitions, and same Linear integration model so a task can move
freely between Claude and pi.

## Layout

```
.pi/
├── IMPLEMENTATION_SUMMARY.md          (this file)
├── AGENTS.md                          pi runtime intent layer
├── extensions/
│   ├── orchestrate/
│   │   ├── index.ts                   pi extension: state machine + tools
│   │   ├── package.json
│   │   └── README.md                  extension-level docs
│   └── linear/
│       ├── index.ts                   pi extension: Linear MCP bridge
│       ├── package.json
│       └── README.md                  extension-level docs
├── scripts/
│   └── check-schema-parity.mjs        PHASE_CONFIG ↔ phase markdown gate
├── orchestrator/
│   ├── README.md                      phase-script index, conventions
│   └── phases/
│       ├── init.md
│       ├── discovery.md
│       ├── design.md
│       ├── plan.md
│       ├── implement.md               (embedded codex+gemini gate)
│       ├── pr.md
│       ├── complete.md
│       ├── spec.md
│       ├── operational.md
│       ├── status.md
│       └── blocked.md
├── agents/
│   ├── claude-designer.md             draft design docs
│   ├── gemini-architect.md            design / impl architecture review
│   ├── codex-pre-pr.md                pre-PR validation gate
│   ├── ollama-rust-reviewer.md        local-only Rust footgun pass
│   ├── security-reviewer.md           conditional security audit
│   ├── ops-reviewer.md                conditional ops audit
│   └── README.md                      persona routing
├── lessons/
│   └── pr-review-failures.md          durable PR review lessons
└── skills/
    └── pr-review-cycle.md             PR review handling procedure
```

## What the extension exposes

`.pi/extensions/orchestrate/index.ts` registers:

- `task:orchestrate` slash command - dispatches the right phase based
  on flags (`--status`, `--spec`, `--ops`, `--skip-discovery`).
- `update_workflow_state` tool - merges phase / workflow / linear /
  root updates into the YAML and appends a history event. Concurrency
  is guarded by per-task write locks.
- `transition_phase` tool - validates the requested transition against
  `ALLOWED_TRANSITIONS`, `TYPE_ALLOWED_PHASES`, and `PHASE_CONFIG`,
  then advances `workflow.current_phase`.

Validation rules enforced at transition time:

1. `from_phase` must equal current `workflow.current_phase`.
2. `to_phase` must be in `ALLOWED_TRANSITIONS[from]`.
3. `to_phase` must be permitted for `task_type`.
4. Outgoing phase must have `status: complete` or `status: skipped`.
5. All required fields and history events for the outgoing phase must
   be present (skipped phases bypass this).

`validation_override: true` exists for manual unblocking but should
be a last resort.

## Schema parity with Claude

Top-level keys that must remain identical across Claude and pi:

```yaml
task_id: CLO-XX
task_title: ...
task_url: https://linear.app/cloud-ai/issue/clo-xx/...
task_type: development | specification | operational
classification_reason: ...

linear:
  team: Cloud-ai
  project: Lok
  status_at_start: ...
  priority: ...
  branch_suggested: ...
  branch_actual: feat/clo-xx-...
  blocks: []
  blocked_by: []

workflow:
  current_phase: ...
  status: active | blocked | paused | complete | in_progress | checkpoint
  created_at: ...
  updated_at: ...

phases:
  discovery: { status, approved, ... }
  spec: { status, spec_file, approved, review_completed, ... }
  design: { status, design_doc, draft_ready, finalized, review_completed, ... }
  plan: { status, plan_file, approved }
  implement: { status, commits[], codex_validated, codex_verdict, codex_report, gemini_validation_report }
  pr: { status, pr_url, pr_number, ci_passed, reviews_addressed, merged_at, merge_commit }
  complete: { status, aggregation_files_updated, merged_at }

history:
  - { timestamp, action, phase, details }
```

The complete phase block reference lives in
`extensions/orchestrate/README.md`.

## Differences vs the mentis pi orchestrator

The structural skeleton is borrowed from `~/Code/mentis/.pi/`. The
lok version diverges in these places:

| Aspect | Mentis | lok |
|---|---|---|
| Task ID | `MENTI-XX` | `CLO-XX` |
| Tracker | Plane.so | Linear (MCP) |
| Tracker tools | `mcp__plane__*` | `mcp__linear__*` |
| Status file | `docs/status/menti-XX-workflow.yaml` | `docs/status/clo-XX-workflow.yaml` |
| Review phase | Separate `review` phase before `pr` | None - validation gate is inside `implement.md` step 4 |
| Stack | Tauri v2 + Rust + React | Single Rust crate `lokomotiv` (bins `lok`, `lokomotiv`) - modules `backend`, `workflow`, `conductor`, `tasks`, `apply_verify`, `role`, etc. |
| Validation workflow | mentis-internal | `lok workflow run pre-pr-validation <design_doc> <plan_file> CLO-XX` |
| Pre-merge gate | `cargo fmt --manifest-path src-tauri/...` | `cargo fmt --check && cargo clippy -- -D warnings && cargo test` |
| Aggregation files | per-mentis layout | `PROJECT.md`, `ROADMAP.md`, `DEPENDENCIES.md` synced via `/project:sync` |

The "no review phase" choice keeps the codex+gemini gate close to the
code it validates and matches the Claude flow this `.pi/` tree mirrors.

## The pre-PR validation gate

Step 4 of `implement.md` runs the `pre-pr-validation` workflow defined
in `.lok/workflows/pre-pr-validation.toml`. Invocation:

```bash
lok workflow run pre-pr-validation <design_doc> <plan_file> CLO-XX
```

Arguments:

- `arg.1` = path to design doc (e.g. `docs/design-docs/clo-XX-design.md`)
- `arg.2` = path to implementation plan (e.g. `docs/plans/clo-XX-plan.md`)
- `arg.3` = Linear task ID (e.g. `CLO-212`)

Outputs (written to `docs/reviews/`):

- `docs/reviews/clo-XX-codex-validation.md`
- `docs/reviews/clo-XX-gemini-validation.md`
- `docs/reviews/clo-XX-validation-synthesis.md`

Verdict vocabulary used by the workflow: `PASS`, `PASS_WITH_NOTES`,
`FAIL`. `implement.md` maps these onto its YAML
`phases.implement.codex_verdict` field (which also accepts the legacy
synonyms `approve|approve_with_changes|pivot|rework` for backward
compatibility with older workflow YAMLs).

## Aggregation files

lok tracks roadmap state in three top-level files synced by the
`/project:sync` slash command:

- `PROJECT.md` - per-task status + priority
- `ROADMAP.md` - phase-by-phase plan
- `DEPENDENCIES.md` - inter-task graph

Phase scripts that mutate aggregation state (init starts sync,
complete updates it) MUST call `/project:sync CLO-XX --start` and
`/project:sync CLO-XX --complete` rather than writing the files
directly. The Claude side already enforces this; the pi side must
match.

## Auto Mode behaviour

Auto Mode is the default expected operating mode for both Claude and
pi. Each phase file's "approval checkpoint" section names the exact
preconditions that allow auto-approval. When those preconditions hold,
pi auto-approves and records the reason in
`phases.<phase>.auto_approval_reason`. When they do not hold, pi
prompts the user.

## Linear integration

Pi does NOT inherit Claude's MCP server configuration. Each pi
extension that needs MCP must establish its own client connection. So
the lok pi setup ships a thin bridge extension at
`.pi/extensions/linear/` that connects to Linear's hosted MCP and
re-registers the approved 7-tool subset under the `mcp__linear__`
prefix.

The Claude side of lok currently uses the `mcp__linear-server__*`
prefix (the full hosted-MCP surface). The pi bridge intentionally uses
the shorter `mcp__linear__` prefix to keep the agent prompt small and
the tool surface predictable. Both prefixes route to the same Linear
workspace (`Cloud-ai`, identifier `CLO`).

The approved subset: `list_issues`, `get_issue`, `save_issue`,
`list_comments`, `save_comment`, `list_issue_statuses`,
`list_projects`, plus `get_team` as a conditional. Linear's MCP
exposes ~30 tools total; filtering keeps the agent prompt small.
`LINEAR_MCP_FULL_SURFACE=1` is an escape hatch.

The bridge:

- Reads `LINEAR_API_KEY` from env.
- Connects to `https://mcp.linear.app/mcp` via Streamable HTTP
  (or `https://mcp.linear.app/sse` if `LINEAR_MCP_TRANSPORT=sse`).
- Lists Linear's tools, filters to the approved subset (unless
  `LINEAR_MCP_FULL_SURFACE=1`), prefixes each with `mcp__linear__`,
  and registers them with pi.

See `extensions/linear/README.md` for setup.

## Installation

Both extensions install the same way. Install both - the orchestrator
calls Linear tools, so the linear bridge must be active for end-to-end
runs.

```bash
# orchestrator (state machine + phase dispatcher)
cd .pi/extensions/orchestrate
npm install
ln -s $(pwd) ~/.pi/agent/extensions/lok-task-orchestrator

# linear bridge (mcp__linear__* tools)
cd ../linear
npm install
ln -s $(pwd) ~/.pi/agent/extensions/lok-linear

# required env
export LINEAR_API_KEY=lin_api_...

# temporary load (one-shot, both extensions)
pi -e .pi/extensions/orchestrate/index.ts -e .pi/extensions/linear/index.ts
```

## Usage

```bash
/task:orchestrate CLO-42                  # start or resume
/task:orchestrate CLO-42 --status         # show current state
/task:orchestrate CLO-42 --spec           # specification task
/task:orchestrate CLO-42 --ops            # operational task
/task:orchestrate CLO-42 --skip-discovery # development, skip discovery
```

## Maintenance rules

- Any change to a phase file or schema rule must be mirrored on the
  Claude side under `.claude/commands/task/`.
- Any change to required fields / history must be mirrored in
  `PHASE_CONFIG` inside `extensions/orchestrate/index.ts`. Run
  `node .pi/scripts/check-schema-parity.mjs` before committing.
- Any change to the YAML schema must be reflected in the phase file's
  "Required exit state" section AND in the extension README.

## See also

- `extensions/orchestrate/README.md` - extension-level docs
- `orchestrator/README.md` - phase-script index
- `.claude/commands/task/orchestrate.md` - canonical Claude flow
- `.lok/workflows/pre-pr-validation.toml` - validation gate runner
- `AI-AGENTS.md` (repo root) - lok project overview
- `PROJECT.md`, `ROADMAP.md`, `DEPENDENCIES.md` - aggregation files
