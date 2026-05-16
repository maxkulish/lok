# Codex ↔ Lok Interaction: Quick Reference

## Key Codex Flags for Lok

| Flag | Since | Purpose | In Lok? |
|------|-------|---------|---------|
| `--json` | v0.118.0 | JSONL output stream | ✅ Yes |
| `--output-schema <file>` | v0.119.0 | Structured output via JSON Schema | ❌ No |
| `-o` / `--output-last-message <file>` | v0.119.0 | Write final message to file | ❌ No |
| `--ephemeral` | v0.119.0 | Don't persist session artifacts | ❌ No |
| `-s` / `--sandbox <profile>` | v0.118.0 | Sandbox mode: `read-only`, `workspace-write`, `danger-full-access` | ✅ Partial (hardcoded) |
| `--ignore-user-config` | v0.122.0 | Skip user config for clean CI runs | ❌ No |
| `--ignore-rules` | v0.122.0 | Skip exec-policy rules files | ❌ No |
| `--model <name>` | v0.118.0 | Override model | ✅ Partial (not threaded) |
| `--skip-git-repo-check` | v0.119.0 | Bypass git check requirement | ❌ No |

## JSONL Event Types (for `--json`)

| Event | Contains |
|-------|----------|
| `thread.started` | `thread_id` |
| `turn.started` | — |
| `turn.completed` | `usage` (`input_tokens`, `cached_input_tokens`, `output_tokens`, `reasoning_output_tokens`) |
| `turn.failed` | `error` details |
| `item.started` | item type (`command_execution`, `agent_message`, etc.) |
| `item.completed` | `item.text` for agent messages, structured data for tools |
| `error` | Error description |

## Recommended Default Args

```toml
# Current:
args = ["exec", "--json", "-s", "read-only"]

# Recommended:
args = ["exec", "--json", "--ephemeral", "-s", "read-only"]

# For apply_edits steps (dynamic):
args = ["exec", "--json", "--ephemeral", "--sandbox", "workspace-write"]
```

## Per-Task Output Schema Ideas

### Hunt
```json
{
  "type": "object",
  "properties": {
    "issues": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "type": {"type": "string", "enum": ["error", "perf", "dead-code"]},
          "file": {"type": "string"},
          "line": {"type": "integer"},
          "description": {"type": "string"}
        },
        "required": ["file", "description"]
      }
    }
  },
  "required": ["issues"]
}
```

### Audit
```json
{
  "type": "object",
  "properties": {
    "vulnerabilities": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "type": {"type": "string", "enum": ["injection", "auth", "secrets"]},
          "severity": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
          "file": {"type": "string"},
          "line": {"type": "integer"},
          "mitigation": {"type": "string"}
        },
        "required": ["type", "severity", "file", "line"]
      }
    }
  },
  "required": ["vulnerabilities"]
}
```
