# Gemini JSON Envelope Fixtures

Scrubbed fixtures for the Gemini backend parser unit and integration tests.

## Capture command

```bash
echo '' | npx @google/gemini-cli --skip-trust --output-format json 'Reply exactly: ok.' \
  > tests/fixtures/gemini/success-with-stats.json
```

## Scrub checklist

- [ ] `session_id` replaced with `00000000-0000-0000-0000-000000000000`
- [ ] No file system paths (`/Users/`, `/home/`, etc.)
- [ ] No API keys, tokens, or credentials
- [ ] No email addresses
- [ ] Token values are rounded / simplified for determinism

## Fixture inventory

| File | Purpose |
|------|---------|
| `success-with-stats.json` | Populated `stats.models.*.tokens` shape (current CLI ≥0.42) |
| `success-no-stats.json` | Envelope with `response` but no `stats` block |
| `malformed.json` | Truncated JSON to exercise text-mode fallback |
| `error-envelope.json` | JSON with top-level `error` (not consumed by parser on success path) |
