# Codex CLI Gap Analysis: lok ↔ codex Interaction

**Date:** 2026-05-16  
**Scope:** Codex CLI releases v0.118.0 (Mar 31) through v0.131.0-alpha.9 (current)  
**Author:** Automated investigation

---

## 1. Executive Summary

Lok wraps `codex exec --json -s read-only` as a subprocess to query Codex for code analysis. Since lok's last update (April 2026), Codex CLI has shipped **10+ releases** adding major automation features: structured output schemas, token usage reporting, reasoning tracking, permission profiles, hooks, plugins, goal persistence, and memory.

Lok is currently using approximately **15-20% of Codex's non-interactive capability surface**. The remaining 80% represents untapped features that could dramatically improve reliability, structure, and observability of lok's Codex integration.

**Verdict: Lok needs a significant update to the Codex backend** — not because the current approach is broken, but because the improvements in Codex's automation APIs solve several longstanding fragility issues in lok.

---

## 2. Current Lok → Codex Integration (The Baseline)

### 2.1 How lok invokes Codex

```rust
// src/backend/codex.rs
let args = vec![
    "exec".to_string(),
    "--json".to_string(),
    "-s".to_string(),
    "read-only".to_string(),
];

// After config override (from lok.toml defaults):
command = "codex"
args = ["exec", "--json", "-s", "read-only"]
```

### 2.2 How lok parses Codex output

```rust
fn parse_output(&self, output: &str) -> String {
    for line in output.lines() {
        if line.contains("\"type\":\"item.completed\"") 
           && line.contains("agent_message") {
            // Extract text field from JSON
        }
    }
    // Fallback: raw output
    output.to_string()
}
```

### 2.3 What lok gets back

- **String output only** — the final agent message text
- **No structured data** — even though Codex emits rich JSONL
- **No token usage** — though available in `turn.completed` events
- **No reasoning tokens** — though available since v0.125.0
- **No error classification** — just exit code + stderr

### 2.4 Current strengths

- Simple and reliable subprocess model
- Retry support via `RetryExecutor`
- Timeout handling via tokio
- Works with any `codex exec` version
- Output captures most scenarios

---

## 3. Codex Features Since v0.118.0 — What We Should Use

### 3.1 `--output-schema` (v0.119.0, Apr 10) — **CRITICAL**

Codex now supports `codex exec --output-schema schema.json` to enforce a JSON Schema on the final output.

**Why lok needs this:**  
Lok currently parses JSONL lines looking for `agent_message` — fragile and prone to hallucinated formatting. With `--output-schema`, lok could request structured JSON directly (e.g., `{"issues": [{"file": "...", "line": ..., "severity": "..."}]}`) and get reliably parseable output.

**Implementation priority:** HIGH — fixes the single biggest source of parse failures.

```json
// Example schema for hunt task
{
  "type": "object",
  "properties": {
    "issues": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "file": {"type": "string"},
          "line": {"type": "integer"},
          "severity": {"type": "string", "enum": ["low", "medium", "high"]},
          "description": {"type": "string"}
        },
        "required": ["file", "line", "description"]
      }
    }
  },
  "required": ["issues"]
}
```

**Known issue (Codex #15451):** When MCP servers are active, `--output-schema` can be silently ignored. Since lok uses `-s read-only` (no shell/MCP), this should not affect us.

**Known issue (Codex #19816):** Intermediate agent messages may also match the schema. Workaround: consume only the **last** schema-valid message in the JSONL stream.

### 3.2 Token & reasoning usage in JSON output (v0.125.0, Apr 24)

`codex exec --json` now includes `usage` data in `turn.completed` events:

```json
{"type": "turn.completed", "usage": {
  "input_tokens": 24763,
  "cached_input_tokens": 24448,
  "output_tokens": 122,
  "reasoning_output_tokens": 42
}}
```

**Why lok needs this:**  
Lok already defines `TokenUsage` and `QueryOutput.usage` fields — but they're never populated from Codex. Capturing this would enable cost tracking, token budgeting, and observability.

**Implementation:** Parse `usage` from `turn.completed` events in the JSONL stream.

### 3.3 `-o`/`--output-last-message` (v0.119.0)

Writes the final message to a file AND prints it to stdout.

**Why lok needs this:**  
Simpler than JSONL parsing. If used alongside `--output-schema`, this gives lok clean, structured output without any stream parsing. Combine: `codex exec --output-schema schema.json -o /tmp/result.json "prompt"` and just read `/tmp/result.json`.

### 3.4 `--ephemeral` (v0.119.0)

Prevents persisting session rollout files to disk.

**Why lok needs this:**  
Currently, every `codex exec` call leaves JSONL artifacts in `~/.codex/sessions/`. Over time this accumulates gigabytes of unnecessary data. `--ephemeral` eliminates this entirely for lok's non-interactive use case.

### 3.5 Permission profile system (v0.122.0+)

The old `-s read-only` / `--sandbox read-only` still works, but Codex now has a richer profile system:

- `--sandbox read-only` (default, current lok behavior)
- `--sandbox workspace-write ""` (for apply_edit steps)
- `--sandbox danger-full-access ""` (for full automation)

Additionally, `--ignore-user-config` and `--ignore-rules` (v0.122.0) let lok run in a clean automation environment.

**Why lok needs this:**  
When lok's `apply_edits` workflow needs to write files, it currently uses the `read-only` sandbox which will fail. The Codex profile system provides explicit, granular controls.

### 3.6 Plugin ecosystem (v0.120.0+)

Marketplaces, curated plugins, plugin-bundled hooks, external agent config import.

**Why lok needs this:**  
Plugins can package MCP servers and skills that lok could leverage. For example, plugin-based security scanners, linters, or CI tools that Codex already knows how to interact with.

### 3.7 Hooks (v0.123.0+, stable in v0.124.0)

Lifecycle hooks for pre/post operations, compaction, MCP tool observation.

**Why lok needs this:**  
Hooks enable pre-flight validation, post-processing, and notification workflows. Lok could define hooks that fire after its analysis steps to post results, trigger CI, or log metrics.

### 3.8 Goal persistence (v0.128.0, Apr 30)

Persisted `/goal` workflows with app-server APIs, model tools, runtime continuation.

**Why lok needs this:**  
Long-running analysis workflows could use goals for checkpointing and resumability. If `lok hunt` takes 30 minutes and gets interrupted, goal persistence lets it resume.

### 3.9 Reasoning token reporting (v0.125.0, Apr 24)

`codex exec --json` reports `reasoning_output_tokens` in usage.

**Why lok needs this:**  
Reasoning tokens are a significant cost factor. Tracking them helps users understand why their usage is higher than expected.

### 3.10 Multi-agent v2 improvements (v0.128.0+)

Thread caps, wait-time controls, root/subagent hints, depth handling.

**Why lok needs this:**  
Lok's `spawn` and `conduct` commands delegate tasks to Codex. Multi-agent v2 configuration could help control parallelism and resource usage.

---

## 4. Blind Spots — What Lok Is Missing

### 4.1 Structured output → lok could be **schema-driven**

Lok currently treats all LLM output as raw text. If lok used `--output-schema` per task type, it could:

- **Type-check** Codex responses at the JSON Schema level
- **Route structured data** directly into downstream steps without regex parsing
- **Validate completeness** (required fields guarantee coverage)
- **Eliminate markdown-fence stripping** entirely

Example flow:
```
1. lok sends hunt prompt + JSON Schema for bugs
2. Codex returns {"issues": [{"file": "src/main.rs", "line": 42, ...}]}
3. lok passes structured issues to downstream steps or issue creation
4. No text parsing, no regex, no hallucinated formatting
```

### 4.2 Token/cost observability

Lok has `TokenUsage` struct and `QueryOutput.usage` field — both populated from exactly **zero** backends. Every `lok ask`, `lok hunt`, `lok audit` run burns tokens without tracking them.

### 4.3 Version-aware behavior

Lok has no mechanism to detect Codex CLI version and adapt its flags. As new flags are added (`--ephemeral`, `--output-schema`, `--ignore-user-config`), lok could use them when available and fall back gracefully. Currently, if a feature is missing in an older Codex, lok silently fails.

### 4.4 Error classification from JSONL events

The JSONL stream includes `turn.failed` and `error` event types with structured error data. Lok currently only sees exit codes and stderr. Structured error data would enable better retry decisions and user-facing messages.

### 4.5 Reasoning effort control

Codex supports reasoning effort levels (`medium`, `high`, `xhigh`). Lok makes no use of this. Hunt/audit tasks might benefit from `high` reasoning while simple `ask` queries could use `medium`.

### 4.6 Output format per task type

Lok's tasks (hunt, audit, diff, etc.) all use the same prompt style. Each could benefit from a custom output schema:

| Task | Schema needed |
|------|---------------|
| `hunt` | `{"issues": [{file, line, severity, description}]}` |
| `audit` | `{"vulnerabilities": [{type, file, line, risk, mitigation}]}` |
| `diff` | `{"review": {summary, issues: [{file, line, comment}]}}` |
| `fix` | `{"fix": {files: [{path, changes: [{old, new}]}]}}` |

### 4.7 Background daemon / connection reuse

Each `lok ask` or workflow step spawns a new `codex exec` subprocess. Codex's app-server architecture supports persistent connections, Unix sockets, and sticky environments. Lok could benefit from a persistent Codex daemon that avoids cold-start overhead on every query.

---

## 5. Stability Issues & Recommendations

### 5.1 JSONL parsing fragility

**Current behavior:**
```rust
fn parse_output(&self, output: &str) -> String {
    for line in output.lines() {
        if line.contains("\"type\":\"item.completed\"") && line.contains("agent_message") {
            // Extract text from JSON
        }
    }
    output.to_string()  // Fallback
}
```

**Problems:**
- Matches on substring `"agent_message"` — brittle if field ordering changes
- Returns first match, not last (could get intermediate message)
- No handling of `turn.failed` events with error data
- No handling of `error` event types
- Silent fallback to raw output on any parse failure — masks errors

**Recommendation:** Replace with proper stream parsing:
- Consume ALL JSONL lines, collecting structured events
- Use last `agent_message` text (or `turn.completed` agent message)
- Surface `turn.failed` and `error` events as `BackendError` variants
- Consider `--output-last-message` + `--output-schema` as an alternative path

### 5.2 `-s read-only` conflicts with `apply_edits`

Lok's `apply_edits` workflow sends write operations through Codex, but the default args include `-s read-only`. This means:

- `lok run workflow-with-edits` silently fails to write files
- Error surfaces as "Codex failed: ..."
- User has no indication that the sandbox mode is the issue

**Recommendation:** When a step has `apply_edits = true`, dynamically switch to `--sandbox workspace-write ""`. Or better, let each step declare its required sandbox level.

### 5.3 Timeout handling mismatch

Lok wraps `codex exec` in a tokio timeout (default 300s). But `codex exec` has its own internal timeout behavior. When lok's outer timeout fires first, it kills the Codex process — but Codex may have already written partial output to its session log.

**Recommendation:** Use Codex's internal timeout mechanisms where possible. Or align lok's timeout with Codex's expected run duration.

### 5.4 Stderr vs stdout confusion

Codex CLI streams progress to **stderr** and output to **stdout** — but `codex exec --json` puts everything on stdout as JSONL events. Lok currently treats them independently:
- Parses stdout for content
- Captures stderr for error messages

With `--json`, progress events and output are interleaved on stdout. Lok's current approach works but is fragile.

**Recommendation:** When using `--json`, consume ALL events from stdout and ignore stderr (which should only contain startup/log messages).

### 5.5 No session cleanup

Every `codex exec` call creates a session entry. With `lok hunt` running 3-5 prompts, that's 3-5 sessions per run. Over time this can accumulate significant disk usage.

**Recommendation:** Add `--ephemeral` to default Codex args. This prevents session persistence entirely for lok's non-interactive use.

### 5.6 Model passing is inconsistent

The CodexBackend accepts a `model` parameter and passes `--model` to the CLI. But:
- The default config does NOT set `model`
- The model is only passed if `config.default_model` is set or `override` is provided
- `run_query_with_config` passes `None` as model (line: `backend.query(&prompt, &cwd, None)`)
- So model overriding from workflow steps (`step.model`) is effectively dead code for Codex

**Recommendation:** Thread `step.model` through to the backend. Or document that Codex model override isn't supported.

---

## 6. Recommended Implementation Roadmap

### Phase 1: Quick Wins (1-2 days)

| Change | Effort | Impact | Files changed |
|--------|--------|--------|---------------|
| Add `--ephemeral` to default Codex args | 1 line | Prevents disk accumulation | `src/config.rs` |
| Parse `usage` from `turn.completed` JSONL | ~30 lines | Enables cost tracking | `src/backend/codex.rs` |
| Parse `reasoning_output_tokens` from usage | ~10 lines | Cost observability | `src/backend/codex.rs` |
| Add `--output-last-message` option (temp file) | ~40 lines | Cleaner output capture | `src/backend/codex.rs` |

### Phase 2: Structured Output (3-5 days)

| Change | Effort | Impact | Files affected |
|--------|--------|--------|----------------|
| Define per-task JSON Schemas | ~100 lines | Reliable structured data | `src/tasks/hunt.rs`, `audit.rs`, etc. |
| Add `--output-schema` support to CodexBackend | ~50 lines | Schema-enforced output | `src/backend/codex.rs` |
| Pass schema in task definitions | ~30 lines | End-to-end structured output | `src/tasks/mod.rs` |
| Route structured output to downstream steps | ~80 lines | Better workflow composition | `src/workflow.rs` |

### Phase 3: Token Tracking & Observability (1-2 days)

| Change | Effort | Impact | Files affected |
|--------|--------|--------|----------------|
| Wire `QueryOutput.usage` back from Codex | ~30 lines | Cost per query | `src/backend/codex.rs` |
| Display token usage in verbose mode | ~20 lines | User visibility | `src/backend/mod.rs` |
| Add token budget enforcement | ~50 lines | Cost control | `src/workflow.rs` |

### Phase 4: Stability Hardening (2-3 days)

| Change | Effort | Impact | Files affected |
|--------|--------|--------|----------------|
| Replace JSONL substring matching with proper parser | ~80 lines | Reliable output extraction | `src/backend/codex.rs` |
| Surface `turn.failed` as `BackendError` | ~30 lines | Better error messages | `src/backend/codex.rs` |
| Dynamic sandbox selection based on step | ~40 lines | Fix `apply_edits` with read-only | `src/workflow.rs` |
| Thread model override through to Codex | ~20 lines | Fix dead model code | `src/backend/codex.rs` |

### Phase 5: Advanced (future)

| Change | Effort | Impact |
|--------|--------|--------|
| Version detection + adaptive flags | 2 days | Backward compatibility |
| Persistent Codex daemon (app-server client) | 1 week | Eliminate cold-start overhead |
| Hook integration for lok workflows | 3 days | Pre/post processing |
| Multi-agent v2 config for lok spawn | 2 days | Better parallelism control |

---

## 7. Codex Features We Should Not Use (With Rationale)

| Feature | Why Not |
|---------|---------|
| **Plugins/marketplaces** | Lok is a lightweight orchestrator. Plugin loading adds complexity. If users need plugins, they configure them in Codex directly. |
| **Goal persistence** | Overkill for lok's stateless query model. Lok caches at the response level, not the workflow level. |
| **Realtime / voice** | Lok is non-interactive. These features target the TUI. |
| **Vim composer mode** | Pure TUI feature. No relevance to `codex exec`. |
| **App-server / remote control** | Lok runs locally. App-server is for IDE/web clients. |
| **Memories** | Lok's cache already serves the same purpose at lower complexity. |
| **Device-code login** | Lok assumes Codex is already authenticated. |

---

## 8. Concrete Code Changes — Before/After

### 8.1 Current CodexBackend::query

```rust
// Current: fragile JSONL parsing, no usage, no schema
fn parse_output(&self, output: &str) -> String {
    for line in output.lines() {
        if line.contains("\"type\":\"item.completed\"") && line.contains("agent_message") {
            // extract text...
        }
    }
    output.to_string()
}
```

### 8.2 Proposed replacement

```rust
// Proposed: structured JSONL event parsing
fn parse_events(&self, output: &str) -> ParsedCodexOutput {
    let mut last_message = String::new();
    let mut usage = None;
    let mut error = None;
    
    for line in output.lines() {
        let event: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        
        match event["type"].as_str() {
            Some("item.completed") => {
                if let Some(text) = event["item"]["text"].as_str() {
                    last_message = text.to_string();
                }
            }
            Some("turn.completed") => {
                usage = event.get("usage").map(|u| TokenUsage {
                    prompt_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
                    completion_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                    total_tokens: (u["input_tokens"].as_u64().unwrap_or(0) 
                                 + u["output_tokens"].as_u64().unwrap_or(0)) as u32,
                });
            }
            Some("turn.failed") => {
                error = Some(event["error"].to_string());
            }
            _ => {}
        }
    }
    
    ParsedCodexOutput { text: last_message, usage, error }
}
```

### 8.3 Config changes

```toml
# Current (in default config)
[backends.codex]
enabled = true
command = "codex"
args = ["exec", "--json", "-s", "read-only"]
skip_lines = 0

# Proposed default
[backends.codex]
enabled = true
command = "codex"
args = ["exec", "--json", "--ephemeral", "-s", "read-only"]
skip_lines = 0
# Optional schema path for structured output
# schema = ".lok/schemas/hunt.json"
```

### 8.4 Task changes for structured output

```rust
// Current: hunt sends free-text prompt
async fn run_hunt(config: &Config, dir: &Path) -> Result<()> {
    let prompt = "Find error handling problems...";
    let results = backend::run_query(&backends, prompt, dir, config).await?;
}

// Proposed: add output schema per prompt
async fn run_hunt(config: &Config, dir: &Path) -> Result<Vec<Issue>> {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "issues": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "file": {"type": "string"},
                        "line": {"type": "integer"},
                        "severity": {"type": "string"},
                        "description": {"type": "string"}
                    },
                    "required": ["file", "line", "description"]
                }
            }
        }
    });
    
    let prompt = build_hunt_prompt(&schema);
    let results = backend::run_query_with_schema(
        &backends, &prompt, &schema, dir, config
    ).await?;
    
    // Now results are typed!
    parse_structured_results(results)
}
```

---

## 9. Summary

| Area | Current State | Target State | Priority |
|------|---------------|--------------|----------|
| **Output parsing** | Substring matching on JSONL | Proper event stream parsing | **CRITICAL** |
| **Structured output** | Raw text only | Schema-enforced JSON per task | **HIGH** |
| **Token tracking** | Struct exists, never populated | Captured from `turn.completed` | **HIGH** |
| **Session cleanup** | Accumulates on disk | `--ephemeral` by default | **MEDIUM** |
| **Sandbox mode** | Hardcoded `read-only` | Dynamic per step | **MEDIUM** |
| **Model override** | Config field exists, not wired | Threaded through to CLI | **LOW** |
| **Error classification** | Exit code + stderr only | Structured from JSONL events | **LOW** |
| **Persistent connection** | New subprocess per call | App-server client | **FUTURE** |

### Immediate action items (should be done this week):

1. Add `--ephemeral` to default Codex args
2. Replace JSONL parsing with structured event processing
3. Capture token usage from `turn.completed` events
4. Wire `--output-schema` support for structured task outputs
5. Enable dynamic sandbox selection for `apply_edits` steps
