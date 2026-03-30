# CLO-180: Extend Backend::query() to Return QueryOutput

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change `Backend::query()` from returning `Result<String>` to `Result<QueryOutput>` so stderr and exit codes are captured separately from stdout.

**Architecture:** Define a `QueryOutput` struct with `stdout`, `stderr`, `exit_code` fields. CLI backends (Claude CLI, Gemini, Codex) populate all three fields from process output. API backends (Claude API, Ollama, Bedrock) return `stderr: None, exit_code: None`. All callers extract `.stdout` where they previously used the raw string.

**Tech Stack:** Rust, async_trait, tokio::process::Command

**PRD:** `docs/prds/prd-output-validation-pipeline.md` (FR-1, FR-3, FR-4, FR-5)

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/backend/mod.rs` | Modify | Define `QueryOutput`, change `Backend` trait, update `run_query_with_config` |
| `src/backend/claude.rs` | Modify | Return `QueryOutput` from both API and CLI modes |
| `src/backend/gemini.rs` | Modify | Return `QueryOutput` with stderr + exit_code from CLI |
| `src/backend/ollama.rs` | Modify | Return `QueryOutput` with `stderr: None, exit_code: None` |
| `src/backend/codex.rs` | Modify | Return `QueryOutput` with stderr + exit_code from CLI |
| `src/backend/bedrock.rs` | Modify | Return `QueryOutput` with `stderr: None, exit_code: None` |
| `src/workflow.rs` | Modify | Update all `backend.query()` callers to use `.stdout` |
| `src/conductor.rs` | Modify | Update `backend.query()` caller to use `.stdout` |
| `src/debate.rs` | Modify | Update `backend.query()` caller to use `.stdout` |
| `src/team.rs` | Modify | Update `backend.query()` callers to use `.stdout` |
| `src/spawn.rs` | Modify | Update `backend.query()` callers to use `.stdout` |
| `tests/integration.rs` | Verify | Existing tests must pass unchanged |

---

### Task 1: Define QueryOutput and Change Backend Trait

**Files:**
- Modify: `src/backend/mod.rs:22-27` (trait definition)
- Modify: `src/backend/mod.rs:29-34` (QueryResult struct)

- [ ] **Step 1: Define QueryOutput struct**

Add above the `Backend` trait definition in `src/backend/mod.rs`:

```rust
/// Structured output from a backend query, separating stdout from stderr/exit_code.
/// CLI backends populate all fields; API backends set stderr and exit_code to None.
#[derive(Debug, Clone)]
pub struct QueryOutput {
    pub stdout: String,
    pub stderr: Option<String>,
    pub exit_code: Option<i32>,
}

impl QueryOutput {
    /// Create a QueryOutput from just stdout (for API backends)
    pub fn from_stdout(stdout: String) -> Self {
        Self {
            stdout,
            stderr: None,
            exit_code: None,
        }
    }

    /// Create a QueryOutput from process output (for CLI backends)
    pub fn from_process(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self {
            stdout,
            stderr: Some(stderr),
            exit_code: Some(exit_code),
        }
    }
}
```

- [ ] **Step 2: Change Backend trait signature**

Change the trait in `src/backend/mod.rs`:

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput>;
    fn is_available(&self) -> bool;
}
```

- [ ] **Step 3: Update run_query_with_config to extract stdout**

In `src/backend/mod.rs`, the `query_one` closure on line ~148 needs to extract `.stdout` into the `QueryResult.output` field:

```rust
match result {
    Ok(Ok(query_output)) => QueryResult {
        backend: backend.name().to_string(),
        output: query_output.stdout,
        success: true,
        elapsed_ms,
    },
    // Ok(Err(e)) and Err(_) branches stay the same
```

- [ ] **Step 4: Verify it compiles (expect errors in backends)**

Run: `cargo check 2>&1 | head -30`
Expected: Errors in backend implementations (claude, gemini, ollama, codex, bedrock) because they still return `Result<String>`. This confirms the trait change propagated correctly.

---

### Task 2: Update Claude Backend

**Files:**
- Modify: `src/backend/claude.rs:92-143` (query_api), `src/backend/claude.rs:145-179` (query_cli), `src/backend/claude.rs:181-190` (query_with_system), `src/backend/claude.rs:193-207` (Backend impl)

- [ ] **Step 1: Update query_api to return QueryOutput**

In `query_api()`, change the return at the end (line ~142):

```rust
    Ok(QueryOutput::from_stdout(text))
```

Change the method signature:

```rust
async fn query_api(&self, system: &str, prompt: &str) -> Result<QueryOutput> {
```

- [ ] **Step 2: Update query_cli to return QueryOutput with stderr + exit_code**

Change `query_cli()` signature and body:

```rust
async fn query_cli(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput> {
    let (command, model) = match &self.mode {
        ClaudeMode::Cli { command, model } => (command, model),
        ClaudeMode::Api { .. } => anyhow::bail!("CLI mode required for this operation"),
    };

    let mut cmd = Command::new(command);
    cmd.arg("-p")
        .arg("--output-format")
        .arg("text");

    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }

    cmd.arg("--")
        .arg(prompt)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .context("Failed to execute claude command")?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !output.status.success() {
        anyhow::bail!("Claude CLI failed: {}", stderr);
    }

    Ok(QueryOutput::from_process(stdout, stderr, exit_code))
}
```

- [ ] **Step 3: Update query_with_system to return QueryOutput**

Change signature and body:

```rust
pub async fn query_with_system(&self, system: &str, prompt: &str) -> Result<QueryOutput> {
    match &self.mode {
        ClaudeMode::Api { .. } => self.query_api(system, prompt).await,
        ClaudeMode::Cli { .. } => {
            let full_prompt = format!("{}\n\n{}", system, prompt);
            self.query_cli(&full_prompt, Path::new(".")).await
        }
    }
}
```

- [ ] **Step 4: Update Backend trait impl**

```rust
#[async_trait]
impl super::Backend for ClaudeBackend {
    fn name(&self) -> &str {
        "claude"
    }

    async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput> {
        match &self.mode {
            ClaudeMode::Api { .. } => {
                self.query_with_system("You are a helpful assistant.", prompt)
                    .await
            }
            ClaudeMode::Cli { .. } => self.query_cli(prompt, cwd).await,
        }
    }

    fn is_available(&self) -> bool {
        match &self.mode {
            ClaudeMode::Api { api_key, .. } => !api_key.expose_secret().is_empty(),
            ClaudeMode::Cli { command, .. } => which::which(command).is_ok(),
        }
    }
}
```

- [ ] **Step 5: Add import**

Add at top of `src/backend/claude.rs`:

```rust
use super::QueryOutput;
```

- [ ] **Step 6: Check compilation of this file**

Run: `cargo check 2>&1 | grep -E "claude\.rs|error\["`
Expected: No errors in claude.rs. Errors in other backends still expected.

---

### Task 3: Update Gemini Backend

**Files:**
- Modify: `src/backend/gemini.rs:41-83`

- [ ] **Step 1: Add import and update query()**

Add import at top:

```rust
use super::QueryOutput;
```

Update the `Backend` impl `query()` method:

```rust
async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput> {
    let escaped_prompt = prompt.replace("'", "'\\''");
    let shell_cmd = format!(
        "echo '' | {} {} '{}'",
        &self.command,
        self.args.join(" "),
        escaped_prompt
    );

    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(&shell_cmd)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .context("Failed to execute gemini command")?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        anyhow::bail!("Gemini failed: {}", stderr);
    }

    Ok(QueryOutput::from_process(
        self.parse_output(&stdout),
        stderr,
        exit_code,
    ))
}
```

- [ ] **Step 2: Verify**

Run: `cargo check 2>&1 | grep -E "gemini\.rs|error\["`
Expected: No errors in gemini.rs.

---

### Task 4: Update Ollama Backend

**Files:**
- Modify: `src/backend/ollama.rs:96-111`

- [ ] **Step 1: Add import and update query()**

Add import at top:

```rust
use super::QueryOutput;
```

Update `Backend` impl:

```rust
#[async_trait]
impl Backend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn query(&self, prompt: &str, _cwd: &Path) -> Result<QueryOutput> {
        let stdout = self.chat(prompt).await?;
        Ok(QueryOutput::from_stdout(stdout))
    }

    fn is_available(&self) -> bool {
        true
    }
}
```

- [ ] **Step 2: Verify**

Run: `cargo check 2>&1 | grep -E "ollama\.rs|error\["`
Expected: No errors in ollama.rs.

---

### Task 5: Update Codex Backend

**Files:**
- Modify: `src/backend/codex.rs:57-89`

- [ ] **Step 1: Add import and update query()**

Add import at top:

```rust
use super::QueryOutput;
```

Update `Backend` impl `query()`:

```rust
async fn query(&self, prompt: &str, cwd: &Path) -> Result<QueryOutput> {
    let mut cmd = Command::new(&self.command);
    cmd.args(&self.args)
        .arg("--")
        .arg(prompt)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .context("Failed to execute codex command")?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        anyhow::bail!("Codex failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(QueryOutput::from_process(
        self.parse_output(&stdout),
        stderr,
        exit_code,
    ))
}
```

- [ ] **Step 2: Verify**

Run: `cargo check 2>&1 | grep -E "codex\.rs|error\["`
Expected: No errors in codex.rs.

---

### Task 6: Update Bedrock Backend

**Files:**
- Modify: `src/backend/bedrock.rs:122-159`

- [ ] **Step 1: Add import and update query()**

Add import at top:

```rust
use super::QueryOutput;
```

Update `Backend` impl `query()`:

```rust
async fn query(&self, prompt: &str, _cwd: &Path) -> Result<QueryOutput> {
    let messages = vec![Message {
        role: "user".to_string(),
        content: MessageContent::Text(prompt.to_string()),
    }];

    let response = self.invoke_with_messages(None, messages, None).await?;

    let text = response
        .content
        .into_iter()
        .filter_map(|block| match block {
            ResponseBlock::Text { text } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(QueryOutput::from_stdout(text))
}
```

- [ ] **Step 2: Verify all backends compile**

Run: `cargo check 2>&1 | grep "error\["`
Expected: No backend errors. Remaining errors should be in callers (workflow.rs, conductor.rs, debate.rs, team.rs, spawn.rs) that still expect `Result<String>`.

---

### Task 7: Update Callers - conductor.rs, debate.rs, team.rs, spawn.rs

These files all use `backend.query()` and expect a `String`. They need `.stdout` extraction.

**Files:**
- Modify: `src/conductor.rs:185`
- Modify: `src/debate.rs:230`
- Modify: `src/team.rs:82,121`
- Modify: `src/spawn.rs:173,282`

- [ ] **Step 1: Update conductor.rs**

Line 185, change:

```rust
let result = backend.query(prompt, cwd).await?;
```

to:

```rust
let result = backend.query(prompt, cwd).await?.stdout;
```

- [ ] **Step 2: Update debate.rs**

Line 230, change:

```rust
match backend.query(&prompt, &self.cwd).await {
    Ok(response) => {
```

to:

```rust
match backend.query(&prompt, &self.cwd).await {
    Ok(query_output) => {
        let response = query_output.stdout;
```

- [ ] **Step 3: Update team.rs**

Line 82, change:

```rust
let primary_result = primary_backend.query(task, &self.cwd).await?;
```

to:

```rust
let primary_result = primary_backend.query(task, &self.cwd).await?.stdout;
```

Line 121, change:

```rust
match other_backend.query(&prompt, &self.cwd).await {
    Ok(response) => {
```

to:

```rust
match other_backend.query(&prompt, &self.cwd).await {
    Ok(query_output) => {
        let response = query_output.stdout;
```

- [ ] **Step 4: Update spawn.rs**

Line 173, change:

```rust
let output = backend.query(&prompt, &self.cwd).await?;
```

to:

```rust
let output = backend.query(&prompt, &self.cwd).await?.stdout;
```

Line 282, change:

```rust
match backend.query(&prompt, &cwd).await {
    Ok(output) => AgentResult {
```

to:

```rust
match backend.query(&prompt, &cwd).await {
    Ok(query_output) => AgentResult {
        name: task.name,
        backend: backend.name().to_string(),
        output: query_output.stdout,
        success: true,
    },
```

(Remove the duplicate `name`, `backend`, `output`, `success` fields that were previously in the match arm - keep only the struct with `.stdout` extraction.)

- [ ] **Step 5: Verify**

Run: `cargo check 2>&1 | grep "error\["`
Expected: Only errors in `workflow.rs` remaining.

---

### Task 8: Update Callers - workflow.rs

The largest file. There are 7 places where `backend.query()` is called. Each returns `Result<QueryOutput>` now and the match arms need to extract `.stdout`.

**Files:**
- Modify: `src/workflow.rs` (lines ~797, ~968, ~1072, ~1182, ~1389, ~1437)

- [ ] **Step 1: Update for_each loop query (line ~797)**

Find the `backend.query(&iter_prompt, &cwd)` call inside the for_each iteration. The match arm `Ok(Ok(text))` becomes:

```rust
Ok(Ok(query_output)) => {
    let text = query_output.stdout;
```

- [ ] **Step 2: Update multi-backend consensus query (line ~968)**

Find `backend.query(&prompt, &cwd)` inside the multi-backend spawn. The match arm `Ok(Ok(text))` becomes:

```rust
Ok(Ok(text)) => (bn.clone(), Ok(text.stdout)),
```

- [ ] **Step 3: Update synthesis backend query (line ~1072)**

Find `synth_backend.query(&synth_prompt, &cwd)`. Change:

```rust
Ok(Ok(synthesized)) => {
```

to:

```rust
Ok(Ok(query_output)) => {
    let synthesized = query_output.stdout;
```

- [ ] **Step 4: Update single-backend query (line ~1182)**

Find `backend.query(&prompt, &cwd)` in the single backend path. Change:

```rust
Ok(Ok(t)) => {
    text = t;
```

to:

```rust
Ok(Ok(query_output)) => {
    text = query_output.stdout;
```

- [ ] **Step 5: Update fix retry queries (lines ~1389, ~1437)**

Find both `backend.query(&fix_prompt, &cwd)` calls in the fix retry loop. Each `Ok(Ok(new_text))` becomes:

```rust
Ok(Ok(query_output)) => {
    let new_text = query_output.stdout;
```

(For both occurrences - one for the verify-fail retry, one for the apply-edits retry.)

- [ ] **Step 6: Verify full compilation**

Run: `cargo check`
Expected: Clean compilation with no errors.

---

### Task 9: Run Tests and Commit

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: All 4 integration tests pass (test_interpolation_workflow, test_conditionals_workflow, test_retry_workflow, test_parallel_workflow).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | tail -20`
Expected: No warnings.

- [ ] **Step 3: Commit**

```bash
git add src/backend/mod.rs src/backend/claude.rs src/backend/gemini.rs src/backend/ollama.rs src/backend/codex.rs src/backend/bedrock.rs src/workflow.rs src/conductor.rs src/debate.rs src/team.rs src/spawn.rs
git commit -m "feat(CLO-180): extend Backend::query() to return QueryOutput struct

Change Backend trait query() return type from Result<String> to
Result<QueryOutput> where QueryOutput carries stdout, stderr, and
exit_code separately. CLI backends capture stderr via Stdio::piped()
and preserve process exit codes. API backends return stderr: None,
exit_code: None. All callers extract .stdout for unchanged behavior."
```
