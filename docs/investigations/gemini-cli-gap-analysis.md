# Gemini CLI Gap Analysis & Integration Report

**Date:** 2026-05-16
**Latest gemini-cli stable:** v0.42.0 (2026-05-12)
**Latest nightly:** v0.44.0-nightly.20260515

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Latest 10 Releases: Key Themes](#2-latest-10-releases-key-themes)
3. [Current Gemini Integration in Lok](#3-current-gemini-integration-in-lok)
4. [Gaps & Required Updates](#4-gaps--required-updates)
5. [Gemini CLI Features Lok Should Use But Isn't](#5-gemini-cli-features-lok-should-use-but-isnt)
6. [Blind Spots](#6-blind-spots)
7. [Improving Lok → Gemini CLI Interaction & Stability](#7-improving-lok--gemini-cli-interaction--stability)
8. [Prioritized Action Items](#8-prioritized-action-items)
9. [Appendix: Full Release Breakdown](#9-appendix-full-release-breakdown)

---

## 1. Executive Summary

Lok currently wraps gemini-cli via a brittle shell pipe (`echo '' | npx @google/gemini-cli 'prompt'`). This integration is **functional but fragile** — it doesn't leverage gemini-cli's modern headless/JSON output modes, streaming, session management, or subagent capabilities. The gemini-cli has undergone significant architectural evolution in the last 30 releases:

- **Agent protocol** (LocalSubagentProtocol, RemoteSubagentProtocol) enabling composition
- **Headless/JSON output** with structured `--output-format json` and `stream-json`
- **Session export/import** for resumable workflows
- **Skills-based composition** (the repo agent is being refactored toward skills)
- **MCP server integration** for tool extensibility
- **Approval modes** merging into a single Auto mode

Lok's current integration misses nearly all of this surface area. The good news: there is no breaking change in the last 30 releases that breaks the existing `-p` flag usage. The bad news: Lok is leaving massive value on the table.

---

## 2. Latest 10 Releases: Key Themes

### v0.42.0 (stable, 2026-05-12) — Major themes:

| Theme | Changes | Relevance to Lok |
|-------|---------|-----------------|
| **Agent protocol** | LocalSubagentProtocol, RemoteSubagentProtocol, agent session protocol refactoring, SubagentState enum | Lok could use gemini-cli as a subagent in its conductor/debate modes |
| **Session export/import** | `feat: export session to file and import via flag` (#26514) | Allows saving/loading entire gemini sessions — Lok could cache full session state |
| **Auto Memory** | Auto Memory inbox flow, skill extraction, canonical-patch contract | Gemma can now learn from its sessions; Lok could tap into this for long-running workflows |
| **Headless improvements** | JSON output for AgentExecutionStopped, ADK non-interactive sessions | Directly relevant to Lok's integration |
| **Edit tool steering** | Model steered to use edit tool for surgical edits | Relevant for Lok's `apply_edits` workflow feature |
| **Proxy support** | https-proxy-agent, NO_PROXY for MCP | For enterprise users running Lok behind proxies |
| **Plan Mode** | Read-only safety during planning | Could be used in Lok workflows for review-before-apply |

### v0.43.0-preview.0 (2026-05-12) — Key features:

| Feature | Impact on Lok |
|---------|---------------|
| `export session to file and import via flag` | **High** — Enables resumable workflows |
| `LocalSubagentProtocol` / `RemoteSubagentProtocol` | **High** — Architectural shift toward composable agents |
| `Incremental refactor repo agent towards skills-based composition` | **Medium** — Gemini is evolving toward the same "skills" concept Lok has in workflows |
| `steer model to use edit tool for surgical edits` | **Medium** — Improves edit reliability when Lok calls gemini |

### v0.44.0-nightly (2026-05-14/15) — Stability & UX:

| Fix | Impact on Lok |
|-----|---------------|
| `Auto modes merged into a single Auto mode` | Low — simplifies gemini's own UX |
| `Agent registration first-wins, prioritize project` | Medium — affects how Lok can register custom agents |
| `Respect NO_PROXY for MCP servers` | Low — enterprise proxy fix |
| `EISDIR errors during file processing` | Low — edge case fix |
| `Permission denied in sandbox on NixOS` | Low — NixOS compatibility |

### Notable Fixes Affecting Lok:

- `fix(core): retry on ERR_STREAM_PREMATURE_CLOSE errors` (v0.42.0-nightly) — **Critical for stability**: gemini-cli now handles network flakiness internally
- `fix(core): preserve system PATH in Git environment to fix ENOENT` (v0.42.0-nightly) — Important when Lok triggers git commands through gemini
- `fix(core): cache model routing decision in LocalAgentExecutor` (v0.42.0-nightly) — Performance improvement
- `fix(cli): allow keychain auth for --list-sessions and non-interactive mode` — Non-interactive auth improvements

---

## 3. Current Gemini Integration in Lok

### File: `src/backend/gemini.rs`

```rust
// Current flow:
// 1. Construct: echo '' | npx @google/gemini-cli '<prompt>'
// 2. Spawn: sh -c "<shell command>"
// 3. Parse: skip_lines(1) + collect stdout
// 4. Return: raw stdout as string
```

### Current Problems:

1. **No structured output**: Uses raw text parsing via `skip_lines` — brittle
2. **No timeout/streaming**: Full blocking wait for completion
3. **No model selection**: Uses gemini-cli default model; `--model` flag exists but unused in production config
4. **No session management**: Every call starts a fresh session; no context reuse
5. **No error classification**: All gemini failures are `ExecutionFailed`; no way to distinguish auth from rate-limit from timeout
6. **Single prompt per invocation**: Can't leverage gemini's multi-turn reasoning
7. **No JSON output parsing**: Even when gemini returns JSON (via `--output-format json`), Lok ignores it
8. **Token usage unknown**: No `TokenUsage` extraction from gemini-cli's JSON stats field

### Config (from `lok.toml`):

```toml
[backends.gemini]
enabled = true
command = "npx"
args = ["@google/gemini-cli"]
skip_lines = 1
timeout = 600
```

---

## 4. Gaps & Required Updates

### 4.1 Critical: Switch from shell pipe to proper headless mode

**Current:**
```bash
echo '' | npx @google/gemini-cli 'prompt'
```

**Target:**
```bash
npx @google/gemini-cli -p 'prompt' --output-format json
```

**Benefits:**
- Structured JSON response with `response`, `stats` (token usage), and `error` fields
- No `skip_lines` hack needed
- Proper exit codes: 0=success, 1=error, 42=input error, 53=turn limit
- Deterministic parsing instead of fragile line-skipping

**Risk:** JSON output uses one round-trip. If gemini-cli tries to use tools (shell commands, file writes), it may block awaiting TTY. Must test with `-s read-only` or equivalent.

### 4.2 High: Add model selection flag to gemini backend

The gemini backend already has a `--model` flag in `GeminiBackend::query()` (via `model_flag`), but:
- The `lok.toml` config doesn't set a model
- The `run_query_with_config` function always passes `None` to `backend.query()`
- No way to specify model per-workflow-step

Users should be able to set `model = "gemini-2.5-flash"` or `model = "gemini-2.5-pro"` per backend config or per workflow step.

### 4.3 High: Proper error classification from gemini-cli exit codes

| Exit Code | Meaning | Lok BackendError |
|-----------|---------|-----------------|
| 0 | Success | — |
| 1 | General error / API failure | `ExecutionFailed` |
| 42 | Input error (invalid prompt/args) | `Config { message }` |
| 53 | Turn limit exceeded | `Timeout { message }` |

Currently all non-zero exits become `ExecutionFailed`. Must map exit codes to proper `BackendError` variants.

### 4.4 Medium: Add streaming support for long-running analysis

Gemini's `stream-json` output emits newline-delimited JSON events:
- `init` — session metadata
- `message` — response chunks
- `tool_use` — tool calls
- `tool_result` — tool outputs
- `result` — final outcome with aggregated stats

For workflow steps that take 5+ minutes, Lok could track progress via stream events instead of showing a frozen progress bar.

### 4.5 Medium: Enable token usage tracking

The JSON output includes a `stats` object with `promptTokenCount`, `candidatesTokenCount`, `totalTokenCount`. Map these to `TokenUsage` in `QueryOutput`.

---

## 5. Gemini CLI Features Lok Should Use But Isn't

### 5.1 Session Export/Import (`feat: export session to file and import via flag`)

**What:** Gemini CLI can now save full session state to a file and reload it on startup:
```bash
gemini -p "Analyze this" --output-format json --export session.json
gemini --import session.json -p "Continue analysis"
```

**Why for Lok:**
- **Context persistence**: A multi-step workflow could export a session after each step, preserving full conversation context
- **Resumable workflows**: If a step crashes (timeout, network), Lok could resume from the last checkpoint
- **Debugging**: Export sessions for post-mortem analysis when gemini produces unexpected results

**Integration pattern:**
```toml
[[steps]]
name = "deep_analysis"
backend = "gemini"
session_export = ".lok/sessions/deep_analysis.json"  # NEW config field
prompt = "Analyze the security of this codebase..."
```

Lok would:
1. Check if `.lok/sessions/deep_analysis.json` exists → use `--import` to resume
2. Run gemini with `--export` flag
3. On success, keep the session file for next workflow run

### 5.2 Subagent Protocol (LocalSubagentProtocol / RemoteSubagentProtocol)

**What:** Gemini CLI now has a formal AgentProtocol for subagent communication:
- `LocalSubagentProtocol` — spawns subagents in-process
- `RemoteSubagentProtocol` — communicates with external agents via ACP

**Why for Lok:**
- **Deep integration**: Instead of Lok piping text to gemini-cli and parsing text back, Lok could register itself as a "subagent" and communicate via the protocol
- **Structured tool calls**: Get structured tool requests/responses instead of raw text
- **Progress updates**: SubagentState enum gives visibility into gemini's internal progress

**Implementation complexity:** High. Requires understanding the AgentProtocol schema and implementing it in Rust.

**Recommendation:** Monitor this feature. It's still evolving (refactoring in progress). Revisit in 2-3 months when it stabilizes.

### 5.3 GitHub Action Integration

**What:** `google-github-actions/run-gemini-cli` provides:
- PR review automation
- Issue triage
- `@gemini-cli` mentions in issues/PRs
- Custom workflows with `GEMINI.md` context

**Why for Lok:**
- Lok already has `lok run review-pr 123` and `lok run fix 123` commands
- Instead of calling gemini-cli locally, Lok could invoke the GitHub Action for CI-integrated reviews
- Could delegate the "deep analysis" step to the action while handling orchestration in Lok

**Integration pattern:**
```toml
[[steps]]
name = "pr_review"
backend = "github_action"  # NEW backend type
action = "google-github-actions/run-gemini-cli@v1"
with:
  task: "review-pr"
  pr_number: "{{ arg.1 }}"
```

### 5.4 Plan Mode for Safe Code Review

**What:** Gemini's Plan Mode is a read-only mode for analysis before making changes. It uses `/plan` command and `enter_plan_mode`/`exit_plan_mode` tools.

**Why for Lok:**
- Lok's `apply_edits` workflow feature already handles code modification
- Before applying edits, Lok could tell gemini to enter Plan Mode for analysis-only
- This prevents accidental tool execution during the analysis phase

### 5.5 MCP Server Integration for Custom Tools

**What:** Gemini can connect to MCP servers for custom tools:
```json
{
  "mcpServers": {
    "database": { "command": "npx", "args": ["@my/db-mcp"] }
  }
}
```

**Why for Lok:**
- Lok could provide an MCP server for its own workflows → gemini could invoke Lok as a tool
- Bidirectional integration: Lok calls gemini AND gemini calls Lok
- Enables recursive analysis where gemini delegates back to Lok's other backends (Claude, Codex)

### 5.6 Memory / Skills Persistence

**What:** Gemini's Auto Memory system persists learnings across sessions via `GEMINI.md` files and skill extraction.

**Why for Lok:**
- Lok's long-running workflows could benefit from gemini "learning" about a codebase over multiple invocations
- Instead of re-analyzing from scratch on each `lok hunt`, gemini could load its previous analysis context
- Use `--import` / session export to maintain context across Lok workflow steps

### 5.7 `--model` Flag for Per-Step Model Selection

**What:** gemini-cli supports `-m model-name` or `--model model-name`.

**Current state in Lok:**
- `GeminiBackend::new()` accepts a `model` field from config
- `query()` supports a `model` parameter but it's `Option<&str>`
- `run_query_with_config()` always passes `None`
- No per-step model override in workflow TOML

**Fix:**
1. Add `model` to `WorkflowStep` config (overridable per step)
2. Thread it through `run_query_with_config` → `Backend::query()`
3. Add `--model` flag to `lok ask` command

### 5.8 Approval Mode Awareness for Non-Interactive Use

**What:** Gemini now supports `--approval-mode` flags:
- `default` — prompt for approval
- `auto_edit` — auto-approve edit tools
- `plan` — read-only, no mutations
- `yolo` — auto-approve all (CLI only)

**Why for Lok:**
- Lok's `apply_edits` workflows currently parse JSON edits from stdin
- Gemini could directly apply edits if told `--approval-mode auto_edit`
- This avoids the fragile "parse JSON from text output" pattern

---

## 6. Blind Spots

### 6.1 Gemini CLI version detection

Lok has no mechanism to detect which version of gemini-cli is installed. Features like `--output-format json` (v0.42+) only work on recent versions. `lok doctor` should check the version.

**Fix:** Add version detection:
```bash
npx @google/gemini-cli --version
```
Then store in `BackendConfig` and expose in `lok doctor` output.

### 6.2 No health check beyond binary existence

`GeminiBackend::is_available()` only checks if `npx` (the command) exists. It doesn't verify that gemini-cli is actually installed or that authentication works. A user could have npx but no @google/gemini-cli installed.

**Fix:** Add a lightweight ping:
```bash
npx @google/gemini-cli -p "ok" --output-format json
```
Cache the result for TTL (30s) to avoid auth-prompting on every check.

### 6.3 OAuth vs API key vs Vertex AI

Lok doesn't distinguish between gemini-cli authentication methods:
- OAuth (sign in with Google) — uses browser flow, blocks in headless
- `GEMINI_API_KEY` — simple, works headless
- `GOOGLE_API_KEY` + Vertex AI — enterprise
- `GOOGLE_CLOUD_PROJECT` — Code Assist license

When Lok runs headless (as `echo '' | ...`), OAuth prompts may hang waiting for browser redirect. The fix in v0.42.0 for `prevent silent hang during OAuth auth on headless Linux` helps, but Lok should prefer API key auth.

**Recommendation:** `lok doctor` should detect which auth method is available and warn if only OAuth is configured (since it may hang in headless mode).

### 6.4 Multi-line prompts

Current shell-escaped single-quote approach breaks on prompts containing single quotes. MiniJinja templates may produce multi-line prompts with embedded quotes, leading to shell injection or truncation.

**Fix:** Use `--prompt-file` or pipe prompt via stdin with `-p -` (if gemini-cli supports it). Alternatively, base64-encode the prompt and decode in the shell command.

### 6.5 No support for `--include-directories`

Gemini's `--include-directories` flag lets it read project files. Lok never passes this, so gemini operates without file context. Other backends (Claude, Codex) automatically include context from the working directory, but gemini-cli requires explicit directory inclusion.

**Fix:** When `cwd` is set, pass `--include-directories <cwd>` to gemini-cli.

### 6.6 Token/rate limit awareness

Gemini's free tier has 60 req/min and 1000 req/day limits. Lok doesn't track rate limits, leading to silent 429 errors that fall through to generic `ExecutionFailed`.

### 6.7 `skip_lines` fragility

The `skip_lines = 1` in config assumes gemini-cli always outputs one line of banner/metadata before the response. Any gemini-cli update that changes output prefix would break parsing silently.

---

## 7. Improving Lok → Gemini CLI Interaction & Stability

### 7.1 Architecture: From Pipe to Protocol

**Current (brittle, text-only):**
```
Lok ──sh -c "echo '' | npx @google/gemini-cli 'prompt'"──▶ Gemini CLI
Lok ◀── stdout (raw text, skip_lines=1) ────────────────▶ Gemini CLI
```

**Target (structured, reliable):**
```
Lok ──npx @google/gemini-cli -p <prompt> --output-format json ──▶ Gemini CLI
Lok ◀── JSON { response, stats, error } ──────────────────────────▶ Gemini CLI
```

### 7.2 Stability Improvements

| Area | Current | Improvement |
|------|---------|-------------|
| **Command construction** | Shell-escaped single quotes | `--prompt-file` or base64-encoded pipe |
| **Output parsing** | `skip_lines` + raw text | `--output-format json` + `serde_json` |
| **Error handling** | Generic `ExecutionFailed` | Exit code mapping + JSON error field |
| **Timeout** | Process-level kill (600s) | Gemini's turn limit (exit 53) + Lok timeout |
| **Authentication** | None (relies on env vars) | `lok doctor` auth method detection |
| **Context** | No `--include-directories` | Auto-pass cwd as included directory |
| **Model selection** | Hardcoded default | Configurable per-backend, per-step |
| **Retry** | Generic retry loop | Gemini-specific retry on 429/53 |
| **Token tracking** | None | `stats` from JSON response → `TokenUsage` |
| **Version check** | None | `--version` in `lok doctor` |

### 7.3 Workflow Integration

The following workflow step config additions would dramatically improve gemini-cli integration:

```toml
[[steps]]
name = "deep_analysis"
backend = "gemini"

# NEW: Use JSON output for structured parsing
output_format = "json"

# NEW: Session persistence for multi-step context
session = { export = ".lok/sessions/gemini.json", import = true }

# NEW: Model selection per step
model = "gemini-2.5-pro"

# NEW: Include project files as context
include_directories = true

# NEW: Approval mode for non-interactive use
approval_mode = "plan"

prompt = "Analyze the security of this codebase..."
```

### 7.4 Proposed `GeminiBackend` Rewrite

```rust
// Target architecture for gemini.rs rewrite

pub struct GeminiBackend {
    command: String,       // "npx" or "gemini"
    args: Vec<String>,     // ["@google/gemini-cli"]
    default_model: Option<String>,
    output_format: OutputFormat, // Text | Json | StreamJson
    session: Option<SessionConfig>,
    include_directories: bool,
    approval_mode: ApprovalMode,
}

impl Backend for GeminiBackend {
    async fn query(&self, prompt: &str, cwd: &Path, model: Option<&str>) -> Result<QueryOutput, BackendError> {
        // 1. Build args: [-p <prompt>, --output-format json, --model <m>, --include-directories <cwd>]
        // 2. Spawn process (no shell pipe)
        // 3. Parse JSON response
        // 4. Map stats → TokenUsage
        // 5. Map errors by exit code
        // 6. Return structured QueryOutput
    }
}
```

### 7.5 Testing Strategy

| Test | What it validates |
|------|------------------|
| `gemini_json_output_parsing` | JSON response from `--output-format json` parses correctly |
| `gemini_exit_code_mapping` | Exit codes 0, 1, 42, 53 map to correct BackendError variants |
| `gemini_token_usage_extraction` | stats field in JSON maps to TokenUsage correctly |
| `gemini_multi_line_prompt` | Prompts with quotes, newlines, special chars don't break shell |
| `gemini_auth_detection` | `lok doctor` correctly reports OAuth vs API key vs none |
| `gemini_version_detection` | `--version` output parses correctly |
| `gemini_session_export_import` | Round-trip session save/load works across workflow steps |
| `gemini_skip_lines_regression` | If `skip_lines` is still used, catch upstream format changes |

---

## 8. Prioritized Action Items

### 🔴 P0 — Must fix (stability/security)

| # | Action | Effort | Impact | Notes |
|---|--------|--------|--------|-------|
| 1 | Switch to `--output-format json` for structured output | 2d | Eliminates fragile `skip_lines` parsing. Enables token tracking. | Blocker for all other improvements |
| 2 | Proper exit code → BackendError mapping | 0.5d | Correct error classification, better retry decisions | Depends on #1 |
| 3 | Fix shell escaping for multi-line prompts | 0.5d | Prevents shell injection, supports template-generated prompts | Use `--prompt-file` or base64 pipe |
| 4 | Add gemini-cli version detection to `lok doctor` | 0.5d | Prevents users from using incompatible features | Check for `--output-format` support |

### 🟡 P1 — Should fix (feature parity)

| # | Action | Effort | Impact | Notes |
|---|--------|--------|--------|-------|
| 5 | Pass `--include-directories <cwd>` for file context | 0.5d | Gemini can now read project files | Currently operates context-free |
| 6 | Wire model selection through config → query() | 1d | Per-backend and per-step model selection | Config field exists but unused |
| 7 | Add auth method detection to `lok doctor` | 1d | Warn on OAuth-only in headless mode | Prevents silent hangs |
| 8 | Extract token usage from JSON stats → TokenUsage | 0.5d | Cost tracking, observability | Depends on #1 |

### 🟢 P2 — Nice to have (power features)

| # | Action | Effort | Impact | Notes |
|---|--------|--------|--------|-------|
| 9 | Session export/import for workflow context persistence | 3d | Multi-step workflows with memory | Track gemini-cli `--export`/`--import` API stability |
| 10 | Stream-json support for progress tracking | 2d | Visible progress on 5+ min analysis | Complex; worth prototyping |
| 11 | Approval mode awareness | 1d | Non-interactive `auto_edit` for apply_edits workflows | Depends on #1 |
| 12 | GitHub Action backend type | 5d | CI-integrated deep analysis | Explore before committing |
| 13 | MCP server for bidirectional Lok↔Gemini | 8d+ | Gemini can invoke Lok backends as tools | Long-term architecture |
| 14 | Subagent protocol integration | 10d+ | Structured agent-to-agent communication | Monitor stability first |

### Quick Wins (can be done in <2 hours each)

- [ ] Add `model` field to `lok.toml` `[backends.gemini]` section (field exists, just update config docs)
- [ ] Add `gemini --version` check to `lok doctor`
- [ ] Update README to recommend `npm install -g @google/gemini-cli` (vs npx) for stable installations
- [ ] Document known exit codes (0, 1, 42, 53) in developer docs
- [ ] Add `--output-format json` to the default args in `Config::default()` for gemini backend

---

## 9. Appendix: Full Release Breakdown

### v0.44.0-nightly.20260515.g928a311fb (2026-05-15)
- RAG snippets to local log file for debugging
- Fix conflicting credentials on enterprise gateways
- `NO_PROXY` support for MCP servers
- Permission denied fix for NixOS sandbox
- EISDIR fixes for file processing
- `https-proxy-agent` externalized for proxy support

### v0.44.0-nightly.20260514.g77078b3e8 (2026-05-14)
- **Merge Auto modes into a single Auto mode**
- **Agent registration first-wins, prioritize project**
- Incremental refactor repo agent towards skills-based composition
- Fix preserved OAuth refresh tokens
- Keychain auth for non-interactive mode
- Auto-approve shell redirections in AUTO_EDIT mode
- Fix EISDIR on virtual drives

### v0.43.0-preview.0 (2026-05-12)
- **Steer model to use edit tool for surgical edits**
- **Export session to file and import via flag** ← HIGH VALUE
- **LocalSubagentProtocol behind AgentProtocol** ← HIGH VALUE
- **RemoteSubagentProtocol behind AgentProtocol** ← HIGH VALUE
- **Shell command safety evals**
- Adaptive token calculator for content sizes
- Fix chat corruption bug in context manager
- JSON output for AgentExecutionStopped in non-interactive mode
- ADK non-interactive session support
- A2A server race condition fix

### v0.42.0 (stable, 2026-05-12)
- Prevent automatic updates from switching to less stable channels
- Handle DECKPAM keypad Enter sequences
- Add `--delete` flag to `/exit` command for session deletion
- Add `@mention` for gemini robot in GitHub issues
- OAuth fields support in subagent parsing
- Disconnect extension-backed MCP clients
- Add ability to @mention gemini robot

### v0.42.0-nightly.20260512.gc987b9939
- Snapshotter model config improvements
- Install extensions from SSH repos
- Prevent infinite thought loop in ACP mode
- Static tool name in confirmation prompts
- Handle malformed projects.json
- Ignore .pak and .rpa game archive formats

### v0.42.0-nightly.20260511.g1a894c18e
- Preserve system PATH in Git environment (ENOENT fix)
- Cache model routing decision in LocalAgentExecutor
- Hide /memory add subcommand when memoryV2 enabled
- Prevent false command conflicts from home directory
- **Export session to file and import via flag**
- Machine hostname in CLI interface

### v0.42.0-nightly.20260507.ga809bc7c5
- JSON output for AgentExecutionStopped
- Shell command safety evals
- Handle invalid custom plans directory gracefully
- Randomize sandbox container names
- Fix hysteresis in async context management
- Tighten private Auto Memory patch allowlist

### v0.42.0-nightly.20260506.g80d269054
- A2A server tool approval race condition fix
- Allow queuing messages during compression
- Retry on ERR_STREAM_PREMATURE_CLOSE errors
- Generalist profile fixes
- Reject numeric project IDs in GOOGLE_CLOUD_PROJECT

### v0.41.2 (patch, 2026-05-06)
- Cherry-pick A2A server race condition fix

---

## References

- [Gemini CLI GitHub](https://github.com/google-gemini/gemini-cli)
- [Headless Mode Documentation](https://geminicli.com/docs/cli/headless)
- [Commands Reference](https://geminicli.com/docs/reference/commands)
- [Tools Reference](https://geminicli.com/docs/reference/tools)
- [GitHub Action: run-gemini-cli](https://github.com/google-github-actions/run-gemini-cli)
- [Lok: AI-AGENTS.md](../AI-AGENTS.md)
- [Lok: Backend Implementation](../src/backend/gemini.rs)
- [Lok: Backend Trait](../src/backend/mod.rs)
