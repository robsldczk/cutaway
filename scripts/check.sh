#!/usr/bin/env bash
# Everything that must be green before a commit: brand sync, fmt, clippy, tests, UI lint and typecheck.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "build output: keep target/ under its limit"
scripts/clean-target.sh || true

step "tool versions"
node -e '
const fs = require("fs");
const want = fs.readFileSync(".nvmrc", "utf8").trim().split(".")[0];
const engines = require("./app/package.json").engines.node;
if (engines !== `>=${want}`) { console.error(`app/package.json engines.node is ${engines}, .nvmrc says ${want}. Make them agree.`); process.exit(1); }
const have = process.versions.node.split(".")[0];
if (+have < +want) { console.error(`Node ${want} or newer is needed (.nvmrc). This is ${process.version}.`); process.exit(1); }
console.log(`node ${process.version}, needs ${want}+`);
'
rustc --version

step "brand"
node scripts/rebrand.mjs --check

step "licenses"
node scripts/licenses.mjs --check

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
