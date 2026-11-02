#!/usr/bin/env bash
# Creates a fresh demo episode and serves it over the HTTP bridge for the UI tests.
set -euo pipefail
cd "$(dirname "$0")/../.."
DIR="$(mktemp -d)/cutaway-e2e"
export CUTAWAY_AI_PROVIDER=mock CUTAWAY_NO_AUDIO=1
# show library indexes go next to the demo, not into the real app data
export CUTAWAY_DATA_DIR="$(dirname "$DIR")/data"
# keys in memory (never the real keychain), Epidemic Sound from the stand-in
export CUTAWAY_SECRETS=memory CUTAWAY_EPIDEMIC_URL=http://127.0.0.1:7359
unset EPIDEMIC_SOUND_API_KEY ELEVENLABS_API_KEY
cargo build -q -p apc-cli
FILE=$(./target/debug/cutaway demo "$DIR" --analyze | sed -n 's/.*"file": "\(.*\)".*/\1/p')
exec ./target/debug/cutaway --project "$FILE" serve --addr 127.0.0.1:7358
