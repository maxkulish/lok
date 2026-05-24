# Design Review: CLO-394

**Reviewer**: Gemini architecture persona (manual fallback)
**Reviewed**: 2026-05-24
**Pipeline**: lok design-review fallback (workflow failed before writing review files)

---

## 1. Completeness Check

All required sections are present: Problem, Goals / Non-goals, Architecture, Public API surface, Assumptions, Test plan, Migration / rollout, and Open questions. The design ties back to discovery and includes concrete file paths and test names.

## 2. Architecture Assessment

**Strengths**

- Correctly replaces shell-string construction with direct argv-based `tokio::process::Command`, removing an entire shell escaping failure class.
- Keeps `Backend`, `QueryOutput`, `TokenUsage`, and workflow TOML schemas stable.
- Explicitly maps sandbox modes to opencode agents and keeps `apply_edits` behavior visible.

**Concerns**

- The original draft implied pinned custom Gemini CLI configs would continue to work while replacing the parser entirely with opencode parsing. That would surface old Gemini JSON envelopes as raw JSON. This should be addressed by preserving the legacy Gemini-envelope parser as a fallback or by documenting pinned CLI configs as unsupported.
- The design needs to normalize model identifiers carefully to avoid `google/google/...` double prefixes while still prefixing bare `gemini-*` models.
- The opencode output schema remains unverified; fixture capture must be the first implementation sub-task.

## 3. ADR Compliance

No ADR conflict observed. The design follows existing backend patterns (direct command execution similar to Codex/Claude) and keeps additive internal helpers rather than broad trait changes.

## 4. Security Review

The move away from `sh -c` is a security improvement because prompt/config/model text is no longer interpreted by a shell. The test plan should keep the hostile prompt argv test as a required guard.

## 5. Implementation Concerns

- Preserve the existing non-zero exit precedence: parser fallback must never hide failed subprocess exits.
- Update both `GeminiBackend::new` and `Config::default` in the same commit.
- Ensure `lok doctor` hints do not imply `GOOGLE_API_KEY` is mandatory after the migration.

## 6. Blind Spots

- opencode may emit NDJSON rather than a single JSON object; parser shape must be settled from real fixtures.
- stdin behavior is unknown; smoke test with `Stdio::null()` should happen before merge.
- Auth fallback via env vars needs confirmation.

## 7. Verdict

APPROVE_WITH_SUGGESTIONS

## 8. Actionable Feedback

1. Preserve old Gemini envelope parsing as a compatibility fallback for pinned `command = "npx"` configs, or explicitly mark pinned legacy configs unsupported. **Applied**: design now keeps a legacy parser fallback.
2. Add explicit model normalization tests: provider-prefixed values pass through unchanged, bare Gemini names get `google/` prefixed. **Applied**.
3. Make real opencode fixture capture ST1 in the plan phase before parser code. **Applied in assumptions/test plan; plan phase should enumerate it first**.
