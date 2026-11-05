#!/usr/bin/env bash
# Runs scripts/clean-target.sh every hour through a macOS LaunchAgent, for this user.
# Logs to ~/Library/Logs/cutaway-cleaner.log.
#
# Usage: scripts/install-cleaner.sh [--uninstall]
set -euo pipefail

label="com.robsladeczek.cutaway-cleaner"
plist="$HOME/Library/LaunchAgents/$label.plist"
log="$HOME/Library/Logs/cutaway-cleaner.log"
domain="gui/$(id -u)"
script="$(cd "$(dirname "$0")" && pwd)/clean-target.sh"

launchctl bootout "$domain/$label" 2>/dev/null || true

if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$plist"
  echo "Removed $label"
  exit 0
fi

cat >"$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$label</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>$script</string>
  </array>
  <key>StartInterval</key>
  <integer>3600</integer>
  <key>ProcessType</key>
  <string>Background</string>
  <key>StandardOutPath</key>
  <string>$log</string>
  <key>StandardErrorPath</key>
  <string>$log</string>
</dict>
</plist>
EOF

launchctl bootstrap "$domain" "$plist"
echo "Installed $label, running hourly. Log: $log"
echo "Run it now: launchctl kickstart $domain/$label"
