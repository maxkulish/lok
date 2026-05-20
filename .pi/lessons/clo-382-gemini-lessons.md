# Lessons: CLO-382 FR-26 Gemini Backend Token Extraction

Durable rules from parsing a CLI backend's JSON envelope whose actual schema diverged from design assumptions.

---

## L1 - Capture a real CLI fixture before finalizing the parser schema, even for "obvious" field names

**Source incident:** CLO-382 design phase assumed `stats.promptTokenCount` / `stats.candidatesTokenCount` / `stats.cachedContentTokenCount` based on PRD §4 FR-26 and Codex's flat event structure. During implementation, a real capture from `gemini-cli` 0.42.0 revealed the actual shape is `stats.models.<model>.tokens {prompt, candidates, cached, thoughts}` — deeply nested, cross-model, with different key names entirely. The parser had to be rewritten mid-implementation to sum tokens across models.

**Rule:** For any backend that emits structured JSON, capture a real envelope from the current CLI version *before* the design doc locks field names. Do not rely on PRD text, API documentation screenshots, or assumptions from sibling backends (Codex JSONL is not authoritative for Gemini JSON). The capture should exercise the smallest possible real invocation (e.g. `echo '' | npx @google/gemini-cli --output-format json 'Reply exactly: ok.'`).

**How to apply:** Add "capture real fixture" as ST1 in the implementation plan, before any parser code is written. Use `--skip-trust` or equivalent non-interactive flags if the CLI requires trust configuration. Scrub the captured fixture of `session_id`, paths, and credentials, then commit it to `tests/fixtures/<backend>/` before writing the parser. If the capture environment is unavailable (e.g. no API key), mark the task blocked rather than coding against assumed schema.

---

## L2 - Design-review multi-model workflows need `depends_on` for variable-resolution, not just step ordering

**Source incident:** CLO-382 design review. The `.lok/workflows/design-review.toml` template references `{{ steps.health_check.output }}` in the `gemini_review` and `ollama_review` steps. The workflow engine failed with "unknown variable" despite the `health_check` step preceding them in file order. Adding `depends_on = ["health_check"]` to both downstream steps resolved the variable resolution, but the template was shipped without it.

**Rule:** In lok workflow TOML, `steps.X.output` variables are only resolvable when `depends_on` explicitly declares the dependency. File-order proximity is not sufficient. This applies to any workflow with a pre-flight health check whose output gates subsequent steps.

**How to apply:** When authoring or editing `.lok/workflows/*.toml`, grep for `steps\.` variable references and ensure every referenced step name appears in a `depends_on` array of the step that uses it. Document this in the workflow README if one exists. If a workflow is consumed by the orchestrator, add a synthetic pre-flight validation step that dry-runs `lok run <workflow> --dry-run` (if supported) or runs a minimal invocation to catch variable-resolution errors before the real design review.

---

## L3 - Default backend args that change CLI output format must be reflected in both `BackendConfig::new` and `Config::default`

**Source incident:** CLO-382 implementation. `--output-format json` was appended to default args in `GeminiBackend::new` (line 24 in `src/backend/gemini.rs`), but `Config::default()` in `src/config.rs` also hardcodes default backend args. Changing only the backend constructor caused a failing unit test (`gemini_default_config_includes_output_format_json`) because the config-level defaults and the backend-level defaults diverged.

**Rule:** When a backend's default argv changes (especially output-format flags that affect parsing), update BOTH the backend constructor AND the `Config::default()` entry for that backend. The two sources of truth must remain in sync.

**How to apply:** Treat `Config::default()` as part of the backend's public API surface. Any PR that adds, reorders, or removes default args in a backend constructor must include a matching diff in `src/config.rs`. Add a unit test that constructs the backend from `Config::default()` and asserts the expected argv snapshot, so the regression is caught at PR time rather than during pre-PR validation.
