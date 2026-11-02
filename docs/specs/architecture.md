# Cutaway: Architecture

## Goals

- **Local first.** Desktop on Mac first, Windows second, web later. Large files stay on the user's drives.
- **Non-destructive.** Source audio is never modified.
- **Safe to work in.** Autosave, backups and versioned scripts, so nothing is lost.
- **Model agnostic.** Transcription runs on a local model. AI interpretation runs through a swappable provider, Claude Code for now.
- **Controllable from outside.** Everything the UI can do, a CLI and an MCP client (like Claude) can do too.
- **Show-level memory.** Sound effects, music, room tones and character aliases live at show level and are reused across episodes.

## Naming (central)

The product name is **Cutaway**. It lives in one file, `brand.json` at the repo root, so it can change later without touching code:

| Key | Default | Used for |
|---|---|---|
| `displayName` | `Cutaway` | window title, menus, About, export metadata, MCP server name |
| `slug` | `cutaway` | CLI binary, app-support folder, log prefixes |
| `episodeExt` / `showExt` | `cutaway` / `cutawayshow` | episode and show file extensions |
| `bundleId` | `com.cutaway.app` | macOS/Windows bundle identifier |

- Rust reads it at compile time (`apc-core` build script → `apc_core::brand`).
- The UI imports it (`app/src/lib/brand.ts`).
- Files that cannot import JSON (`tauri.conf.json`, the CLI `[[bin]]` name) are synced by `scripts/rebrand.mjs`; `scripts/check.sh` fails if they drift.
- Internal crate names keep the `apc-` prefix; they never reach the user.

## Stack

| Layer | Technology | Why |
|---|---|---|
| App shell | Tauri | Native app on Mac and Windows, small footprint, web UI reusable online |
| Core | Rust | Real-time audio, large files, one codebase for both platforms, compiles to WebAssembly later |
| UI | TypeScript + Svelte 5, SVG/canvas for waveforms, design tokens in `app/src/theme/` | Fast to iterate, same UI online later, re-skinnable |
| Audio I/O | cpal (output), symphonia (decode) | Native, streams from disk |
| Transcription | Parakeet TDT 0.6B via ONNX Runtime (CoreML on Mac, DirectML/CUDA on Windows); whisper.cpp fallback | Offline, word timestamps, one runtime on both platforms |
| AI interpretation | Provider interface; first implementation: Claude Code CLI | Text tasks need judgment; provider stays swappable |
| Sound sourcing | Provider interface: library search and generation, via API or MCP | Open to Epidemic Sound, ElevenLabs, Magnific, local folders |
| Remote control | MCP server (stdio now, HTTP later) + CLI | Claude and other clients drive the app |
| Storage | SQLite per episode + SQLite library per show + cache files | Robust, transactional, autosave-friendly |

## Layers

```
Clients
  ├ UI (web)            commands in, state out, never touches audio
  ├ CLI (apc)           same commands, headless
  └ MCP server          same commands, for Claude and other MCP clients
        │
Core (Rust)
  ├ Document            episode: scenes, lines, takes, picks, edits, sound placements
  ├ Library             show: sound effects, music, room tones, aliases, tags
  ├ Command bus         every change is a command, logged, undoable (from any client)
  ├ Audio engine        playback, gain, fades, layers, room tone, playhead to UI
  ├ Media               import, sync, peaks, loudness, hashing
  ├ Project I/O         show and episode folders, autosave, backups, script versions
  ├ Job queue           background work, progress, cancel, resume
  └ Export              FCPXML, SRT, WAV/MP3, cut list, cue sheet
Analysis and services (separate process)
  ├ ASR provider        Parakeet ONNX, whisper.cpp
  ├ Aligner             deterministic script matching, take detection
  ├ AI provider         Claude Code CLI now, others later
  ├ Sound providers     search (Epidemic Sound, local), generate (ElevenLabs, Magnific)
  └ Model manager       download once, verify checksum, version
```

Rules:
1. **Audio stays out of the UI.** The audio engine plays straight from disk. The UI only receives the playhead position.
2. **One source of truth, one way in.** UI, CLI and MCP all send the same commands through the command bus. An action from Claude is undoable like any other and shows up live in the open app.
3. **One timeline, several pages.** Options, Arrange and Edit are different displays of the same timeline in the document. Dialogue clips store take, trims and gap to the previous clip (or absolute time when pinned). Every page reads and writes these same values, so an edit on any page shows on all of them.
4. **Analysis is cached and never edited in place.** It is keyed by file hash, model and prompt version. Decisions reference stable IDs (script line, take, source timecode), so re-running analysis never breaks an edit.
5. **Separate analysis process.** A crash or a long job never blocks the editor.
6. **Providers behind interfaces.** ASR, AI, sound search and sound generation can each be swapped without touching the core.

## Show and episode folders

A show is a normal folder with a shared library and one folder per episode. Everything is visible in Finder and Explorer, like a Logic project.

```
TARMAC/                              show
├ TARMAC.cutawayshow                 show file: library database, settings, aliases
├ Library/
│  ├ SFX/
│  │  ├ Doors/  Alarms/  Machines/  Foley/  ...
│  ├ Ambience/                       room tones per location (Canteen, Time Spring Command, ...)
│  └ Music/
├ Episodes/
│  └ Ep02/
│     ├ Ep02.cutaway                 openable episode file (SQLite)
│     ├ Audio/
│     │  └ Session 2026-10-30/
│     │     ├ Paul.wav  Sara.wav  Room.wav
│     ├ Script/
│     │  ├ v001 TARMAC Ep2.fountain
│     │  └ v002 TARMAC Ep2.fountain
│     ├ Backups/
│     ├ Exports/
│     ├ Cache/                       safe to delete, rebuilt on demand
│     └ Logs/
│        └ ai-calls.jsonl
```

- **Standalone episode:** an episode can also exist on its own without a show. It then keeps a local library in the same structure.
- **Opening:** double-click the `.cutaway` file to open an episode, or the `.cutawayshow` file to open the show overview (episodes, library).

**Media handling:**
- **Copy into project (default).** Import copies files in, so the folder is self-contained.
- **Reference in place.** Optional, for very large sessions on another drive.
- **Relink.** Missing files are found again by file hash.
- **Dedup.** Library files are deduplicated by hash: the same effect dropped in twice is stored once.

**Autosave:**
- Every command is written to the database immediately (SQLite WAL), so a crash loses nothing.
- **Backups:** a full snapshot goes to `Backups/` every 10 minutes while changes are happening. The last 20 are kept, plus one per day.
- **Save Version:** creates a named snapshot (e.g. "Director cut 1") that you can return to.
- **Revert:** open any backup or version from a list inside the app.

**Script versioning:**
- Each imported script is stored as a new version (`v001`, `v002`, ...). Old versions are never overwritten.
- When a new version comes in, the app diffs it against the current one:
  - unchanged lines keep their IDs, takes and picks;
  - changed lines are re-matched, and their old picks are kept as suggestions;
  - new lines appear without takes; deleted lines move their takes to the unmatched bin.
- A version switch shows a short change summary before it applies.

## Importer

**Step 1: script.** Import the Throughline/Fountain file. The app parses scenes, locations, characters and lines. *(Changed in implementation, D54: `.thrl` is read natively, not as Fountain; its story context is kept with the episode.)*

**Step 2: audio.** Drop a folder or files. Each file is assigned a role automatically:

| Rule | Example | Result |
|---|---|---|
| File name contains a character name (fuzzy, case-insensitive, with aliases) | `Paul-9min_v001_.wav` | PAUL |
| File name contains a room keyword (room, director, regie, talkback, TB) | `Paul-Room-9min_v001_.wav` | Room track of Paul's session |
| No match by name | `Take_03.wav` | Content check: transcribe, then match against each character's lines; suggest the best character |

Then:
- **Sessions:** files with overlapping timecode are grouped into one session, which is how the room track gets paired with its actor tracks.
- **Confirmation:** one screen shows every file, its role and how sure the app is. The user corrects anything wrong in one click.
- **Aliases:** corrections are remembered as aliases at show level (e.g. "Col" = THE COLONEL), so the next episode imports cleaner.

**Step 3: analysis.** It runs in the background, scene by scene, so the first scene is ready to work on before the whole session is done.

## Sound effects

### Spotting (AI)

- **What it does:** an AI task reads each scene (action lines, dialogue, location) and suggests sound cues. For example, in the TARMAC canteen scene: "aluminum foil burrito set down on table", "canteen ambience", "coffee machine in background".
- **Each cue has:**
  - a description,
  - a scene and a script anchor (the action line or dialogue word it belongs to),
  - a type: hard effect, foley, ambience, walla or music cue,
  - tags,
  - a priority.
- **Ambience cues** link to the location's room tone and ambience at show level.
- **Manual cues:** the user can add, edit, merge or delete cues by hand.

### Bucket

The bucket is the collection of cues and sounds, organised per scene and across the show.

**Wish list.** Cues move through four statuses:
1. **Wanted:** the cue exists, no sound yet.
2. **Candidates:** found or generated sounds are attached.
3. **Chosen:** one sound is picked.
4. **Placed:** it's in the timeline.

**Views:** by scene, by tag, or show-wide. Repeating cues (alarm, Time Spring hum) appear once at show level and are reused in every scene and episode that needs them.

**Library assets** each carry:
- the file,
- tags,
- duration,
- the source: imported, licensed or generated,
- license info and the generation prompt, where relevant.

**Dragging sounds in:**
- **Drop from Finder or Explorer onto the bucket:** the file is copied into `Library/SFX/<category>/` and tagged (from the file name, plus the AI where it's unclear). It is then added to the current scene's bucket, and to the selected cue if there is one.
- **Drop straight onto the timeline:** the same import happens, and the sound is placed where you dropped it.

### Sourcing providers

Sound providers sit behind one open interface with two capabilities:

| Capability | Examples | Result |
|---|---|---|
| Search | Epidemic Sound (API or MCP, if available), local folders, the show library | Candidates attached to a cue |
| Generate | ElevenLabs sound effects, Magnific | Several variations attached as candidates, prompt stored |

- **Connections:** providers connect via API key or as MCP servers. New services plug in without core changes.
- **Licensing:** licensed downloads keep their license info, which feeds the cue sheet export.

### Placement

- **Anchoring:** placed sounds anchor to a script line or word plus an offset. If dialogue timing changes (a take swap or gap change), the effect moves with its line instead of drifting.
- **Unanchoring:** you can unanchor a sound to pin it to absolute time.

## AI tasks

Transcription and alignment are local and deterministic. The AI provider only does interpretation.

| Task | Input | Output |
|---|---|---|
| Line matching (ambiguous cases only) | Transcript window + candidate script lines | Line ID, take type (full, partial, pickup, ad-lib), confidence |
| Room classification | Room segments + scene script | Each segment: cue reading, direction, feedback, chatter |
| Note condensing | Direction and feedback text per take | A few words each, plus tags |
| Pick detection | Feedback text after takes | Pick strength and which take it refers to |
| Sound spotting | Scene script + location | Sound cues with anchor, type, tags, priority |
| Sound tagging | File name + duration (+ cue context) | Category and tags for imported sounds |
| Name matching (fallback) | File names + character list | Role suggestions |

The deterministic aligner handles the clear cases (most of them). Only ambiguous or interpretive work goes to the AI, batched per scene to keep calls few.

## AI provider interface

**The interface:**
- Every AI task has a JSON input, a JSON output schema and a versioned prompt file.
- The core calls `AiProvider.run(task, input) -> output` and validates the result against the schema.
- Invalid output gets one retry, then the task is flagged for manual review.

**First implementation: Claude Code CLI.**
- **Call:** headless (`claude -p`) with the prompt and the input JSON, JSON output requested (`--output-format json`).
- **Model:** set in project settings, default Sonnet 5.5 (`claude-sonnet-5-5`) with medium effort *(changed in implementation: was high, Rob 2026-10-31, D35)*. Settings live in a provider config, not in code:

```toml
[ai]
provider = "claude-code"
model = "claude-sonnet-5-5"
effort = "medium"
batch = "scene"
timeout_s = 300

[sound.search]
providers = ["library", "epidemic"]

[sound.generate]
providers = ["elevenlabs", "magnific"]
variations = 3
```

- **Other providers later:** Anthropic API direct, OpenAI-compatible endpoints, or a local LLM through llama.cpp. They implement the same interface, so no other code changes.
- **Caching:** results are cached by input hash + prompt version + provider + model. Re-opening a project never re-runs AI work that is still valid.
- **Traceability:** every call is logged to `Logs/ai-calls.jsonl` (task, model, input hash, output, duration).
- **Privacy:** only text (transcripts and script) leaves the machine, never dialogue audio. The app says so in settings, and a local LLM provider removes even that.

## MCP server

The app includes an MCP server, so Claude (desktop app or Claude Code) or any other MCP client can operate it.

**Transport:** stdio for local clients now; streamable HTTP for the web version later.

**Tools** mirror the command bus. Examples:

| Area | Tools |
|---|---|
| Project | `open_project`, `project_status`, `list_scenes`, `import_script`, `import_audio`, `confirm_roles` |
| Analysis | `run_transcription`, `run_alignment`, `run_notes`, `job_status` |
| Editing | `list_lines`, `list_takes`, `select_take`, `apply_take_to_passage`, `set_gap`, `trim_line`, `undo` |
| Sound | `spot_sound_effects`, `list_bucket`, `add_cue`, `attach_sound`, `search_sounds`, `generate_sound`, `place_sound` |
| Export | `export_fcpxml`, `export_mix`, `export_srt`, `export_cut_list` |

**Resources** are read-only views of the project: script versions, the current cut list, the scene bucket, the match report.

**Safety:**
- MCP writes go through the command bus: logged, undoable, and visible live in the open app.
- Destructive actions (deleting library files, overwriting exports) need confirmation in the app.

This is the opposite direction from the AI provider. The AI provider is the app calling an AI for help; the MCP server is an AI client driving the app. Both can be active at the same time.

## Command-line interface

Every pipeline step is available headless. The CLI, the MCP server and the desktop app share the same core.

```
cutaway show new "TARMAC"                      create show folder with library
cutaway new "Ep02" --show TARMAC               create episode inside the show
cutaway import-script script.fountain          add a new script version
cutaway import-audio ./session --copy          import and auto-assign roles
cutaway roles                                  show/confirm the file-to-role mapping
cutaway transcribe                             run the local ASR (downloads the model once)
cutaway align                                  deterministic matching + AI for ambiguous parts
cutaway notes                                  room classification, note condensing, picks
cutaway sfx spot --scene S2                    AI sound spotting into the bucket
cutaway sfx import ./sounds                    add files to the show library
cutaway export fcpxml --scene S1               export a rough cut
cutaway status                                 progress, match rate, flagged items
cutaway mcp                                    run as MCP server (stdio)
```

## Model manager

- **Download:** models are downloaded on first use, with progress and checksum verification.
- **Storage:** in the app-support folder, shared by all projects. Each project records which model version it used.
- **Offline:** after the first download, everything except AI interpretation and online sound providers runs offline.

## Path to web

- The document model, aligner and export are pure Rust and compile to WebAssembly.
- Online, the same UI talks to a server running the analysis engine and plays streamed proxy audio in the browser.
- The show folder maps to cloud storage with the same structure. The MCP server switches to HTTP transport.

## Build order

1. **Core CLI:** episode folder, importer with role matching, Parakeet transcription, aligner, Claude Code provider, FCPXML export. Test: the TARMAC session opens correctly in Resolve.
2. **MCP server** on top of the CLI commands, so Claude can run and inspect the pipeline early.
3. **Audio engine + minimal UI:** play the scene, flip takes, autosave and backups working.
4. **Designed UI** per the frontend spec: Options, Arrange, box editor.
5. **Edit page, show library and sound:** full multitrack Edit page (dialogue editable), show folder, bucket, AI spotting, Finder drag-in, providers.
6. **Room tone and music lanes.**
7. **Windows build.**
8. **Web.**
