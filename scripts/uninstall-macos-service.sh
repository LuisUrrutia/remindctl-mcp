#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
	echo "error: this script is for macOS only" >&2
	exit 1
fi

SERVICE_LABEL="${SERVICE_LABEL:-com.remindctl.mcp}"
PLIST_PATH="$HOME/Library/LaunchAgents/${SERVICE_LABEL}.plist"

echo "==> unloading service if running"
launchctl bootout "gui/$(id -u)" "$PLIST_PATH" >/dev/null 2>&1 || true

if [[ -f "$PLIST_PATH" ]]; then
	echo "==> removing plist $PLIST_PATH"
	rm -f "$PLIST_PATH"
fi

echo "==> done"
echo "To verify: launchctl print gui/$(id -u)/${SERVICE_LABEL}"
