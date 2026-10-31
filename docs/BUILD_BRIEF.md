# Build Brief

## Scope of this run

Build the Cutaway desktop app on macOS from an empty repo to a working app, following the three specs in `docs/specs/`. Implement milestones M0 to M8 in order.

- **Done means green.** Each milestone is done when its acceptance checks pass. Then commit, push to `dev`, update `docs/PLAN.md`, and move on.
- **Out of scope for this run:**
  - the web version,
  - AAF export,
  - waveform-precise cutting in the box editor,
  - modifier-key cut modes,
  - the cue-reading timing template.

If time runs out, `docs/PLAN.md` and `docs/RUN_REPORT.md` must make it obvious where the next run continues.

## Phase 1: plan (before writing code)

Write `docs/PLAN.md` containing:
1. **Crates and app modules:** each one's responsibility and its public interface (main types and traits).
2. **Schemas:**
   - the SQLite schema for the episode `.cutaway` and the show `.cutawayshow`,
   - the command list (every command, with its parameters and its undo).
3. **Interfaces:**
   - the AI task list with input/output JSON schemas,
   - the provider interfaces: `AsrProvider`, `AiProvider`, `SoundSearchProvider`, `SoundGenerateProvider`.
4. **Tasks:** every milestone below broken into checkbox tasks.
5. **Risks:** a risk list with a fallback for each.
6. **Progress log:** an empty section at the end.

Commit as `docs: implementation plan`, push to `dev`, then start M0 immediately.

## Milestones

### M0: Repository and tooling
- Rust workspace with the crates listed in CLAUDE.md, a Tauri app skeleton in `app/`, and `scripts/check.sh`.
- `.gitignore` covers `fixtures/*` (except the README), `target/`, `node_modules/`, `.env` and `Cache/`.
- GitHub Actions workflow running `scripts/check.sh` on macOS and Windows runners. Windows may be allowed to fail until M8.
- **Accept:** `scripts/check.sh` passes; the `dev` branch is pushed.

### M1: Core document and project folders
- Show and episode folders exactly as in the architecture spec:
  - `.cutawayshow` and `.cutaway` files as SQLite (WAL),
  - the folders `Audio/`, `Script/`, `Backups/`, `Exports/`, `Cache/`, `Logs/` and `Library/`.
- Command bus with an undo/redo log; every command persisted immediately.
- Autosave backups: a snapshot every 10 minutes while changes happen; keep the last 20 plus one per day. Named versions; revert.
- Script versioning:
  - import a new version as `vNNN <name>.fountain`,
  - diff it against the current version, keeping line IDs for unchanged lines,
  - move takes of deleted lines to the unmatched bin,
  - produce a change summary.
- Media import: copy into the project by default (reference optional), hash files, relink missing files by hash.
- **Accept:** unit tests for undo/redo, autosave rotation, script diff (edit a line, delete a line, insert a line), and relink.

### M2: Import and analysis pipeline
- **Fountain parser** that handles the Throughline file in fixtures:
  - title page and the `===` page break,
  - scene headings,
  - character cues with extensions like `(O.S.)`,
  - parentheticals splitting a dialogue block into sub-lines,
  - forced elements `!`, `>` and `--` action lines.
- **Role matching** by file name: character names and aliases, room keywords (room, director, regie, talkback, TB). Content-based fallback.
- **Sessions:** files grouped into sessions by overlapping timecode (BWF `bext`/iXML when present).
- **Sync:** offset measured per file pair by envelope cross-correlation. The fixture room track runs about 0.25 s late.
- **ASR providers:**
  - `TranscriptImportProvider`: reads the fixture JSON format (segment-level `lines[]` with `startTime`, `endTime`, `text`) and spreads word times across each segment.
  - `ParakeetProvider`: Parakeet TDT 0.6B v3 through ONNX Runtime, word timestamps, CoreML execution provider on Mac. Candidates are `sherpa-onnx` bindings or the `ort` crate; decide with a short spike and record the choice in DECISIONS.md.
  - The model manager downloads on first use, verifies the checksum and caches the model in the app-support folder.
- **Aligner:** port and improve the Stage 0 logic in `docs/reference/stage0/pipeline.py`:
  - align against *all* characters' lines so bleed is absorbed by other roles,
  - one- and two-word lines must stand alone or open a segment,
  - two passes (long lines find the active scenes, then all lines within those scenes and their neighbours),
  - snap clip edges to speech using the role track's loudness,
  - prevent overlaps between neighbouring takes,
  - drop loudness-detected bleed,
  - flag partial takes.
- **Take detection:** take type (full, partial, pickup, micro) and take number per line.
- **Room classification:** sort room speech into cue reading (matches the script, including action lines read aloud), direction, feedback or chatter.
- **Note linking:** feedback = room speech within 8 s after a take; direction = up to 30 s before it.
- **AI provider:**
  - The `AiProvider` trait with schema-validated JSON in and out.
  - `ClaudeCodeProvider` calls `claude -p` with `--output-format json`. The model comes from project config, default `claude-sonnet-5-5`, effort `medium` (changed from high, D35).
  - `MockProvider` for tests.
  - Versioned prompt files in `crates/apc-ai/prompts/`.
  - Results cached by input hash + prompt version + provider + model; every call logged to `Logs/ai-calls.jsonl`.
- **AI tasks:** ambiguous line matching, room classification refinement, note condensing (a few words for direction and feedback, plus tags), pick detection, sound spotting, sound tagging, name-matching fallback.
- **Defaults:** default take = director pick, else the last complete take.
- **Unmatched bin:** loud role-track speech that matches no line.
- **Accept** (fixture `tarmac-ep2`, `TranscriptImportProvider`, mock AI with recorded responses):
  - All 9 PAUL lines in scenes S1 and S2 that appear in the recording have at least one take (see `docs/reference/stage0/report.md`). Scene numbering may differ; match by line text.
  - At least 35 PAUL takes in total. Sara's lines heard on Paul's mic are never PAUL takes.
  - Takes found for:
    - "So you don't think I'm a scientist": 5,
    - "I told you! I'm bringing mine!...": 4,
    - "Lunch.": 4,
    - "I put this in the time spring two weeks ago.": 10, including one flagged partial ("time machine").
  - Detected picks:
    - "Science" line: take 4 (around 02:09, "that was the one I liked"),
    - "Lunch.": take 4 (around 06:54, "that one was the right one"),
    - "edible" line: take 3 (around 10:56, "we have the edible, definitely").
  - The detected room offset is within 0.1 s of 0.25 s.
  - **Parakeet** (if the model downloads): it runs on the fixture MP3s and the same checks pass with word-level timing. If the download is impossible, the test is skipped with a reason.

### M3: Export
- **FCPXML** importable by current DaVinci Resolve and Logic Pro. Use a version Resolve imports (1.9 is the safe default; record the choice).
  - One audio track per role, clips referencing the original files with source timecode, 1 s handles where available.
  - Crossfades on comp/edit splices; scene markers; note markers.
- **SRT** from final word timings.
- **As-cut transcript / cut list** (CSV and Markdown): scene, line, character, take, source file, source TC in and out, note.
- **WAV/MP3 bounce** per scene through the offline render of the audio engine (M5). Until M5, a simple ffmpeg-based bounce behind the same interface is fine.
- **Accept:**
  - The FCPXML validates against the DTD for its version (the DTDs ship with Final Cut Pro; if none is available offline, a structural validation test).
  - A golden test for the fixture cut list.
  - `RUN_REPORT.md` tells Rob how to check the import in Resolve manually.

### M4: CLI and MCP server
- The `cutaway` CLI (crate `apc-cli`) with all commands from the architecture spec, including `cutaway mcp`.
- MCP server on stdio, using the official Rust MCP SDK (`rmcp`) unless DECISIONS.md records a reason not to.
  - Tools as listed in the architecture spec.
  - Resources: script versions, cut list, scene bucket, match report.
  - Writes go through the command bus; destructive tools require a `confirm: true` argument.
- **Accept:** an integration test drives the full fixture pipeline through the CLI and through the MCP tools (in-process client) with identical results. A README section shows how to add the server to Claude Desktop and Claude Code.

### M5: Audio engine
- Real-time playback with cpal + symphonia, streaming from the source files.
- Per-clip gain (loudness match from integrated LUFS), equal-power fades at every splice, gap scheduling, room tone bed per location, multiple lanes and layers.
- Playhead events to the UI at about 60 Hz. Low-latency start, stop and seek.
- Offline render shares the same graph (used by the export).
- **Accept:**
  - Unit tests for the scheduler (gaps, ripple, anchors).
  - An offline render of fixture scene S1 matches the expected length within 10 ms.
  - Splices have no clicks: no sample jump above the threshold in a test signal.

### M6: Desktop app: Options, Arrange, box editor
- Tauri app per the frontend spec, visually matching the **Option C prototype** (`docs/design/prototype/`):
  - frosted glass on the night-blue stage, Figtree + Courier Prime,
  - a Japanese-palette character color per role, take shades per take,
  - all visual values as **design tokens** in `app/src/theme/` (no literal colors in components),
  - the app name from `brand.json`,
  - dialogue as the most prominent element, the current box as a deck with take pills,
  - notes beside the box,
  - smaller action lines as glass pills,
  - one Play/Stop transport with the scene map, locator and play-from-locator,
  - the numbered scene rail,
  - an inspector on demand,
  - a settings sheet for pacing and room tone,
  - keyboard help overlay (?).
- **Importer UI:** script, then audio with role confirmation, then background analysis with progress.
- **Options page:**
  - take carousel with keyboard navigation,
  - passage mode,
  - edge trim with ripple,
  - unmatched bin.
- **Arrange page:** drag reorder with ripple, waveform strips with head/tail trim and the "Cuts a word" warning, I/O at the locator, pause rows you drag (negative = overlap), the "Moved" badge.
- **Box editor overlay:**
  - comp mode (paint across take lanes, text-aligned by script words, nudgeable segments, seam warnings, use whole take 1…5, reset),
  - edit mode (delete words with strike-through restore, close or keep the pause, pause trim, head and tail).
- **Save and history:** autosave indicator, Save Version, a list of backups and versions with revert.
- **Accept:**
  - Playwright or Tauri WebDriver smoke tests: open the fixture project, switch takes with the arrow keys, play/stop, delete a word in the box editor and undo it.
  - Manual checklist for Rob in `RUN_REPORT.md`.

### M7: Edit page, show library and sound
- **Edit page:** a horizontal multitrack timeline of the same document, built from the box editor's lane components and tokens.
  - Groups: dialogue (one lane per role, fully editable), sound effects (auto layers), ambience, music.
  - Ripple by default; Option to move a single clip and allow overlap.
  - Nudge, trim, fades, clip gain, anchors to dialogue words or lines.
  - "Edited in Edit" markers on the simple pages for edits they can't express.
- **Show library:**
  - `Library/` folders,
  - dedup by hash,
  - tags,
  - drag-in from Finder onto the bucket (import + tag) or straight onto the timeline (import + place).
- **Sound bucket:** cues per scene and show-wide, statuses (wanted, candidates, chosen, placed), audition, manual editing, merge duplicates into a show-level cue.
- **AI sound spotting** per scene, writing cues anchored to script lines.
- **Sound providers:**
  - `LibrarySearchProvider` (local),
  - ElevenLabs generation (API key from the environment),
  - Epidemic Sound search (API or MCP if available; otherwise a stub behind the interface).
  - Optional: an MCP-client provider so a configured MCP server (e.g. Magnific) can generate sounds.
- **Accept:**
  - Unit tests for anchoring: an effect follows its line after a take swap or gap change, and an effect pinned to absolute time stays put.
  - A sync test: an edit made on the Edit page shows up correctly on the Options page.
  - Spotting on fixture S2 with the mock AI produces anchored cues.

### M8: Windows readiness
- The CI build passes on the Windows runner.
- Paths, file dialogs and the audio device run through cross-platform APIs.
- ONNX uses the DirectML or CPU execution provider on Windows.
- **Accept:** the Windows CI job is green (build + unit tests).

## End of run
1. `docs/RUN_REPORT.md` written.
2. `docs/PLAN.md` fully updated.
3. Everything pushed to `dev`.
4. A pull request `dev -> main` opened, not merged.
