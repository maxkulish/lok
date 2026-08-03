# Handoff: consolidate Codex user hooks

## Objective

Remove this Codex 0.146.0 startup warning without losing either integration:

```text
loading hooks from both /Users/mk/.codex/hooks.json and /Users/mk/.codex/config.toml; prefer a single representation for this layer
```

Use `~/.codex/config.toml` as the only user-level hook representation. Preserve:

- the six agterm lifecycle hooks already in `config.toml`;
- the Plannotator `Stop` hook currently in `hooks.json`, including its 345600-second timeout.

## Current state verified on 2026-08-03

`/Users/mk/.codex/hooks.json` contains only:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/Users/mk/.local/bin/plannotator",
            "timeout": 345600
          }
        ]
      }
    ]
  }
}
```

`/Users/mk/.codex/config.toml` contains an agterm-managed block ending with:

```toml
[[hooks.Stop]]
[[hooks.Stop.hooks]]
type = "command"
command = "'/Users/mk/.config/agterm/agent-status/agterm-codex-status.sh' stop"
# <<< agterm agent-status <<<
```

It also contains this now-stale trust record for the JSON hook:

```toml
[hooks.state."/Users/mk/.codex/hooks.json:stop:0:0"]
trusted_hash = "sha256:88a431b24a3f0a6d5a5d7fe6beabc2805b0d3b484fb7cafa6dbc818602243c6c"
```

## Apply

1. Close the Codex `/hooks` screen before editing, because that screen saves hook-state changes automatically.
2. Re-read both files and stop if their hook definitions differ from the verified state above.
3. In `/Users/mk/.codex/config.toml`, delete only the stale JSON trust-record table shown above.
4. Append this block after the agterm managed block. Keep it outside the `# >>> agterm agent-status >>>` / `# <<< agterm agent-status <<<` markers so an agterm reinstall does not own it.

```toml

# >>> plannotator stop hook (migrated from hooks.json) >>>
[[hooks.Stop]]
[[hooks.Stop.hooks]]
type = "command"
command = "/Users/mk/.local/bin/plannotator"
timeout = 345600
# <<< plannotator stop hook <<<
```

5. Validate the edited TOML before retiring the JSON file:

```sh
codex --strict-config doctor --json
```

The `config.load` check must be `ok`, and `config.toml parse` must be `ok`.

6. Retire the legacy representation recoverably; do not delete it:

```sh
mv /Users/mk/.codex/hooks.json /Users/mk/.codex/hooks.json.disabled-2026-08-03
```

If that destination already exists, choose another explicit backup name rather than overwriting it.

## Verify

1. Start a fresh interactive Codex session. The dual-representation warning must be absent.
2. Open `/hooks` and verify:
   - `PreToolUse` still contains the trusted agterm command;
   - `Stop` contains both the agterm command and `/Users/mk/.local/bin/plannotator`;
   - the Plannotator timeout is 345600 seconds.
3. Trust and enable the migrated Plannotator hook if Codex prompts. This should create a new `config.toml:stop:1:0` hook-state record; let Codex write that record instead of manufacturing its hash manually.
4. Re-run:

```sh
codex --strict-config doctor --json
```

## Rollback

If the migrated hook is not loaded, remove only the marked Plannotator TOML block and restore the backup:

```sh
mv /Users/mk/.codex/hooks.json.disabled-2026-08-03 /Users/mk/.codex/hooks.json
```

The startup warning will return after rollback, but the previous hook behavior will be restored.

