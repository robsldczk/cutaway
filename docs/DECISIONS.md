# Decisions

One entry per decision: context, choice, reason. Newest at the bottom.

## Already decided by Rob (before the first build run)

- **D1 Platform:** desktop first, macOS first, then Windows, web later. Large files stay local.
- **D2 Stack:** Tauri app, Rust core, web UI. Swift was ruled out because it locks the app to the Mac.
- **D3 Transcription:** local Parakeet TDT (the NVIDIA model Rob uses in MacWhisper), downloaded once. Optional whisper.cpp fallback.
- **D4 AI interpretation:** through a model-agnostic provider interface. The first provider is the Claude Code CLI with Sonnet 5.5 at high effort. AI only interprets (matching, notes, picks, spotting); it never transcribes.
- **D5 Project format:** Logic-style visible folders. A show folder holds the shared library and the episodes; each episode folder holds the openable project file, audio, script versions, backups, exports, cache and logs. Audio is copied into the project by default.
- **D6 Safety:** autosave on every command; backup snapshots every 10 minutes; named versions; versioned scripts.
- **D7 One timeline, three pages:** Options, Arrange and Edit display the same document, as in DaVinci Resolve's Cut and Edit pages. Dialogue is editable on every page, including Edit.
- **D8 Remote control:** a built-in MCP server, plus a CLI with the same commands.
- **D9 Export:** FCPXML (Resolve and Logic) instead of Resolve's proprietary .drt.
- **D10 Git:** all work on `dev`, pushed regularly; Rob merges to `main`.

## Decisions made during implementation

(Claude Code appends here.)

### D11 App name and central naming (Rob, 2026-10-31)
- **Context:** Rob named the app *cutaway* and wants the name changeable later.
- **Options:** rename everything by hand; one config file read by every layer.
- **Choice:** `brand.json` at the repo root holds `displayName` (Cutaway), `slug` (cutaway), the file extensions (`.cutaway`, `.cutawayshow`), the bundle id and the CLI name. Rust reads it through a build script in `apc-core`; the UI imports it; `scripts/rebrand.mjs` syncs `tauri.conf.json` and the CLI `[[bin]]` name, and `scripts/check.sh` fails on drift. Crates keep the internal `apc-` prefix.
- **Reason:** one edit renames the product; internal names never reach users.

### D12 Visual design from the Option C prototype (Rob, 2026-10-31)
- **Context:** Rob chose the clickable prototype *Option C, Arrange, Box editor* as the design.
- **Choice:** the frontend spec was rewritten around it (see its *Changed for the Option C design* section). The prototype moved from the repo root to `docs/design/prototype/` so its path has no spaces.
- **Reason:** the prototype is the source of truth for looks; the spec keeps the behaviour the prototype doesn't show (Edit page, inspector, importer, bucket).

### D13 Design tokens kept separate (Rob, 2026-10-31)
- **Choice:** `app/src/theme/tokens.css` (CSS custom properties) + `palette.ts` (character colors, take shades, helpers). Components never use literal colors. A skin is a folder with the same token names; light theme via `[data-theme="light"]`.
- **Reason:** re-skinning without touching components.

### D14 Favorites and director picks
- **Context:** the prototype marks *favorites* (★, key F); the spec had a separate director-pick star.
- **Choice:** one ★ concept. Picks found by the analysis are stored as `pick` strength and seed the favorite with origin `director`; the user toggles favorites (origin `user`). The inspector shows the origin.
- **Reason:** the prototype shows one star; two stars would compete.

### D15 Keyboard conflicts
- **Context:** the spec used I for the inspector; the prototype uses I/O for in/out on Arrange.
- **Choice:** I/O = in/out on Arrange; I = inspector on Options and Edit. Enter = play line in context (spec), kept.
- **Reason:** follows both sources with the least surprise; listed as an open question for Rob.

### D16 Overlap by negative pause in the MVP
- **Context:** overlap was V2 in the spec; the prototype's Arrange page lets a pause go below zero.
- **Choice:** gaps may be negative (down to −1 s) in Arrange; the timeline overlaps the clips. Option-drag overlap on the Edit page stays as specified.
- **Reason:** the prototype shows it and it costs little in the timing model.

### D17 Git start without a `main` commit
- **Context:** the remote was empty and local `main` had no commit, so `dev` could not branch from `main`.
- **Choice:** `dev` starts with the untouched package as its first commit (`chore: initial package`). Nothing was committed or pushed to `main`. Rob can create `main` from that first commit (`git push origin <sha>:refs/heads/main`) before merging the PR.
- **Reason:** the rules forbid commits to `main`.

### D18 Extra crate `apc-app` (service layer)
- **Context:** the UI, CLI and MCP must share the same commands, and the orchestration needs analysis, audio and export together, which `apc-core` cannot depend on.
- **Choice:** `apc-app` holds the `Service` (open project, method registry with JSON schemas, jobs, view models, demo generator, HTTP dev bridge). The CLI, the MCP server and the Tauri shell are thin layers over it.
- **Reason:** one dispatch table guarantees identical behaviour across clients.

### D19 Entity store with generic changes
- **Context:** every command must be persisted immediately and be undoable from any client.
- **Choice:** one `entity(kind, id, parent, ord, data JSON)` table plus typed SQL views; commands produce `Change {kind, id, before, after}` lists, so undo/redo is generic. The log stores the changes.
- **Reason:** undo can't drift from the command logic; schema stays queryable through views.

### D20 AI output validation
- **Context:** results must be schema-validated JSON.
- **Choice:** schemas are generated with `schemars` from the Rust I/O structs (and sent to the provider); validation is strict typed deserialization (`deny_unknown_fields`) plus range checks, instead of a JSON-Schema validator crate.
- **Reason:** one definition for schema and type; far fewer dependencies.

### D21 Show-level operations
- **Context:** the command bus works on the episode document; aliases, library assets and locations live in the show file.
- **Choice:** show-level operations write the show database directly and are logged in its `command_log`; they are not undoable from the episode history. Deleting a library asset moves its file to `Backups/Trash/` instead of deleting it.
- **Reason:** one undo history per open document keeps the semantics simple; the show library changes rarely and destructive actions stay recoverable.

### D22 Timing model details
- **Context:** the specs fix "gap to the previous clip" but not how action lines and the breath gap combine.
- **Choice:** the pause before a box is its `gap_s` override, else `(breath gap + enabled spacers since the previous box) × scene tightness`. Spacer length defaults to `0.6 s + 0.04 s per word` (max 2.5 s). Lines without takes take no time. A new script version resets the scene order to script order (ad-libs keep their place).
- **Reason:** matches the prototype's single pause per gap row and keeps one number per gesture.

### D23 Parakeet runtime: sherpa-onnx
- **Context:** the brief asks for a short spike between `sherpa-onnx` bindings and the `ort` crate for Parakeet TDT 0.6B v3.
- **Options:** `ort` (ONNX Runtime directly; we would write the mel front end and the TDT greedy decoder ourselves) or the official `sherpa-onnx` crate (Apache-2.0; ships prebuilt static ONNX Runtime; supports NeMo transducers incl. Parakeet TDT v3 with token timestamps and durations).
- **Choice:** `sherpa-onnx` 2.0.1 behind the cargo feature `parakeet` (off by default). The spike built in 12 s without cmake. The model is the int8 release tarball from the sherpa-onnx `asr-models` release, unpacked into `<app support>/cutaway/models/`. Its BLAKE3 is recorded on first download (trust on first use) because no upstream checksum is published in a form we can pin; pin it in `PARAKEET_TDT_V3` once known. Execution provider: CoreML on macOS, DirectML on Windows, CPU fallback (`CUTAWAY_ONNX_PROVIDER` overrides).
- **Reason:** least custom DSP code and a maintained decoder; keeping it optional keeps CI light.

### D24 Claude Code effort setting
- **Context:** the spec wants Sonnet 5.5 at high effort through `claude -p --output-format json`; the CLI flag for effort could not be confirmed offline (the `claude` CLI is not installed on this machine).
- **Choice:** the provider passes `--model` and exports `CUTAWAY_AI_EFFORT`; extra CLI flags come from `CUTAWAY_CLAUDE_ARGS`, so the effort flag can be added without a rebuild. Listed as an open question.
- **Reason:** never guess at CLI flags that could break every call.

### D25 Mock AI provider
- **Choice:** the mock answers every task with deterministic rules (Stage 0 pick phrases, keyword note condensing, keyword spotting) and can replay recorded responses from `<dir>/<task>/<input-hash>.json`. The `line_match` input carries the aligner's proposal and score; the mock confirms it.
- **Reason:** tests run offline and deterministic; recorded responses from real runs can't be committed (they contain fixture text), so they stay in `fixtures/`.

### D26 FCPXML details
- **Context:** the brief asks for an FCPXML version Resolve imports (1.9 suggested), one track per role, source timecode, handles and crossfades.
- **Choice:** FCPXML **1.9**. A 1080p25 sequence holds one empty gap in the spine; every clip is a connected `asset-clip` on its role's lane (`-1` = first character, then sound lanes) with `audioRole` `dialogue.<Name>` / `effects` / `music`. Times are exact rationals in 1/48000 s (not frame-rounded). Assets point at the original files (`media-rep`) and cover the whole file, so handles are always available in the NLE. Splices are butted within a lane with short `fadeIn`/`fadeOut` on both sides (connected clips on one lane can't overlap). Scene and note markers sit on the gap.
- **Reason:** the smallest element set Resolve and Logic both read; sample-accurate edits matter more than frame alignment for dialogue. Needs Rob's manual import check (RUN_REPORT).

### D27 Offline render before the real-time engine
- **Context:** M3 allows an ffmpeg bounce until M5.
- **Choice:** the schedule, mixer and offline render of the M5 engine were built during M3, so the bounce already uses the shared graph; ffmpeg is used only to encode MP3.
- **Reason:** avoids a throwaway bounce path.

### D28 Epidemic Sound and MCP-client sound providers are stubs
- **Context:** M7 asks for Epidemic Sound search (API or MCP if available) and optionally an MCP-client generation provider (Magnific).
- **Choice:** both sit behind the provider traits and report "unavailable" with a reason; ElevenLabs generation and local library search are real. `[sound.search]` / `[sound.generate]` in the project config pick the providers.
- **Reason:** no Epidemic API credentials or documented endpoint were available offline, and an MCP client inside the app is a larger piece of work; the interfaces are in place.

### D29 Analysis jobs run on a worker thread, not a separate process
- **Context:** the architecture wants analysis in a separate process so a crash never blocks the editor.
- **Choice:** `start_analysis` runs on a worker thread with its own database connection and `catch_unwind`; the editor picks up the result through the revision check. The CLI (`cutaway align`) is the process-separated path today.
- **Reason:** same isolation for panics and long jobs with far less plumbing; a sidecar process can replace the thread later without changing the UI.

### D30 One service, many clients
- **Choice:** `apc-app::Session::call(method, params)` is the only entry point. The registry (`methods()`) carries a JSON schema per method; the MCP tool list is generated from it, the CLI maps subcommands onto it, the Tauri shell and the HTTP dev bridge forward `rpc(method, params)` to it. A `command` method accepts any document command (used by the UI).
- **Reason:** identical behaviour across UI, CLI and MCP is guaranteed by construction (the parity test checks it).

### D31 Relative bleed detection
- **Context:** Stage 0 drops takes quieter than −38 dB. On the demo session a reader's cue picked up by an open mic passed that threshold.
- **Choice:** a take is bleed if it is below −38 dB **or** more than 10 dB below the median level of that mic's takes.
- **Reason:** absolute thresholds depend on gain staging; the fixture results are unchanged.
