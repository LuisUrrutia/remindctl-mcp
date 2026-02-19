#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
	echo "error: this installer is for macOS only" >&2
	exit 1
fi

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SERVICE_LABEL="${SERVICE_LABEL:-com.remindctl.mcp}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:8787}"
AUTH_REQUIRED="${AUTH_REQUIRED:-true}"
REMINDCTL_BIN="${REMINDCTL_BIN:-remindctl}"
REMINDCTL_READ_TIMEOUT_SECS="${REMINDCTL_READ_TIMEOUT_SECS:-60}"
REMINDCTL_WRITE_TIMEOUT_SECS="${REMINDCTL_WRITE_TIMEOUT_SECS:-20}"
API_KEY="${API_KEY:-}"
LAUNCH_AGENTS_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$LAUNCH_AGENTS_DIR/${SERVICE_LABEL}.plist"
LOG_DIR="$HOME/.openclaw/logs"
OUT_LOG="$LOG_DIR/remindctl-mcp.out.log"
ERR_LOG="$LOG_DIR/remindctl-mcp.err.log"

if [[ "$AUTH_REQUIRED" == "true" && -z "$API_KEY" ]]; then
	echo "error: API_KEY is required when AUTH_REQUIRED=true" >&2
	exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
	echo "error: cargo is required" >&2
	exit 1
fi

if ! command -v launchctl >/dev/null 2>&1; then
	echo "error: launchctl is required" >&2
	exit 1
fi

echo "==> installing remindctl-mcp binary"
cargo install --path "$ROOT_DIR" --locked --force

BIN_PATH="$HOME/.cargo/bin/remindctl-mcp"
if [[ ! -x "$BIN_PATH" ]]; then
	echo "error: expected binary not found at $BIN_PATH" >&2
	exit 1
fi

echo "==> preparing directories"
mkdir -p "$LAUNCH_AGENTS_DIR" "$LOG_DIR"

echo "==> writing launchd plist: $PLIST_PATH"
cat >"$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${SERVICE_LABEL}</string>

  <key>ProgramArguments</key>
  <array>
    <string>${BIN_PATH}</string>
  </array>

  <key>EnvironmentVariables</key>
  <dict>
    <key>BIND_ADDR</key>
    <string>${BIND_ADDR}</string>
    <key>AUTH_REQUIRED</key>
    <string>${AUTH_REQUIRED}</string>
    <key>API_KEY</key>
    <string>${API_KEY}</string>
    <key>REMINDCTL_BIN</key>
    <string>${REMINDCTL_BIN}</string>
    <key>REMINDCTL_READ_TIMEOUT_SECS</key>
    <string>${REMINDCTL_READ_TIMEOUT_SECS}</string>
    <key>REMINDCTL_WRITE_TIMEOUT_SECS</key>
    <string>${REMINDCTL_WRITE_TIMEOUT_SECS}</string>
  </dict>

  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>

  <key>StandardOutPath</key>
  <string>${OUT_LOG}</string>
  <key>StandardErrorPath</key>
  <string>${ERR_LOG}</string>
</dict>
</plist>
EOF

echo "==> reloading service"
launchctl bootout "gui/$(id -u)" "$PLIST_PATH" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
launchctl enable "gui/$(id -u)/${SERVICE_LABEL}"
launchctl kickstart -k "gui/$(id -u)/${SERVICE_LABEL}"

echo "==> done"
echo "service: ${SERVICE_LABEL}"
echo "plist:   ${PLIST_PATH}"
echo "logs:    ${OUT_LOG} / ${ERR_LOG}"
echo "status:  launchctl print gui/$(id -u)/${SERVICE_LABEL}"
