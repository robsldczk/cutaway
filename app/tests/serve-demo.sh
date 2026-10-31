#!/usr/bin/env bash
# Creates a fresh demo episode and serves it over the HTTP bridge for the UI tests.
set -euo pipefail
cd "$(dirname "$0")/../.."
DIR="$(mktemp -d)/cutaway-e2e"
export CUTAWAY_AI_PROVIDER=mock CUTAWAY_NO_AUDIO=1
cargo build -q -p apc-cli
FILE=$(./target/debug/cutaway demo "$DIR" --analyze | sed -n 's/.*"file": "\(.*\)".*/\1/p')
exec ./target/debug/cutaway --project "$FILE" serve --addr 127.0.0.1:7358
