# Lok

Declarative multi-LLM orchestration. Define workflows in TOML, run them against
multiple backends, get synthesized results.

## What It Is

- **Multi-backend queries**: Ask the same question to Claude, Codex, Gemini, and
  Ollama in parallel, then synthesize or vote on the results
- **Declarative workflows**: TOML files that define multi-step LLM pipelines with
  dependencies, retries, and error handling
- **Backend abstraction**: Swap `backend = "claude"` for `backend = "ollama"`
  without changing your workflow logic

## What It's Not

- **Not an agent**: Lok doesn't make decisions or write code. It runs queries and
  returns results. Use it *with* an agent (Claude Code, Cursor, etc.) that acts
  on the output.
- **Not a wrapper for one LLM**: If you only use Claude, you don't need lok. The
  value is in multi-backend orchestration and consensus.

## Using lokomotiv as a library

The `lokomotiv` library is not on crates.io. The crate of that name on
[crates.io](https://crates.io/crates/lokomotiv) is published by the upstream
project, [ducks/lok](https://github.com/ducks/lok), is binary-only, and does not
contain this repository's changes.

Depend on this repository through git instead, with **no default features** to
avoid pulling in CLI-only dependencies. The example below also needs `tokio`,
because it uses `#[tokio::main]`:

```toml
[dependencies]
lokomotiv = { git = "https://github.com/maxkulish/lok", tag = "v20260914.0.0", default-features = false }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The `tag` selects the source revision to build. Each release is tagged `v` plus
the crate version (`vYYYYMMDD.N.P`, for example `v20260914.0.0`).

A crate that depends on `lokomotiv` through git cannot itself be published to
crates.io: crates.io needs a registry version for every dependency, and no
library version of `lokomotiv` exists there (see
[specifying dependencies from multiple locations](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations)).

Then build a backend and run a query:

```rust,no_run
use std::path::Path;
use lokomotiv::{
    Backend, BackendConfig, RetryDefaults, RetryPolicy, StepContext,
    create_backend,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BackendConfig {
        command: Some("http://localhost:11434".into()),
        model: Some("llama3.2".into()),
        max_retries: Some(2),
        retry_delay_ms: Some(500),
        ..Default::default()
    };

    let defaults = RetryDefaults {
        max_retries: 3,
        retry_delay_ms: 1000,
    };
    let policy = RetryPolicy::from_backend_config(&config, defaults);
    let backend = create_backend("ollama", &config, policy)?;

    let ctx = StepContext::from_prompt(
        "What is the capital of France?",
        Path::new("."),
        None,
    );
    let output = backend.query(ctx).await?;
    println!("Answer: {}", output.stdout);
    Ok(())
}
```

For the full API documentation, covering all available types, backends, and
configuration options, build it from a checkout of this repository with
`cargo doc --no-default-features --open`.

> **Note**: `lokomotiv` shares its version number with the `lok` binary. Both are
> built from the same repository under a single `Cargo.toml`. The version
> follows a date-based scheme (`YYYYMMDD.N.P`) rather than semver. Pin a release
> tag in your git dependency to avoid unexpected changes.

## Quick Start

`--locked` builds with the repository's `Cargo.lock`, which `cargo install`
otherwise ignores. Running `cargo install lokomotiv` without `--git` installs
the upstream crate from crates.io instead of this repository.

```bash
cargo install --locked --git https://github.com/maxkulish/lok --tag v20260914.0.0 lokomotiv
                             # Package "lokomotiv"; installs the "lok" and "lokomotiv" binaries

lok doctor                   # Check what backends are available
lok ask "Explain this code"  # Query all available backends
lok hunt .                   # Find bugs in current directory
```

Example `lok doctor` output when backends are configured:

```
Checking backends...

  ✓ codex - ready
  ✓ gemini - ready
  ✓ claude - ready

✓ 3 backend(s) ready.
```

## Prerequisites

Lok wraps existing LLM CLI tools. Install the ones you want to use:

| Backend | Install | Notes |
|---------|---------|-------|
| Codex | `npm install -g @openai/codex` | Fast code analysis |
| Gemini | `brew install anomalyco/tap/opencode` or `curl -fsSL https://opencode.ai/install | bash` | Deep security audits. After install, run `opencode auth login` to authenticate with Google. |
| Claude | [claude.ai/download](https://claude.ai/download) | Claude Code CLI |
| Ollama | [ollama.ai](https://ollama.ai) | Local models, no API keys |

For issue/PR workflows, you also need:

| Tool | Install | Used by |
|------|---------|---------|
| gh | [cli.github.com](https://cli.github.com) | `lok run fix`, `lok run review-pr` |

Run `lok doctor` to see which backends are detected. Core commands (`lok ask`,
`lok hunt`, `lok audit`) work without `gh`.

## Commands

### Analysis

```bash
lok ask "Find N+1 queries"              # Query all backends
lok ask -b codex "Find dead code"       # Specific backend
lok hunt .                              # Bug hunt (multiple prompts)
lok hunt --issues                       # Bug hunt + create GitHub issues
lok audit .                             # Security audit
lok explain                             # Explain codebase structure
```

### Code Review

```bash
lok diff                                # Review staged changes
lok diff main..HEAD                     # Review branch vs main
lok run review-pr 123                   # Multi-backend PR review + comment
```

### Issue Management

```bash
lok run fix 123                         # Analyze issue, propose fix, comment
lok ci 123                              # Analyze CI failures
```

### Multi-Agent Modes

```bash
lok debate "Should we use async here?"  # Backends argue and refine
lok spawn "Build a REST API"            # Break into parallel subtasks
lok conduct "Find and fix perf issues"  # Fully autonomous
```

### Workflows

```bash
lok run workflow-name                   # Run a workflow
lok workflow list                       # List available workflows
```

### Utilities

```bash
lok doctor                              # Check installation
lok backends                            # List configured backends
lok suggest "task"                      # Suggest best backend for task
lok init                                # Create config file
```

## Workflows

Workflows are TOML files that define multi-step LLM pipelines. Steps can depend
on previous steps and run in parallel when possible.

```toml
# .lok/workflows/example.toml
name = "example"

[[steps]]
name = "scan"
backend = "codex"
prompt = "Find obvious issues in this codebase"

[[steps]]
name = "deep-dive"
backend = "gemini"
depends_on = ["scan"]
prompt = "Investigate these findings: {{ steps.scan.output }}"

[[steps]]
name = "comment"
depends_on = ["deep-dive"]
shell = "gh issue comment 123 --body {{ steps[\"deep-dive\"].output | shell_escape }}"
```

Step output inserted into a `shell` field is data, not shell source. End every
`steps.*` output expression with `| shell_escape`, and place it as a complete
shell word or assignment value; do not put it inside another quote or a dynamic
heredoc. Use `printf '%s\\n'` for file output, `string` before `shell_escape`
for numeric or other non-string parsed fields, and validate model JSON before
passing its fields as quoted CLI arguments. The documented single-quoted or
bare `command_wrapper` forms preserve this boundary; custom double-quoted
wrappers are not safe and are tracked separately.

### Workflow Resolution

Lok searches for workflows in this order (first match wins):

1. **Project**: `.lok/workflows/{name}.toml`
2. **User**: `~/.config/lok/workflows/{name}.toml`
3. **Embedded**: Built into the lok binary

This means you can override any built-in workflow by creating your own version
at the project or user level.

### Built-in Workflows

Lok ships with these workflows embedded in the binary:

| Workflow | Description |
|----------|-------------|
| `diff` | Review git changes with multiple backends |
| `explain` | Explain codebase structure and architecture |
| `audit` | Security audit with multiple backends |
| `hunt` | Bug hunt with multiple backends |

Run `lok workflow list` to see all available workflows. Built-in workflows show
as "(built-in)", which you can override by creating your own version:

```bash
lok workflow list              # Shows: diff (built-in)
# Create override:
mkdir -p .lok/workflows
lok run diff > /dev/null       # See what it does, then customize:
cat > .lok/workflows/diff.toml << 'EOF'
name = "diff"
description = "My custom diff review"
# ... your custom steps
EOF
lok workflow list              # Now shows: diff (local)
```

### Consensus and Error Handling

For multi-backend steps, you can require consensus and handle partial failures.

**Workflow-level defaults** apply to all steps (steps can override):

```toml
name = "my-workflow"
continue_on_error = true    # All steps continue on failure by default
timeout = 300000            # All steps get 5 minute timeout by default

[[steps]]
name = "fast_step"
backend = "codex"
timeout = 60000             # Override: this step gets 1 minute
prompt = "Quick analysis..."

[[steps]]
name = "critical_step"
backend = "claude"
continue_on_error = false   # Override: this step must succeed
prompt = "Important work..."
```

**Step-level consensus** for multi-backend synthesis:

```toml
[[steps]]
name = "propose_claude"
backend = "claude"
prompt = "Propose a fix..."

[[steps]]
name = "propose_codex"
backend = "codex"
prompt = "Propose a fix..."

[[steps]]
name = "debate"
backend = "claude"
depends_on = ["propose_claude", "propose_codex", "propose_gemini"]
min_deps_success = 2        # Need at least 2/3 backends to succeed
prompt = "Synthesize the proposals: {{ steps.propose_claude.output }}..."
```

When `min_deps_success` is set (a dependency threshold, distinct from the
multi-backend `consensus` strategy above):
- Step runs if at least N dependencies succeeded
- Failed dependencies with `continue_on_error` pass their error output to the prompt
- Logs "dependency threshold met (2/3 succeeded)" when the threshold is met

This prevents wasted tokens when one backend times out or hits rate limits.

### Hard vs Soft Failures

Steps fail in two ways:

- **Hard failure**: Step fails and workflow stops. This is the default behavior.
- **Soft failure**: Step fails but workflow continues. Enabled with `continue_on_error = true`.

When a soft failure occurs, the error message is passed to dependent steps instead
of the normal output. This lets downstream steps handle the failure gracefully:

```toml
[[steps]]
name = "risky_step"
backend = "gemini"
continue_on_error = true   # Soft failure - workflow continues
prompt = "..."

[[steps]]
name = "handler"
depends_on = ["risky_step"]
prompt = """
{% if "error" in steps.risky_step.output %}
Handle the error: {{ steps.risky_step.output }}
{% else %}
Process result: {{ steps.risky_step.output }}
{% endif %}
"""
```

### Retries

Steps can retry on transient failures with exponential backoff:

```toml
[[steps]]
name = "flaky_backend"
backend = "gemini"
retries = 3              # Retry up to 3 times (default: 0)
retry_delay = 2000       # Start with 2 second delay (default: 1000ms)
prompt = "..."
```

The delay doubles after each retry: 2s, 4s, 8s. Retries help with rate limits
and temporary network issues. After all retries are exhausted, the step fails
normally (hard or soft depending on `continue_on_error`).

### Agentic Features

Workflows can apply code edits and verify them:

```toml
[[steps]]
name = "fix"
backend = "claude"
apply_edits = true
verify = "cargo build"
prompt = """
Fix this issue. Output JSON:
{"edits": [{"file": "src/main.rs", "old": "...", "new": "..."}]}
"""
```

**How `apply_edits` works:**

1. Parses JSON from LLM output looking for `{"edits": [...]}`
2. For each edit, finds `old` text in `file` and replaces with `new`
3. If `verify` is set, runs the command after edits
4. If verification fails, the step fails (edits remain applied)

**Risks and failure modes:**

- **File not found**: Edit fails if the target file doesn't exist
- **Text not found**: Edit fails if `old` text isn't in the file
- **Ambiguous match**: Edit fails if `old` text appears multiple times
- **Partial application**: If edit 3 of 5 fails, edits 1-2 remain applied

**Automatic rollback with git-agent:**

If [git-agent](https://github.com/ducks/git-agent) is installed and initialized,
lok automatically creates a checkpoint before applying edits. If edits fail or
verification fails, lok rolls back to the checkpoint.

```bash
# Install git-agent
cargo install --git https://github.com/ducks/git-agent

# Initialize in your project
git-agent init
git-agent begin "Working on feature X"

# Now lok will auto-checkpoint before apply_edits
lok run my-workflow  # Creates checkpoint, applies edits, rolls back on failure
```

When git-agent is active, you'll see:
```
  → Applying edits...
    ✓ git-agent checkpoint created
    ✓ Applied 3 edit(s)
  verify: cargo build
    ✗ Verification failed: ...
    ↩ Rolled back via git-agent
```

Without git-agent, lok still works but won't auto-rollback.

**Recommendations:**

- Use git-agent for automatic rollback on failures
- Start with `verify` commands to catch bad edits early
- Review LLM output before running with `--apply` in production
- Keep `old` text specific enough to match exactly once

### Structured Output

Workflows can produce JSON output for programmatic consumption. Use the
`output_format` field to control how LLM responses are parsed:

```toml
[[steps]]
name = "analyze"
backend = "codex"
output_format = "json"    # Parse output as JSON
prompt = "Return findings as JSON array..."
```

Output format options:
- `text` (default): Raw text output
- `json`: Parse as JSON object
- `json_array`: Parse as JSON array
- `jsonl`: Parse as newline-delimited JSON

Downstream steps can access parsed fields:

```toml
[[steps]]
name = "report"
depends_on = ["analyze"]
prompt = "Summarize: {{ steps.analyze.output.findings }}"
```

## Configuration

Works without config. For customization, create `~/.config/lok/lok.toml`
(user config) or `lok.toml` in the directory you run lok from (project config):

```toml
[defaults]
parallel = true
timeout = 300
# Wrap shell commands for isolated environments (NixOS, Docker).
# User config only - see "Project vs user config" below.
# command_wrapper = "nix-shell --run '{cmd}'"
# command_wrapper = "docker exec dev sh -c '{cmd}'"

[backends.codex]
enabled = true
command = "codex"
# User config only: these args differ from the default.
args = ["exec", "--json", "-s", "read-only"]

[backends.ollama]
enabled = true
command = "http://localhost:11434"
model = "qwen2.5-coder:7b"

[cache]
enabled = true
ttl_hours = 24
```

### Project vs user config

lok merges three layers: built-in defaults, then `~/.config/lok/lok.toml`, then
`./lok.toml` in the current directory. Parent directories are not searched.
`--config <file>` replaces both files.

A project `lok.toml` usually arrives with a cloned repository, so it cannot
change the keys that decide what lok executes and where prompts and API keys
go:

| Key | Why it is protected |
|-----|---------------------|
| `backends.<name>.command` | The binary lok spawns, or the URL an HTTP backend sends prompts to |
| `backends.<name>.args` | Arguments to that binary, including sandbox-bypass flags |
| `backends.<name>.api_key_env` | Which environment variable is sent as the API key |
| `defaults.command_wrapper` | Wraps every workflow shell command |

A project file may restate one of these keys with the built-in default or with
the value your user config sets, so files written by `lok init` keep working.
The restated value never takes effect; your user config decides. Any other
value stops lok from loading, and every subcommand fails with an error naming
the file and the keys:

```
Error: /path/to/repo/lok.toml sets keys that a project config cannot change: backends.codex.command.
```

To fix it, move the keys to `~/.config/lok/lok.toml`, delete them from the
project file, or run with `--config ~/.config/lok/lok.toml`. For example, to run
codex at high reasoning effort everywhere, put this in your user config:

```toml
[backends.codex]
args = ["exec", "--json", "--ephemeral", "-c", "model_reasoning_effort=\"high\""]
```

The protection covers config keys only. A project's `.lok/workflows/<name>.toml`
is still found before your global workflows, so only run workflows by name in
repositories you trust.

### Command Wrapper (NixOS/Docker)

If you use isolated environments, shell commands in workflows may fail due to
missing dependencies. Use `command_wrapper` in `~/.config/lok/lok.toml` to wrap
all shell commands. A project `lok.toml` cannot set it (see
[Project vs user config](#project-vs-user-config)):

```toml
[defaults]
# For NixOS with nix-shell
command_wrapper = "nix-shell --run '{cmd}'"

# For Docker
command_wrapper = "docker exec dev sh -c '{cmd}'"

# For direnv
command_wrapper = "direnv exec . {cmd}"
```

The `{cmd}` placeholder is replaced with the actual command.

## Backend Strengths

| Backend | Best For | Speed |
|---------|----------|-------|
| Codex | Code patterns, N+1, dead code | Fast |
| Gemini | Security audits, deep analysis (opencode-driven) | Slow (thorough) |
| Claude | Orchestration, reasoning | Medium |
| Ollama | Local/private, no rate limits | Varies |

## Real World Results

Lok found 25 bugs in its own codebase, then found a real bug in Discourse
(35k stars) that became a merged PR.

```bash
lok hunt ~/dev/discourse --issues -y    # Found hardlink limit bug
```

## Why "Lok"?

Swedish/German: Short for "lokomotiv" (locomotive). The conductor sends trained
models down the tracks.

Sanskrit/Hindi: "lok" means "world" or "people", as in "Lok Sabha" (People's
Assembly). Multiple agents working as a collective.

## License

MIT
