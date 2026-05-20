**VERDICT: APPROVE_WITH_SUGGESTIONS**

This is a well-structured and thoughtful design document. It cleanly solves the reliability issue of JSONL diagnostic streams by leveraging the new CLI capability, maintains backwards compatibility with strict parser expectations, and makes excellent use of Rust's RAII patterns for cleanup. 

However, there is a critical functional flaw regarding whitespace handling, and a concurrency safety detail to address before implementation.

### Key Findings

1. **Architecture & Precedence:** The precedence hierarchy (Terminal Error > `-o` File > JSONL text > Parse Error) is logically sound and correctly prioritizes failure detection over partial success.
2. **Codebase Alignment:** Reusing `build_argv_prefix` and adding `parse_jsonl_diagnostics` alongside the strict parser perfectly aligns with the existing codebase and preserves established test contracts.
3. **Whitespace Corruption Risk:** The open question regarding whitespace trimming identifies a highly destructive path if leading whitespace is trimmed. 
4. **Async Blocking I/O:** The proposed file read occurs inside an `async fn`. Doing this synchronously will block the executor thread.

### Prioritized Actionable Items

**1. CRITICAL: Preserve Leading Whitespace (Resolving the Open Question)**
*   **Issue:** The "Open questions" section mentions: *"read_last_message() trims leading/trailing whitespace... If exact byte preservation matters... switch to trimming only trailing newlines."*
*   **Action:** You **must** switch to trimming only trailing newlines. If the LLM generates Python, YAML, Markdown lists, or indented code blocks, stripping leading whitespace will silently corrupt the output syntax. Update the design to explicitly forbid trimming leading whitespace.

**2. IMPORTANT: Concurrency Safety (Avoid Blocking the Async Executor)**
*   **Issue:** `CodexBackend::query` is an `async fn`. The `read_last_message` helper is designed to read a file from the filesystem. If implemented with `std::fs::read_to_string`, this is a blocking I/O operation that will stall the Tokio (or equivalent) worker thread.
*   **Action:** Specify that `read_last_message` should be `async` and utilize `tokio::fs::read_to_string` (or the async runtime equivalent) to read the temporary file without blocking the executor. (Synchronous creation of the `NamedTempFile` is generally acceptable as it is effectively instantaneous, but reading potentially larger output should be async).

**3. MODERATE: Graceful Degradation for Older Codex CLIs**
*   **Issue:** Assumption A2 states that older CLIs will fail noisily if `-o` is passed, and "version-gated fallback can be added later if that proves common". If it fails noisily, the `CodexBackend` will return a hard error, completely breaking Lok for any user with an older Codex CLI.
*   **Action:** Evaluate your deployment environment. If you do not have strict control over forcing users to upgrade to Codex CLI `v0.119.0+`, consider catching the specific exit code/stderr associated with an unknown argument and automatically retrying once *without* the `-o` flag. Otherwise, explicitly document this as a breaking requirement for end-users.

**4. MINOR: Security Posture / File Permissions**
*   **Issue:** LLM outputs may contain sensitive project context or generated secrets. 
*   **Action:** Add a brief note under assumptions confirming that `tempfile::NamedTempFile` inherently creates files with secure permissions (e.g., `0600` on Unix), preventing other local users from snooping the `-o` output before it is read and dropped.
