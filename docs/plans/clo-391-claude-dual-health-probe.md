# CLO-391 Implementation Plan: Claude dual-mode health probe (Api vs Cli)

**Linear Task**: https://linear.app/cloud-ai/issue/CLO-391/fr-13a-claude-dual-mode-health-probe-api-vs-cli
**Design Document**: docs/design-docs/clo-391-claude-dual-health-probe.md
**Architecture Reference**: none
**Created**: 2026-05-24
**Overall Progress**: 0% (0/15 tasks completed)

---

## Architecture Context

The Claude backend already splits into `ClaudeMode::Api` and `ClaudeMode::Cli` variants. The existing `health_check()` returns a bare `HealthStatus::new_available()` — no version info, no mode discriminator, no flag-support detection. The Ollama backend (CLO-389) already probes `/api/version` and `/api/tags`. This task adds mode-aware probes on `ClaudeBackend` and a `mode` + `diagnostic` field to `HealthStatus`.

Key patterns from existing codebase:
- All backends override `Backend::health_check()` (Codex, Gemini, Ollama, Bedrock)
- `HealthCache` stores results by `backend.name()` — `"claude"` stays the key, `mode` field disambiguates for the doctor renderer
- No `Backend` trait changes needed

---

## Tasks

### Phase 1: HealthStatus fields

- [ ] Task 1: Add `mode` + `diagnostic` fields to `HealthStatus`
  - [ ] Add `pub mode: Option<String>` and `pub diagnostic: Option<String>` to `src/backend/context.rs`
  - [ ] Update `new_available()` / `new_unavailable()` to set both to `None`
  - [ ] Add serde round-trip test for new fields

### Phase 2: Claude probe implementation

- [ ] Task 2: Implement `probe_api()` private method
  - [ ] Match on `ClaudeMode::Api` — extract `api_key` and `model`
  - [ ] Check `api_key.expose_secret().trim().is_empty()` — return `available: false, diagnostic`
  - [ ] Check `model.trim().is_empty()` — return `available: false, diagnostic`
  - [ ] Return `available: true, mode: Some("api"), diagnostic: None`

- [ ] Task 3: Implement `probe_cli()` private method
  - [ ] Match on `ClaudeMode::Cli` — extract `command`
  - [ ] Check `which::which(command)` — return `available: false, mode: Some("cli")` if not found
  - [ ] Run `claude --version` with 2s timeout via `tokio::process::Command`
  - [ ] Parse semver from stdout using regex (`X.Y.Z`)
  - [ ] Run `claude --help` with 2s timeout
  - [ ] Check for `--output-format` + `json` in help output
  - [ ] Populate `unusable_flags` if json flag missing
  - [ ] Return `HealthStatus` with `mode: Some("cli")`, version, diagnostic

- [ ] Task 4: Add helper `parse_semver_line()`
  - [ ] Use regex to find `\d+\.\d+\.\d+` anywhere in input line
  - [ ] Return `Option<String>` — graceful fallback to None

- [ ] Task 5: Add `get_help_output()` with per-version memoization
  - [ ] `OnceLock<Mutex<HashMap<String, String>>>` for thread-safe cache
  - [ ] Check cache on version match before shelling out
  - [ ] Cache `--help` output keyed by version string
  - [ ] Do NOT cache if version parsing failed (re-run on next warmup)
  - [ ] Add `tracing::warn!` on timeout, `tracing::debug!` on version parse failure

- [ ] Task 6: Wire `health_check()` override to dispatch
  - [ ] Match on `&self.mode`
  - [ ] `ClaudeMode::Api` -> `probe_api().await`
  - [ ] `ClaudeMode::Cli` -> `probe_cli().await`

### Phase 3: Unit tests

- [ ] Task 7: Api with valid env — `available: true, mode: Some("api"), version: None`
  - [ ] Set `ANTHROPIC_API_KEY` and non-empty model in test config
  - [ ] Assert `HealthStatus.available == true` and `mode == Some("api")`

- [ ] Task 8: Api without key — `available: false, mode: Some("api")`
  - [ ] Set empty `ANTHROPIC_API_KEY`
  - [ ] Assert `available: false` and `diagnostic: Some(_)`

- [ ] Task 9: Api with empty model — `available: false, mode: Some("api")`
  - [ ] Set valid key but empty model
  - [ ] Assert `available: false`

- [ ] Task 10: Cli present + json supported — version parsed, no unusable flags
  - [ ] Mock `claude` binary on a temp PATH
  - [ ] `claude --version` returns `"Claude CLI 0.42.0"`
  - [ ] `claude --help` contains `--output-format` and `json`
  - [ ] Assert version = Some("0.42.0"), no unusable_flags

- [ ] Task 11: Cli present + json missing — unusable_flags populated
  - [ ] Mock `claude` binary without `--output-format json` in help
  - [ ] Assert `unusable_flags` contains `"--output-format json"`

- [ ] Task 12: Cli absent — `available: false, mode: Some("cli")`
  - [ ] Ensure `claude` not on PATH
  - [ ] Assert `available: false`

### Phase 4: Testing & Validation

- [ ] Task 13: Run unit tests
  - [ ] `cargo test claude` — all 6 new tests + existing pass

- [ ] Task 14: Run clippy
  - [ ] `cargo clippy -- -D warnings` — no new warnings

- [ ] Task 15: Manual verification
  - [ ] Verify `HealthStatus` serialization round-trips with new fields
  - [ ] Verify `HealthStatus::new_available()` sets `mode: None`

### Phase 5: Finalization

- [ ] Task 16: Create PR
  - [ ] Verify all commits follow format: `feat(CLO-391): description`
  - [ ] Push branch: `git push origin feat/clo-391-claude-dual-health-probe`
  - [ ] Create PR: `gh pr create --title "feat(CLO-391): Claude dual-mode health probe (Api vs Cli)" --body "Implements FR-13a per PRD v5 §4"`
  - [ ] Link PR to Linear task CLO-391
  - [ ] Request review

---

## Module Structure

- `src/backend/context.rs` — Modified: add `mode` + `diagnostic` fields to `HealthStatus`
- `src/backend/claude.rs` — Modified: new probes, helpers, unit tests
- `docs/plans/clo-391-claude-dual-health-probe.md` — This plan

---

## Status Indicators

- `[ ]` = To do
- `[~]` = In progress
- `[x]` = Done
- `[!]` = Blocked (needs manual intervention)

**To update progress**: Edit this file and change checkboxes. The overall percentage will be recalculated based on completed tasks.

---

## Notes

- Cache keying: `HealthCache` stores by `backend.name()` which stays `"claude"`. No change needed since each `ClaudeBackend` is in exactly one mode. The `mode` field on the stored `HealthStatus` disambiguates for the doctor renderer.
- The `std::sync::Mutex` in `get_help_output` is safe because the critical section is a short HashMap lookup/insert. Never hold across an `.await`.
- API probe must NOT make network calls (offline only — key + model validation).
- CLI probe 2s timeout applies to both `--version` and `--help` commands.
