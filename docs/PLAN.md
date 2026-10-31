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
| `note_condense` | `{takes:[{take_id, line_text, direction_text, feedback_text}]}` | `{notes:[{take_id, direction (≤ 6 words), feedback (≤ 6 words), tags:[string]}]}` |
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
- [ ] Theme tokens + palette from the prototype; fonts
- [ ] Bridge (Tauri + HTTP dev bridge), stores, keyboard map
- [ ] Header, scene rail, transport + scene map, hint line, help overlay
- [ ] Options page (deck, take pills, notes aside, action pills, word highlight, passage mode)
- [ ] Arrange page (rows, waveform strips, trims, locator, I/O, gap drag, overlap, reorder)
- [ ] Box editor (comp mode, edit mode)
- [ ] Inspector, settings sheet, unmatched drawer, importer, summary card, history/versions, export panel
- [ ] Playwright smoke tests (open project, arrow keys switch takes, play/stop, delete a word in the box editor and undo)

### M7: Edit page, show library and sound
- [ ] Edit page: dialogue lanes per role, SFX layers, ambience, music; ripple / Option-move; nudge, trim, fades, gain, anchors; "edited in Edit" markers
- [ ] Show library (folders, dedup, tags), Finder drag-in to bucket / timeline
- [ ] Bucket (cues per scene + show-wide, statuses, audition, editing, merge)
- [ ] AI spotting per scene → anchored cues
- [ ] Providers: library search, ElevenLabs, Epidemic (stub), MCP-client provider (stub)
- [ ] Tests: anchoring follows take swap / gap change, pinned stays; Edit→Options sync; spotting on S2 with mock AI

### M8: Windows readiness
- [ ] Cross-platform paths, dialogs, audio device; ONNX DirectML/CPU provider selection
- [ ] Windows CI job green (build + unit tests)

### End of run
- [ ] `docs/RUN_REPORT.md`, PLAN updated, pushed, PR `dev → main`

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
