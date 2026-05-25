# CLO-393: FR-14 — lok doctor HealthStatus renderer (table + JSON)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-393
**Status**: Implemented
**Author**: Mk Km
**Created**: 2026-05-25

---

## Summary

Replace the hardcoded `Commands::Doctor` handler with a renderer that reads `HealthStatus` from `BACKEND_CACHE` (populated by `warmup_backends`) and outputs either a human-readable table or machine-readable JSON.

---

## Background

Before CLO-393, `lok doctor` had a hardcoded list of backends and API keys:
```rust
let checks = vec![
    ("codex", "codex", "npm install -g @openai/codex"),
    ("gemini", "opencode", "Install opencode: ..."),
    ("claude", "claude", "Install Claude Code: ..."),
];
```

This was disconnected from the actual backend configuration and didn't surface `HealthStatus` data from the new `health_check()` probes added in CLO-388, CLO-389, CLO-391, CLO-392, CLO-395.

CLO-393 makes `lok doctor` read from `BACKEND_CACHE` to report real, per-backend health status including version, auth mode, and diagnostic messages.

### Dependencies

- **CLO-388** — `warmup_backends` + `HealthStatus` struct (already landed)
- **CLO-391** — Claude dual-mode probe adds `mode: Option<String>` discriminator (already landed)
- **CLO-395** — Gemini health probe via opencode (already landed)

---

## Design

### Location

All Doctor rendering logic lives in `src/main.rs` — the change is small enough (~270 lines) that a separate module is unnecessary.

### Command interface

```bash
lok doctor              # Human-readable table (default)
lok doctor --output json # Machine-readable JSON
```

The `Commands::Doctor` variant changes from a unit variant to a struct variant:
```rust
Commands::Doctor {
    #[arg(long, value_name = "FORMAT", default_value = "table")]
    output: String,
}
```

### Table output

| Column    | Source                                    |
|-----------|-------------------------------------------|
| BACKEND   | Config key (e.g., "gemini", "codex")      |
| MODE      | `HealthStatus.mode` or "—"                |
| VERSION   | `HealthStatus.version` or "—"             |
| AVAILABLE | `✓ yes` (green) or `✗ no` (red)          |
| NOTES     | `HealthStatus.diagnostic`, truncated      |

Terminal width: check `COLUMNS` env var, fallback to 80.

### JSON output

Array of flat objects, each with `backend` key from config plus all `HealthStatus` fields:
```json
[
  {
    "backend": "gemini",
    "available": true,
    "version": "1.15.10",
    "mode": "api-key",
    "diagnostic": null,
    ...
  }
]
```

### Exit codes

- `0`: all configured, enabled backends are available
- `1`: any backend is unavailable or absent from cache

### Edge cases

- **Backend configured but not in cache**: shown as unavailable with diagnostic explaining construction may have failed
- **No backends configured**: prints "No backends configured." in yellow
- **Claude dual-probe**: CLO-391 dependency; shows single row for now

### No new dependencies

Existing crates suffice: `serde_json` (JSON), `colored` (terminal colors).

---

## Acceptance Criteria

1. `lok doctor` outputs a table with BACKEND, MODE, VERSION, AVAILABLE, NOTES columns
2. `lok doctor --output json` outputs valid JSON array with `backend` + all `HealthStatus` fields
3. Exit code 0 when all backends available, 1 otherwise
4. Backends in config but not in cache are shown as unavailable with diagnostic
5. `cargo fmt --check && cargo clippy -- -D warnings && cargo test` passes
6. Integration test exists for `--output json`

---

## Assumptions

No design assumptions were recorded for this task. The design is straightforward with no external dependencies beyond what was already landed.
