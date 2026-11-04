# Cutaway: Implementation Plan

Scope: `docs/BUILD_BRIEF.md` milestones M0–M8. Specs: `docs/specs/architecture.md`, `docs/specs/how-it-works.md`, `docs/specs/frontend-design.md`. Design: `docs/design/prototype/` (Option C). Decisions: `docs/DECISIONS.md`.

Naming: the product is **Cutaway**; every user-facing name comes from `brand.json` (see the architecture spec, *Naming*). Crate names keep the internal `apc-` prefix.

---

## 1. Crates and app modules

Dependency direction (no cycles): `core ← media ← audio`, `core ← ai ← analysis`, `core,media ← sound`, `core,media,audio ← export`, all ← `app` ← `cli`, `mcp`, `app/src-tauri`.

### `apc-core`: document, command bus, undo, project I/O
| Module | Responsibility | Public interface |
|---|---|---|
| `brand` | Names from `brand.json` (build script) | `DISPLAY_NAME`, `SLUG`, `EPISODE_EXT`, `SHOW_EXT`, `BUNDLE_ID` |
| `ids` | Stable short IDs | `Id` (newtype over `String`), `Id::new(prefix)` |
| `model` | Typed entities (see §2) | `Document`, `Scene`, `ScriptLine`, `Character`, `SourceFile`, `Session`, `Take`, `Word`, `Note`, `Unmatched`, `BoxState`, `CompSegment`, `WordEdits`, `Spacer`, `SoundCue`, `Placement`, `Anchor`, `ScriptVersion` |
| `store` | SQLite (WAL) open/create/migrate; entity rows | `EpisodeStore::{create, open, load_document, apply_changes}`, `ShowStore` |
| `bus` | Commands → changes, persisted log, undo/redo | `Command` (serde enum), `Change {kind, id, before, after}`, `CommandBus::{execute, undo, redo, can_undo, history, seq}` |
| `project` | Show/episode folders | `ShowFolder::create/open`, `EpisodeFolder::create/open`, `EpisodePaths` |
| `backup` | Autosave snapshots, rotation, named versions, revert | `BackupManager::{maybe_autosave, snapshot, save_version, list, revert}`, `RotationPolicy`, `Clock` trait |
| `fountain` | Fountain/Throughline parser | `parse(&str) -> ParsedScript`, `ParsedScene`, `ParsedUnit {kind, character, extension, text, paren, forced}` |
| `script` | Script versions + diff | `import_version(...) -> ScriptImport`, `diff(old, new) -> ScriptDiff`, `ChangeSummary` |
| `media` | Import (copy/reference), hash, relink | `hash_file`, `import_files(...)`, `relink(...)` |
| `timeline` | Resolve box timing (gaps, spacers, trims, pins, comps, edits, anchors) | `SceneLayout`, `layout_scene(doc, scene) -> SceneLayout`, `ClipPiece`, `resolve_anchor` |
| `config` | Project TOML config (`[ai]`, `[sound.*]`, pacing) | `ProjectConfig` |
| `text` | Word normalization shared by parser, aligner and comps | `norm_word`, `norm_words`, `tokenize_display` |

### `apc-media`: decode, peaks, loudness, sync, BWF
| Module | Interface |
|---|---|
| `decode` | `AudioSource::open(path)`, `info() -> MediaInfo {sample_rate, channels, frames, duration_s}`, `read_mono(range) -> Vec<f32>`, `read_interleaved(range)` (symphonia: WAV, MP3, AIFF, FLAC) |
| `envelope` | `Envelope::compute(src, fps=100)`, `rms_db(t0,t1)`, `percentile`; cached in `Cache/` by file hash |
| `peaks` | `peaks(src, t0, t1, buckets) -> Vec<(f32,f32)>` |
| `loudness` | `integrated_lufs(samples, sr)` (EBU R128) |
| `sync` | `measure_offset(env_a, env_b, max_lag_s) -> SyncResult {offset_s, confidence}` (envelope cross-correlation, coarse 100 fps then fine 1 kHz) |
| `bwf` | `read_bwf(path) -> Option<BwfInfo {time_reference_samples, origination, ixml_scene, ixml_take, tc_start_s}>` |
| `wav` | `write_wav(path, sr, channels, samples)` |

### `apc-audio`: real-time engine and offline bounce
| Module | Interface |
|---|---|
| `schedule` | `PlaySchedule::from_layout(&SceneLayout, &MixSettings) -> PlaySchedule` (clips with sample positions, gain, equal-power fades, lanes, room-tone bed) |
| `mixer` | `Mixer::render(&PlaySchedule, from_frame, &mut [f32])` (shared by RT and offline) |
| `offline` | `render_to_wav(schedule, sources, path)`, `render_to_buffer` |
| `engine` | `Engine::{new, load(schedule), play(from_s), stop, seek, position_s}`; `PlayheadEvent` stream at ~60 Hz (cpal + feeder thread) |

### `apc-ai`: AI provider interface, providers, prompts
| Module | Interface |
|---|---|
| `task` | `AiTask` enum and typed I/O structs (§3), `schema_for(task)` (schemars) |
| `provider` | `trait AiProvider { fn id(&self)->&str; fn model(&self)->&str; fn run(&self, req:&AiRequest) -> Result<serde_json::Value, AiError>; }` |
| `claude_code` | `ClaudeCodeProvider` (`claude -p --output-format json --model … `, effort from config) |
| `mock` | `MockProvider` (rule-based answers; optional recorded responses directory) |
| `prompts` | `prompts/<task>.v<N>.md`, `Prompt::load(task)` with version |
| `runner` | `AiRunner::run<T: AiTaskIo>(input) -> AiOutcome<T>`: validate, 1 retry, flag; cache by input hash + prompt version + provider + model; log to `Logs/ai-calls.jsonl` |

### `apc-analysis`: ASR, aligner, take detection, room classification
| Module | Interface |
|---|---|
| `asr` | `trait AsrProvider { fn id(&self)->&str; fn transcribe(&self, req:&AsrRequest) -> Result<Transcript, AsrError>; }`, `Transcript {segments: Vec<Segment>, words: Vec<TWord>}`, `TranscriptImportProvider`, `ParakeetProvider`, `ModelManager` |
| `fuzzy` | `ratio(a,b) -> f64` (Indel similarity, bit-parallel LCS; matches rapidfuzz `fuzz.ratio`) |
| `align` | `Aligner::align(units, words, opts) -> Vec<Hit>`; two-pass active scenes; short-line rule |
| `edges` | `refine(env, t0, t1, thr)` (snap to loudness), `resolve_overlaps` |
| `takes` | `build_takes(hits, …) -> Vec<Take>`: numbering, type (full/partial/pickup/micro), partial flag, bleed drop |
| `room` | `classify_room(...)`: cue reading / direction / feedback / chatter |
| `notes` | `link_notes(takes, notes)`: feedback ≤ 8 s after, direction ≤ 30 s before |
| `picks` | keyword fallback `pick_score`, defaults (pick, else last complete take) |
| `roles` | `match_roles(files, characters, aliases) -> Vec<RoleGuess>` |
| `sessions` | `group_sessions(files) -> Vec<SessionGroup>` |
| `pipeline` | `analyze(doc, inputs, providers) -> AnalysisResult` (+ progress callback); `MatchReport` |

### `apc-sound`: library, bucket, sound providers
| Module | Interface |
|---|---|
| `library` | `Library::{import(path, category) (dedup by hash), search(query, tags), tag, assets}` on the show store (or the episode's local library) |
| `bucket` | `cue_status`, `merge_cues`, `bucket_view(scene)` |
| `provider` | `trait SoundSearchProvider { fn search(&self, q:&SoundQuery) -> Result<Vec<SoundCandidate>> }`, `trait SoundGenerateProvider { fn generate(&self, req:&GenerateRequest) -> Result<Vec<GeneratedSound>> }` |
| `providers` | `LibrarySearchProvider`, `ElevenLabsProvider` (`ELEVENLABS_API_KEY`), `EpidemicProvider` (stub unless `EPIDEMIC_SOUND_API_KEY`), `McpGenerateProvider` (stub) |
| `tagging` | `tag_from_name(file_name) -> (category, tags)` (+ AI `sound_tag` fallback) |

### `apc-export`: FCPXML, SRT, cut list, bounce
`export_fcpxml(doc, scope, opts) -> String` (FCPXML 1.9), `export_srt`, `export_cut_list_csv`, `export_cut_list_md`, `bounce(scope, format)` (offline render; MP3 through ffmpeg when present).

### `apc-app` (added; see D18): the shared service
`Service::{open_episode, create_episode, create_show, call(method, params_json) -> json}`; `methods()` lists every method with its JSON schema (used by the CLI, the MCP tools, the Tauri bridge and the HTTP dev bridge); `jobs` (background analysis with progress); `views` (UI view models: `scene_view`, `edit_view`); `demo` (synthetic session generator for CI and UI tests); `bridge` (HTTP JSON-RPC + SSE for browser/Playwright).

### `apc-cli`: the `cutaway` binary
clap subcommands from the architecture spec: `show new`, `new`, `import-script`, `import-audio`, `roles`, `transcribe`, `align`, `notes`, `sfx spot|import`, `export fcpxml|srt|cutlist|mix`, `status`, `undo`, `redo`, `versions`, `mcp`, plus `demo` and `serve` (dev bridge).

### `apc-mcp`: MCP server
rmcp stdio server; tools mirror `apc-app` methods (§2.3); resources: `cutaway://script/{version}`, `cutaway://cutlist`, `cutaway://bucket/{scene}`, `cutaway://report`.

### App (`app/`)
| Module | Responsibility |
|---|---|
| `src-tauri/` | Tauri 2 shell: `rpc(method, params)`, `playhead` events, file dialogs, drag-drop |
| `src/lib/brand.ts` | App name from `brand.json` |
| `src/theme/` | `tokens.css`, `palette.ts`, `index.ts` (design tokens, see the frontend spec) |
| `src/lib/bridge/` | `Bridge` interface; `TauriBridge`, `HttpBridge` (dev bridge), shared types |
| `src/lib/stores/` | Project view, selection, playback, UI state (Svelte stores) |
| `src/lib/keys.ts` | Keyboard map |
| `src/pages/` | `Options`, `Arrange`, `Edit` |
| `src/components/` | Header, SceneRail, Transport, SceneMap, CurrentBox, LineBox, ActionPill, TakePills, NotesAside, BoxEditor (+ lanes), Inspector, SettingsSheet, HelpOverlay, Importer, UnmatchedDrawer, Bucket, ExportPanel, HistoryPanel, Waveform |
| `tests/` | Playwright smoke tests (against the dev bridge) |

---

## 2. Schemas

### 2.1 Episode file `<name>.cutaway` (SQLite, WAL)

```sql
PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
  -- schema_version, created_at, app_version, show_path, asr_model, config (TOML)
CREATE TABLE entity (
  kind   TEXT NOT NULL,            -- see entity kinds below
  id     TEXT NOT NULL,
  parent TEXT,                      -- scene for lines, line for takes/boxes, …
  ord    REAL NOT NULL DEFAULT 0,   -- order within parent
  data   TEXT NOT NULL,             -- JSON of the typed entity
  PRIMARY KEY (kind, id)
);
CREATE INDEX entity_parent ON entity(kind, parent, ord);
CREATE TABLE command_log (
  seq     INTEGER PRIMARY KEY AUTOINCREMENT,
  ts      TEXT NOT NULL,
  client  TEXT NOT NULL,            -- ui | cli | mcp | pipeline
  name    TEXT NOT NULL,
  params  TEXT NOT NULL,            -- JSON (large payloads summarised)
  changes TEXT NOT NULL,            -- JSON [{kind,id,parent,ord,before,after}]
  state   TEXT NOT NULL CHECK (state IN ('done','undone','discarded'))
);
CREATE TABLE jobs (id TEXT PRIMARY KEY, kind TEXT, state TEXT, progress REAL, message TEXT,
                   started_at TEXT, finished_at TEXT, result TEXT);
CREATE TABLE backups (id TEXT PRIMARY KEY, kind TEXT CHECK (kind IN ('auto','named','pre-revert')),
                      name TEXT, file TEXT NOT NULL, created_at TEXT NOT NULL, seq INTEGER);
-- Typed read views (one per kind), e.g.:
CREATE VIEW v_takes AS SELECT id, parent AS line_id, json_extract(data,'$.source_id') AS source_id,
  json_extract(data,'$.take_no') AS take_no, json_extract(data,'$.t0') AS t0, json_extract(data,'$.t1') AS t1
  FROM entity WHERE kind='take';
```

Entity kinds and their JSON (`parent` in brackets):
| Kind | Fields |
|---|---|
| `script_version` | `number, file, name, hash, imported_at, summary` |
| `scene` | `number, heading, location, time_of_day, order: [item ids], tightness, room_tone: Option<asset/range>` |
| `line` [scene] | `kind (dialogue/action/transition/forced), character, extension, text, paren, words, number, version_added, changed` |
| `character` | `name, aliases, color_index` |
| `session` | `name, tc_start_s, files` |
| `source` | `path, stored (copied/referenced), hash, size, role (character/room/unknown), character, session_id, sample_rate, channels, duration_s, tc_start_s, offset_s, role_confidence, missing` |
| `take` [line] | `source_id, take_no, take_type (full/partial/pickup/micro), t0, t1, words: [{w, raw, t0, t1, script_idx}], score, heard, lufs, partial, pick (0/1/2), favorite, favorite_origin, direction, feedback, tags, note_ids, confidence, needs_rematch` |
| `note` | `source_id, t0, t1, text, class (cue_reading/direction/feedback/chatter), take_ids, summary, tags` |
| `unmatched` [scene] | `source_id, t0, t1, text, db, origin (analysis/deleted_line)` |
| `box` [line] | `take_id, comp: [CompSegment {take_id, from_word, to_word, nudge_s}], edits: WordEdits {deleted: [word idx], keep_pause: [cut idx], pause_trims: {idx: s}}, head_s, tail_s, gap_s: Option, pinned_s: Option, gain_db, split_at: [s], edited_in_edit` |
| `spacer` [scene] | `line_id (action), duration_s: Option, enabled` |
| `cue` [scene] | `description, anchor: Anchor, cue_type (hard/foley/ambience/walla/music), tags, priority, status, candidates: [asset ids], chosen, show_cue_id, ai_suggested, accepted` |
| `placement` [scene] | `asset_id, cue_id, lane (sfx/ambience/music), layer, anchor: Anchor or at_s, offset_s, in_s, out_s, fade_in_s, fade_out_s, gain_db` |
| `asset` | (standalone episodes only: local library, same fields as the show `asset`) |

`Anchor = {line_id, word: Option<usize>, edge: start|end}`; resolved by `timeline` after dialogue.

### 2.2 Show file `<name>.cutawayshow` (SQLite, WAL)

*Changed in implementation (D64):* the show file is JSON (`ShowFile` in `apc-core::show`: name, id, layout, theme, aliases); the library is the show's `Sound/` folders with a local hash index and `Sound/Cutaway/library.json` (D65). The schema below is no longer used.

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE asset (id TEXT PRIMARY KEY, file TEXT NOT NULL, hash TEXT NOT NULL UNIQUE, category TEXT,
  duration_s REAL, source TEXT CHECK (source IN ('imported','licensed','generated')), license TEXT,
  prompt TEXT, provider TEXT, created_at TEXT, lufs REAL);
CREATE TABLE asset_tag (asset_id TEXT REFERENCES asset(id) ON DELETE CASCADE, tag TEXT, PRIMARY KEY(asset_id, tag));
CREATE TABLE alias (alias TEXT PRIMARY KEY, character TEXT NOT NULL);       -- "Col" = THE COLONEL
CREATE TABLE location (name TEXT PRIMARY KEY, room_tone_asset TEXT REFERENCES asset(id), ambience_asset TEXT);
CREATE TABLE show_cue (id TEXT PRIMARY KEY, description TEXT, cue_type TEXT, tags TEXT, chosen_asset TEXT);
CREATE TABLE episode (id TEXT PRIMARY KEY, name TEXT, path TEXT);
CREATE TABLE command_log (...same as the episode...);
```

### 2.3 Command list

Every command goes through `CommandBus::execute`; undo applies the recorded `before` values of its changes (the generic inverse); redo re-applies `after`. A new command discards the redo tail.

| Command | Parameters | Undo |
|---|---|---|
| `import_script_version` | `path, name` | restores lines/scenes/takes/unmatched; the copied file in `Script/` stays (never deleted) |
| `import_media` | `paths[], mode (copy/reference), session_name?` | removes the source entities; copied files stay |
| `set_role` | `source_id, role, character?` | old role (the show alias it records is a show command) |
| `relink_media` | `source_id, path` | old path |
| `apply_analysis` | `result` (sessions, offsets, takes, notes, unmatched, boxes) | previous analysis rows |
| `set_config` | `key, value` | old value |
| `select_take` | `line_id, take_id` | old take |
| `apply_take_to_passage` | `line_ids[], take_no` | old takes |
| `toggle_favorite` | `take_id` | flips back |
| `set_gap` | `line_id, gap_s or null` | old gap |
| `set_trim` | `line_id, head_s, tail_s` | old trims |
| `set_spacer` | `spacer_id, enabled?, duration_s?` | old values |
| `move_item` | `scene_id, item_id, to_index` | old order |
| `set_scene_tightness` | `scene_id, tightness` | old value |
| `set_comp` | `line_id, segments[]` | old comp |
| `reset_comp` | `line_id` | old comp |
| `delete_words` / `restore_words` | `line_id, from, to` | old edits |
| `set_cut_pause` | `line_id, cut, keep` | old flag |
| `set_pause_trim` | `line_id, word, trim_s` | old trim |
| `pin_clip` | `line_id, at_s or null` | old pin |
| `move_clip` | `line_id, delta_s, ripple` | old gap/pin |
| `set_clip_gain` | `line_id, gain_db` | old gain |
| `split_clip` | `line_id, at_s` | old splits |
| `use_unmatched` | `unmatched_id, scene_id, index` | removes the ad-lib line/box |
| `set_room_tone` | `location, source range or asset, enabled` | old value |
| `add_cue` / `update_cue` / `delete_cue` | cue fields / `cue_id, patch` / `cue_id` | inverse |
| `merge_cues` | `cue_ids[], into_show` | restores cues |
| `apply_spotting` | `scene_id, cues[]` | removes the new cues |
| `attach_candidate` / `choose_candidate` | `cue_id, asset_id` | inverse |
| `place_sound` | `asset_id, cue_id?, lane, anchor or at_s, offset_s` | removes placement |
| `update_placement` | `placement_id, patch` | old values |
| `unanchor_placement` | `placement_id` | old anchor |
| `delete_placement` | `placement_id` | restores it |
| Show: `add_alias`, `import_asset`, `tag_asset`, `delete_asset {confirm}`, `set_location_tone` | … | inverse (delete keeps the file in `Backups/Trash/`) |

Not commands (no undo, own safety): `undo`, `redo`, `save_version`, `revert` (makes a `pre-revert` backup first), exports (refuse to overwrite without `confirm`).

---

## 3. Interfaces

### 3.1 AI tasks (input → output JSON; schemas generated by schemars from these structs, validated strictly)

| Task | Input | Output |
|---|---|---|
| `line_match` | `{segments:[{index,t0,t1,text}], candidates:[{line_id,character,text}]}` | `{matches:[{index, line_id: string or null, take_type: "full"/"partial"/"pickup"/"adlib", confidence: 0..1}]}` |
| `room_classify` | `{segments:[{id,t0,t1,text}], scene_script:[{line_id,kind,character,text}]}` | `{segments:[{id, class:"cue_reading"/"direction"/"feedback"/"chatter"}]}` |
| `note_condense` (v2, D73) | `{gaps:[{gap_id, previous: {take_id, character, line_text} or null, next: same or null, text}]}` | `{notes:[{gap_id, summary (1–2 sentences), tags:[string]}]}` |
| `pick_detect` | `{takes:[{take_id, take_no, line_text, feedback_text}]}` | `{picks:[{take_id, strength: 0/1/2, refers_to: take_id or null}]}` |
| `sound_spot` | `{scene:{id, heading, location, items:[{line_id, kind, character, text}]}}` | `{cues:[{description, anchor_line_id, anchor_word: int or null, cue_type, tags, priority 1..3}]}` |
| `sound_tag` | `{file_name, duration_s, cue_description?}` | `{category, tags:[string]}` |
| `name_match` | `{files:[string], characters:[string], aliases:{alias: character}}` | `{assignments:[{file, role:"character"/"room"/"unknown", character: string or null, confidence}]}` |

### 3.2 Provider interfaces

```rust
pub trait AsrProvider: Send + Sync {
    fn id(&self) -> &str;                       // "transcript-import", "parakeet-tdt-0.6b-v3"
    fn transcribe(&self, req: &AsrRequest) -> Result<Transcript, AsrError>;
}
pub struct AsrRequest { pub audio: PathBuf, pub sidecar: Option<PathBuf>, pub hotwords: Vec<String>, pub language: String }

pub trait AiProvider: Send + Sync {
    fn id(&self) -> &str;                       // "claude-code", "mock"
    fn model(&self) -> &str;
    fn run(&self, req: &AiRequest) -> Result<serde_json::Value, AiError>;
}
pub struct AiRequest { pub task: AiTask, pub prompt: String, pub prompt_version: u32, pub input: serde_json::Value, pub output_schema: serde_json::Value }

pub trait SoundSearchProvider: Send + Sync {
    fn id(&self) -> &str;
    fn search(&self, q: &SoundQuery) -> Result<Vec<SoundCandidate>, SoundError>;
}
pub trait SoundGenerateProvider: Send + Sync {
    fn id(&self) -> &str;
    fn generate(&self, req: &GenerateRequest) -> Result<Vec<GeneratedSound>, SoundError>;
}
```

---

## 4. Tasks

### M0: Repository and tooling
- [x] `brand.json`, `scripts/rebrand.mjs` (sync + `--check`)
- [x] Rust workspace with the crates (`apc-core`, `apc-media`, `apc-audio`, `apc-analysis`, `apc-ai`, `apc-sound`, `apc-export`, `apc-app`, `apc-cli`, `apc-mcp`)
- [x] Tauri 2 + Svelte 5 + TypeScript skeleton in `app/`, theme tokens folder, ESLint + svelte-check
- [x] `scripts/check.sh`: brand check, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm run lint`, `npm run check`
- [x] `.gitignore` (fixtures except README, target, node_modules, .env, Cache)
- [x] GitHub Actions: `scripts/check.sh` on macOS and Windows (Windows `continue-on-error` until M8)
- [x] Push `dev`

### M1: Core document and project folders
- [x] Show and episode folder creation (`Audio/ Script/ Backups/ Exports/ Cache/ Logs/ Library/`), `.cutawayshow` / `.cutaway` SQLite WAL
- [x] Entity store + typed document load
- [x] Command bus: execute, persist, undo, redo, discard redo tail; history
- [x] Autosave backups (10 min while changing; keep 20 + one per day), named versions, revert
- [x] Fountain parser (title page, `===`, scene headings, cues with extensions, parentheticals, `!`/`>`/`--`, notes/sections/synopses as metadata)
- [x] Script versioning: `vNNN <name>.fountain`, diff with stable IDs, deleted-line takes → unmatched, change summary
- [x] Media import: copy (default) / reference, BLAKE3 hash, relink by hash
- [x] Unit tests: undo/redo, autosave rotation, script diff (edit, delete, insert), relink

### M2: Import and analysis pipeline
- [x] Fountain parser checks against the TARMAC fixture
- [x] Role matching by file name (+ aliases, room keywords), content fallback
- [x] Sessions by overlapping timecode (BWF `bext` / iXML)
- [x] Decode (symphonia), envelope, sync offset (cross-correlation); fixture room offset 0.25 s ± 0.1
- [x] `TranscriptImportProvider` (fixture JSON)
- [x] `ParakeetProvider` spike (sherpa-onnx vs `ort`), model manager (download, checksum, cache)
- [x] Fuzzy ratio (bit-parallel LCS), aligner port (all characters, short-line rule, two passes)
- [x] Edge snapping, overlap prevention, bleed drop, partial flag
- [x] Take detection (type + number), defaults (pick, else last complete take)
- [x] Room classification, note linking (8 s / 30 s)
- [x] `AiProvider`, `ClaudeCodeProvider`, `MockProvider`, prompts, cache, `ai-calls.jsonl`
- [x] AI tasks wired (line match, room refine, note condense, picks, spotting, tagging, name match)
- [x] Unmatched bin
- [x] Acceptance test on `tarmac-ep2` (skips without fixtures) + synthetic demo test (runs in CI)

### M3: Export
- [x] FCPXML 1.9: one audio track (lane) per role, asset-clips with source TC, 1 s handles, crossfades, scene and note markers
- [x] SRT from final word timings
- [x] Cut list CSV + Markdown
- [x] Bounce per scene (offline render; MP3 via ffmpeg)
- [x] Structural FCPXML validation test (DTD check runs when Final Cut Pro is installed)
- [x] Cut list golden test (demo session, `crates/apc-app/tests/golden/`)

### M4: CLI and MCP server
- [x] `apc-app` service + method registry (JSON schemas)
- [x] `cutaway` CLI with all spec commands
- [x] MCP server (rmcp, stdio): tools, resources, `confirm: true` for destructive tools
- [x] Integration test: pipeline via CLI and via MCP (in-process client) with identical results
- [x] README: add the server to Claude Desktop and Claude Code

### M5: Audio engine
- [x] Schedule from layout: per-clip gain (LUFS match), equal-power fades at splices, gaps, room tone bed, lanes/layers
- [x] Mixer + offline render (shared)
- [x] cpal real-time engine, feeder thread, ~60 Hz playhead events, start/stop/seek
- [x] Tests: scheduler (gaps, ripple, anchors), offline length of S1 within 10 ms, no clicks at splices

### M6: Desktop app
- [x] Theme tokens + palette from the prototype; fonts
- [x] Bridge (Tauri + HTTP dev bridge), stores, keyboard map
- [x] Header, scene rail, transport + scene map, hint line, help overlay
- [x] Options page (deck, take pills, notes aside, action pills, word highlight, passage mode)
- [x] Arrange page (rows, waveform strips, trims, locator, I/O, gap drag, overlap, reorder)
- [x] Box editor (comp mode, edit mode)
- [x] Inspector, settings sheet, unmatched drawer, importer, summary card, history/versions, export panel
- [x] Playwright smoke tests (open project, arrow keys switch takes, play/stop, delete a word in the box editor and undo)

### M7: Edit page, show library and sound
- [x] Edit page: dialogue lanes per role, SFX layers, ambience, music; ripple / Option-move; nudge, trim, fades, gain, anchors; "edited in Edit" markers
- [x] Show library (folders, dedup, tags), Finder drag-in to bucket / timeline
- [x] Bucket (cues per scene + show-wide, statuses, audition, editing, merge)
- [x] AI spotting per scene → anchored cues
- [x] Providers: library search, ElevenLabs, Epidemic (stub, D28), MCP-client provider (stub, D28)
- [x] Tests: anchoring follows take swap / gap change, pinned stays; Edit→Options sync; spotting on S2 with mock AI

### M8: Windows readiness
- [x] Cross-platform paths (no Windows verbatim prefixes in stored paths or FCPXML URLs), dialogs (Tauri plugin), audio device (cpal default host); ONNX DirectML/CPU provider selection; LF line endings via .gitattributes
- [ ] Windows CI job green (build + unit tests): not run, pushes are blocked in this session (see RUN_REPORT)

### M9: Round 2 screens (tasks/, second run)
- [x] Core: sound buckets (entity + commands), lane mix (volume, pan, mute, solo) applied by the schedule, fade curves, split, `ApplyMatch` with hand-set room links
- [x] Service: `buckets`, `spot_buckets`, bucket CRUD, `bucket_search` (paging, length and leave-out filters), `bucket_download`, `bucket_generate`, `place_from_bucket` (download-on-drop job), `play_sound`; `import_view`, `match_files`, `link_room`, `suggest_character`; edit view with lane mix and room tone seams
- [x] Epidemic previews, paged search, download progress; engine output level
- [x] UI: header tabs with Import; Import › Files; Import › Sounds with the bucket panel; Edit with the control bar, tools, cycle, lane groups with M/S, script strip, anchors, inspector, mixer, docked bucket and the collapsed tab; keys B, ⇧B, I, X, C
- [x] Tests: buckets and import matching (Rust, mock Partner API), lane mix and pan/fade unit tests, Playwright for the three Round 2 areas
- [x] Docs: `docs/design/round2-screens.md` (the spec files are read-only here); D40–D43

### M10: Take rescue, scoped re-analysis and the dev panel (Rob, 2026-11-02)
Why: Parakeet mishears real takes; the matcher only compares text, so misheard reads end up unmatched. Claude gets the clean context to rescue them, and Rob gets a look under the hood with a way to point at what was missed.

Rules that hold throughout:
- Takes come only from a character's **session mic**. The **director mic** is notes, cues and context only, never a take (checked in the answer validation and again where takes are created).
- The normal UI gets no new markers. A take's origin (`matcher`, `rescue`, `manual`) is stored and shown only in the dev panel.
- Every change is a command (one undo step); UI, CLI and MCP share the methods.
- A re-analysis replaces results only inside its scope. User favorites survive.

**10.1 Sentences and the scene timeline** (`apc-analysis::sentences`, `::timeline`)
- [x] Merge Parakeet fragments per mic into sentences (pause ≤ 0.7 s, no sentence end; caps 40 words / 15 s). Each sentence keeps its word range, so every word keeps its exact timing.
- [x] Per scene and actor session: the actor's session-mic sentences in the scene's stretch, interleaved by time with the session's director-mic sentences. IDs `s…` (session mic) and `d…` (director mic).
- [x] Doubles: the same speech on both mics is kept once. Actor speech always from the session mic, director speech always from the director mic (level relative to each mic's median decides which it is).
- [x] Scene stretch on a mic: from the matcher's takes for that scene (−30 s … +15 s), or the gap between the neighbouring scenes' takes when the scene has none.

**10.2 Take rescue** (`AiTask::TakeRescue`, prompt v1)
- [x] Plain-text input: the scene script (`L1 SARA: …`, all characters) and the recording (`s43 SARA [= L3]: …`, `s45 SARA [?]: …`, `d44 DIRECTOR: …`, `— 12 s —` for long pauses). Optional marks from Rob ("s45–s47: take of L3, misheard").
- [x] Output by ID: `{"rescues": [{"id", "line", "read_as", "confidence", "from"?, "to"?}]}`. Timecodes never come from Claude: ID → sentence → words → word timings → edges snapped to the audio.
- [x] Hard checks: only `s` IDs marked `[?]`, only the mic owner's lines in this scene, not bleed, no overlap with a take, confidence high or medium.
- [x] Mock provider: looser fuzzy match of `[?]` sentences against the owner's lines (tests stay offline).
- [x] Runs inside `analyze` after the matcher's takes, so notes and picks see rescued takes.

**10.3 Scoped re-analysis** (dev switch)
- [x] `AnalysisScope { characters, line_ids, ranges }`: the deterministic part runs as before (cheap, transcripts cached); AI calls, takes, notes and unmatched are limited to the scope.
- [x] `AnalysisPayload.source_scope`: notes and unmatched are replaced only for those sources.
- [x] Methods: `analyze_scope` (sync) / `start_analysis` with a scope (job). CLI: `cutaway align --character SARA`, `--line ID`.

**10.4 Dev panel: under the hood** (new `dev` page behind the dev switch)
- [x] `dev_transcript` view: per actor session, every sentence with mic, speaker, time, text and what it became (matcher take of L…, rescued take of L…, near miss, unmatched, bleed, director note/cue).
- [x] Three columns, scrolling together: full transcript · rescued by Claude · script position. Search, scene/character filter, click to play.
- [x] Rerun buttons: this character, this line's takes.

**10.5 Marks: link to the script and send to Claude**
- [x] `DevMark` entity (source, t0..t1, kind: missed take / wrong line / not a take / edges / other, optional line, note, status, proposal). Commands `add_mark`, `update_mark`, `delete_mark`.
- [x] Select sentences, link them to a script line, add a note. "Send" runs the rescue for that stretch only, with the mark as a hint; the proposal shows before/after; Apply is one command (`apply_mark`), Discard keeps the mark.

**10.6 Learning section**
- [x] Pick a dialogue-heavy scene; Rob marks everything wrong or missing, then marks the scene reviewed. Reviewed scenes are the reference set.
- [x] `cutaway eval` / `dev_eval`: runs variants (merge pause, bleed margin, match threshold, rescue on/off, Rob's examples in the prompt) on the reference scene and a held-out scene; found / wrong / minutes per variant.
- [x] What it learns: settings (adopted by Rob), up to 5 worked examples from his marks in the rescue prompt, and a show glossary of misheard words (heard → script) used by the matcher and Claude. Changes go into DECISIONS.md.

**10.7 Verification**
- [x] Unit tests (merge, mapping back, doubles, director never a take, validation), mock rescue on the fixture, scoped apply keeps out-of-scope data, Playwright smoke test of the dev panel.
- [x] Copy of Tarmac-EP02 with Claude (Sara): rescue numbers in the progress log. Full episode with Claude: not run yet (about 15–25 min).

### M11: Editor basics on the Edit page (Rob, 2026-11-02, branch `editor`)
Why: Rob's first real session in the Edit page: the time scale jumps when a clip moves, clips can't be trimmed, the waveform can't be read, and there is no clip mute. The page should work like a minimal NLE with Logic Pro key commands.

Rob's answers: a muted clip keeps its slot (plus a separate ripple delete); "frame handles" are clip-edge trim handles; unlocking a clip frees it from the script order and pauses; Return stays "play line in context"; ←/→ select the previous/next clip on the Edit page only (takes elsewhere); scrubbing is silent.

Rules that hold throughout: every change is a command (UI, CLI and MCP share them); source media is never touched; mute and ripple delete show in every view and are heard in playback, bounce and export.

**11.1 Fixed time scale and a solid playhead** (UI)
- [x] Zoom is absolute (px/s); scene length, clip moves and panel widths never change it. Zoom anchors to the pointer (⌘-scroll) or the playhead (keys). ⌘←/→ zoom, Z toggles fit-scene and back.
- [x] Full-height playhead with a handle in the ruler; drag scrubs silently, click on the ruler or an empty lane jumps.
- [x] The view follows the playhead while playing.

**11.2 Real dialogue splits** (core)
- [x] Split points stored by source time (take + seconds), so they survive trims and take-level edits. Each part after a split has its own offset, trims at the split, mute and drop.
- [x] Layout cuts the pieces at split points; parts shift by their offsets; dropped parts close up (ripple inside the line); words follow.
- [x] `split_clip` takes scene time; `move_segment` moves a part (part 0 moves the clip); `clear_splits` stays.

**11.3 Mute and ripple delete** (core + every view)
- [x] `set_clip_muted` (a part or the whole line) and `muted` on placements: kept in place, skipped in playback and bounce, disabled clips in FCPXML, left out of SRT, marked in the cut list.
- [x] Ripple delete = `set_clip_dropped` on a dialogue part (or line): the part drops out and the time closes; `dropped: false` restores. Placements: Delete removes.
- [x] Options and Arrange grey out muted lines and parts, show dropped ones struck through with restore.
- [x] Keys: ⌃M mute, Delete = ripple delete when Ripple is on, mute otherwise (dialogue).

**11.4 Trim handles** (core + UI)
- [x] `trim_clip` (dialogue: head/tail of the line and inner split edges, content stays in place, ripple setting respected, limited to the source file) and `trim_placement` (in point and length, limited to the asset).
- [x] Edge handles on every clip; the hidden audio shows faintly while dragging; edges snap to words; trim start/end to playhead.

**11.5 Waveforms you can read** (app + UI)
- [x] Each piece draws its own source range (matches what plays); peaks fetched for the visible range at the current zoom, cached; normalized per source like the take peaks.
- [x] Lane height and waveform vertical zoom.

**11.6 Lock to script**
- [x] A global "Lock" switch and a per-clip lock. Unlocked clips move freely in absolute time (pin) and never ripple; locking again keeps the clip where it is (`pin_clip` without a time turns the pin back into a pause).

**11.7 Script strip and collapsed groups**
- [x] Word row in the script strip (each word at its time, current word lit while playing), visible whatever is collapsed.
- [x] A collapsed group shows a preview row of its clips merged into blocks (Dialogue with sentence labels).

**11.8 Selection, fades, keys**
- [x] Multi-select (⇧-click, rubber band on empty lanes), ⇧F select all following, ⌘U cycle to selection; move, nudge, mute, delete on the selection.
- [x] ⌘T split at playhead; fade handles on clip corners (dialogue included).
- [x] Logic keys on the Edit page: ←/→ previous/next clip, , / . previous/next word, T tool menu, M/S on the selected lane, ⌘↑/↓ lane height, ⌥[ / ⌥] trim to playhead; help overlay updated. One `batch` command per multi-clip change (one undo step).

**11.9 Verification**
- [x] Core tests: splits survive head trims, offsets and drops, mute skipped in schedule/bounce, FCPXML disabled clip, SRT skip, trims clamp to the source, unpin keeps the position.
- [x] Playwright: zoom stays fixed when a clip moves, trim handle, mute greys the line in Arrange.
- [x] DECISIONS entries, "Changed in implementation" note in frontend-design.md, progress log.

### M12: Throughline import and story context (Rob, 2026-11-02, branch `throughline`)
Why: Rob writes in Throughline. Its `.thrl` export is the script plus what the writers know about it (beat titles and summaries, acts, who is in a scene, setting groups and descriptions, character bios, the emotional arc). Cutaway should import it directly and use that knowledge where it helps: in the AI calls and on screen. Decisions in D61.

Rules that hold throughout:
- A `.thrl` file becomes the same `ParsedScript` a Fountain file does: versioning, the ID-keeping diff, alignment and export don't change. Fountain and `.thrl` versions can follow each other.
- Story context is per episode (Rob). The newest import wins: its non-empty fields replace the old ones, empty fields don't erase.
- Context is read-only in Cutaway; it's edited in Throughline and re-imported. Sparks, relationships, recycled cards, writer assignment and author contact details are ignored.
- Every change is a command (one undo step); UI, CLI and MCP share the methods.

**12.1 `.thrl` import** (`apc-core::thrl`)
- [x] Detect by content (gzip magic or JSON with `version`), decompress (`flate2`), refuse a `minVersion` newer than the reader knows (1.12).
- [x] Story order from `storyOrder` (older files: computed from `sequence`/`stackedOn`/`stackOrder` as the format guide says). Act cards are skipped; each beat is a scene.
- [x] Heading `INT. SETTING - TIME` from `intExt`, the beat's setting and `timeOfDay`, split like a Fountain heading. `beat.sceneNumber` as the forced number.
- [x] TipTap blocks as Throughline's own Fountain export writes them: action (a blank line starts a new unit), cue with extension, dialogue (blank lines dropped, a `…` line splits), parenthetical (splits), transition, `paragraph` by `screenplayType` else action; `versionBlock` → only the active children. Underline/italic/bold become Fountain emphasis, as in the export.
- [x] Title page: title, credit, authors, draft date (no contact details).
- [x] `script::read_script_file(path)` picks the parser; `store_version_file` keeps the real extension (`Script/v002 Name.thrl`); the MCP `script/N` resource renders a `.thrl` version as Fountain text.
- [x] `split_heading` handles `INT./EXT.`.
- [x] Import page and the script file picker accept `.thrl`.

**12.2 Story context in the document**
- [x] `ParsedScene` gains `title`, `act`, `cast`, `tone`, `beat_id`; `ParsedScript` gains `story` (characters with summary/want/need/flaw/notes, settings with description and parent).
- [x] Fountain fills the same fields: synopses (`=`) → synopsis, top-level sections (`#`) → act, carried to the following scenes.
- [x] `Scene` gains `title`, `synopsis`, `act`, `cast`, `tone`, `beat_id`; `Character` gains `profile`; `Location` gains `description` and `parent`.
- [x] `apply_version` writes scene context from the script, merges `story` newest-wins (names matched to cues exactly, without a leading "THE", or by alias), and reports it in the change summary (`story.characters_updated`, `locations_updated`, `unknown_characters`).
- [x] Scene identity across versions: kept lines first, then the Throughline beat ID, then the heading.
- [x] `import_story` (command `ImportStoryContext`): takes only the context from a `.thrl` (e.g. the pilot's character bios into EP02), no script change.

**12.3 On screen**
- [x] Scene list: the beat title under the scene number, act dividers, the synopsis as tooltip.
- [x] Inspector: a Scene section (title, act, synopsis, cast, setting with its description and parent) and the current line's character profile.
- [x] Room tone: a setting without its own room tone uses its parent's (Canteen → TARMAC).

**12.4 In the AI calls** (behind `ai.story_context`, on by default)
- [x] `sound_spot` v2: the scene's title, act, synopsis, tone, cast (with one-line profiles) and setting description and parent. The prompt treats context as background: no cues the scene text doesn't support.
- [x] `name_match` v2: one-line character profiles, so full names and nicknames in file names match.
- [x] Mock provider accepts the new inputs; context stays small (no per-take calls get it, see D53).

**12.5 Verification**
- [x] Unit tests: synthetic `.thrl` (gzip and plain), story order fallback, version blocks, splitting rules, newest-wins merge, context-only import, beat-ID scene identity, parent room tone, Fountain synopses/sections.
- [x] Fixture test (skips without the files): EP02 `.thrl` gives the same lines as Throughline's Fountain export of the same draft; in the app, importing the `.thrl` over the Fountain export changes 0 lines.
- [ ] (Rob) Manual: import the EP02 `.thrl` in the app, then the pilot's context; check the scene list, inspector and a spotting run.

### M13: Lengthen clips on Arrange (Rob, 2026-11-02)
Why: a cut that sits wrong (a clipped first word, a cut-off tail) has to be pulled out on Arrange, not only trimmed in. Decision D63.
- [x] `set_trim` clamps extensions to the source file; core test.
- [x] Take view gives the source length.
- [x] Arrange strip: source window with handles on both sides, handle peaks from `peaks_batch`, take-edge mark, handles drag out into the window.
- [x] Playwright: lengthen the end, undo.
- [ ] (Rob) Manual: pull out a clipped head and a cut-off tail on a real scene and listen.

### M14: Studio mode, shows in a production pipeline (Rob, 2026-11-02)
Why: a series has one sound library and one folder per episode in the production's own (Syncthing) pipeline, e.g. `tm-2610/`. Cutaway fits into that layout instead of its own. Decisions D64–D68.

Layout (defaults, stored in the show file, nothing tm-2610 specific in code):
```
tm-2610/                       show root (picked in "New show…")
├ tm-2610.cutawayshow          show file (JSON): name, layout, theme, aliases
├ Episodes/Episode 2/Sound/    the episode's Cutaway folder: Episode 2.cutaway, Audio/, Script/,
│                              Backups/, Exports/, Cache/, Logs/, Library/Vocals/ (episode only)
└ Sound/                       show library: sfx/<bucket>/, music/<bucket>/, loose files
   └ Cutaway/                  backdrop.jpg, library.json (licence, provider ids, tags)
```

S1 Show file and layout (core)
- [x] `apc-core::show`: JSON show file (`ShowFile`, `ShowLayout`, theme, aliases), create in an existing root, open, find from an episode by walking up.
- [x] Episode list: `Episodes/*/`, enriched read-only from `episode.jetplane.json` (number, title); has-project flag; sync-conflict and hidden entries skipped.
- [x] `EpisodePaths` for an episode inside a show (`Episodes/<ep>/Sound/`), `asset_path` for `show:` paths; the old SQLite show file and `Library/` show layout go (never used outside tests).
- [x] Tests on a temp tree shaped like tm-2610.

S2 Show library and folder buckets
- [x] `apc-sound::show_library`: scan `sfx/` and `music/` (audio only, no zips, no Syncthing temp or conflict files), hash + duration cached locally by path/size/mtime, ids from the hash, tags from folder and file names, durable metadata from `Sound/Cutaway/library.json` (merged with sync-conflict copies).
- [x] `Library` routes: show assets (`show:sfx/doors/x.wav`) into the show, vocals into the episode's own `Library/`; a moved file is found again by hash.
- [x] Buckets are folders: `SoundBucket.folder`; first-level folders of `sfx/` and `music/` become buckets (track stem folders don't); a bucket's sounds are its folder's files plus what it keeps; new buckets made by hand create their folder, AI buckets create it with their first sound; downloads, generated sounds and drops into a bucket land in its folder; removing a folder sound hides it in this episode only; renames never move folders.
- [x] Spotting reuses an existing folder bucket when the group or its keywords name it (doors, footsteps …).
- [x] Rescan (method, CLI) after Syncthing brings new files.

S3 Sync safety
- [x] WAL folded into the episode file on close and after each autosave snapshot.
- [x] Lock file next to the episode file (host, user, time): opening an episode open elsewhere warns.
- [x] Sync-conflict copies of the episode file are reported on open.
- [x] Imports into a show episode are always copied (no paths outside the pipeline).

S4 Methods, CLI, MCP
- [x] `create_show` (make a root a show), `open_show`, `show_view`, `show_backdrop`, `set_show_backdrop`, `open_episode` (creates the project when missing), `close_show`, `rescan_library`, `migrate_episode` (dry run first).
- [x] CLI `cutaway show init|episodes|open|rescan|backdrop|migrate`.
- [x] Tests: show flow end to end on the demo session; migration copy and verify.

S5 UI
- [x] Start screen: Show (open, new) and Standalone (open, new, demo); recent shows and episodes.
- [x] Show home: the backdrop full bleed, show name, episode dropdown (number, title, has project), Open / Create, set backdrop, rescan, migrate a standalone episode.
- [x] Theme: colours taken from the backdrop tint the stage inside the show's episodes.
- [x] Header: show name and episode switcher; back to the show home.
- [x] Native Show menu (Open Show…, New Show from a Folder…, Open Standalone Episode…, Show Home, Close).
- [x] Playwright: open a show, pick an episode, see the folder buckets.

S6 Migration of Tarmac-EP02
- [x] Show file in `tm-2610/`, dry run, then copy `~/Documents/Cutaway/Tarmac-EP02` → `Episodes/Episode 2/Sound/` (source untouched); verify sources and sounds resolve.
- [ ] (Rob) Manual: set the backdrop, open Episode 2 in the app, drag an Epidemic result into a folder bucket and check the file lands in `Sound/sfx/<bucket>/`; open the show on a second machine after Syncthing has synced.
- [ ] (Rob) `.stignore` on each device: `*.cutaway-wal`, `*.cutaway-shm` (and `Cache/media` if the sync volume matters).

### M15: Sounds beside the dialogue, card A: data layer, room bus and depth (Rob, 2026-11-04)

Spec: `tasks/Bokeh · live prototype-html/audioplay-cutter-handover-arrange-sounds.md`. Decisions: D75–D77. Cards B (Arrange UI) and C (opened sound) build on this.

- [x] Clip parameters on the existing sound clip: pan, depth, auto fade; length within the source; anchors as M7 has them.
- [x] Commands, each one undo step: move/anchor, trim, fades, gain, pan, depth, slip, add with a starting depth, remove. `Batch` for multi-select. Absolute values, so a UI previews locally and commits once.
- [x] The same in the CLI (`cutaway sfx …`) and over MCP, with `list_sounds` and `scene_rooms` to read back.
- [x] One room per scene in the document, `set_scene_room`, kept across script versions; schema 2 migration.
- [x] Room bus in `apc-audio`: FDN reverb (D75), depth macro (D76), `Renderer` shared by the engine and the bounce, warm-up on seek, the tail in both.
- [x] Room controls on Import › Sounds.
- [x] Tests: every command with undo and redo, batch, migration (also on a copy of Episode 2 and through its old undo log), determinism, block-size and seek parity, depth monotonicity, scene isolation, CLI/MCP parity.
- [ ] (Rob) By ear: the depth curve and the default room on real sounds (`depth.rs`, `RoomShape::new`).
- [ ] (Rob) Update every machine on the Syncthing share before a migrated episode reaches it (D76).
- [ ] (Rob) Confirm the gain range (D77).

### M16: Sounds beside the dialogue, card B: the sounds layer on Arrange (Rob, 2026-11-04)

Decisions: D78, D79. Card C (opened sound) builds on this.

- [x] Layout: dialogue cards, spine, length lines, balls, floating bucket with eyes.
- [x] Interactions: add, move and anchor, depth, trim, loudness, pan, hover, select, delete, Space, scrub, ↑/↓, `S` and the Sounds button. One command per gesture.
- [x] Multi-select: ⇧ click, band, one undo step, offsets kept, Esc and empty click clear, keys act on the selection.
- [x] Rules of handover 5: settle, merge, split on hover, ties by added order, sticky lanes, no dimming but hidden buckets.
- [x] Motion of handover 6 with reduced motion; spine and lines as a few paths.
- [x] Keyboard and announcing; map in the spec, help overlay and D78.
- [x] Playback with the room; balls pulse.
- [x] Tests: vitest for the logic, Playwright for every gesture. Checked on a copy of Episode 2.
- [ ] (Rob) Confirm which sounds show on Arrange (`sfx` and `ambience`; music stays on Edit, D78).
- [ ] (Rob) By hand: the feel of a real drag, and playback on real sounds.

### M17: Sounds beside the dialogue, card C: opened sound, room shortcut, keyboard pass (Rob, 2026-11-04)

Decisions: D80–D82.

- [x] Opened sound: wedge, dialogue column, source and used waveform with height by loudness, edges, corner dots, middle, ⌥ slip, quick buttons, header, playhead. One command per gesture.
- [x] Opens by double click on a ball or line and Enter. Closes by Esc, a click outside, a hidden bucket, the sound going.
- [x] Room read-out and quick size in the transport, link to Import › Sounds. Same command, no second copy.
- [x] Changes are heard during playback: the app's own edits never reached a running schedule (D81).
- [x] Keyboard pass: every part of the opened sound, Tab through the balls in time, ⇧ Enter and ⌘A, focus in and back, Enter and Space on buttons in the panel.
- [x] Announcements in one live region; reduced motion on the column, the panel and the view.
- [x] Handover 10: pan and depth in the Edit inspector.
- [x] Tests: vitest for snapping, trim, slip, fades, auto fade and quick buttons; Playwright for every gesture.
- [ ] (Rob) By ear: depth and room size on real material. See RUN_REPORT.
- [ ] (Rob) By hand: the feel of the opened view with a real sound, and VoiceOver in the app.

### Sounds in the real app: drag and preview player (Rob, 2026-11-04)

Decisions: D83, D84.

- [x] Drag with the mouse everywhere: HTML5 drag never reaches the page in the desktop app.
- [x] The bucket drawer's drag lands on the lanes; Finder audio on Arrange is a sound, not a recording.
- [x] Preview player on its own deck: bucket Play leaves the timeline alone.
- [x] Tried in the real WKWebView with the real audio device (in-page mouse events, temp project).
- [ ] (Rob) By hand: a drag with a real mouse, and a Finder drop on Arrange. See RUN_REPORT.

### Drop lengths, loops, room tone per scene, calmer toggle (Rob, 2026-11-04)

Decisions: D85–D87.

- [x] A sound dropped from a bucket: up to 6 s keeps its length; longer is cropped to 10 s (`sound::place`, drops only).
- [x] Loop per bucket sound (on by default for ambience and room tone). Over 6 s with Loop on: the ambience lane, repeating to the scene's end. `Placement.looped`; the length rules, schedule and UI limits know a looped sound may outlast its file.
- [x] Loop seams crossfade in the mixer, for loops and room tone alike. FCPXML repeats the file.
- [x] Room tone per scene: `Scene.tone`, `set_scene_tone`, a picker and level in Import › Sounds › Rooms, CLI. A scene without one plays its location's.
- [x] The Sounds toggle glides and fades: no overshoot, a short stagger.
- [x] Tests, docs, RUN notes.

### ⇧ Space from a sound, rooms for Episode 2 (Rob, 2026-11-04)

Decision: D88.

- [x] ⇧ Space with sounds selected on Arrange plays from the first of them.
- [x] Music keeps its full length on a drop: Rob's decision (D85).
- [x] Starting rooms entered in Episode 2 at Rob's request, through the CLI with the app closed.

### Bucket search, voices in the room, mix settings in Dev (Rob, 2026-11-04)

Decisions: D90, D91.

- [x] Search bar in the docked bucket: filters this scene's buckets, and finds the rest of the library to drag in.
- [x] Voices get a little of their scene's room: a small send per character (`Character.wet`), so a scene's reverb sits on the dialogue too.
- [x] Dev tab › Room mix: each scene's Wet and each character's wet. Wet leaves Import › Sounds › Rooms.
- [x] Arrange shows the action lines between the dialogue, and a sound can snap to one and hang on it.
- [x] Tests, docs, RUN notes.

### End of run
- [x] `docs/RUN_REPORT.md`, PLAN updated
- [x] Pushed `dev` (after the run, at Rob's request)
- [ ] PR `dev → main`: needs `gh` and a `main` branch on the remote (steps in RUN_REPORT)

---

## 5. Risks

| Risk | Fallback |
|---|---|
| Parakeet via ONNX (model size, CoreML EP, crate maturity) | Stub behind `AsrProvider`; `TranscriptImportProvider` drives all tests; skipped test with reason |
| Fuzzy matching speed in Rust debug builds | Bit-parallel LCS + length prefilter; opt-level 3 for analysis crates in dev/test profiles |
| Aligner port drifts from Stage 0 numbers | Port line by line first, then improve; acceptance test pins the fixture counts |
| rmcp API churn (v3) | Read the crate source; fall back to a small hand-written JSON-RPC stdio server recorded in DECISIONS |
| FCPXML not importing in Resolve | Stick to 1.9 and a minimal element set; structural test; manual Resolve check in RUN_REPORT |
| No offline FCPXML DTD | Structural validation test (element/attribute rules from the 1.9 DTD) |
| Real-time audio glitches (decode in callback) | Feeder thread decodes ahead into a ring buffer; callback only copies |
| Tauri WebDriver unavailable on macOS | Playwright against the Vite UI and the HTTP dev bridge (same `apc-app` service) |
| No `gh` CLI / GitHub auth | Keep committing; note in PLAN; Rob opens the PR |
| `main` has no commits | `dev` starts with the untouched package commit; Rob creates `main` from it before the PR |
| Fixtures absent in CI | Synthetic demo session (generated audio + transcript + script) for CI; fixture tests skip with a message |
| Windows CI untestable locally | Cross-platform crates only; Windows job allowed to fail until M8 |
| Claude Code CLI missing | Mock provider by default in tests; live tests `#[ignore]`; app shows provider status |

---

## 6. Progress log

(dated lines, newest last)
- 2026-10-31 M0 done: workspace (10 crates + Tauri shell), brand.json + rebrand check, Svelte 5 app skeleton with bundled fonts, check.sh green, CI for macOS + Windows. `gh` CLI not installed: the PR at the end needs Rob (or gh).
- 2026-10-31 M1 done: entity store + command bus (persisted undo/redo, multi-client rev sync), Fountain parser (matches Stage 0 unit numbering on the fixture, 27 PAUL lines), script versions with ID-keeping diff, autosave/rotation/named versions/revert, copy/reference import with BLAKE3 + relink, shared timing model. 39 tests. Push after M1 was refused by the session's permission check; commits stay local until a push is allowed.
- 2026-10-31 M2 done: media (symphonia decode, envelopes, peaks, EBU R128, sync, BWF), AI layer (tasks + schemas, Claude Code + mock providers, versioned prompts, cache, call log), aligner port (bit-parallel fuzzy ratio, two passes, short-line rule, bleed, edges, overlaps), takes/types/word maps, room classification, note linking, picks. Fixture acceptance passes: 36 PAUL takes on 9 lines, 5/4/4/10 takes, one 'time machine' partial, picks Science T4 / Lunch T4 / edible T3 as defaults, room offset +0.275 s. Analysis runs in 1.2 s. Parakeet via sherpa-onnx behind the `parakeet` feature (fixture run pending at commit time).
- 2026-10-31 M3 done: FCPXML 1.9 (lanes per role, source TC, fades, scene + note markers), SRT, cut list CSV/Markdown, per-scene bounce through the shared offline mixer (MP3 via ffmpeg). Structural FCPXML test green; no FCP DTD on this Mac. Schedule/mixer/offline render of M5 built early (D27). Parakeet fixture run: works end to end, 26/35+ takes (test ignored with reason).
- 2026-10-31 M4 done: apc-app service (method registry with schemas, views, jobs, player with virtual clock, sound glue, synthetic demo session, HTTP dev bridge), `cutaway` CLI with all spec commands, rmcp 3.8 MCP server (tools from the registry, 4 resource kinds, confirm for destructive tools). Parity test: CLI and MCP give identical results on the demo and the TARMAC fixture. apc-sound library/providers written early. Golden cut list on the demo session.
- 2026-10-31 M5 done: schedule (gaps, ripple, anchors, loudness offsets, room-tone bed, lanes/layers), shared mixer with equal-power fades, offline render, cpal engine (feeder thread + ring buffer, ~60 Hz playhead, play/stop/seek, stop-at). Fixture S1 renders at exactly the layout length (23.240 s); splice click test green; engine smoke test played on this Mac's device.
- 2026-10-31 M6 + M7 done: Svelte 5 UI to the Option C prototype with tokens in app/src/theme (Options deck/pills/notes, Arrange rows/strips/pauses/reorder, box editor comp + edit modes, inspector, unmatched bin, settings, history/versions, export, importer + summary card, welcome), Tauri shell (rpc + dialogs + Finder open). Edit page (dialogue lanes, SFX layers, ambience, music, ripple/⌥ move, nudge, gain, fades, pin/unanchor, anchor lines), sound bucket (spotting, search, generate, choose, place, merge, library tab, drag to timeline). Playwright smoke tests green (takes with arrows, play/stop, delete word + undo). M7 service tests green (anchoring after take swap and gap change, pinned stays, Edit→Options sync, spotting on demo and fixture S2).
- 2026-10-31 M8 partly done: portability fixes and a blocking Windows CI job; the job itself has not run because pushes from this session are refused.
- 2026-10-31 End of run: RUN_REPORT written. Next run starts with: push + PR, first Windows CI result, Parakeet recall on the fixture (VAD chunking), Epidemic/MCP sound providers, single-take audition in the box editor.
- 2026-10-31 dev pushed to origin (7d3ffc4..513913f) after the run, at Rob's request.
- 2026-10-31 M9 done (second run): the four Round 2 screens implemented and wired (Import › Files, Import › Sounds, Edit docked and full screen) on a new bucket/match/lane-mix backend. Download-on-drop checked end to end against a local mock of the Epidemic API; light theme checked on all four screens. Playwright 5/5, check.sh green.
- 2026-11-02 Fixes from Rob's first real session pushed (mixer sample-exact, bleed relative to the mic, short-line roles, Claude CLI lookup, CLI replies over 64 KiB, loudness matching off for playback). M10 done: sentences and scene timeline, take rescue (plain-text scene input, ids back to words), scoped re-analysis (character, line) in app/CLI/MCP, dev marks with proposals, Dev page (under the hood, link and send, learn with dry-run variants, examples, glossary). Sara on a copy of Ep2 with Claude: 6 rescue calls of 5–14 s, 6 takes rescued, 72 takes (was 42 before the bleed fix). check.sh green, Playwright 7/7.
- 2026-11-02 Push of the M10 commits (206dda8..fe853e5) was refused by the session's permission check; they are committed on local dev. Rob pushes, or allows the push. (Pushed later with the `editor` merge.)
- 2026-11-02 M11 done on branch `editor` (worktree `../cutaway-editor`, so the session on `dev` is not disturbed): fixed time scale, playhead scrub and follow, real dialogue splits by source time, clip mute and ripple delete in every view and export, trim and fade handles, per-piece waveforms from the cached peaks, lock to script (pinned clips leave the ripple chain), collapsed group previews, word row, multi-select and Logic keys. 61 core tests (8 new), schedule and FCPXML/SRT mute tests, Playwright 13/13 (6 new in `editor.spec.ts`), check.sh green.
- 2026-11-02 Branch `epidemic` (D60): Spot script on demand (new and changed scenes, or re-spot all; background job with progress), Epidemic bucket pages of 5 with a stored offset, free Epidemic search bar with drag into buckets or a new bucket (pinned previews). CLI `sfx spot-script`, `sfx search`. Playwright gets an Epidemic stand-in (`app/tests/epidemic-standin.mjs`) and keys in memory. check.sh green, Playwright 8/8. Not verified against the real Epidemic API (needs Rob's key).
- 2026-11-02 `editor` (M11, D54–D59) and `epidemic` (D60) merged into `dev`; the epidemic hand-off fixes (spotting per scene makes buckets, bucket edits on the latest document, exact "more", re-spot tidies) and the toast moved off the top bars. check.sh green, Playwright 14/14; pushed to origin/dev.
- 2026-11-02 M12 done (D61) on branch `throughline` (worktree `../cutaway-throughline`, because another session works in the main folder on `dev`): native `.thrl` reader (gzip JSON, format up to 1.12) giving the same parsed script as Fountain, story context per episode (scene title/summary/act/cast/arc, character bios, setting descriptions and groups) merged newest-wins, `import_story` for bios from another file (the pilot's), scene list titles and act dividers, inspector story section, parent room tone, story context in spotting and file-name matching (prompts v2, `ai.story_context`). EP02: the `.thrl` matches Throughline's Fountain export line for line. Fixed on the way: `!CUT TO:` parsed as a transition, a flaky dev-panel eval test. check.sh green.
- 2026-11-02 Pacing (D62): breath gap down to −500 ms (overlap), tightness slider right = tighter up to ×10. M13 done (D63): Arrange lengthens clips into handle audio from the source, `set_trim` clamps to the file. check.sh green, Playwright green.
- 2026-11-02 M14 done (D64–D68): studio mode. A show is the production folder (tm-2610) with a JSON show file at its root; episodes in `Episodes/<Episode>/Sound/`, the show library is `Sound/{sfx,music}` with its folders as buckets (scan + local hash index, `Sound/Cutaway/library.json`), new bucket sounds saved into the folder, vocals stay with the episode. Sync safety (WAL checkpoint, advisory lock, conflict warnings), backdrop + theme colours, show home with episode dropdown, native Show menu, CLI `cutaway show …`, migration with dry run. Tarmac-EP02 copied into `tm-2610/Episodes/Episode 2/Sound/` (561 files, 4.9 GB, 0 missing; source untouched). check.sh green, Playwright 16/16 (2 of 7 full runs had timing failures in editor/smoke specs under load, unrelated to this work).
- 2026-11-02 Playback leveling (D69) on branch `audio-level` (worktree `../cutaway-audio-level`, because another session works in the main folder on `dev`): on by default, per-recording offsets from the median take LUFS, take and sound auditions matched per item, −1 dBFS safety limiter on the output; playback only, bounces and exports unchanged. Settings toggle, `playback.level` config key.
- 2026-11-02 `audio-level` merged into `dev` (decision renumbered D69). Checked on a copy of Tarmac-EP02: role offsets −3.4 … +8.9 dB; across the 8 scenes with two or more roles the loudness spread between roles drops from 8.3 dB to 3.0 dB on average; scene 1 plays on the real device under the −1 dBFS ceiling. Playwright `playback.spec.ts` for the Settings switch. check.sh green.
- 2026-11-03 Editor feel (D70–D72), on `dev` (worktree `../cutaway-dev`, because another session works in the main folder on `director-text`): ⌥ scroll zooms the Edit page; ⇧ scroll flies through lines on Options/Arrange; drags hold their preview until the command lands (no snap-back) and pause rows follow the mouse; undo/redo wait for an in-flight command; **Follow** playhead switch (header + Settings, on by default); lead-in 100 ms / tail 150 ms / edge fade 20 ms around every line, heard and exported, stopping short of other words (Settings sliders, `pacing.*` keys); boxes and word lanes show the script's words timed by what was heard. check.sh green, Playwright 17/17. `gh` not authenticated: no PR opened.
- 2026-11-04 Branch `sfx-matching` (D74): show sounds reach the buckets by file name (matched when the view is read, nothing stored), a Loose SFX bucket, whole-show buckets in the docked bucket, and Clean up: one AI call proposes folders for loose SFX, a review sheet moves what is confirmed, placed sounds are repointed. On a copy of Episode 2: spotted buckets without sounds 13 → 0, SFX in a bucket 68 → 104 of 104, reachable from the docked bucket 26 → 104. One live clean-up plan on the copy: 23 moves, 13 left loose. Nothing moved in the real pipeline. check.sh green, Playwright 15/18: `editor.spec.ts` (trim handles, Arrange lengthens a clip) and `smoke.spec.ts` (flip takes, delete a word) fail with identical numbers on untouched `dev` at bc2d9e4, so they predate this branch; not investigated here.
- 2026-11-04 M15 card A done on `feature/1-sound-fx-a-data-layer-room-bus-and-depth` (D75–D77): sound clips gain pan, depth and auto fade; undoable commands for all of it from the UI, CLI and MCP; one room per scene (schema 2) with controls on Import › Sounds; a 16-line FDN room on a shared bus through one `Renderer` in the engine and the bounce, bit-identical from the start and below −80 dB after any seek. Depth 0 → 1 on noise: room against sound −30.8 → −4.8 dB, 12.7 dB quieter, 5× darker. `float_roundtrip` fixes last-bit drift on undo (12 % of full-precision values). A copy of Episode 2 migrated (16 scenes), and undo and redo through its old log came back hash-identical. Fixture tests run and green. check.sh green, Playwright green. Not listened to: the curves wait for Rob's ears.
- 2026-11-04 M16 card B done on `feature/2-sound-fx-b-arrange-sounds-layer-spine-le` (D78, D79): the sounds layer on Arrange. Spine, sticky length lines, balls and the bucket with eyes, every gesture one command on release with a local preview until it lands, multi-select as one undo step, settle/merge/split per handover 5, motion per 6 with reduced motion. Logic in `lib/sounds` with vitest (new, in check.sh), every gesture in Playwright; three deliberate breakages were caught by the right tests. On a copy of Episode 2 (scene 2: 27 lines, 17 overlapping) no errors and drag moves at about 17 ms; that run also found the header wrapping on real scene names, and cards running under the header. Fixed.
- 2026-11-04 `dev` brought together: `sfx-matching`, Sound FX A (merged by the board) and the Bokeh reference. The Playwright failures noted above were test timing, not app bugs: `openEdit` measured clips before the timeline's first measurement, and the lengthen test grabbed its handle while the column was still gliding. The trim test failed before its undo, which failed the tests after it. check.sh green, Playwright 21/21 on 8 runs in a row.
- 2026-11-04 M17 card C done on `soundeffects-ui` (D80–D82): the opened sound on Arrange, the room in the transport, and the keyboard and screen reader pass over the feature. The branch was new: cards A and B had merged into `dev`, so it starts from there. Pan and depth were missing from the Edit inspector, which handover 10 asks for: added. A browser test found the wheel dead over the view's handles (the app ignores wheels over sliders): fixed. The app's own edits never reached a running schedule, so a room change was only heard from the next Play: `playhead` now reloads when the schedule differs (D81). check.sh green, Playwright 85/85. Not listened to, and not tried on real material or with VoiceOver: that waits for Rob.
- 2026-11-04 Sounds in the real app (D83, D84): Rob couldn't drag a sound in, and bucket Play moved the playhead. Every drag was HTML5 drag and drop, which Tauri's file drop keeps from the page, so it only ever worked in the browser tests: all drags now use the mouse. A preview has its own deck and a small player in the bucket panels. A Finder drop on Arrange no longer files a sound as a recording. Checked in the real webview with the real device through in-page mouse events; a real mouse and a Finder drop wait for Rob.
- 2026-11-04 Drop lengths, Loop, room tone per scene, calmer toggle (D85–D87). Under 6 s a dropped sound keeps its length, past it 10 s, or with Loop on it repeats on the ambience lane to the scene's end. Each Rooms row picks a room tone. Loop seams crossfade in the mixer: they clicked before. FCPXML repeats the file. The toggle glides and fades. Rust tests, Playwright; not listened to, and the toggle's feel is Rob's to judge.
- 2026-11-04 ⇧ Space from a selected sound on Arrange (D88). Rooms entered in the real Episode 2 at Rob's request: 16 `set_scene_room` commands with the app closed, rehearsed on a copy, and compared with a backup afterwards: only the 16 scenes' rooms changed.
- 2026-11-04 Arrange moves evenly (D89): the column's overshoot was two motions adding up, 170 to 260 px past its mark, not a curve. One move now, on one curve, measured frame by frame in a test. The pop lost its overshoot too.
- 2026-11-04 Bucket search, voices in the room, Room mix in Dev, actions on Arrange (D90, D91). Each voice now takes a little of its scene's room, 0.25 by default, and every scene rings out a little past its last line. Wet moved to Dev › Room mix. Actions read in full between the cards and take sounds. Not listened to: the voice send is Rob's to judge.
