# Spec Review: clo-637

**Reviewer**: Codex via Ollama (glm-5.2:cloud)
**Reviewed**: 2026-08-06
**Pipeline**: lok spec-review

---

Error: Shell command failed: PROMPT=$(sed -e 's|__SPEC_PATH__|specs/2026-08-06-clo-637-qodo-rereview-poll.md|g' -e 's|__LINEAR_TITLE__|Make /pr:review's re-review poll recognise a Qodo comment update instead of waiting for a review object|g' -e 's|__LINEAR_DESC__|Step 9.5 polls for a review object on the new head SHA after requesting a re-review, but Qodo edits its existing review comment in place and never submits a new review object on a clean pass, so the poll runs to full timeout. Fix: dual-shape detection (review object OR completion comment), fail closed, reconcile the contradictory sections.|g' -e 's|__LINEAR_LABELS__|AFK|g' .lok/prompts/spec-review-prompt.md)

OUTPUT=$(env -u CLAUDECODE timeout 300 ollama launch codex --model "${OLLAMA_MODEL:-glm-5.2:cloud}" -- exec "$PROMPT" --sandbox read-only --oss --local-provider ollama --ephemeral < /dev/null 2>/tmp/lok-ollama-spec-stderr.log)

if [ -z "$OUTPUT" ] || [ $(printf '%s' "$OUTPUT" | wc -c) -lt 100 ]; then
  STDERR=$(head -5 /tmp/lok-ollama-spec-stderr.log 2>/dev/null || echo "no stderr")
  echo "REVIEW_FAILED: Empty output from Ollama/Codex (stderr: $STDERR)"
  exit 0
fi

echo "$OUTPUT"
sh: -c: line 4: unexpected EOF while looking for matching `''
sh: -c: line 11: syntax error: unexpected end of file
