# Cutaway

Script-matched rough cuts from unslated audio play sessions: one WAV per role, a director room mic and the script go in; a cut you can flip through take by take comes out, with exports to Resolve and Logic.

The product name lives in `brand.json` (see `docs/specs/architecture.md`, *Naming*). Run `node scripts/rebrand.mjs` after changing it.

## Quick start

### What you need

| Tool | Version | Notes |
|---|---|---|
| Rust | Set in `rust-toolchain.toml`, through [rustup](https://rustup.rs) | rustup downloads it at the first build |
| Node.js | 22 or newer (`.nvmrc`) | CI uses 22 |
| Tauri's system dependencies | | macOS: Xcode Command Line Tools (`xcode-select --install`). Windows: Microsoft C++ Build Tools and WebView2. See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) |

Cutaway runs on macOS and Windows; CI builds and tests both. Linux is not a target.

The first build needs a network connection: it downloads the Rust crates and the ONNX Runtime used for speech recognition.

### Run the app

Get the code:

```sh
git clone https://github.com/robsldczk/cutaway.git
```

Open a terminal in the `cutaway` folder. If you opened the folder in VS Code, its terminal is already there. Then run:

```sh
cd app
npm ci
npm run tauri dev
```

The npm commands only work in `app/`: the repo root has no `package.json`. Use `npm ci`, not `npm install`: it installs exactly what the lockfile says and never changes it.

The first build compiles the whole Rust workspace, which takes about 2 minutes on a recent Apple Silicon Mac and longer on slower machines. Later builds are incremental.

The app opens on the welcome screen. To try it without any recordings, click **Demo session** and choose a folder: it builds a synthetic episode, analyzes it and opens it. Once the welcome screen lists projects, the same option is at **Show → Demo Session…**.

You can also make the demo from the command line and open it directly. Run the first command from the repo root:

```sh
cd ..           # back to the repo root, if you are still in app/
cargo run -p apc-cli -- demo /tmp/cutaway-demo --analyze
cd app && CUTAWAY_PROJECT=/tmp/cutaway-demo/Demo/Demo.cutaway npm run tauri dev
```

Keep `--analyze`: without it the episode has no takes, and the take controls stay empty.

### Optional extras

The app runs without any of these. Each one adds a feature.

| Extra | What it adds | Without it |
|---|---|---|
| [ffmpeg](https://ffmpeg.org) on your `PATH` | MP3 and FLAC exports | Exports are WAV only |
| [Claude Code](https://claude.com/claude-code) (`claude` on your `PATH`) | AI notes, picks and spotting | Built-in rules. `CUTAWAY_AI_PROVIDER=mock` forces them |
| Parakeet speech model (~490 MB) | Local transcription of your own recordings | Nothing to set up: it downloads once, the first time an analysis needs it. Files with `<audio>.json` transcript sidecars skip it, and the demo doesn't use it |
| Epidemic Sound and ElevenLabs API keys | Sound search and generation | Those services stay off. Add keys in Settings → Connections, or set `EPIDEMIC_SOUND_API_KEY` and `ELEVENLABS_API_KEY` |

### Check your setup

```sh
scripts/check.sh    # fmt, clippy, tests, UI lint and typecheck (on Windows, run it from Git Bash)
```

`fixtures/` holds unreleased production material and isn't in the repo, so a fresh clone has no test recordings. Tests that need them skip with a message.

### Use it from the command line

Build the `cutaway` CLI and put it on your `PATH`. Run this from the repo root:

```sh
cargo build --release -p apc-cli
export PATH="$PWD/target/release:$PATH"
cutaway --help
```

On a real session:

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

To build without speech recognition, and so without the ONNX Runtime download: `cargo build -p apc-cli --no-default-features`. Transcripts then come only from the `<audio>.json` sidecars.

### If something goes wrong

- **`cutaway` opens the desktop app instead of the CLI (macOS):** `tauri build` writes `Cutaway` into the same `target/` folder as the CLI's `cutaway`, and the default macOS disk ignores case. Run `rm target/debug/cutaway && cargo build -p apc-cli`, using `target/release` and `--release` if you built a release. `npm run tauri dev` doesn't cause this.
- **The first build hangs or fails while fetching `sherpa-onnx`:** that's the ONNX Runtime download. Check your network and build again.

### Start it from the Dock (macOS)

One script makes a Dock app that builds the latest Cutaway each time you open it:

```sh
scripts/dock-app.sh      # installs ~/Applications/App It/Cutaway Latest.app
```

Drag it to the Dock. Each open pulls `dev` and builds if the code changed, then starts Cutaway. If nothing changed, Cutaway opens at once. If Cutaway is already open, it comes to the front without a build. The first open takes several minutes.

The pull only runs on `dev` with no local changes. Otherwise a notification says so, and the Dock app uses the code on disk.

| Command, from the repo root | What it does |
|---|---|
| `scripts/dock-app.sh --doctor` | Reports what is installed and built |
| `scripts/dock-app.sh --build` | Updates and builds now, without opening |
| `scripts/dock-app.sh --quit` | Quits Cutaway |
| `scripts/dock-app.sh --background` | Also rebuilds every 15 minutes while Cutaway is closed, so opens don't wait. `--no-background` stops it |

The same commands are `npm run desktop:install`, `desktop:build`, `desktop:doctor` and `desktop:quit` in `app/`. The build log is `~/Library/Logs/app-it/cutaway/build.log`. Run `scripts/dock-app.sh` again if you move the repo.

The Dock app builds into `target/dock/`, so it can't overwrite the `cutaway` CLI (see *If something goes wrong*).

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
