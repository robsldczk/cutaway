# Cutaway

Script-matched rough cuts from unslated audio play sessions: one WAV per role, a director room mic and the script go in; a cut you can flip through take by take comes out, with exports to Resolve and Logic.

The product name lives in `brand.json` (see `docs/specs/architecture.md`, *Naming*). Run `node scripts/rebrand.mjs` after changing it.

## Quick start

```sh
scripts/check.sh                                  # fmt, clippy, tests, UI lint and typecheck
cargo run -p apc-cli -- --help                    # the `cutaway` CLI
cargo run -p apc-cli -- demo /tmp/cutaway-demo --analyze   # a synthetic session to play with
cd app && npm install && npm run tauri dev        # the desktop app
```

Headless pipeline on a real session:

```sh
cutaway new Ep02 --parent ~/Shows               # or: cutaway show new TARMAC, then cutaway new Ep02 --show ~/Shows/TARMAC
export CUTAWAY_PROJECT=~/Shows/Ep02
cutaway import-script tarmac-episode-2.fountain
cutaway import-audio ./session                  # copies by default; --reference keeps files in place
cutaway roles                                   # check; correct with --set src_x=character:PAUL
cutaway align                                   # sync, matching, takes, notes, picks
cutaway report                                  # match report (Markdown)
cutaway export fcpxml                           # Exports/Ep02.fcpxml for Resolve / Logic
```

Local Parakeet transcription is behind a cargo feature: `cargo run -p apc-cli --features parakeet -- align` (downloads the model, ~490 MB, on first use). Without it, transcripts are read from `<audio>.json` sidecars next to each file.

AI interpretation (notes, picks, spotting) uses the Claude Code CLI (`claude`) when it is installed, otherwise built-in rules. `CUTAWAY_AI_PROVIDER=mock` forces the rules.

## Using Cutaway from Claude (MCP)

`cutaway mcp` runs an MCP server on stdio. Every tool goes through the same command bus as the app, so actions are logged, undoable (`undo`) and appear live in an open window. Destructive tools (`revert`, `delete_cue`, `delete_asset`) need `"confirm": true`.

Build the binary once: `cargo build --release -p apc-cli` (it lands in `target/release/cutaway`).

**Claude Code:**

```sh
claude mcp add cutaway -- /path/to/cutaway/target/release/cutaway mcp --project ~/Shows/Ep02
```

**Claude Desktop** (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "cutaway": {
      "command": "/path/to/cutaway/target/release/cutaway",
      "args": ["mcp", "--project", "/Users/you/Shows/Ep02"]
    }
  }
}
```

Without `--project`, start with the `open_project` or `create_episode` tool. Resources: `cutaway://script/{n}`, `cutaway://cutlist`, `cutaway://bucket/{scene}`, `cutaway://report`.

## What's in here

| Path | Purpose |
|---|---|
| `brand.json` | Product names (one place) |
| `crates/` | Rust workspace: `apc-core` (document, commands, undo, project I/O), `apc-media`, `apc-audio`, `apc-analysis`, `apc-ai`, `apc-sound`, `apc-export`, `apc-app` (shared service), `apc-cli` (`cutaway`), `apc-mcp` |
| `app/` | Tauri 2 + Svelte 5 desktop app; design tokens in `app/src/theme/` |
| `docs/specs/` | Architecture, pipeline, frontend design |
| `docs/design/prototype/` | The Option C clickable prototype the UI follows |
| `docs/PLAN.md`, `docs/DECISIONS.md`, `docs/RUN_REPORT.md` | Plan, decisions, run report |
| `docs/reference/stage0/` | Python prototype and its match report |
| `fixtures/` | Test sessions (local only, gitignored) |
