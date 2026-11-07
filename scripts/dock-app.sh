#!/usr/bin/env bash
# "Cutaway Latest.app": a Dock app that opens the latest Cutaway. Each open builds the checkout
# if the code changed, then starts the app. macOS only. The repo path is baked in, so run this
# again if the repo moves.
#
#   scripts/dock-app.sh                  install the Dock app
#   scripts/dock-app.sh --build          update and build now, without opening
#   scripts/dock-app.sh --doctor         report what is installed and built
#   scripts/dock-app.sh --quit           quit Cutaway
#   scripts/dock-app.sh --background     also rebuild every 15 minutes (LaunchAgent)
#   scripts/dock-app.sh --no-background  stop that
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NAME="Cutaway Latest"
DEST="${APP_IT_DEST:-$HOME/Applications/App It}"
APP="$DEST/$NAME.app"
ICON="$ROOT/app/src-tauri/icons/icon.icns"
BUNDLE_ID="$(node -p "require('$ROOT/brand.json').bundleId")"
DOCK_ID="com.user.cutaway-latest"
# Own target folder: on a case-insensitive disk, target/release/Cutaway would replace the CLI's
# target/release/cutaway. Keep it in sync with the wipe in clean-target.sh.
TARGET="$ROOT/target/dock"
BUILT="$TARGET/release/bundle/macos/Cutaway.app"
AGENT="$HOME/Library/LaunchAgents/$DOCK_ID.update.plist"
LOG="$HOME/Library/Logs/app-it/cutaway/build.log"

[ "$(uname)" = Darwin ] || { echo "macOS only" >&2; exit 1; }

install_app() {
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
  <key>CFBundleIdentifier</key><string>$DOCK_ID</string>
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
TARGET="$TARGET"
BUILT="$BUILT"
BUNDLE_ID="$BUNDLE_ID"
LOG="$LOG"
RUN
  cat >> "$APP/Contents/MacOS/run" <<'RUN'
UPDATE_ONLY=0; [ "${1:-}" = --update ] && UPDATE_ONLY=1
export PATH="$HOME/.cargo/bin:$HOME/.volta/bin:$HOME/.bun/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
# Dock launches start with a bare PATH, so /usr/local/bin may hold an old Node. Prefer the newest nvm Node.
NV=$(ls -d "$HOME"/.nvm/versions/node/v* 2>/dev/null | sort -V | tail -1)
[ -n "$NV" ] && export PATH="$NV/bin:$PATH"
export CARGO_TARGET_DIR="$TARGET"
mkdir -p "$(dirname "$LOG")"
LOCK="$(dirname "$LOG")/build.pid"
STAMP="$TARGET/built-commit"

say() { /usr/bin/osascript -e "display notification \"$1\" with title \"Cutaway Latest\"" >/dev/null 2>&1 || true; }
fail() {
  echo "FAILED: $1" >>"$LOG"
  if [ "$UPDATE_ONLY" = 1 ]; then say "Update failed: $1"; exit 1; fi
  /usr/bin/osascript -e "display alert \"Cutaway could not build\" message \"$1 Full log: $LOG\" as critical" >/dev/null 2>&1 || true
  exit 1
}
running() { /usr/bin/pgrep -f "Cutaway.app/Contents/MacOS/Cutaway" >/dev/null; }
clean_tree() { [ -z "$(git status --porcelain)" ]; }

# Already running: a build would replace the app under an open session.
if running; then
  [ "$UPDATE_ONLY" = 1 ] && exit 0
  /usr/bin/osascript -e "tell application id \"$BUNDLE_ID\" to activate" >/dev/null 2>&1
  exit 0
fi

[ -d "$ROOT" ] || fail "The repo moved. Run scripts/dock-app.sh again."
if [ -f "$LOCK" ] && kill -0 "$(cat "$LOCK")" 2>/dev/null; then
  [ "$UPDATE_ONLY" = 1 ] || say "A build is already running."
  exit 0
fi
echo $$ > "$LOCK"
trap 'rm -f "$LOCK"' EXIT

cd "$ROOT" || fail "Cannot open $ROOT."

# Pull only when it cannot touch local work: on dev, clean tree.
BRANCH="$(git rev-parse --abbrev-ref HEAD 2>/dev/null)"
if [ "$BRANCH" != dev ]; then
  say "On branch $BRANCH, so no pull. Using this code."
elif ! clean_tree; then
  say "Local changes, so no pull. Using this code."
elif ! PULL="$(git pull --ff-only 2>&1)"; then
  echo "$PULL" >>"$LOG"
  say "Could not pull. Using the code on this Mac."
fi

# Nothing changed since the last build: skip it.
HEAD_NOW="$(git rev-parse HEAD)"
if [ -d "$BUILT" ] && clean_tree && [ "$(cat "$STAMP" 2>/dev/null)" = "$HEAD_NOW" ]; then
  [ "$UPDATE_ONLY" = 1 ] || /usr/bin/open "$BUILT"
  exit 0
fi

: > "$LOG"
[ "$UPDATE_ONLY" = 1 ] || say "Building the latest Cutaway…"
command -v cargo >/dev/null || fail "Rust is missing. Install it from rustup.rs."
command -v node  >/dev/null || fail "Node.js is missing."
NEED="$(cut -d. -f1 "$ROOT/.nvmrc" 2>/dev/null)"
[ "$(node -p 'process.versions.node.split(".")[0]')" -ge "${NEED:-22}" ] || fail "Node ${NEED:-22} or newer is needed. Found $(node -v)."
cd "$ROOT/app" || fail "app/ is missing."
[ -d node_modules ] || npm ci >>"$LOG" 2>&1 || fail "npm ci failed."
rm -f "$STAMP"
npm run tauri build -- --bundles app >>"$LOG" 2>&1 || fail "The build failed."

[ -d "$BUILT" ] || fail "The build finished but $BUILT is missing."
/usr/bin/xattr -cr "$BUILT" 2>/dev/null || true
cd "$ROOT" || fail "Cannot open $ROOT."
if clean_tree; then echo "$HEAD_NOW" > "$STAMP"; fi
[ "$UPDATE_ONLY" = 1 ] || /usr/bin/open "$BUILT"
RUN
  chmod +x "$APP/Contents/MacOS/run"
  /usr/bin/xattr -cr "$APP" 2>/dev/null || true
  /usr/bin/codesign --force --deep --sign - "$APP" >/dev/null
  echo "Installed: $APP"
}

need_app() { [ -x "$APP/Contents/MacOS/run" ] || { echo "Not installed. Run scripts/dock-app.sh first." >&2; exit 1; }; }

doctor() {
  local bad=0 head stamp
  row() { printf '%-6s %s\n' "$1" "$2"; [ "$1" != fail ] || bad=1; }
  if [ -x "$APP/Contents/MacOS/run" ]; then row ok "Dock app: $APP"; else row fail "Dock app is not installed"; fi
  if grep -qs "ROOT=\"$ROOT\"" "$APP/Contents/MacOS/run"; then row ok "Dock app points at this repo"; else row fail "Dock app points elsewhere. Run scripts/dock-app.sh"; fi
  head="$(git -C "$ROOT" rev-parse HEAD)"
  stamp="$(cat "$TARGET/built-commit" 2>/dev/null || true)"
  if [ ! -d "$BUILT" ]; then row warn "No build yet. The next open builds it"
  elif [ "$stamp" = "$head" ]; then row ok "Build matches ${head:0:7}"
  else row warn "Build is not ${head:0:7}. The next open rebuilds it"; fi
  if [ -z "$(git -C "$ROOT" status --porcelain)" ]; then row ok "Checkout is clean"; else row warn "Local changes: opens build them and skip the pull"; fi
  if [ -f "$AGENT" ]; then row ok "Background rebuild is on"; else row ok "Background rebuild is off"; fi
  if [ -f "$LOG" ]; then row ok "Build log: $LOG"; fi
  return "$bad"
}

background_on() {
  need_app
  mkdir -p "$(dirname "$AGENT")"
  cat > "$AGENT" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>$DOCK_ID.update</string>
  <key>ProgramArguments</key><array><string>$APP/Contents/MacOS/run</string><string>--update</string></array>
  <key>StartInterval</key><integer>900</integer>
  <key>ProcessType</key><string>Background</string>
</dict></plist>
PLIST
  launchctl bootout "gui/$(id -u)/$DOCK_ID.update" 2>/dev/null || true
  launchctl bootstrap "gui/$(id -u)" "$AGENT"
  echo "Background rebuild is on. It runs every 15 minutes while Cutaway is closed."
}

background_off() {
  launchctl bootout "gui/$(id -u)/$DOCK_ID.update" 2>/dev/null || true
  rm -f "$AGENT"
  echo "Background rebuild is off."
}

case "${1:-}" in
  '') install_app ;;
  --build) need_app; "$APP/Contents/MacOS/run" --update ;;
  --doctor) doctor ;;
  --quit) /usr/bin/osascript -e "tell application id \"$BUNDLE_ID\" to quit" ;;
  --background) background_on ;;
  --no-background) background_off ;;
  -h | --help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//' ;;
  *) echo "unknown option: $1" >&2; exit 2 ;;
esac
