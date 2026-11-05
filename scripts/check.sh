#!/usr/bin/env bash
# Everything that must be green before a commit: brand sync, fmt, clippy, tests, UI lint and typecheck.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "build output: keep target/ under its limit"
scripts/clean-target.sh || true

step "brand"
node scripts/rebrand.mjs --check

step "ui: install"
if [ ! -d app/node_modules ]; then (cd app && npm ci); fi

step "ui: build (Tauri embeds app/dist)"
(cd app && npm run build --silent)

step "cargo fmt"
cargo fmt --all -- --check

step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "cargo test"
cargo test --workspace

step "ui: unit tests"
(cd app && npm run test --silent)

step "ui: lint"
(cd app && npm run lint --silent)

step "ui: typecheck"
(cd app && npm run check --silent)

printf '\n\033[1;32mcheck.sh: all green\033[0m\n'
