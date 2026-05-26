# lok Setup Guide

A practical guide for configuring lok in your projects. Everything here comes from patterns proven across real workflows.

> ## ⚠️ Breaking change: Gemini CLI → opencode
>
> Google deprecated `@google/gemini-cli` (see [official migration notice](https://antigravity.google/docs/gcli-migration)).
> The lok `gemini` backend now runs **opencode** under the hood. Existing workflows that say `backend = "gemini"` keep working **without TOML changes**, but every machine that runs lok must install `opencode` and run `opencode auth login`.
>
> Action required: jump to the [opencode Migration Guide](#opencode-migration-guide) and follow the three steps (install, auth, drop any custom `command/args` overrides).

## What's New in Phase 2

Phase 2 ("Predictable CLI Execution") shipped a set of features that affect every project using lok. Each item links to the section with full details.

| Capability | Why it matters | Section |
|------------|----------------|---------|
| **opencode replaces Gemini CLI** | Google deprecated the npm CLI; lok now defaults to opencode. Workflow TOML stays the same. | [opencode Migration Guide](#opencode-migration-guide) |
| **Token usage on every step** | `StepResult.usage` populated by all backends; visible in `--output json`. | [Token Usage Observability](#token-usage-observability) |
| **Per-step sandbox routing** | Codex `-s` and opencode `--agent` set per step from `sandbox = "..."`. Auto-defaults to `workspace-write` when `apply_edits = true`. | [Per-Step Sandbox](#per-step-sandbox) |
| **Per-step timeout layering** | `timeout = "30s"` resolves step > backend > global; humantime strings accepted everywhere. | [Tips and Gotchas](#tips-and-gotchas) |
| **`lok doctor` health command** | Table + JSON view of backend availability, version, auth mode, diagnostics. | [lok doctor](#lok-doctor) |
| **Health-check + warmup** | All enabled backends are probed once at startup; subsequent `is_available` reads from cache (no extra subprocess spawns). | [lok doctor](#lok-doctor) |
| **Codex JSONL event parser** | Codex output is now event-driven (`turn.completed` / `--output-last-message`); ANSI escapes and mid-turn chatter no longer leak into results. | [Tips and Gotchas](#tips-and-gotchas) |

## Directory Structure

lok discovers configuration from a `.lok/` directory at your project root:

```
your-project/
  .lok/
    lok.toml              # Backend config + defaults
    workflows/            # Workflow definitions (TOML)
      design-review.toml
      pre-pr-validation.toml
    prompts/              # Reusable prompt templates (markdown)
      design-review-prompt.md
      validation-prompt.md
```

Workflow discovery: `lok workflow list` scans `.lok/workflows/` for all `.toml` files.

Prompt separation: keep prompts in `.lok/prompts/` as standalone markdown files. Workflows reference them via `sed` or file inclusion in shell steps. This avoids long inline strings in TOML and makes prompts independently testable.

---

## lok.toml - Project Configuration

The config file defines backends, defaults, caching, and (optionally) role routing.

### Minimal config

```toml
[defaults]
parallel = true
timeout = 300

[backends.claude]
enabled = true
command = "claude"

[backends.gemini]
enabled = true
# command and args use opencode by default; no manual config needed
# To pin a specific model:
#   model = "google/gemini-2.5-flash"
timeout = 300
```

> **Migrating from Gemini CLI?** See the [migration guide below](#opencode-migration-guide) for the
> install + auth + sandbox delta.

### Full config reference

```toml
[defaults]
parallel = true                    # Run independent steps in parallel
timeout = 300                      # Global timeout per step (seconds)
max_retries = 0                    # Default retry count
retry_delay_ms = 1000              # Base delay between retries (doubles each attempt)
command_wrapper = "nix-shell --run '{cmd}'"  # Optional wrapper for shell commands
team = "frontend"                  # Default team for role resolution

[cache]
enabled = false                    # Response caching

[conductor]
max_rounds = 5                     # Max rounds for multi-round orchestration
max_tokens = 4096                  # Max tokens for conductor context

# ---- Backends ----

[backends.claude]
enabled = true
command = "claude"
args = []
model = "sonnet"                   # Optional default model
timeout = 300                      # Per-backend timeout override
max_retries = 2                    # Per-backend retry override
retry_delay_ms = 2000              # Per-backend delay override

[backends.gemini]
enabled = true
# command and args use opencode by default; only set if you need to override
# args = ["run", "--format", "json"]
timeout = 300
stderr = "separate"                # Keep stderr separate from stdout

[backends.ollama]
enabled = true
command = "http://localhost:11434"  # HTTP endpoint for API backends
model = "glm-5:cloud"
timeout = 300

[backends.codex]
enabled = true
command = "codex"
args = ["exec"]

# Bedrock requires --features bedrock at build time
[backends.bedrock]
enabled = true
command = "bedrock"
model = "us.anthropic.claude-sonnet-4-20250514-v1:0"
api_key_env = "AWS_ACCESS_KEY_ID"

# ---- Role Routing (optional) ----

[roles.code_review]
backends = ["claude", "gemini"]
strategy = { Fallback = {} }

[roles.security_audit]
backends = ["gemini", "claude"]
strategy = { Parallel = { min_success = 1, timeout_secs = 30 } }

# Team-specific overrides
[teams.frontend.roles.code_review]
backends = ["claude"]
strategy = { First = {} }
```

### Backend types

Backends fall into two categories based on how lok communicates with them:

| Type | Detection | Stdout/Stderr | Exit Code | Examples |
|------|-----------|---------------|-----------|----------|
| CLI | `command` is an executable name | Captured separately | Available | claude, gemini (opencode), codex |
| API | `command` starts with `http://` | N/A (text only) | N/A | ollama, bedrock |

CLI backends capture stderr and exit codes through `QueryOutput`. API backends return text only - `stderr` and `exit_code` are `None`.

---

## Workflow Anatomy

A workflow is a TOML file with a name, description, and ordered list of steps.

### Simplest possible workflow

```toml
name = "summarize"
description = "Summarize a document"

[[steps]]
name = "summary"
backend = "claude"
prompt = "Summarize this document: {{ arg.1 }}"
```

Run: `lok run summarize docs/design-docs/my-doc.md`

### Full workflow reference

```toml
name = "my-workflow"
description = "What this workflow does"
extends = "base-workflow"          # Inherit steps from another workflow (optional)
continue_on_error = false          # Global error policy
timeout = 300000                   # Global timeout (milliseconds)

[[steps]]
name = "step_name"                 # Unique identifier, used in {{ steps.step_name.output }}

# --- Execution mode (pick one) ---
backend = "claude"                 # LLM backend query
# OR
backends = ["claude", "gemini"]    # Multi-backend with consensus
# OR
shell = "echo hello"               # Shell command

# --- LLM-specific ---
model = "haiku"                    # Model override for this step
prompt = "Your prompt here"        # Required for backend steps
consensus = "synthesis"            # For multi-backend: first, synthesis, vote, weighted_vote

# --- Dependencies ---
depends_on = ["step_a", "step_b"]  # Wait for these steps to complete
min_deps_success = 1               # How many deps must succeed (default: all)
when = "steps.plan.success"        # Conditional execution (MiniJinja expression)

# --- Error handling ---
continue_on_error = true           # Don't fail the workflow if this step fails
retries = 3                        # Retry on failure
retry_delay = 1000                 # Base delay (ms), doubles each retry
timeout = 300000                   # Step timeout: integer (ms) OR humantime string ("30s", "5m", "1h")

# --- Sandbox (CLI backends only: Codex, Gemini) ---
sandbox = "read-only"              # "read-only" | "workspace-write" | "danger-full-access"
                                   # Maps to Codex `-s` / Gemini `--agent` (opencode)
                                   # If omitted with apply_edits=true, defaults to "workspace-write"

# --- Edit workflow ---
apply_edits = true                 # Parse JSON edits from LLM output and apply to files
verify = "cargo test"              # Shell command to verify edits
fix_retries = 3                    # Re-query LLM with error feedback on verify failure

# --- Iteration ---
for_each = "steps.plan.output"     # Iterate over JSON array from a prior step
# Available in loop: {{ item }}, {{ item.field }}, {{ index }}

# --- Output ---
output_format = "json"             # "text" (default), "json", "lines"

# --- Validation ---
[steps.validate]
check = "min_length(200)"          # Heuristic check (fast, free)
backend = "claude"                 # LLM validator backend (only runs if heuristic passes)
model = "haiku"                    # Use cheap model for validation
prompt = "..."                     # Validation prompt with {{ output }} and {{ stderr }}
replace_output = true              # Replace step output with cleaned version
max_input_length = 100000          # Truncate output before sending to validator
timeout_ms = 60000                 # Validation-specific timeout
on_error = "pass"                  # What to do if validator itself fails: fail/pass/skip
```

---

## Template Variables

lok uses MiniJinja for interpolation. All `{{ }}` expressions in prompts, shell commands, and when-conditions have access to these variables.

### Steps

```
{{ steps.STEP_NAME.output }}     # Full output text
{{ steps.STEP_NAME.success }}    # Boolean: true/false
{{ steps.STEP_NAME.field_name }} # Parsed JSON field (requires output_format = "json")
```

### Arguments

Positional arguments from the CLI, 1-indexed:

```
{{ arg.1 }}    # First argument
{{ arg.2 }}    # Second argument
```

Usage: `lok run my-workflow "first arg" "second arg"`

### Environment

```
{{ env.API_KEY }}           # Environment variable
{{ env.HOME }}
```

### Loop variables

Inside a `for_each` step:

```
{{ item }}         # Current iteration value
{{ item.name }}    # Field on current item (for object arrays)
{{ index }}        # 0-indexed iteration counter
```

### Workflow metadata

```
{{ workflow.backends }}   # Formatted backend list, e.g. "Claude + Gemini"
```

### Validation prompt variables

Inside `validate.prompt` only (separate namespace from step interpolation):

```
{{ output }}    # The step's raw output
{{ stderr }}    # The step's stderr (CLI backends only)
```

---

## Workflow Patterns

### Pattern 1: Parallel reviews with synthesis

The most common lok pattern. Two or more models review independently, then a synthesis step cross-references their findings.

```toml
name = "design-review"

# Step 1: Independent review (runs in parallel with step 2)
[[steps]]
name = "gemini_review"
timeout = 300000
continue_on_error = true
shell = """
PROMPT=$(sed 's|__DOC__|{{ arg.1 }}|g' .lok/prompts/review-prompt.md)
opencode run --model google/gemini-3.1-pro-preview --agent plan -- "$PROMPT"
"""

[steps.validate]
check = "min_length(200)"
backend = "claude"
model = "haiku"
replace_output = true
max_input_length = 100000
timeout_ms = 60000
on_error = "pass"
prompt = """
{{ output }}

---
You are a review output validator. Produce ONE JSON object.

If valid review: {"status": "pass", "output": "<cleaned content>"}
If noise/garbage: {"status": "fail", "reason": "<why>"}
"""

# Step 2: Second reviewer (parallel with step 1)
[[steps]]
name = "codex_review"
timeout = 300000
continue_on_error = true
shell = """
PROMPT=$(sed 's|__DOC__|{{ arg.1 }}|g' .lok/prompts/review-prompt.md)
timeout 300 codex exec -m gpt-5.4 "$PROMPT" --sandbox read-only
"""

[steps.validate]
check = "min_length(200)"
backend = "claude"
model = "haiku"
replace_output = true
max_input_length = 100000
timeout_ms = 60000
on_error = "pass"
prompt = """
{{ output }}

---
You are a review output validator. Produce ONE JSON object.

If valid review: {"status": "pass", "output": "<cleaned content>"}
If noise/garbage: {"status": "fail", "reason": "<why>"}
"""

# Step 3: Synthesis (waits for both, tolerates failures)
[[steps]]
name = "synthesis"
backend = "claude"
depends_on = ["gemini_review", "codex_review"]
min_deps_success = 0
prompt = """
GEMINI (success={{ steps.gemini_review.success }}):
{{ steps.gemini_review.output }}

CODEX (success={{ steps.codex_review.success }}):
{{ steps.codex_review.output }}

Cross-reference into: Agreement, Disagreement, Novel Insights, Verdict.
If both failed: return NO_REVIEWS_AVAILABLE
"""
```

Key decisions in this pattern:

- `continue_on_error = true` on review steps - one reviewer failing shouldn't kill the workflow
- `min_deps_success = 0` on synthesis - it handles partial data gracefully
- `replace_output = true` on validation - downstream steps see cleaned output, not MCP noise
- `on_error = "pass"` - if the validator itself fails, pass through the raw output rather than failing the step
- Shell steps for reviewers - gives full control over CLI invocation (timeouts, args, env vars)
- Backend step for synthesis - simpler, cheaper, just needs prompt + response

### Pattern 2: Discovery probe with external APIs

Parallel research queries to an external API, synthesized by an LLM.

```toml
name = "discovery-probe"
description = "Multi-angle research probe"

# Parallel research angles
[[steps]]
name = "query_prior_art"
timeout = 120000
continue_on_error = true
shell = """
TOPIC='{{ arg.1 }}'
curl -sS --max-time 90 https://api.perplexity.ai/chat/completions \
  -H "Authorization: Bearer $PERPLEXITY_API_KEY" \
  -H "Content-Type: application/json" \
  -d "$(jq -nc --arg p "Research prior art for: $TOPIC" \
       '{model: "sonar", messages: [{role: "user", content: $p}]}')" \
  | jq -r '.choices[0].message.content // "QUERY_FAILED"'
"""

[[steps]]
name = "query_pitfalls"
timeout = 120000
continue_on_error = true
shell = """
TOPIC='{{ arg.1 }}'
curl -sS --max-time 90 https://api.perplexity.ai/chat/completions \
  -H "Authorization: Bearer $PERPLEXITY_API_KEY" \
  -H "Content-Type: application/json" \
  -d "$(jq -nc --arg p "Common pitfalls for: $TOPIC" \
       '{model: "sonar", messages: [{role: "user", content: $p}]}')" \
  | jq -r '.choices[0].message.content // "QUERY_FAILED"'
"""

# Synthesis
[[steps]]
name = "synthesis"
backend = "claude"
model = "haiku"
depends_on = ["query_prior_art", "query_pitfalls"]
min_deps_success = 0
prompt = """
Synthesize research on: {{ arg.1 }}

PRIOR ART (success={{ steps.query_prior_art.success }}):
{{ steps.query_prior_art.output }}

PITFALLS (success={{ steps.query_pitfalls.success }}):
{{ steps.query_pitfalls.output }}

Produce: Approaches, Libraries, Risks, Open Questions.
If all failed: return NO_PROBE_DATA_AVAILABLE
"""

# Write results to disk
[[steps]]
name = "write_cache"
depends_on = ["synthesis"]
min_deps_success = 0
shell = """
mkdir -p /tmp
cat > /tmp/probe-{{ arg.2 }}.md << 'EOF'
# Discovery Probe: {{ arg.2 }}
{{ steps.synthesis.output }}
EOF
echo "Written to /tmp/probe-{{ arg.2 }}.md"
"""
```

Key decisions:

- Shell steps for API calls - lok doesn't need to know about Perplexity's API; `curl` + `jq` handles it
- `model = "haiku"` on synthesis - cheap model is sufficient for combining existing content
- Write step at the end - persists results for other tools to consume
- `jq -r '.choices[0].message.content // "QUERY_FAILED"'` - graceful fallback on malformed responses

### Pattern 3: Edit-verify-fix loop

LLM generates code edits, a shell command verifies them, and on failure the LLM retries with error context.

```toml
name = "implement-fix"

[[steps]]
name = "generate_fix"
backend = "codex"
prompt = """
Fix this issue: {{ arg.1 }}
Return JSON edits in this format:
[{"file": "path", "search": "old code", "replace": "new code"}]
"""
apply_edits = true
verify = "cargo test --quiet"
fix_retries = 3
```

How this works:

1. LLM produces output with JSON edit blocks
2. `apply_edits = true` parses the edits and applies them to files
3. `verify` runs a shell command - if it exits non-zero, edits are reverted
4. `fix_retries` re-queries the LLM with the error output, up to N times
5. On final failure, the step gets `StepFailureKind::VerifyFailed`

When the backend is Codex or Gemini (via opencode) and `apply_edits = true`, lok auto-defaults the sandbox to `workspace-write` so the subprocess can actually write to the workspace. To opt out, set `sandbox = "read-only"` explicitly - lok emits a warning but honors the choice.

### Pattern 4: Iterating over dynamic lists

Process each item from a previous step's output independently.

```toml
name = "review-files"

[[steps]]
name = "find_files"
shell = 'git diff --name-only main | jq -R -s "split(\"\n\") | map(select(length > 0))"'
output_format = "json"

[[steps]]
name = "review_each"
depends_on = ["find_files"]
for_each = "steps.find_files.output"
backend = "claude"
model = "haiku"
prompt = "Review this file for issues: {{ item }}"
continue_on_error = true
```

The `for_each` field accepts:

- A step reference: `"steps.step_name.output"` (expects JSON array)
- A step field: `"steps.step_name.field_name"` (parsed JSON field)
- An inline array: `'["item1", "item2"]'`

---

## Per-Step Sandbox

CLI backends that wrap a coding agent (Codex, Gemini) accept a sandbox mode that bounds what the subprocess is allowed to touch. lok routes a per-step `sandbox` field to the right CLI flag - `-s` for Codex, `--agent` for Gemini (opencode).

```toml
[[steps]]
name = "explore"
backend = "codex"
prompt = "Find the request-handling code in this repo"
sandbox = "read-only"              # Default for analysis steps

[[steps]]
name = "apply_patch"
backend = "codex"
prompt = "Apply the fix from {{ steps.explore.output }}"
apply_edits = true                 # Implies workspace-write
verify = "cargo test --quiet"
```

| Mode | Codex (`-s`) | Gemini (`--agent`, opencode) | When to use |
|------|--------------|------------------------------|-------------|
| `read-only` | `read-only` | `plan` | Discovery, analysis, review (default) |
| `workspace-write` | `workspace-write` | `build` | Edit-and-verify steps |
| `danger-full-access` | `danger-full-access` | `build --dangerously-skip-permissions` | Sandboxed CI only; avoid on dev machines |

### Sandbox defaulting rules

1. Explicit `sandbox = "..."` on the step always wins.
2. If `sandbox` is omitted and `apply_edits = true`, lok defaults to `workspace-write` (the subprocess needs to write).
3. If `sandbox` is omitted and `apply_edits = false`/unset, lok defaults to `read-only`.
4. If you set `sandbox = "read-only"` AND `apply_edits = true`, lok emits a warning - the subprocess will fail to write edits.

For backends that do not understand a sandbox flag (Claude API, Bedrock, Ollama, plain shell steps), the `sandbox` field is ignored without error.

---

## opencode Migration Guide

> **Why this exists:** Google [deprecated `@google/gemini-cli`](https://antigravity.google/docs/gcli-migration).
> The old `npx @google/gemini-cli --output-format json` invocation is gone. lok's `gemini` backend now wraps **[opencode](https://opencode.ai)**
> (upstream [anomalyco/opencode](https://github.com/anomalyco/opencode)) instead.
>
> **What changes for you:** Workflow TOML stays the same — `backend = "gemini"` still works. But every machine that runs lok must install opencode and authenticate once.

### Step 1: Install opencode

Replace the old gemini-cli install with opencode:

```bash
# macOS (recommended — anomalyco tap stays current)
brew install anomalyco/tap/opencode

# Linux / any platform
curl -fsSL https://opencode.ai/install | bash
```

> The homebrew-core `opencode` formula lags upstream. Use the `anomalyco/tap` tap.
>
> If `opencode` is not found after install, restart your terminal or run
> `source ~/.zshrc` / `source ~/.bashrc` to refresh your `$PATH`.

Verify the install:

```bash
opencode --version   # should print 1.15.x or newer
```

### Step 2: Authenticate

Remove any `GEMINI_API_KEY` or `GOOGLE_API_KEY` environment variables you set for the old
CLI (they are no longer required for the OAuth path). Authenticate via Google OAuth:

```bash
opencode auth login   # Opens browser → select Google → OAuth flow
```

> **`opencode auth login` vs `opencode login`:** the first is **provider credentials** — what lok needs. The second is the opencode-console login (console.opencode.ai). They are unrelated; do not confuse them.

For headless environments (SSH, CI, Docker) where a browser cannot open, set
`GEMINI_API_KEY` or `GOOGLE_API_KEY` as a fallback — opencode honors these
environment variables as an API-key path.

Verify auth:

```bash
opencode auth list    # should list "google" with an account
```

`lok doctor` will report `mode: oauth` (or `api-key`) when this is set up correctly.

### Step 3: Remove old overrides

If you previously had something like this in `.lok/lok.toml`:

```toml
[backends.gemini]
command = "npx"
args = ["@google/gemini-cli", "--output-format", "json"]
```

…delete those lines. lok now defaults to:

```bash
opencode run --model google/<model> --format json --agent <plan|build> -- "<prompt>"
```

A minimal modern config:

```toml
[backends.gemini]
enabled = true
# command/args use opencode by default
# Optional: pin a specific model
# model = "google/gemini-2.5-flash"
timeout = 300
```

### Sandbox delta

The old gemini-cli `--approval-mode` flags are replaced by opencode `--agent` flags. lok's per-step `sandbox` field handles this for you — you do not invoke these flags yourself.

| Old flag (gemini-cli) | New flag (opencode) | lok's `sandbox = "..."` |
|-----------------------|---------------------|--------------------------|
| `--approval-mode default` | `--agent plan` | `read-only` |
| `--approval-mode auto_edit` | `--agent build` | `workspace-write` |
| `--approval-mode yolo` | `--agent build --dangerously-skip-permissions` | `danger-full-access` |

### Shell steps invoking opencode directly

If you bypass lok's backend and invoke opencode from a `shell = "..."` step, two things to know:

1. **Message is positional, not `--prompt`:**
   ```bash
   opencode run --model google/gemini-2.5-pro --format json --agent plan -- "$PROMPT"
   ```
2. **Use `--format json` to get NDJSON event output** — required if you want lok to extract token counts. Without `--format json`, the step output is plain text and `StepResult.usage` will be `None` for that step.

## Token Usage Observability

Every successful step's `StepResult` now carries a `usage` field populated by the backend. This makes cost tracking and prompt-tuning measurable instead of guessed.

```rust
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cached_tokens: Option<u32>,     // Prompt-cache hits where the model reports them
    pub reasoning_tokens: Option<u32>,  // Hidden reasoning tokens (o-series, Codex)
}
```

### Coverage matrix

| Backend | Source | Notes |
|---------|--------|-------|
| Claude API | response `usage` object | Full coverage including `cached_tokens` |
| Bedrock | InvokeModel response metadata | Provider-dependent |
| Ollama | `prompt_eval_count` / `eval_count` | No cached/reasoning fields |
| Codex | JSONL `turn.completed.usage` | `input_tokens` / `cached_input_tokens` / `output_tokens` / `reasoning_output_tokens` |
| Gemini | opencode JSONL `type: "text"` events + `step_finish` metadata | Extracted from opencode's `--format json` NDJSON output |
| Shell | n/a | Always `None` |

### Reading usage from a workflow

`StepResult.usage` is exposed as JSON when you run with `--output json`:

```bash
lok run my-workflow --output json | jq '.steps[] | {step: .name, total: .usage.total_tokens, cached: .usage.cached_tokens}'
```

For multi-backend steps, lok sums per-backend usage using saturating arithmetic. If any backend lacks a field (e.g. Ollama has no `cached_tokens`), the aggregated value is `None`.

---

## Validation - Two-Phase Pipeline

lok validates step output in two phases. Phase 1 is fast and free. Phase 2 uses an LLM.

### Phase 1: Heuristic checks

```toml
[steps.validate]
check = "not_empty"              # Output must not be empty/whitespace
check = "min_length(200)"        # At least 200 characters
check = "contains('## Verdict')" # Must contain literal text
```

If the heuristic fails, the LLM validation is skipped entirely (cost saving).

### Phase 2: LLM validation

```toml
[steps.validate]
check = "min_length(200)"
backend = "claude"
model = "haiku"
prompt = """
{{ output }}

---
Validate this output. Return JSON:
{"status": "pass", "output": "<cleaned>"} or {"status": "fail", "reason": "<why>"}
"""
replace_output = true
max_input_length = 100000
timeout_ms = 60000
on_error = "pass"
```

The LLM validator returns structured JSON. The parsing chain is fail-closed:

1. Valid JSON with `"status": "pass"` or `"fail"` - used as-is
2. `REVIEW_FAILED:` prefix - backward compatibility, treated as fail
3. Anything else - treated as `ValidatorError`, not a silent pass

#### Validation prompt tips

- Put `{{ output }}` at the top, instructions at the bottom. The model processes content before instructions.
- Ask for "ONE JSON object as your entire response. No prose. No markdown code fences."
- Define what VALID and NOISE look like explicitly. Models need clear criteria.
- Use `replace_output = true` to strip MCP initialization noise, timestamps, and other non-content.
- Set `max_input_length` to prevent context window overflow on very long outputs.
- Use `on_error = "pass"` when validation is best-effort (the raw output is still usable).
- Use `on_error = "fail"` (the default) when validation is a hard gate.

---

## Consensus Strategies

When a step has multiple backends, lok combines their outputs using a consensus strategy.

```toml
[[steps]]
name = "analyze"
backends = ["claude", "gemini", "ollama"]
consensus = "synthesis"
prompt = "Analyze this code for security issues"
```

| Strategy | Behavior | Best for |
|----------|----------|----------|
| `first` | Use first successful response | Speed-sensitive, any model is fine |
| `synthesis` | LLM synthesizes all responses into one | Reviews, analysis, nuanced tasks |
| `vote` | Majority vote on responses | Classification, yes/no decisions |
| `weighted_vote` | Weighted by backend tier | Same as vote, but trusted models count more |

Backend tier weights for weighted_vote: claude/bedrock = 2.0, codex/gemini = 1.5, ollama = 1.0.

---

## Role Routing

Role routing replaces hardcoded backend selection with config-driven rules. Define roles globally, override per team.

```toml
# Global roles
[roles.code_review]
backends = ["claude", "gemini"]
strategy = { Fallback = {} }

[roles.security_audit]
backends = ["gemini", "claude"]
strategy = { Parallel = { min_success = 1, timeout_secs = 30 } }

# Team override
[teams.frontend.roles.code_review]
backends = ["claude"]
strategy = { First = {} }
```

### Routing strategies

These are distinct from consensus strategies. Routing decides which backends to invoke; consensus decides how to combine their responses.

| Strategy | Behavior |
|----------|----------|
| `First` | Use first available backend |
| `Fallback` | Try in order; transient errors (429, 500) try next; terminal errors (401, 400) stop |
| `Parallel` | Fire all, return when `min_success` respond |

### Resolution order

1. CLI `--team` flag (if provided)
2. `[defaults.team]` config
3. `[teams.<team>.roles.<role>]` (team override)
4. `[roles.<role>]` (global fallback)
5. `Delegator` keyword matching (legacy fallback)

Use `--explain` flag to see which backends were selected and why.

---

## CLI Commands Quick Reference

### Workflow commands

```bash
lok run <workflow> [args...]        # Run a workflow
lok workflow list                   # List available workflows
lok workflow validate <file>        # Validate workflow TOML
```

### Direct query commands

```bash
lok ask "prompt"                    # Query configured backends
lok smart "prompt"                  # Smart backend selection (role-based)
lok smart "prompt" --team frontend  # With team override
lok smart "prompt" --explain        # Show routing decision
```

### Multi-model commands

```bash
lok team "prompt"                   # Team mode (delegation + optional debate)
lok spawn "prompt"                  # Parallel agents
lok debate "prompt"                 # Multi-round debate between backends
lok conduct "prompt"                # Multi-round orchestration with conductor
```

### Code analysis commands

```bash
lok hunt                            # Bug hunt on codebase
lok audit                           # Security audit
lok diff                            # Review git changes
lok pr <number>                     # Review GitHub PR
lok explain                         # Explain codebase architecture
```

### Utility commands

```bash
lok doctor                          # Check backend availability
lok backends                        # List configured backends
lok init                            # Generate lok.toml
lok context                         # Show detected codebase context
```

---

## lok doctor

`lok doctor` checks the health of all enabled backends and reports availability,
version, auth mode, and any diagnostic issues. It is the first thing to run on a new machine.

```bash
# Human-readable table (default)
lok doctor

# Machine-readable JSON
lok doctor --output json
```

### How health checks work

- **Warmup at startup:** every command that touches backends runs an async warmup pass that probes all enabled backends in parallel and stores results in a process-wide `HealthCache`.
- **`is_available` reads the cache** (sync), so step execution never spawns an extra subprocess to ask "are you up?"
- **Per-backend probes:**
  - `claude` — dual-mode (`Api` vs `Cli`); reports `mode` accordingly.
  - `codex` — `codex --version` + version-aware flag matrix (records `unusable_flags` for older builds).
  - `gemini` (opencode) — `opencode --version` + `opencode auth list` (or `auth.json` fallback); reports `mode: oauth | api-key | none`.
  - `ollama` — `GET /api/version` + `GET /api/tags`; populates `models[]` and validates any models referenced in workflows.
  - `bedrock` — feature-gated; presence of AWS creds.

If `lok run <workflow>` fails because a backend is unavailable, the diagnostic from `lok doctor` is the authoritative explanation.

### Table output

| Column    | Source                                  |
|-----------|-----------------------------------------|
| BACKEND   | Backend name from config                |
| MODE      | Auth mode: `oauth`, `api-key`, or `—`  |
| VERSION   | CLI version string from `--version`     |
| AVAILABLE | `✓ yes` or `✗ no`                      |
| NOTES     | `HealthStatus.diagnostic` (truncated)   |

Exit code is 0 when all backends are available, 1 otherwise.

### JSON output

Each entry is a flat object:

```json
[
  {
    "backend": "gemini",
    "available": true,
    "version": "1.15.10",
    "mode": "api-key",
    "auth_method": "api-key",
    "diagnostic": null,
    "capabilities": null,
    "unusable_flags": [],
    "models": [
      { "name": "gemini-2.5-flash", "modified_at": null, "size": null, "digest": null }
    ]
  }
]
```

---

## Error Handling

lok classifies step failures into structured types. Understanding them helps with debugging.

| Failure Kind | Meaning | Typical cause |
|-------------|---------|---------------|
| `Timeout` | Step or backend timed out | Model too slow, network issues |
| `BackendError` | Backend returned an error | API key issues, model unavailable, rate limits |
| `Skipped` | Step skipped | Unmet `when` condition or failed dependency |
| `EditFailed` | Edit parse or apply failed | LLM returned malformed edit JSON |
| `VerifyFailed` | Verify/fix loop exhausted retries | Code changes don't pass verification command |
| `EmptyOutput` | Backend returned nothing | Model refused, empty response |

Validation failures are separate from execution failures:

- Execution failure: the step itself could not run (populates `failure`)
- Validation failure: the step ran but output didn't pass checks (populates `validation`)

These two are mutually exclusive on a given step.

Successful steps additionally carry `usage` (see [Token Usage Observability](#token-usage-observability)) so cost and prompt size are visible in the same `StepResult` envelope as the output.

---

## Tips and Gotchas

**Timeouts: prefer humantime strings.** Every `timeout` field now accepts a string like `"30s"`, `"5m"`, or `"1h"` and parses uniformly. Raw integers still work for backward compatibility, but the units differ by level - workflow/step integers are milliseconds, config/backend integers are seconds. Use strings to avoid the trap. Resolution is layered step > backend > global, so a step-level `timeout = "30s"` overrides a 5-minute backend default.

**Shell steps give you more control than backend steps.** When you need specific CLI flags, environment variable manipulation, or piping, use a shell step. Reserve backend steps for straightforward prompt-response where lok manages the invocation.

**`continue_on_error = true` is essential for parallel review patterns.** Without it, one reviewer failing kills the entire workflow before synthesis can run.

**`min_deps_success = 0` makes synthesis resilient.** The synthesis step can handle partial data - let it decide what to do with failed inputs rather than blocking.

**Use `on_error = "pass"` for best-effort validation.** If the validator backend itself is down, raw output is still better than a failed step. Use `on_error = "fail"` only when validation is a hard quality gate.

**Prompt injection in validation.** The validation prompt interpolation uses single-pass replacement. If step output contains `{{ stderr }}`, it will not be expanded. This is safe by design.

**Shell step stderr separation.** Shell step stdout goes to `{{ steps.X.output }}`. Stderr goes to the separate `stderr` field, not into the output. If your shell step previously relied on seeing stderr in output, this changed.

**`output_format = "json"` enables field access.** Without it, `{{ steps.X.output }}` is raw text. With it, you can access parsed fields: `{{ steps.X.field_name }}`.

**Exponential backoff on retries.** The `retry_delay` doubles each attempt: 1s, 2s, 4s, 8s. For rate-limited APIs, this is usually the right behavior.

**Backend steps support model overrides.** Use `model = "haiku"` for cheap validation or synthesis. Use expensive models only for the core analysis steps.

**The `for_each` loop collects results as JSON.** Each iteration produces `{ index, item, output, success }`. The aggregate step output is the JSON array of all iterations.

**Codex output is now event-driven.** The Codex backend consumes the JSONL stream (`turn.completed`, `item.completed`, `turn.failed`) and uses `--output-last-message` for the authoritative final result. If a model occasionally emits ANSI escapes or mid-turn chatter, lok still ends up with the clean final message and the token counts from `turn.completed.usage`. No workflow changes needed - just keep `backend = "codex"`.

**Gemini token counts come from opencode's JSONL output.** The `backend = "gemini"` step extracts usage from opencode's NDJSON event stream (`step_finish` metadata). Shell wrapping `opencode` directly does not get automatic usage extraction — use `--format json` if you need to capture it manually.

**`health_check = true` in workflows.** This field appears in workflow TOML but is handled at the workflow execution layer - it checks backend availability before attempting the step.
