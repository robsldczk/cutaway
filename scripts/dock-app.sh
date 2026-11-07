#!/usr/bin/env bash
# Make "Cutaway Latest.app": a Dock app that builds the current checkout on every open, then starts it.
# macOS only. The repo path is baked in at install time. Run again if the repo moves.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NAME="Cutaway Latest"
DEST="${APP_IT_DEST:-$HOME/Applications/App It}"
APP="$DEST/$NAME.app"
ICON="$ROOT/app/src-tauri/icons/icon.icns"

[ "$(uname)" = Darwin ] || { echo "macOS only" >&2; exit 1; }
[ -f "$ICON" ] || { echo "missing icon: $ICON" >&2; exit 1; }

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ICON" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>com.user.cutaway-latest</string>
  <key>CFBundleExecutable</key><string>run</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>LSUIElement</key><true/>
</dict></plist>
PLIST

cat > "$APP/Contents/MacOS/run" <<RUN
#!/bin/bash
ROOT="$ROOT"
RUN
cat >> "$APP/Contents/MacOS/run" <<'RUN'
export PATH="$HOME/.cargo/bin:$HOME/.volta/bin:$HOME/.bun/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
# Dock launches start with a bare PATH, so /usr/local/bin may hold an old Node. Prefer the newest nvm Node.
NV=$(ls -d "$HOME"/.nvm/versions/node/v* 2>/dev/null | sort -t. -k1.2,1n -k2,2n -k3,3n | tail -1)
[ -n "$NV" ] && export PATH="$NV/bin:$PATH"
LOGDIR="$HOME/Library/Logs/app-it/cutaway"; LOG="$LOGDIR/build.log"; mkdir -p "$LOGDIR"
BUILT="$ROOT/target/release/bundle/macos/Cutaway.app"
LOCK="${TMPDIR:-/tmp}/cutaway-latest.lock"

say()  { /usr/bin/osascript -e "display notification \"$1\" with title \"Cutaway Latest\"" >/dev/null 2>&1 || true; }
fail() {
  tail -n 15 "$LOG" > "$LOGDIR/tail.txt" 2>/dev/null
  /usr/bin/osascript -e "display alert \"Cutaway could not build\" message \"$1 Full log: $LOG\" as critical" >/dev/null 2>&1 || true
  exit 1
}

# Already running: bring it forward. A rebuild would replace the binary under it.
if /usr/bin/pgrep -f "Cutaway.app/Contents/MacOS/Cutaway" >/dev/null; then
  /usr/bin/open "$BUILT"; exit 0
fi

[ -d "$ROOT" ] || fail "The repo moved. Run scripts/dock-app.sh again."
if ! mkdir "$LOCK" 2>/dev/null; then say "A build is already running."; exit 0; fi
trap 'rmdir "$LOCK"' EXIT

cd "$ROOT" || fail "Cannot open $ROOT."
: > "$LOG"
say "Building the latest Cutaway…"

# Pull only when it cannot touch local work: on dev, clean tree.
if [ "$(git rev-parse --abbrev-ref HEAD 2>/dev/null)" = dev ] && [ -z "$(git status --porcelain)" ]; then
  git pull --ff-only >>"$LOG" 2>&1 || echo "pull skipped (offline or diverged)" >>"$LOG"
fi

command -v cargo >/dev/null || fail "Rust is missing. Install it from rustup.rs."
command -v node  >/dev/null || fail "Node.js is missing."
[ "$(node -p 'process.versions.node.split(".")[0]')" -ge 22 ] || fail "Node 22 or newer is needed. Found $(node -v)."
cd "$ROOT/app" || fail "app/ is missing."
[ -d node_modules ] || npm ci >>"$LOG" 2>&1 || fail "npm ci failed."
npm run tauri build -- --bundles app >>"$LOG" 2>&1 || fail "The build failed."

[ -d "$BUILT" ] || fail "The build finished but $BUILT is missing."
/usr/bin/xattr -cr "$BUILT" 2>/dev/null || true
/usr/bin/open "$BUILT"
RUN
chmod +x "$APP/Contents/MacOS/run"
/usr/bin/xattr -cr "$APP" 2>/dev/null || true
/usr/bin/codesign --force --deep --sign - "$APP" >/dev/null
echo "Installed: $APP"
