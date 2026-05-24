# Gemini Backend Fixtures (Gemini CLI + opencode migration)

Scrubbed fixtures for the Gemini/opencode backend parser unit and integration tests.

## Capture command(s)

Legacy Gemini CLI fixture capture:

```bash

echo '' | npx @google/gemini-cli --skip-trust --output-format json 'Reply exactly: ok.' \
  > tests/fixtures/gemini/success-with-stats.json
```

Captured opencode fixture command (NDJSON stream):

```bash
opencode run --format json -- "Reply exactly: ok." \
  | sed -n '1,2p' \
  > tests/fixtures/gemini/success-with-stats.json
```

To capture a no-stats variant (response only), trim the usage-bearing lines:

```bash
opencode run --format json -- "Reply exactly: ok." \
  | grep '"type":"text"' \
  > tests/fixtures/gemini/success-no-stats.json
```

## Scrub checklist

- [x] `sessionID` replaced with `00000000-0000-0000-0000-000000000000` where present
- [x] No file system paths (`/Users/`, `/home/`, etc.)
- [x] No API keys, tokens, or credentials
- [x] No email addresses
- [x] Token values are rounded / simplified for determinism
- [x] Canonical fields retained (`type`, `part`, `timestamp`, `sessionID`) where available

## Fixture inventory

| File | Purpose |
|------|---------|
| `success-with-stats.json` | Opencode-shaped output (NDJSON) with response + usage |
| `success-no-stats.json` | Opencode-shaped output (single JSON line) without usage |
| `malformed.json` | Truncated JSON to exercise parser fallback |
| `error-envelope.json` | JSON with top-level `error` for fallback coverage |
