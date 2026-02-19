# remindctl-mcp

MCP server in Rust that wraps [`remindctl`](https://github.com/steipete/remindctl) so remote clients can manage Apple Reminders through a macOS host.

This project is designed for setups where your Linux machine (or any non-Apple machine) needs to read/write reminders via your Mac.

## What it exposes

- MCP **tools** for reminders and list management (`reminders_list`, `reminder_add`, `reminder_delete`, `lists_list`, `process_pending_actions`, etc.)
- MCP **resources** for status/lists/config snapshots
- Streamable HTTP transport at `/mcp`

## Requirements

- macOS host with Apple Reminders
- `remindctl` installed and authorized on that host
- Rust toolchain (`cargo`)

Quick verify on the host:

```bash
remindctl status --json --no-input
```

## Build and run

From this directory:

```bash
cargo run --release
```

By default, the server binds to `127.0.0.1:8787`.

### Environment variables

- `BIND_ADDR` (default: `127.0.0.1:8787`)
- `AUTH_REQUIRED` (default: `true`)
- `API_KEY` (required when `AUTH_REQUIRED=true`)
- `REMINDCTL_BIN` (default: `remindctl`)
- `REMINDCTL_READ_TIMEOUT_SECS` (default: `10`)
- `REMINDCTL_WRITE_TIMEOUT_SECS` (default: `20`)

Examples:

```bash
# Safe default: auth enabled
AUTH_REQUIRED=true API_KEY="change-me" BIND_ADDR=127.0.0.1:8787 cargo run --release

# Local trusted testing only
AUTH_REQUIRED=false BIND_ADDR=127.0.0.1:8787 cargo run --release
```

## Quick install (macOS service)

If you just want it running at boot/login on your Mac:

```bash
API_KEY="change-me" AUTH_REQUIRED=true BIND_ADDR=127.0.0.1:8787 ./scripts/install-macos-service.sh
```

If you have many reminders, set a higher read timeout:

```bash
API_KEY="change-me" AUTH_REQUIRED=true BIND_ADDR=127.0.0.1:8787 REMINDCTL_READ_TIMEOUT_SECS=60 ./scripts/install-macos-service.sh
```

Then verify:

```bash
launchctl print gui/$(id -u)/com.remindctl.mcp
```

## Run as macOS service (start at boot)

Use `launchd` so the MCP server starts automatically on login/boot.

Recommended: use the automation script.

```bash
# install/update binary, write plist, load and start service
API_KEY="change-me" AUTH_REQUIRED=true BIND_ADDR=127.0.0.1:8787 ./scripts/install-macos-service.sh
```

Optional variables:

- `SERVICE_LABEL` (default: `com.remindctl.mcp`)
- `BIND_ADDR` (default: `127.0.0.1:8787`)
- `AUTH_REQUIRED` (default: `true`)
- `API_KEY` (required when `AUTH_REQUIRED=true`)
- `REMINDCTL_BIN` (default: `remindctl`)
- `REMINDCTL_READ_TIMEOUT_SECS` (default: `60` in installer)
- `REMINDCTL_WRITE_TIMEOUT_SECS` (default: `20` in installer)

Uninstall:

```bash
./scripts/uninstall-macos-service.sh
```

### Manual setup (if you prefer)

Install the binary into `~/.cargo/bin`, then point the service to that stable path.

1) Build + install/update the binary:

```bash
cargo install --path . --locked --force
# binary path: ~/.cargo/bin/remindctl-mcp
```

2) Create logs directory:

```bash
mkdir -p ~/.openclaw/logs
```

3) Create `~/Library/LaunchAgents/com.remindctl.mcp.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.remindctl.mcp</string>

  <key>ProgramArguments</key>
  <array>
    <string>/Users/YOUR_USER/.cargo/bin/remindctl-mcp</string>
  </array>

  <key>EnvironmentVariables</key>
  <dict>
    <key>BIND_ADDR</key>
    <string>127.0.0.1:8787</string>
    <key>AUTH_REQUIRED</key>
    <string>true</string>
    <key>API_KEY</key>
    <string>change-me</string>
    <key>REMINDCTL_BIN</key>
    <string>remindctl</string>
  </dict>

  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>

  <key>StandardOutPath</key>
  <string>/Users/YOUR_USER/.openclaw/logs/remindctl-mcp.out.log</string>
  <key>StandardErrorPath</key>
  <string>/Users/YOUR_USER/.openclaw/logs/remindctl-mcp.err.log</string>
</dict>
</plist>
```

4) Load and enable it:

```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.remindctl.mcp.plist
launchctl enable gui/$(id -u)/com.remindctl.mcp
launchctl kickstart -k gui/$(id -u)/com.remindctl.mcp
```

5) Verify service + endpoint:

```bash
launchctl print gui/$(id -u)/com.remindctl.mcp
npx mcporter list remindctl
npx mcporter call remindctl.server_health
```

Note: raw `curl` requests to `/mcp` must follow MCP initialization
(`initialize` -> `notifications/initialized` -> tool calls in the same session).
Calling `tools/list` directly without initialize returns:
`Unexpected message, expect initialize request`.

Useful commands:

```bash
# Check status
launchctl print gui/$(id -u)/com.remindctl.mcp

# Restart after binary/env changes
launchctl kickstart -k gui/$(id -u)/com.remindctl.mcp

# Tail logs
tail -f ~/.openclaw/logs/remindctl-mcp.err.log

# Stop and remove service
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.remindctl.mcp.plist
```

Notes:

- Replace `YOUR_USER` and `API_KEY` before loading.
- After code updates, run `cargo install --path . --locked --force` and then `launchctl kickstart -k ...`.
- Prefer `AUTH_REQUIRED=true` for any non-localhost exposure.
- `LaunchAgents` run in the logged-in user session, which is usually what you want for Reminders access.

## OpenClaw + MCPorter setup

This project is intended to be consumed from OpenClaw through `mcporter`.

Install MCPorter (any one):

```bash
npx mcporter list
# or: pnpm add -g mcporter
# or: brew install steipete/tap/mcporter
```

Create `~/.mcporter/mcporter.json` (or `config/mcporter.json` in your workspace):

```json
{
  "mcpServers": {
    "remindctl": {
      "description": "Apple Reminders MCP via remindctl-mcp",
      "baseUrl": "http://127.0.0.1:8787/mcp",
      "headers": {
        "Authorization": "Bearer change-me"
      }
    }
  }
}
```

If `AUTH_REQUIRED=false`, remove the `headers.Authorization` key.

Quick verification with MCPorter:

```bash
npx mcporter list remindctl
npx mcporter call remindctl.server_health
npx mcporter call remindctl.reminders_list filter=pending
```

---

## Remote access option 1: SSH tunnel

Use this when the MCP server runs on your Mac and your client runs on Linux.

### 1) Run server on Mac (localhost only)

```bash
AUTH_REQUIRED=true API_KEY="change-me" BIND_ADDR=127.0.0.1:8787 cargo run --release
```

### 2) Create tunnel from Linux to Mac

```bash
ssh -N -L 8787:127.0.0.1:8787 your-mac-user@your-mac-host
```

Now Linux can reach the MCP endpoint at:

```text
http://127.0.0.1:8787/mcp
```

### 3) Point MCPorter (on Linux) to localhost

Set `baseUrl` in MCPorter config to `http://127.0.0.1:8787/mcp` and keep the same bearer key.

Why this is good:

- no public port exposure
- simple and encrypted
- works without changing firewall/router

---

## Remote access option 2: Tailscale

Use this when both machines are in the same Tailnet.

### 1) Install/login Tailscale on both machines

- Mac host and Linux client must both appear in `tailscale status`.

### 2) Run server on Mac

You can keep localhost + Tailscale SSH, or bind directly for Tailnet access:

```bash
AUTH_REQUIRED=true API_KEY="change-me" BIND_ADDR=0.0.0.0:8787 cargo run --release
```

Then use the Mac Tailnet IP (example `100.x.y.z`).

### 3) Configure MCPorter on Linux

```json
{
  "mcpServers": {
    "remindctl": {
      "description": "Apple Reminders MCP via remindctl-mcp",
      "baseUrl": "http://100.x.y.z:8787/mcp",
      "headers": {
        "Authorization": "Bearer change-me"
      }
    }
  }
}
```

Then verify:

```bash
npx mcporter list remindctl
npx mcporter call remindctl.server_health
```

### Recommended hardening with Tailscale

- Keep `AUTH_REQUIRED=true` (defense in depth)
- Restrict access using Tailnet ACLs to only your Linux node/user
- Do not expose this service through public tunnels/funnels

---

## Behavior notes

- Write operations never use numeric index semantics.
- Short IDs are accepted only when unambiguous.
- `reminder_delete` is idempotent-friendly:
  - can report already-missing refs without failing by default
  - can use recent reminder context when no ID is provided

## OpenClaw skill

This repo includes an OpenClaw-compatible skill at `skill/remindctl-mcp/SKILL.md`.

To use it in OpenClaw:

1. Copy/symlink `skill/` into your OpenClaw workspace as `skills/remindctl-mcp/`.
2. Ensure `mcporter` is installed and your MCPorter config has the `remindctl` server entry.
3. Start a new OpenClaw session (skills are snapshotted at session start).

The skill teaches OpenClaw to use MCPorter for reminders flows with minimal calls and to avoid unrelated filesystem verification.

It also defines an offline-safe queue workflow for OpenClaw:

- queue file: `./.openclaw/{current_workspace}/remindctl-pending-actions.jsonl`
- when MCP is down, write operations are queued instead of retried in tight loops
- queued actions are retried on each heartbeat beat using one batch call (`process_pending_actions`)

This matches OpenClaw's heartbeat-first automation model for long offline windows.

`{current_workspace}` should match the active OpenClaw workspace name/path context.
The queue file must be workspace-scoped so different workspaces do not mix pending actions.

### HEARTBEAT.md instructions (copy/paste once)

Add this block to your OpenClaw `HEARTBEAT.md` one time:

```md
- If `./.openclaw/{current_workspace}/remindctl-pending-actions.jsonl` exists and has entries:
  - Call `remindctl.server_health` once.
  - If unhealthy, keep the queue unchanged and respond with `HEARTBEAT_OK`.
  - If healthy, send queued actions in one call to `remindctl.process_pending_actions`.
  - For each success result, remove that action from queue.
  - For each failure result, increment attempts and keep it queued with last error.
  - If one or more actions were applied, notify the user with a concise summary of what was completed.
  - If no actions were completed in this cycle, keep output concise and respond with `HEARTBEAT_OK`.
```

Important:

- Do not append this block multiple times.
- If the same instructions already exist in `HEARTBEAT.md`, reuse them as-is.
- Preferred flow: ask OpenClaw to add the block only when missing.

## Development checks

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```
