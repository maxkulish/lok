# Pre-PR validation: clo-391

**Reviewer**: Codex (gpt-5.5)
**Validated**: 2026-05-24
**Pipeline**: lok pre-pr-validation
---

## Verdict: FAIL

## Findings

- HIGH: `ANTHROPIC_API_KEY` unset does not produce the required API health status. [ClaudeBackend::new](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/claude.rs:76) still fails construction when the env var is missing, so [warmup_backends](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/mod.rs:490) logs and skips caching any `HealthStatus`. The implemented `probe_api()` missing-key branch is only reachable in tests that manually construct an empty `SecretString`, not through real config/warmup. This misses the design acceptance criterion: API unset -> `available: false`, `mode: "api"`.

- HIGH: CLI timeout does not reliably stop the spawned process. Both [get_help_output](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/claude.rs:343) and [probe_cli](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/claude.rs:414) wrap `Command::output()` in `tokio::time::timeout`, but the command is not configured with `kill_on_drop(true)` or explicitly killed on timeout. A hung `claude --version` or `claude --help` can continue after the 2s budget, which violates the probe budget and is a process-management regression risk.

- MEDIUM: New `HealthStatus` fields are not backward-compatible for deserialization. [mode/diagnostic](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/context.rs:63) are required serde fields because they lack `#[serde(default)]`. Any existing serialized health payload without these fields will fail to deserialize; the design explicitly called out adding serde defaults if this risk appears.

- LOW: Logging does not match the design. The implementation uses `eprintln!` for CLI failures and unsupported JSON output at [claude.rs:427](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/claude.rs:427), [claude.rs:431](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/claude.rs:431), and [claude.rs:446](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/claude.rs:446). The design requires `tracing::warn!` for timeouts and `tracing::debug!` for version parse failures; `--help` timeout is currently silent.

- LOW: Version parsing only checks the first stdout line. [probe_cli](/Users/mk/Code/orchestrator/lok--feat-clo-391/src/backend/claude.rs:423) calls `parse_semver_line(stdout.lines().next()...)`, while the design preference/edge case says to find `X.Y.Z` anywhere in `claude --version` output.

## Missing Items

- Real public-path coverage for `ANTHROPIC_API_KEY` unset via `ClaudeBackend::new` / `Engine::warmup_backends`.
- Safe timeout handling that kills timed-out CLI probes.
- Backward-compatible serde defaults for newly added `HealthStatus` fields.
- Design-specified tracing behavior.
- Verification is incomplete: `cargo test claude` could not run in this read-only sandbox because Cargo could not open `target/debug/.cargo-lock` (`Operation not permitted`).

## Recommendations

- Make API-mode construction non-fatal for missing env during health probing, or teach warmup to cache an unavailable Claude API status with `mode: Some("api")` when construction fails for a missing key.
- Replace the probe command helper with a small async utility that sets `kill_on_drop(true)`, applies the 2s timeout, and kills/waits on timeout.
- Add `#[serde(default)]` to `HealthStatus.mode` and `HealthStatus.diagnostic`, plus a test deserializing the old JSON shape.
- Use `tracing::{warn, debug}` consistently and add timeout tests with a mock `claude` script that sleeps.
- Parse semver across the full version output, not just the first line.
