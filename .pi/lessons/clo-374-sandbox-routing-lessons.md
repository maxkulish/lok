# Lessons: CLO-374 Per-step sandbox routing

Durable rules from implementing per-step sandbox routing for Codex and Gemini backends (FR-21).

---

## L1 — Shell command construction must escape all components, not just the prompt

**Source incident:** CLO-374 pre-PR validation — gemini-code-assist bot flagged that `GeminiBackend::build_shell_cmd` joined `command` and `args` with spaces without shell-escaping them. Only the `prompt` was escaped via single-quote wrapping. The command (`npx`) and args (`@google/gemini-cli`) happened to contain no shell-metacharacters, so the issue was latent — but any config change adding a flag with a space or special character would have broken the shell command.

**Rule:** When constructing a shell command string programmatically (e.g., for `sh -c` execution), every single component inserted into the command line must be individually shell-escaped. "The prompt is user-controlled so it's the only one that needs escaping" is wrong — configuration values, model names, and backend args are equally capable of containing shell metacharacters.

**How to apply:** Extract shell command construction into a central `build_shell_cmd` method (or free function) that both `query()` and tests call. The escaping closure should wrap each component in single quotes and escape internal single quotes (`s.replace("'", "'\\''")`). Apply it uniformly to:
- The executable command
- Every argument
- The prompt (already done, but must use the same helper)
- Any model/approval-mode flags built from user- or config-supplied strings

**Test tip:** Add a test with a deliberately shell-hostile command name or arg (e.g., `command = "my tool"` with a space) and assert the shell command string is syntactically valid after escaping. Relying on happy-path values (`"npx"`, `"@google/gemini-cli"`) misses the regression window.
