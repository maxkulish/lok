# Lessons: CLO-396 — opencode migration docs + setup guide refresh

## L1 - Docs-only PRs: Codex model timeouts are expected

**Source incident:** CLO-396 implementation (FR-12c). Codex reviewer (gpt-5.5 with reasoning.effort="high") exceeded 900s timeout in the pre-pr-validation workflow. Fallback gpt-4.1 also hit 90s timeout on a lightweight focused prompt.

**Rule:** Documentation-only changes often trigger model timeouts because LLM reviewers attempt to read full design docs, plans, and all diffed files. For doc-only PRs, always maintain a locally-verifiable pre-merge gate (grep-based) and be prepared to substitute manual assessment when both primary and fallback model reviews timeout.

**How to apply:**
- For code PRs: trust the `pre-pr-validation` workflow timing.
- For docs-only PRs: supplement with manual diff review before starting the validation gate. If the model times out, record timeout+manual-assessment explicitly rather than retrying indefinitely.
- Consider lightening `CODEX_MODEL` or reasoning effort for documentation PRs.

## L2 - Pre-merge re-fetch gate counts bot approvals as "new comments"

**Source incident:** CLO-396 PR #53 pre-merge gate. After the author posted fixes and `/gemini review` replies, Gemini-code-assist re-reviewed and approved the fixes (two approval comments). The Step 5.0 re-fetch gate flagged these as "new inline comments since last review_addressed", treating them as unaddressed review suggestions even though both threads were resolved.

**Rule:** Bot review approval comments are not new review suggestions. The re-fetch gate should differentiate between bot-initiated review comments (first comment in a thread) and bot reply/approval comments (subsequent comments in existing threads).

**How to apply:**
- When re-fetching after bot reviews, check if new comments are inside existing threads (where the latest is a bot approval) or new threads entirely.
- If the only new comments are bot approvals in previously-resolved threads, override the gate with rationale ("bot re-verified fixes; no new review suggestions").
- Keep the check for genuinely new unresolved threads, which is the intended safety net.

## L3 - Markdown callouts adjacent to code blocks can nest inside fences

**Source incident:** CLO-396 ST1 initial commit. A "Migrating from Gemini CLI" callout was placed after a ````toml` code block but before its closing ```` ``` ```` fence, nesting the callout inside the code block. This caused:
1. The Markdown link to not render as clickable
2. Invalid TOML syntax visible in the code block
3. A HIGH severity finding from the reviewer

**Rule:** After editing content inside a Markdown code block, always verify the fence placement — especially when adding blockquotes (`>`), nested blocks, or other Markdown syntax immediately after code block content.

**How to apply:**
- After editing near code fences, run a Markdown linter or render preview.
- Be particularly careful with minimal-config code blocks that are often copy-pasted by users; syntax errors there propagate to production configs.
- If the callout belongs to the code block's context, place it after the closing fence, not before.
