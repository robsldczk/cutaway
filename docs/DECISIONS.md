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

### D32 UI framework and testing
- **Choice:** Svelte 5 (runes) + TypeScript + Vite inside Tauri 2. The UI reaches the core only through `rpc(method, params)`: a Tauri command in the app, the HTTP dev bridge (`cutaway serve`) in a browser. UI smoke tests run in Playwright (Chromium) against the bridge on a generated demo session, because Tauri's WebDriver isn't available on macOS.
- **Reason:** Svelte was preferred in CLAUDE.md; the bridge makes the real core testable from a browser.

### D33 UI details not settled by the prototype
- **Playhead:** the UI polls `playhead` every 33 ms while playing (≈30 Hz on screen); the engine itself produces ≈60 Hz events. Simpler than event plumbing through two transports.
- **Box editor lanes** are scaled per take (as in the prototype); painting snaps to word boundaries and is stored as script-word ranges, so the comp is text-aligned in the document even though lanes are drawn per take.
- **Edit mode** in the box editor: click words then ⌫ to delete; click a struck word to restore; a chip on each cut toggles close/keep pause; drag the blue pause markers left to shorten a pause; edge handles trim head and tail.
- **Auditioning a single take lane** plays the comp (the engine plays the scene timeline); clicking a take lane moves the locator to the same script word. Listed as an open question.
- **Finder drops:** a script imports a version; audio imports as role tracks on Options/Arrange, into the library with the bucket open, and into the library plus a placement at the playhead on the Edit page.
- **Fonts** are bundled (`@fontsource`, SIL OFL) so the app works offline.

### D34 In/out wins over the inspector key (Rob, 2026-10-31)
- **Supersedes D15.** I and O set in and out at the locator on Options and Arrange; the inspector is ⌘I (or the low-confidence dot, or a long press).

### D35 Claude at medium effort (Rob, 2026-10-31)
- **Supersedes the "high effort" default of D4 and D24.** `ai.effort` defaults to `medium`; the provider still passes it through `CUTAWAY_AI_EFFORT` and extra CLI flags through `CUTAWAY_CLAUDE_ARGS` until the CLI flag is confirmed on a machine with `claude` installed.

### D36 Parakeet: speech-region decoding (Rob asked for one improvement cycle, 2026-10-31)
- **Finding:** with fixed 25 s chunks Parakeet returned nothing for whole stretches that were mostly silence (5:38–5:50, 8:50–9:20 on the fixture) and merged quick repeats.
- **Choice:** decode speech regions found in the loudness envelope (start at +10 dB over the noise floor, last while above +4 dB, gaps < 0.35 s merged, 0.25 s padding, ≤ 25 s). Region ends also end transcript segments.
- **Result:** Paul's track 376 → 563 words (MacWhisper: 678), decoding twice as fast; takes 26 → 31 of 35+; Science T4 and edible picks found. Still missing: most of the five "Whoooaaah!" takes (spelled W, WOO, Wow, Ooooooo), "Mh-hm", and "Lunch" T4's explicit pick (Parakeet drops "…was the right one"). The fixture test stays ignored with these numbers.

### D37 Sound service keys in the OS keychain (Rob, 2026-10-31)
- **Context:** connect Epidemic Sound with an API key without anyone being able to steal it.
- **Choice:** keys are stored with the `keyring` crate (MIT/Apache-2.0) in the macOS Keychain / Windows Credential Manager under the bundle id, one entry per service. `connect_service` checks the key with a test search before storing it; no method ever returns a key; keys never enter the project file, the command log, `ai-calls.jsonl` or exports; `connect_service`/`disconnect_service` are not MCP tools (an AI client never handles keys). The CLI reads keys from a hidden prompt. Environment variables still work as a fallback. ElevenLabs uses the same store.
- **Epidemic API:** `GET /v0/sound-effects/search` and `/v0/sound-effects/{id}/download?quality=high` on `https://api.epidemicsound.com` with `Authorization: Bearer <key>` and an anonymous `x-partner-user-id` (hash of bundle id + OS user). Search results are remote candidates; "Get" downloads one into the library as licensed (one download against the plan). `length` is taken as seconds (not stated in the docs).

### D38 Light theme
- **Choice:** colours that the prototype used at many alphas became four RGB bases (`--ink-rgb`, `--veil-rgb`, `--night-rgb`, `--shadow-rgb`); components use `rgb(var(--ink-rgb) / a)`. The light theme redefines the bases and the panels (warm paper `#F2EFE8`, white glass, dark ink), uses darker label tints per character colour (`nameLight` in `palette.ts`) and mixes take gradients toward the paper colour. The choice is per viewer (Settings → Appearance), dark by default.

### D39 Single-take audition (Rob, 2026-10-31)
- **Choice:** `play_take` plays one take alone at matched loudness; in the box editor clicking a take lane plays it from the click, ▶ marks the source, and Space plays the last source (comp or take), as in the prototype.

### D40 Sound buckets (Round 2 Import › Sounds and the docked bucket)
- **Context:** the Round 2 screens group sounds into named buckets whose keywords are the search; previews stream, downloads happen only when a sound is kept or dropped.
- **Choice:** a `SoundBucket` entity (name, keywords, scenes, cues, kept asset ids, previews, dismissed remote ids, length and leave-out filters, AI flag). "Spot script" runs the existing AI spotting per scene and groups its cues into buckets by a deterministic word table (Food and foil, Doors and hatches, Phones, Machines, Sci-fi, Footsteps, Walla and crowd, `<place> ambience`, Music, else Props); keywords are the last two content words of each cue. New spotted buckets leave out "music" and "voices" by default; ambience buckets ask for ≥ 10 s.
- **Search:** Epidemic results become previews, 12 per page, skipping dismissed, kept, excluded and out-of-length results; "Load 12 more" asks the API for the next offset. Previews are cached in `Cache/previews/` for audition only. Library search covers each keyword.
- **Drop:** dropping a kept sound places it at once; dropping a preview starts a `download` job (progress for the UI) that downloads, keeps and places it. The worker shares the main session's credential store.

### D41 Import matching and room links
- **Choice:** "Redo match" (`match_files`) groups files into sessions by overlapping timecode, else by the folder they were dropped in (the pipeline's rule), and records when it ran. `link_room` makes a room file and exactly the chosen recordings one session and marks them `link_locked`; a redo keeps locked files together as they were. The analysis uses the matched sessions when they cover every file, so a hand link changes sync and note linking.
- **Sync offsets** are measured by the analysis, not by the match; the cast card shows a room's offset once it is known.
- **Suggest** uses the file name, then AI name matching. Content matching (the transcript against each character's lines) needs a transcript first and is left to the analysis's own role check.
- **"Remember my fixes for the whole show"** maps to `confirm_roles(remember)`: when on, name corrections become show aliases (as before); when off, they apply to this episode only.

### D42 Lane mix
- **Choice:** per-lane volume (−60…+12 dB), constant-power pan (centre unity), mute and solo stored as `LaneMix` entities keyed by lane (character id, `sfx1…`, `ambience`, `music`, `roomtone`) and applied by the schedule, so playback and the mix bounce agree. Fades gain a curve (equal power by default, linear). The Split tool is the `split_placement` command.
- **Mixer view (X):** the rationale leaves it undesigned; the button opens the lane strip (volume, pan, meter) of the edit inspector for every lane at once.

### D43 Round 2 UI details not settled by the designs
- **Import replaces the importer sheet.** Dropping recordings anywhere outside the Edit page imports them, runs the (rule-based) match and opens Import › Files. A new episode opens on Import. The other tabs are disabled until a script is in.
- **B keeps two meanings:** on the Edit page it shows or hides the docked scene bucket; on Options and Arrange it still opens the cue drawer (cue-level search, generate, choose). ⇧B always switches between the editor and Import › Sounds.
- **Snap: words** snaps drops and pinned-sound moves to the nearest word start or action line within 10 px; a snapped drop is anchored there (it follows the dialogue), an unsnapped one is pinned to its time.
- **Marquee** sets the cycle range (the app has no range operations yet); dragging in the ruler does the same, a click seeks.
- **Lane meter:** the engine reports the output peak only, so the inspector's lane meter shows the output level while a clip of that lane sounds at the playhead, scaled by the lane volume. A true per-lane meter needs per-lane peaks from the mixer.
- **Preview waveforms:** previews have no audio until auditioned or downloaded, so their waveforms are a quiet stand-in shape derived from the id; downloaded sounds show their real peaks.
- **Display names** drop the folder, the extension, the download id ("(Epidemic 1000)") and duplicate counters; the files keep them.
- **Inspector values** are text fields that accept "−4,5", "-4.5" or "40", so they read the same in every locale.
- **Spec notes:** `docs/specs/frontend-design.md` and `how-it-works.md` are read-only in this checkout, so the Round 2 behaviour is written up in `docs/design/round2-screens.md` instead of a "Changed in implementation" note in the spec.

### D44 Back to the start screen, and saving (Rob, 2026-10-31)
- **Choice:** the episode name in the header opens a menu with *Back to start screen* (`close_project`). Closing stops playback and, if anything changed since the last auto snapshot, writes one more snapshot into `Backups/`; the same happens when switching episodes and when the app quits. Closing is refused while a background job runs, so an analysis or a download can't lose its window. The start screen lists the last six episodes opened (kept per viewer in the browser storage).
- **Saving as it stands:** every change is written to the episode file (SQLite, WAL) the moment its command runs, so there is no Save command and nothing to lose on a crash. On top of that, a full snapshot goes to `Backups/` every 10 minutes while you work (the last 20 plus one per day are kept), and ⌘S / History saves named versions that are never rotated.

### D45 Director room files pair by name (Rob, 2026-10-31)
- **Finding:** without overlapping timecode, every file dropped from one folder became one session, so each director room file was linked to every recording ("Sara-Director_v001" with Paul's session too).
- **Choice:** after grouping by timecode or folder, a room file whose name points to a character (name, part of it, close spelling or alias; room keywords ignored) becomes a session with that character's recordings only. With several recordings of the character, the ones sharing the most other name tokens win ("v001" with "v001"). It looks in its own group first, then in all groups (recordings with timecode, director files without). Room files that name nobody stay with the rest of their group as an ensemble room. Redo match and the analysis use the same rule; hand-set links still win.

### D46 Parakeet on by default (Rob, 2026-10-31)
- **Supersedes "off by default" in D23.** The desktop app and the `cutaway` CLI build with the `parakeet` feature (sherpa-onnx, static ONNX Runtime downloaded by its build script). `cargo build -p apc-cli --no-default-features` still builds without it. Episodes whose files all have transcript sidecars keep using them.
- **First run:** the ~490 MB model downloads before the analysis starts, shown as its own progress ("Downloading the speech model (once): …"), into `<app support>/cutaway/models/`.
- **Checked:** the TARMAC fixture without its transcript files, through the CLI release build: 31 takes on 8 lines, room sync +0.275 s, 2 picks, 1 min 28 s for 2 × 9 min on this Mac (Stage 0 with MacWhisper transcripts: 36 takes on 9 lines).

### D47 Character vocal buckets, filled by the analysis (Rob, 2026-11-01)
*Removed by D118.*
- **Choice:** the main analysis collects each actor's small sounds from their own mic and keeps them in that character's vocal bucket ("Paul · vocals"). Sources: interjections the recogniser heard outside takes (or the takes of a line that is only an interjection, like "Mh-hm…"), and sounds without words between takes that are louder in this mic than in the other actors' mics (+6 dB, not bleed). Tags: mhm, ah, whoa, laugh, effort, huh, sigh from the words; breath (≥ 6 dB under the voice, ≤ 1.5 s), effort (near voice level, ≤ 1.2 s; longer loud sounds without words are left out as probably untranscribed talk), panting (≥ 4 breaths at a steady pace) and running (≥ 6 short steps at a steady pace) from the level. Per character at most 8 of a word kind, 6 breaths, 4 panting and 4 running runs, the clearest first. Ordinary words are not interjections unless stretched ("of" ≠ "oof", "he" ≠ "hehe"); a hyphen split ("Mh" + "hm.") is one sound.
- **Only the actor's recordings:** a vocal bucket refuses search, generation and keeping outside sounds. Cuts go to `Library/Vocals/<Character>/` as `recorded` assets with 80 ms before and 150 ms after the sound; the source files are untouched. Removing a vocal keeps it out of later analyses.
- **Placement:** a vocal goes on its character's dialogue lane (`Lane::Dialogue` with the character), so it follows that lane's mute, solo and volume and exports on the actor's track; dropping it on an SFX lane puts it there. Any bucket sound can be dropped on a dialogue lane.
- **One undo:** the vocal assets and buckets ride in the analysis command, so undoing an analysis takes its vocals back too (the cut files stay in the library and are found again by hash).
- **Checked:** TARMAC with its transcripts: 25 sounds for Paul (6 ah, 6 mhm, 4 whoa, 6 breaths, 1 panting, 1 effort, 1 huh); takes unchanged (36). The breath, panting and running thresholds need tuning on real sessions; they are listed in the run report.

### D48 Bleed is relative to the mic, not −38 dB (Rob, 2026-11-01)
- **Finding:** on the real TARMAC Ep2 session, 24 of Sara's and 17 of the Colonel's own-mic reads were dropped as bleed. They peak around −20 dBFS on the actor's close mic but average −38 to −44 dB RMS, because those mics were recorded a few dB lower than the Stage 0 fixture. The −38 dB cut was Stage 0's number for that fixture.
- **Options:** keep −38 dB; lower it to another fixed value; make it relative to the mic.
- **Choice:** a take is bleed when it is more than 10 dB under the mic's median take (the director or the reader heard across the room), or under −50 dB whatever the mic. Unmatched speech uses the same rule. Pass 1 also keeps the scenes where a role only has short lines, since the four-word pass can never find them (Nina's "D…d…dad?!").
- **Checked:** TARMAC fixture unchanged (36 Paul takes). Real Ep2 session with cached Parakeet transcripts: 328 → 372 takes; Sara 17 → 20 lines, the Colonel 29 → 31, Nina 0 → 1, bleed drops 47 → 4. A comparison against the room mic's level per take would be the stronger test; not done yet.

### D49 Finding the Claude Code CLI (Rob, 2026-11-01)
- **Finding:** an app started from Finder doesn't get the shell's PATH, and this Mac has no `claude` on PATH at all (only the one inside the VS Code extension), so every AI task silently ran on the mock rules.
- **Choice:** `CUTAWAY_CLAUDE_BIN`, then `claude` on PATH, then the usual install folders (`~/.claude/local`, `~/.local/bin`, npm/bun/volta, Homebrew, `/usr/local/bin`, `%APPDATA%\npm`), then the newest Claude Code extension in VS Code / Cursor (`resources/native-binary/claude`). The first one that answers `--version` wins.

### D50 Take rescue: clean scene text in, sentence ids out (Rob, 2026-11-02)
- **Finding:** Parakeet splits speech into tiny fragments (median 2 words, half of them ≤ 2 words on TARMAC Ep2) and mishears words, so real takes stay unmatched. The old AI tasks sent pretty-printed JSON per fragment with timecodes, up to ~200 fragments per call: slow, cut off at 64 KiB, timing out.
- **Choice:** fragments merge into sentences per mic (pause ≤ 0.7 s, no sentence end; ≤ 40 words / 15 s); each keeps its word range. Per scene and actor session one `take_rescue` call gets plain text: the scene script (all characters, `L1 SARA: …`) and the session as it happened, the actor's session mic (`s…`) interleaved with the session's director mic (`d…`), matcher takes tagged `[= L3]`, left-overs `[?]`, quiet speech `[bleed]`, long pauses `— 12 s —`. No timecodes. Claude answers by sentence id and line label (optionally quoting where a take starts and ends inside a sentence); ids map back to words, so timings come from the transcript and edges from the audio, never from the model.
- **Rules:** only the character's session mic gives takes; the director mic is context, notes and cues. Checked three times: the answer schema (`s…` ids only), the answer check (open `[?]` sentence of this session mic, the actor's own line in this scene, no clash with a take, not low confidence), and where takes are created (`apply_mark` refuses a take whose source is not a character's session mic). The same speech on both mics is kept once: from the mic where it is louder against that mic's typical level.
- **Where:** after the matcher, before note linking, so notes and picks see rescued takes. The ambiguous line-match step skips rescued takes. A scene's stretch on a mic is around its matcher takes (−30 s … +15 s), or the gap between the neighbouring scenes' takes.
- **Checked:** Sara on TARMAC Ep2 with Claude: 6 calls of 5–14 s, no timeouts; 6 takes rescued ("Kitty mean the time" → "Get him to the spring!", four shouted "Paul!" heard as "How?"/"Hello!"), one doubtful ("I heard turkey, I'm good." → "That's all I needed, thank you.").
- **Known:** neighbouring scene stretches overlap, so some sentences are judged twice (wasted calls, no double takes).

### D51 Scoped re-analysis and dev marks (Rob, 2026-11-02)
- **Choice:** `AnalysisScope` (characters, lines, or marks). The deterministic part always runs over everything (cheap with cached transcripts, keeps ids stable); AI calls and the payload stay in the scope. `ApplyAnalysis` replaces takes of the scoped lines and notes/unmatched speech of the scoped sources only. A take starred by hand is never dropped by a re-run; it stays, flagged `needs_rematch`.
- **Marks:** a stretch of a session mic with a kind (missed take, wrong line, not a take, edges, other), an optional line and a note. Sending one judges that stretch again (Claude with the mark as a hint, or "Make take" as linked) into a stored proposal (takes to add and remove, one line per decision); `apply_mark` applies it as one undo step and renumbers the lines involved.
- **Takes record their origin** (matcher, rescue, manual) and Claude's reading; only the dev panel shows them. The normal UI treats all takes alike (Rob: no markers).

### D52 Dev panel and learning (Rob, 2026-11-02)
- **Switch:** Settings › Developer, remembered per viewer (browser storage), not in the episode. The Dev page has two tabs.
- **Under the hood:** per actor session, every sentence of the session mic and the director mic by time in three columns that scroll together: full transcript, rescued by Claude, script position. Colour by what a sentence became (matcher take, rescued, by hand, near miss ≥ 55 % like an own line, unmatched, bleed, director note/cue). Search, scene and kind filters, click to play the stretch, ↻ per line and "Rerun <character>". Ticking session-mic sentences opens the link bar (kind, script line, note; Send to Claude / Make take / Just mark).
- **Learning is measured, not trained:** Rob reviews a dialogue-heavy scene (marks everything wrong or missing, ticks it reviewed: unmarked counts as right). The truth for a scene is its current takes corrected by the marks. `dev_eval` dry-runs the current settings and one change at a time (no rescue, rescue without examples, sentence merge ±0.3 s, bleed margin −3/+4 dB, matching ∓5) on the reference and a held-out scene and reports found / wrong / missed, AI calls and time; Rob adopts what helps. Up to five of his linked marks become worked examples in the rescue prompt (`ai.rescue_examples`). Misheard words (heard vs script inside rescued and hand-made takes) become glossary candidates (`analysis.glossary`): told to Claude, and fixed in the transcript before matching when both sides have the same number of words (the heard words stay visible).
- **Config:** new `[analysis]` section: `merge_gap_s`, `bleed_margin_db`, `match_offset`, `glossary`, `learn` (reference and held-out scene, reviewed scenes); `ai.rescue` (on) and `ai.rescue_examples`.

### D53 Lean Claude Code calls, room notes per session, longer notes (Rob, 2026-11-02)
- **Measured:** a plain `claude -p` call carries Claude Code's own system prompt and tool definitions (~34k tokens, about $0.045 API-equivalent even for a one-line answer), and can take several turns using tools (a 15 kB synthetic input: $0.149, several turns). The room-classification step sent each scene's director speech from every session in the time window (every session clock starts at zero): 196 kB per call on average, 79 % of all input.
- **Choice:** the provider calls `claude -p --output-format json --model … --effort … --tools "" --system-prompt <one line> --strict-mcp-config --disable-slash-commands --no-session-persistence`. The same 15 kB input: one turn, $0.043. Effort now goes through `--effort` (supersedes the environment variable of D24). `--bare` is not used: it requires an API key and would bypass the subscription login. Room classification gets only the director mic of each take's own session, within the note windows around the takes (30 s before, 8 s after): 30 kB per call on average, same takes and picks. Every AI call logs its input size and, from the CLI, tokens and API-equivalent cost in `Logs/ai-calls.jsonl`.
- **Notes:** condensed notes may be longer than a few words (Rob); only a note over 40 words (a copied transcript) is refused.

### D54 Edit page: a fixed time scale (Rob, 2026-11-02, M11)
- **Finding:** the scale was "fit the scene" times a 0..1 zoom, so a clip moved past the end, a gap change or opening the inspector or bucket rescaled the whole timeline.
- **Choice:** zoom is an absolute px per second (2–1000), kept for the session. The first visit fits the scene once; after that only zoom actions change it. Scene length only changes how far you can scroll. ⌘-scroll anchors to the pointer, ⌘←/→ to the playhead (or the middle when it is off screen), Z zooms to the selection or the scene and back, the slider is logarithmic.

### D55 Dialogue splits by source time, each part with its own state (M11)
- **Finding:** `split_clip` stored split points in box time, but nothing (layout, playback, export) read them: splitting a dialogue clip did nothing.
- **Options:** split points in box time (shift with every trim, comp change or word delete); split a line into separate boxes (breaks the one-line-one-box script model the Options/Arrange pages are built on); split points in source time.
- **Choice:** `BoxState.split_points` holds take + source time per split, plus the state of the part after it: offset (extra time before it), trims on either side, mute and drop. Part 0's mute and drop live on the box. The layout cuts pieces at split points; parts start at the end of the last kept part plus their offset; dropped parts close up. A split alone plays exactly as before (no fades at the split). Old box-time `splits` are ignored (they never had an effect).
- **Limits:** inner edges trim only inwards (the audio on the other side belongs to the neighbouring part); a split whose take is no longer in the comp is ignored until the take comes back.

### D56 Clip mute and ripple delete (Rob, 2026-11-02, M11)
- **Rob:** a muted clip keeps its slot; ripple delete is a separate function.
- **Choice:** `set_clip_muted` (part or whole line) and `muted` on placements: kept in the layout, skipped by the play schedule (so playback and bounce), exported as `enabled="0"` clips in FCPXML (Resolve keeps them, switched off), left out of SRT, "(muted)" in the cut list. `set_clip_dropped` is ripple delete: the part leaves the cut and the time closes up; it stays restorable (✂ mark on the Edit page, a "Cut · restore" badge on Options and Arrange, ⌃M there restores). ⌫ on dialogue follows the Ripple switch: ripple delete when on, mute when off. ⌫ on a sound removes it.
- **Why not delete dialogue for real:** lines belong to the script; a dropped line keeps its take and edits, so restoring is lossless.

### D57 Lock to script = pinned clips leave the ripple chain (Rob, 2026-11-02, M11)
- **Rob:** lock and unlock the pairing to move clips freely.
- **Choice:** the control bar's Lock switch and a lock on each clip. Locked clips change their pause when moved (today's model). Moving with Lock off pins the clip to absolute time (`pin_clip`); the clip's own lock toggles it. Pinned clips are now **out of the chain**: the next line follows the last locked line, so a free clip never pushes anything (before: a pinned clip pushed later lines when it reached them). Unpinning keeps the clip where it is (its position becomes a pause); pinning and unpinning keep the next locked line in place.
- **Related:** the first locked line now honours its pause (moving the first clip did nothing before; unlocking line 1 would otherwise pull line 2 to zero). A line reordered to the top drops its pause, so a reorder never adds silence at the start.
- **Check for existing episodes:** clips pinned before this change no longer push later lines; look at scenes with "pinned" clips once.

### D58 Waveforms from the cached peaks, per piece (M11)
- **Choice:** each piece of a clip draws its own source range (the take envelope stretched over a trimmed or comped clip did not match what plays). `peaks_batch` fetches the visible pieces at about one point per 2 px, normalized per file like the take envelopes, cached in the page. Detail tops out at the cached 100 points per second (10 ms), enough for breaths, word gaps and plosives; sample-level zoom would need decoding on demand (not done). Vertical zoom: lane height (⌘↑/↓) and waveform height ×1–8.

### D59 Edit page keys follow Logic Pro (Rob, 2026-11-02, M11)
- **Rob:** Return stays "play line in context"; ←/→ select the previous/next clip on the Edit page only (takes elsewhere); scrubbing is silent.
- **Choice:** Space, C, I, X, ⌥←/→ nudge as before; ⌘T split at the playhead; ⌃M mute; ⌘←/→ zoom, ⌘↑/↓ lane height, Z zoom toggle; T opens the tool menu, then T pointer, I scissors, A fade, G gain, R marquee; M/S mute/solo the selected lane; ⇧F select all following; ⌘A all; ⌘U cycle to the selection; , and . move the playhead to the previous/next word (Logic: one bar; there are no bars here, words are the grid).
- **Not verified against Logic's default key commands (check with ⌥K):** ⌥[ / ⌥] for trim start/end to the playhead, the G letter for gain (Logic has no gain tool), ⌃M for region mute.
- **Multi-clip changes** go through a new `batch` command, so a move, mute or delete of a selection is one undo step.

### D60 Spot script on demand, Epidemic pages of 5, free search with drops (Rob, 2026-11-02)
- **Context:** spotting is one Claude call per scene, so it must not run by itself; Rob wants a button, a re-spot option, fewer results per bucket, and a free Epidemic search whose results can be dragged into buckets.
- **Spotting:** nothing spots automatically (the analysis never did). *Spot script* spots the scenes not spotted yet or whose script changed since; *Re-spot all* (the button's menu) reads every scene again. `apply_spotting` records a hash of what it read per scene (`spot:<scene id>` setting: heading, location, line ids, kinds, characters, text), so undo also forgets it. It runs as a background job (`start_spotting`) with per-scene progress; a scene that fails is reported and the others go on. Re-spotting keeps the cues the user accepted or attached sounds to (as before) and drops the replaced cue ids from their buckets.
- **Bucket search:** 5 previews per page, "Load more" adds 5. The bucket stores how far into the provider's results it has read (`search_offset`), so results ruled out by the filters don't shift the next page (before, the offset was guessed from previews + dismissed).
- **Free search:** `search_epidemic` (10 per page, nothing stored) in a search bar on Import › Sounds; its results open in the side panel. Dropping a result on a bucket (`bucket_add_preview`) or on the *New bucket* card (`create_bucket` with `previews`; named after the search, keyword = the query, the current scene) adds it as a **pinned preview**: nothing is downloaded (the free plan has 50 downloads), and a fresh keyword search keeps it. Vocal buckets take no drops. A drop un-dismisses a dismissed sound.
- **Options not taken:** download on drop into a bucket (costs a download per drag); auto-spot after the first analysis (Rob: not with LLM calls).
- **Fixes after the hand-off (2026-11-02):**
  - `spot_sound_effects` (the B drawer, `cutaway sfx spot`) now groups into buckets like Spot script. Before, it recorded the scene as spotted without making buckets, so Spot script skipped that scene.
  - Bucket changes from spotting, search, drops and `update_bucket` are made inside the transaction, on the latest document (`edit_bucket`, logged as `edit_bucket`), instead of a `PutBucket` of an older copy. A search waiting on the network no longer overwrites what a spotting job or another session changed meanwhile.
  - Re-spotting a scene removes the keywords that only its replaced cues gave (keywords typed by hand stay). It takes the scene off AI buckets that no longer get cues from it, and deletes such a bucket when nothing is left in it: no cues, nothing kept, nothing dropped in by hand.
  - The free search asks for one extra result, so `more` is exact. The bucket panel hides "Load more" once a search reports no more results.

### D61 Throughline `.thrl` import and story context (Rob, 2026-11-02)
- **Context:** the specs assumed the Throughline file was a Fountain flavour (how-it-works, *Inputs*; architecture, *Importer*). A real `.thrl` export is gzipped JSON (Throughline format 1.12): beat cards whose script is a TipTap document, act cards, characters, settings, an emotional arc, sparks and more. It also holds what the script doesn't: beat titles and summaries, who is in a scene, setting groups, and (in the pilot files) character bios.
- **Options:** keep Fountain only (Rob exports by hand, loses the context); a `.thrl` → Fountain converter outside the app; a native `.thrl` reader that yields the same parsed script plus the context.
- **Choice:** a native reader in `apc-core::thrl` producing the same `ParsedScript` as the Fountain parser, so versioning and the diff are untouched. The block rules follow Throughline's own Fountain export (checked on the EP02 draft). `flate2` (MIT/Apache-2.0, already in the lockfile through other crates) decompresses.
- **Context scope (Rob):** per episode, not show level, because the app's scope is the episode for now. The newest import wins; read as: its non-empty fields replace the stored ones, empty fields don't erase (a later episode file with empty bios must not wipe bios taken from the pilot). Context is read-only in Cutaway; Throughline stays its source. A context-only import (`import_story`) brings bios from another file.
- **Ignored:** sparks (Rob: not important), relationships, recycled cards, writer assignment, constellation layout, author contact details.
- **Character names:** Throughline names map to script cues exactly, without a leading "THE" (pilot COLONEL ↔ THE COLONEL), or by an existing alias; others are listed in the import summary. Profiles attach only to characters the script knows.
- **AI use:** context goes to sound spotting and file-name matching only. Notes, picks, line matching and rescue are per take and high volume (D53); bios add tokens there with little gain.
- **Fountain parser changes found through the parity test:** `!CUT TO:` is now a forced action (it was read as a transition with the `!` in its text); a section (`#`) belongs to the scene after it (it was attached to the scene before); synopses (`=`) and the top-level section fill the scene's story synopsis and act. Emphasis stays in the line text as Fountain writes it (`_EDIBLE_`); the `.thrl` reader writes underline/italic/bold the same way so both formats diff as equal.
- **Room tone:** a setting without its own room tone plays its parent's (`Document::room_tone_of`); the Options/Arrange room tone editor still edits the setting's own.

### D62 Punchier pacing: overlap and a right-is-tighter slider (Rob, 2026-11-02)
- **Rob:** the breath gap stops at 0 and the tightness slider at 2×; he wants dialogue punchier than that.
- **Found:** the tightness value multiplies the pauses, so the slider's "×2" doubled them (looser), the opposite of how it reads. And every clip carries ~230 ms of near-silence (80 ms pre-roll, 150 ms tail from `edges::refine`), so a 0 ms breath gap still sounds like a pause.
- **Choice:**
  - Breath gap slider −500 ms … 1.5 s (config accepts −1 s … 5 s). Below zero the next line starts before the last one ends: first into its silent tail, then over the words (cue bites, crosstalk).
  - The tightness slider shows the inverse of the stored multiplier on a log scale, ×0.35 … ×10, right = tighter. The stored value and its 0.1–3 clamp are unchanged, so no migration; a scene set to "×2" before now shows as ×0.50, which is what it was doing.
  - A negative default gap is divided by the multiplier instead of multiplied, so tighter always means an earlier entry. A default overlap never reaches back past the start of the line before. Pauses set by hand stay as they are (they could already overlap, down to −5 s).
- **Options not taken:** trimming the pre-roll and tail per clip when tight (changes the clip edges, which are per take and user-editable); a separate per-scene overlap setting (a third control for the same feel).

### D63 Lengthening clips on Arrange (Rob, 2026-11-02)
- **Rob:** Arrange trims clips; it should also lengthen them when a cut sits wrong.
- **Found:** the model already allowed it (`head_s`/`tail_s` below zero reach into the source, as the Edit page's `trim_clip` does). Only the Arrange strip stopped at the take's edges, because it drew the take alone.
- **Choice:**
  - The strip shows a source window: the take plus a handle on each side of 40 % of the take, 0.75–2 s. Each handle also covers what is already extended, so every drag can go 0.75–2 s further. The window never goes past the ends of the file (`source_len` in the take view).
  - The handle audio is fetched with `peaks_batch` once per window. Until it arrives, the handles are drawn flat.
  - Outside the in and out points the strip is darkened, as for trims. A dashed mark shows the take's original edge.
  - `set_trim` clamps extensions to the source file (the head at its start, the tail at its end when the length is known), so no caller can send a clip before 0 s.
  - Arrange keeps the clip's start in place: lengthening the head moves the words later and ripples the lines after it, the mirror of trimming. On the Edit page, which has a time scale, the words stay where they are.
  - Comps keep the take-only strip, because a comp's head and tail belong to different takes.
- **Options not taken:** a strip that rescales live while dragging past its edge (moving handles under the cursor); a fixed handle length (too short for a cut deep in the next word, too wide for short lines).

### D64 Studio mode: a show is the production's own folder (Rob, 2026-11-02)
- **Rob:** besides standalone episodes, a studio mode where files and assets are organised per show, not per episode, inside the production pipeline (`tm-2610/`, a Syncthing share also used by Jetplane). Cutaway folders live in `Episodes/<Episode>/Sound/`, SFX and music in `Sound/`. A menu point creates or opens a show by its root; opening a show offers a dropdown of its episodes.
- **Found:** the core had a show model (`TARMAC.cutawayshow` as SQLite, `Library/{SFX,Ambience,Music}`, `Episodes/<Ep>/<Ep>.cutaway`) that assumed Cutaway owns the folder. Nothing outside tests created one; the UI never did.
- **Choice:**
  - The show file is `<root>/<name>.cutawayshow` as **JSON** (like the pipeline's `*.jetplane.json`): name, id, layout, theme, file-name aliases. Written through a temporary file and a rename; changes re-read the file first, so two machines don't undo each other's edits.
  - The **layout lives in the show file** with tm-2610's names as defaults (`Episodes`, episode folder `Sound`, library `Sound`, `sfx`, `music`). No path is hard-coded.
  - "New show…" makes an **existing** folder a show: it writes the show file and creates only the folders that are missing. Nothing existing is touched.
  - **Episodes** are the folders under `Episodes/`. Titles and numbers come from `episode.jetplane.json` (read only, never written). Opening an episode without a project creates `Episodes/<Episode>/Sound/<Episode>.cutaway`.
  - An episode finds its show by walking up to a show file whose layout puts it exactly there.
  - The old SQLite show file, its `ShowStore` and the `Library/` show layout are removed. A SQLite `.cutawayshow` is refused with a clear message. Show-level cues keep only their id (nothing ever read the table).
- **Options not taken:** keeping SQLite for the show file (a database inside a sync share, see D66); hard-coding the tm-2610 layout; a hidden `.cutaway/` folder at the root (Rob wants the show file next to Jetplane's).

### D65 The show's library: the folders are the library, the subfolders are the buckets (Rob, 2026-11-02)
- **Rob:** SFX and music in `Sound/` are read into the buckets, and new files are saved there. Each bucket is a folder, so a new bucket can create a folder in the pipeline. Vocals stay with the episode.
- **Choice:**
  - **Scan, don't register.** Every audio file under `Sound/sfx` and `Sound/music` is a library sound. Skipped: zips, hidden files, Syncthing temp files and conflict copies.
  - A **local index** (app data, `shows/<show id>/library-index.json`) keeps each file's BLAKE3 hash and length by path, size and modification time. Only new or changed files are hashed, and each machine hashes once. `CUTAWAY_DATA_DIR` overrides the location (tests).
  - **Ids:** a show sound's id comes from its hash (`as_<12 hex>`), so it is the same on every machine and in every episode. Asset paths are `show:<path under Sound/>`.
  - **Shared metadata:** what a file can't tell (licence, provider, Epidemic id, prompt, extra tags) goes in `Sound/Cutaway/library.json`, keyed by hash. Sync-conflict copies of it are merged on read.
  - **Categories:** from the folder (`SFX/doors`, `Music/Stings`). Ambience comes from folder names (`room-tone`) or file names. The Edit page's lane choice works unchanged.
  - **Buckets are folders:**
    - The first-level folders of `sfx/` and `music/` become buckets (`room-tone` → "Room tone", keyword "room tone").
    - A track's stems folder is not a bucket: it sits next to the `.wav`/`.zip` of the same name, or holds `… STEMS …` files.
    - A bucket's sounds are its folder's files plus what the episode keeps.
    - `SoundBucket.folder` (serde default) stores the link. Linking runs as one undoable command ("link_show_folders"), only when something changed, on open and on rescan.
  - **New buckets:**
    - A bucket made by hand creates its folder at once (`Doors and hatches` → `sfx/doors-and-hatches`).
    - AI-spotted buckets get a folder name, but the folder is made with their first sound, so a spotting run doesn't fill the pipeline with empty folders.
    - Spotting reuses an existing folder bucket when the group or its keywords name it ("Doors and hatches" → `doors`).
  - **New files** (Epidemic downloads, generated sounds, drops onto a bucket) go into the bucket's folder. Sounds dropped without a bucket go loose into `sfx/` or `music/`.
  - **Keeping** a library sound in a folder bucket copies it into the folder, so every episode sees it there. The same file already in that folder is reused, not copied twice.
  - **Removing** a folder sound from a bucket hides it in that episode only; the file stays.
  - **Deleting** a folder bucket that holds files is refused, with a message pointing to the folder. An empty one goes, along with its empty folder.
  - **Renames:** renaming a bucket never moves its folder (other episodes point at its files). A file moved in the Finder is found again by hash.
  - **Vocals** stay in the episode's own `Library/Vocals/` (they are cut from the episode's recordings, so not markers as first thought). Export and playback resolve both kinds through `EpisodePaths::asset_path`.
- **Options not taken:** a library database in the show (a sync hazard, D66); storing folder membership in the document (stale as soon as the pipeline changes); moving files between folders (the pipeline is shared, so Cutaway only adds copies). *Moving is now allowed as a confirmed clean up, and spotting matches folders by bucket name only: see D74.*

### D66 Sync safety in a Syncthing share (2026-11-02)
- **Context:** the episode file is SQLite in WAL mode. Syncthing copies files independently and can carry a `.cutaway` without its `-wal`. Two machines editing the same episode produce a conflict copy that can't be merged.
- **Choice:**
  - The log is folded into the episode file (`wal_checkpoint(TRUNCATE)`) on close and after every autosave snapshot in a show.
  - **Lock file:** the UI writes `.<Episode>.cutaway.lock` (host, user, time) next to the episode file and removes it on close. Opening an episode locked by another host within the last 12 h shows a warning. It doesn't block: Syncthing's delay makes a lock advisory at best. The CLI, MCP and background jobs share the episode without a lock, as before.
  - Sync-conflict copies of the episode file are reported when it opens.
  - Imports into a show episode are always copied, because absolute paths don't travel between machines.
  - Backups stay in the synced episode folder (Rob).
- **Rob, manual:** add `*.cutaway-wal`, `*.cutaway-shm` and maybe `Cache/media` to `.stignore` on each device.

### D67 The show's backdrop and theme (Rob, 2026-11-02)
- **Rob:** opening a show shows the show itself as the splash backdrop, themed to the show, set once and kept near the Cutaway folders.
- **Choice:**
  - `set_show_backdrop` takes any JPEG/PNG/WebP and stores a JPEG copy (longest side 2400 px, quality 85) as `Sound/Cutaway/backdrop.jpg`, next to `library.json`, so it syncs to everyone.
  - It also extracts three theme colours from saturation-weighted hue bins and stores them in the show file.
  - The show home draws the backdrop full bleed. Inside the show's episodes, the colours tint the stage glows.
  - The image travels to the UI as a data URL (`show_backdrop`), because the webview has no file access.
  - New dependencies: `image` 0.25 (jpeg, png, webp; MIT/Apache-2.0) and `base64` 0.22 (MIT/Apache-2.0).

### D68 Copying a standalone episode into a show (Rob, 2026-11-02)
- **Rob:** migrate (copy, not move) the existing project into the show's folders.
- **Choice:**
  - `migrate_episode` (dry run first) copies the episode folder into `Episodes/<Episode>/Sound/`. The database goes through `VACUUM INTO` from a read-only connection: immutable when the episode is closed, so not even `-wal`/`-shm` files appear next to the source.
  - Everything else is copied as is: Audio, Script, Backups, Cache (AI and transcription results are reused), Logs, the vocal library.
  - The episode's own sounds that aren't vocals go into the show's library (a bucket's into its folder). Their document assets keep their ids and point at the show copy.
  - The episode is renamed after the episode folder, and `migrated_from` records the source.
  - The result reports missing sources and sounds; the source stays byte for byte.
  - Sources referenced outside the source folder are listed as warnings.

### D69 Playback leveling, on by default (Rob, 2026-11-02)
- **Rob:** recordings come in at very different levels; playback should even them out so a badly levelled one doesn't blast, on by default, without burning anything in.
- **Found:** take loudness was measured at ingest (`Take.lufs`) and `MixSettings` had a per-take `loudness_match`, but nothing turned it on: scenes, take auditions and sound auditions all played as recorded.
- **Choice:**
  - `[playback] level = true`, `target_lufs = -23` in the project config (a missing section reads as on, so existing episodes get it). Toggle in Settings, also `set_config playback.level` from the CLI and MCP.
  - **Scenes level per recording, not per take.** A source's loudness is the median LUFS of the takes on it (the role's own lines, so silence and bleed don't count), and every clip from that source gets the same offset to the target (±12 dB, as `match_gain_db`). A hot mic comes down and a quiet one comes up, while a whisper stays quieter than a shout. The room tone moves with its source. Library sounds (SFX, ambience, music) keep the editor's gains.
  - **Auditions match per item,** as the spec asks: a take alone to the target from its own LUFS, a bucket sound from its first 30 s (measured once per file and cached for the session). The dev panel's source play uses the source offset.
  - **Safety limiter** on the device output while leveling is on: instant attack, 150 ms release, ceiling −1 dBFS. It runs in the engine's feeder, after the shared mixer.
  - Nothing is stored or exported: only the gains handed to the engine change. `MixSettings::default()` keeps leveling off, which is what bounces use, and FCPXML never sees it.
- **Trade-off:** with leveling on, dialogue sits at the target while placed sounds don't move, so the dialogue-to-sound balance heard in the app can differ from an export of the same mix (by the source offset, up to ±12 dB). Turn leveling off to hear the export as it will be.
- **Options not taken:** per-take matching on the timeline (flattens the performance); measuring whole source files (session WAVs carry bleed, director talk and long silences); a master-bus AGC (keeps the roles unbalanced and pumps); storing the toggle in local UI state (the shared command path keeps UI, CLI and MCP alike, as with room tone).

### D70 Follow playhead, on by default (Rob, 2026-11-03)
- **Rob:** while playing, the box highlight should move with the playhead, or stay where it is so playback can start from there again; a global switch at the top, on by default, also in settings.
- **Found:** Options and Arrange always moved the current box with the playhead (the playhead poll set `curId`), with no way to switch it off; the Edit page always paged along.
- **Choice:**
  - `app.follow`, a per-viewer preference in local storage (`cutaway.follow`, like the theme), default on. A **Follow** pill in the header on Options, Arrange and Edit, and the same switch in Settings.
  - On: unchanged behaviour. Off: the poll no longer moves `curId`, the Edit page doesn't page along, and Space plays from the current box again (the playhead has left it).
  - The Edit page's clip selection never follows the playhead (Rob): only its scene-strip highlight and paging do.
- **Options not taken:** a project config value through the command bus (it's how this viewer watches, not part of the cut, and should carry across episodes).

### D71 Lead-in, tail and edge fade, heard and exported (Rob, 2026-11-03)
- **Rob:** lines start and stop too hard and sometimes click; wants adjustable handles (a run-up of some milliseconds before a line starts and after it ends) as a global setting, applied to exports too.
- **Found:** a line's clip started and ended exactly on its edges (the detected take edges plus trims) with a fixed 5 ms fade, not configurable.
- **Choice:**
  - Three project config values under `[pacing]`: `handle_in_ms = 100`, `handle_out_ms = 150`, `edge_fade_ms = 20`. Sliders in Settings; `set_config` from the CLI and MCP.
  - The layout doesn't change. `layout_scene` fills a new `LayoutItem::audio`: a copy of `pieces` whose first piece starts earlier in the source (and on the timeline) and whose last ends later. Pauses, word times, the box length and every editing command keep working on `pieces`; playback, bounce and FCPXML use `audio`. A handle overlaps the pause (and the neighbour's handle), so the rhythm of the cut stays as it was.
  - A handle stops 40 ms short of any other word on the same recording (from any take), so a word trimmed off stays off, a word a trim cuts through isn't re-opened, and the next take's speech doesn't leak in. It also stops at the file's edges and at the scene start.
  - The edge fade sits at the outer edge of the handle.
- **Trade-off:** the Edit page draws clips at their edit edges, not with the handles; the handles are heard but not drawn.
- **Trade-off:** the first line of a scene starts at scene time 0, so it gets no lead-in (only its edge fade). Moving every scene's start later would shift every time in the cut.
- **Options not taken:** lengthening the clips themselves (would push every pause out by lead-in + tail); a longer fade alone (fades into the first consonant instead of running up to it); per-line handles (Rob asked for one global value; trims still adjust single lines).

### D72 The editor reads the script, timed by what was heard (Rob, 2026-11-03)
*Replaced by D105: the cards now read what was said.*
- **Rob:** transcription errors show in the boxes; the editor should see the script's real words, and the word highlight should carry on through a mismatch. Change nothing in the analysis.
- **Choice:**
  - Display only, in the UI (`app/src/lib/words.ts`). The take words, the matching and the stored data are untouched.
  - Each script word takes the time of the heard words matched to it. A run of script words nothing was matched to is spread (by length) over the unmatched heard words between its neighbours; if nothing was heard there, it is squeezed in between them (at least 60 ms a word, taken from the neighbours' halves). If the editor deleted what was heard there, the script words show as deleted.
  - Which script words a recording covers: the matched span, widened by the unmatched words heard before and after it (a misheard first or last word). A partial take doesn't show the words it never reached.
  - Heard words nothing in the script stands for (a repeat, an ad-lib) ride on the word before them: the highlight covers them and a delete reaches them.
  - Every shown word keeps the heard word indexes it stands for, so the box editor's word delete and restore still act on the heard words.
  - Used on Options (text and karaoke), Arrange, the Edit page (strip, clip labels, snap labels) and the box editor (comp words and take lanes). The Inspector and the Dev page keep showing what was heard; exports (SRT) are unchanged.
- **Trade-off:** an ad-lib that isn't in the script doesn't show as text.

### D73 Room talk between takes, read whole (Rob, 2026-11-03)
- **Rob:** the director notes were cluttered keywords. Everything said in the room before a take, chatter included, should go to Claude as one text and come back as one or two full sentences saying what the people meant. Direction and feedback are fluid: the talk between two takes belongs to both.
- **Found:**
  - Each take got the room speech in a 30 s window before it (direction) and an 8 s window after it (feedback).
  - `note_condense` v1 asked Claude for "a few words" per side.
  - The fallback, and the mock, turned the text into keywords.
  - Talk that was classed as chatter, or fell outside the windows, was lost.
- **Choice:**
  - **Gaps.** After the takes are final (dropped takes removed), each session's room talk is cut at its takes, in session time across every mic. A gap holds the non-cue-reading speech in the first 8 s after the previous take plus the last **60 s** before the next one (Rob: a minute limit), chatter included. There is also a gap before a session's first take and one after its last.
  - **`note_condense` v2** gets whole gaps (text plus the character and line of the take on each side), batched 40 per call, and returns one `summary` of one or two full sentences plus tags. Validation refuses a summary over 80 words, so a copied transcript is caught.
  - **Shared storage.** A gap's summary is written to `direction` of the take after it and to `feedback` of the take before it, with the gap text in `direction_raw` / `feedback_raw`. The data model and file format stay unchanged, and the cut list and FCPXML keep their columns.
  - **Fallback** (no AI answer): keyword hits only, as "After the take: …. For the next: …." or a short quote, with the take flagged "notes condensed by rules".
  - **UI.** The Options page's Direction and Feedback boxes became one room strip per line. It holds the line's takes and one take either side, in recording order, with the talk between them. The current take sits on the centre line, the talk on both sides is bright, further steps fade, and changing takes slides the strip (Rob). Other lines' takes show as "CHARACTER · first words". The raw transcript appears only with the developer switch on (Rob), in the strip and in the Inspector. The take list subtitle shows the first sentence of the take's direction.
  - Pick detection still reads the 8 s after each take, and room classification is unchanged.
- **Options not taken:**
  - Keeping a separate direction and feedback summary per take. This splits a single conversation, and Rob asked for it read whole.
  - Sending the whole gap with no limit. A lunch break between takes would swamp the note.
  - A new gap entity in the document. The take fields already hold both sides, and a new collection would need format and command changes for no visible gain.

### D74 Show sounds reach the buckets by file name; a manual clean up files them (Rob, 2026-11-04)
- **Rob:** the SFX in the show were meant to be read into the buckets, and mostly weren't. Match by file name for now. An AI pass, triggered by hand, "cleans up" and also moves SFX into their folders.
- **Found on Episode 2 (104 SFX, 167 cues):**
  - All files were scanned, but only first-level folders became buckets, so the 36 files straight in `sfx/` were in none.
  - Since `9b18780`, a spotted bucket matches a folder by its name only, so 13 of the 18 spotted buckets pointed at folders that don't exist. They held 119 of the cues, and no sound.
  - Folder buckets have no scenes, so the docked bucket hid them: 26 SFX were reachable there.
- **Choice:**
  - **File-name matching (`apc-sound::matching`).** One query per cue (its tags, in order) or, for a hand-made or folder bucket without cues, per keyword. Tags score on the file's words and its folder: first tag 3, others 2, +2 for a tag of several words, +1 when it hits the name's first part or the folder. Weak words (materials, wide categories) score 1, noise words nothing. 3 is a match, at most 5 sounds per query. Tuned on Episode 2 with a prototype of the scoring: 88 of 154 non-music cues found a sound, and the threshold drops most false hits ("metal" alone no longer gives a telescope every metal impact).
  - **Never stored.** Matches are worked out with each `buckets` call, like folder membership in D65, so a new file shows on rescan and nothing goes stale. No format change and nothing to undo. It takes about 0.2 s of CPU on Episode 2.
  - **Filters.** A bucket's length and leave-out filters apply. An ambience cue only takes sounds of 10 s or more. Music buckets, vocal buckets and music files stay out.
  - **A match is a reference.** Nothing is copied, so a match doesn't create a spotted bucket's missing folder or duplicate audio over Syncthing. Removing a match hides it in this episode, like a folder sound.
  - **Scenes.** With a scene, a bucket shows what that scene's cues ask for.
  - **Whole show.** Buckets of no scene are offered in every scene's docked bucket, under "Whole show", closed. Empty ones are left out.
  - **Loose SFX.** One bucket on `sfx/` itself holds the loose sounds no bucket matched. It disappears when none are left.
  - **Clean up (`sound_sort`, `plan_cleanup`, `apply_cleanup`).** One AI call on names only. It covers loose files in `sfx/` only: folders Rob already filed stay as they are, and music is left alone. The AI may pick a folder on disk, a spotted bucket's folder, or a new one, and answers null when unsure.
  - **Moves are confirmed.** They change the shared pipeline for every machine and episode, so the AI only proposes. The sheet lists the moves, each can be unticked, and nothing moves until *Move N files*. `cutaway sfx clean-up` only prints the plan; `--apply` moves it.
  - **Move safety.** A move is a rename inside `sfx/`, one level deep, and never overwrites. A file that can't move is reported and the others go on. Each run writes `Sound/Cutaway/moves/<stamp>.json` (one file per run, so machines can't conflict) as the way back. There is no undo button.
  - **Moved files stay linked.** Hash ids and `library.json` are unaffected. Playback and export read an asset's stored path, which D65's "found again by hash" didn't cover, so `relink_sounds` repoints `show:` assets by hash on open, on rescan and after a clean up. Other episodes catch up when they next open.
- **Options not taken:**
  - Storing matches on the bucket: stale as soon as the pipeline changes (D65), and would need a format change.
  - Using `bucket_keep` for matches: it copies files into the bucket's folder.
  - Linking spotted buckets to folders by keyword again: `9b18780` removed that because one keyword pulled a whole folder into the wrong bucket. Per-sound matching gets the sounds without that.
  - Moving without review, or sorting files already in folders: too much change to a shared folder for one AI pass.
- **Known:** spotted cues are grouped by substring in `category_of`, so "ring" catches *spring* and *whimpering*, and "hum" catches *thump*. Phones holds UFO, Time Spring and turkey cues, and now their sounds too. Not fixed here; changing it needs a re-spot.

### D75 The room reverb: a 16-line FDN in plain Rust (2026-11-04)
- **Context:** every sound of a scene plays through one room, in the real-time engine and the offline bounce with identical results. Target: a natural tail, no metallic ringing, low CPU, permissive licence. Sound effects are mostly transients (doors, foil, steps), so the first 100 ms of the tail matters as much as the late tail.
- **Measured** on one click at 48 kHz, three rooms (decay 0.41, 0.74 and 1.73 s asked for), in a throwaway harness outside the repo:

  | | Decay vs asked | Late-tail ringing (p99, ~10 dB is noise) | Echo density at 80 ms (1 = noise) | L/R correlation | CPU |
  |---|---|---|---|---|---|
  | **FDN16, modulated (chosen)** | 11–15 % short | 9.9–11.8 dB | 0.66–0.92 | 0.0 | 57–83 ns/frame |
  | fundsp `reverb_stereo` | about 2× long | 9.0–11.7 dB | 0.02–0.49 | 0.42–0.46 | 78 ns/frame |
  | fundsp `reverb2_stereo` | about 3× long | 9.1–11.3 dB | 0.39–0.74 | 0.39–0.42 | 214 ns/frame |
  | fundsp `reverb3_stereo` | 2–3× long | 12.9–22.4 dB | 0.89–0.90 | 0.19–0.28 | 50 ns/frame |
  | Freeverb | 25–50 % long | 12.7–14.0 dB | 0.80–0.86 | 0.0 | 15 ns/frame |

  57–83 ns per frame is under 0.5 % of one core at 48 kHz.
- **Choice:** our own FDN in `apc-audio/src/room.rs`: 16 lines, Hadamard mix, one-pole damping per line, four input diffusers per side, pre-delay, and a slow drift on each line of about 2.5 cents at most. No new dependency and no licence question.
- **Why not fundsp** (MIT/Apache, a good library): its tail is as clean as ours, but the first echoes of `reverb_stereo` are sparse, which flutters on transients, and the sides stay correlated. It would also bring about ten crates into the audio path. Its calibration could be fixed; the sparse start could not without changing its graph.
- **Why not Freeverb:** the comb bank rings measurably more than the FDN on the measure above, and its decay comes from a feedback value rather than a time.
- **Details that matter:**
  - The damping pole is the exact one-pole through the wanted DC and Nyquist loss, `(r − 1) / (r + 1)`, always inside the unit circle. Jot's usual approximation gives a pole above 1 in a small dark room, and the loop blows up.
  - The drift is a function of the absolute frame, so a seek lands on the same modulation as the bounce.
  - A non-finite sample is dropped at the input, and a tiny constant keeps the feedback out of denormals.
- **Not done:** this was measured, not listened to. Rob judges the sound by ear; the settings curves are in `RoomShape::new`.

### D76 The room bus: one room per scene, depth as one macro (Rob, 2026-11-04)
- **Rob:** the sound effects of a scene live in one shared room and go through its reverb on a shared bus. Each scene has its own room, stored in the document, edited in the project setup's Sounds tab. Depth moves a sound two ways at once: dry/wet balance, and placement further back (level and a gentle high roll-off), as one macro that is easy to retune.
- **Room settings** (`Scene::room`, each 0…1). The default is a small furnished room:

  | Setting | Default | Curve (`RoomShape::new`) |
  |---|---|---|
  | size | 0.35 | decay 0.25 … 2.8 s (exponential), pre-delay 2 … 40 ms, line lengths ×0.35 … ×1 |
  | damping | 0.5 | high decay at 0.85 … 0.2 of the low decay |
  | wet | 0.5 | linear output level of the room |

- **Starting depth curve** (`depth.rs`, the only place it lives), linear in dB from 0 close to 1 far:

  | | Close | Far |
  |---|---|---|
  | level offset (dry and send) | 0 dB | −6 dB |
  | dry on top of that | 0 dB | −8 dB (so −14 dB in all) |
  | send on top of that | −20 dB | +4 dB (so −2 dB in all) |
  | roll-off (one pole, on dry and send) | none | 3 kHz, log between |

  Measured on noise in a room at size 0.6, wet 0.8: the room against the sound goes from −30.8 to −4.8 dB, the sound drops 12.7 dB, and its brightness falls by a factor of five.
- **Choices the specs left open:**
  - **What goes through the room:** the `sfx` and `ambience` lanes. Music is outside the story's space, and vocal sounds on a dialogue lane belong with the dialogue (dialogue processing is out of scope). Room tone stays dry. `Placement::in_room` is the one place to change this. `set_sound_depth` keeps a value on the others but says `in_room: false`, so a CLI or MCP user isn't misled.
  - **Close is not fully dry:** a close sound still sends at −20 dB, because "every sound goes through the room". So existing sounds at depth 0 gain a faint room after the update, about 30 dB under the sound with a typical room. Their direct signal is untouched. Set a scene's wet to 0 for exactly the old sound.
  - **Depth of existing sounds:** 0, so nothing moves back by itself.
  - **The room's tail** rings on past the last clip in playback and in the bounce (`PlaySchedule::tail_s`), so a scene with sounds near its end bounces longer by up to the decay time. A scene with no sound in the room is unchanged to the sample. The transport's playhead waits at the scene's end meanwhile.
  - **Pan and the room:** the send is panned like the dry signal, so a sound's side carries a little into the room. A lane's pan now adds to a clip's instead of replacing it (only a clip with its own pan notices).
- **Bounce equals playback.** The mixer stays stateless; a `Renderer` owns the room and both clients run it. Output doesn't depend on block size (tested bit for bit). A seek warms the room with the last 1.5 decay times of the send, so playback from anywhere matches the bounce, exactly within that window and below −80 dB beyond it. The roll-off is applied to the whole clip at decode, so it needs no state at all. Playback's leveling and limiter stay playback-only (D69), so that difference remains by design.
- **File format:** schema 2. Opening an older file writes the defaults into its scene and sound rows in one transaction, with no undo step and no `rev` bump. Written rather than left to serde defaults, so retuning a default later can't change the sound of a finished scene. An older app refuses a schema 2 file. **Syncthing:** every machine on the share needs this version before a migrated file reaches it.
- **Auto fade.** The prototype and handover §5.6 make only the fade **out** automatic, and only setting the fade out turns it off; the fade in starts at 20 ms. Followed as such. The Edit page's fade drag sends both values, so only a changed fade out counts as set by hand. Older sounds keep their fades with auto off; a new sound on a dialogue lane keeps its short edges.
- **Anchors:** M7's `Anchor` already covers all three targets, so no new kind: a word (`word`), a line edge (`edge`), and a pause, which is the end of the word before it. That is also how the prototype names it (*After "toxic."*).
- **Exact undo:** `serde_json` gets `float_roundtrip`. Undo reads the old value back from the log as text, and 12 % of full-precision values came back changed in the last bit (243,103 of 2,000,000 in a gain-range sweep).
- **Not carried into FCPXML:** depth, the room and clip pan stay in the mix. FCPXML keeps gain and fades; a room has no equivalent there, so a Resolve or Logic session gets the sounds dry and centred. The bounce has everything.
- **Options not taken:** one room per location (Rob decided per scene); a send for dialogue (out of scope); rendering the room inside the device callback (the feeder already works ahead, and the callback only copies samples).

### D77 Gain range of a sound: the Edit tool keeps its wider range (2026-11-04)
- **Conflict:** the Arrange brief sets −24 … +6 dB for a sound's gain. The Edit page's gain tool already reaches −40 … +12 dB, and its inspector takes any number, through `update_placement`.
- **Choice:** `set_sound_gain` clamps to −24 … +6, which is what the UI, CLI and MCP will use for Arrange. The Edit tool stays as it is: the codebase wins (handover §1), and narrowing it would quietly change what an existing control can do. A sound already outside the range keeps its value until a gain command touches it.
- **For card B:** the loudness ring must cope with a value beyond its ends, as the Edit page can set one. For Rob to confirm, or to narrow Edit to match.

### D78 The sounds layer on Arrange (2026-11-04)
- **Spec:** `tasks/Bokeh · live prototype-html/audioplay-cutter-handover-arrange-sounds.md`, card B. The prototype's behaviour and timings are ported; where it conflicts with the app, the app won.
- **Which sounds show:** the `sfx` and `ambience` lanes, the ones in the room (D76), so depth always means something on a ball. Music and vocal sounds stay on the Edit page. Vocal buckets are left out of the panel; a music file dropped here is placed on its lane, and a toast says it is on the Edit page. For Rob to confirm.
- **Bucket of a sound:** the first bucket of the scene that keeps its asset. Colours come from six `--bucket-n-rgb` tokens (light and dark), by the bucket's place in the scene's list, so seven or more buckets repeat a colour. Colours repeat for characters too (seven, wrapping), so nothing on the page may be identified or keyed by its colour: paths belong to a bucket or a character. Sounds in no bucket are grey and gather under **Other sounds**, with their own eye.
- **Ties in a group:** ids are random, so nothing recorded which of two sounds came first. `Placement::added` is a new counter, set when a sound is placed or split. Older sounds read 0 and fall back to their id, a stable order. No schema change: the field is read with a default.
- **Keyboard (chosen to fit the existing map):**

  | Key | With sounds selected | Otherwise |
  |---|---|---|
  | ← / → | Depth, 5 % a press | Previous / next take |
  | ⌥← / ⌥→ | Pan, 5 % a press | Nothing |
  | ⇧↑ / ⇧↓ | Loudness, 1 dB a press | Extend the passage |
  | , / . | Move 100 ms (⇧ 10 ms), anchors kept | Shorter / longer pause |
  | ⌫ | Remove | Nothing |
  | Esc | Clear the selection | As before |
  | Tab, Enter | Reach a ball, select it (⇧ Enter adds) | |
  | S | Show or hide the layer, on Arrange | |
  | B | Show or hide the panel, with the layer on | The bucket drawer |

  Arrows keep their everyday meaning until a sound is selected, so reading and picking takes is undisturbed. Enter on a ball selects it for now; card C makes it open the sound (D80).
- **Bursts:** key repeats and ⇧ scroll don't end in a release, and reading the value back between presses would lose steps to commands still in flight. They preview locally and commit once, 350 ms after the last step, as one undo step. ⌘Z sends what is pending first.
- **A move that doesn't snap frees the sound** (M7: "free time when nothing snaps"), for the grabbed ball. The others of a multi-move keep their anchor and take a new offset, so they stay on their lines. Keyboard moves keep every anchor. A group stops as a whole at the start of the scene, and a snap it can't reach is dropped.
- **Pause targets** are the end of the word before the pause, not the middle of the pause as in the prototype: that is the anchor M7 can resolve, and the tooltip reads the same. A pause is a silence of 120 ms or more between kept words.
- **Trim snaps to every target,** with the prototype's end labels, not only line ends as the handover's prose says. Same reason it does in the prototype: a pause is a natural end.
- **Time map:** real scenes overlap (scene 2 of Episode 2: 17 of 27 lines) and pinned clips can sit out of order, so the map doesn't assume rows rise in time. Each row adds only the time and height not covered yet.
- **Scrolling** with the layer on moves the playhead continuously, and the column follows it on the centre line, as in the prototype. With it off, Arrange steps by line as before. Follow off keeps the column still during playback. Space plays from the playhead with the layer on, so a scrub into a pause stays there. The old rule jumped back to the line's start, and pauses are where sounds go.
- **Layout:** the column moves left and narrows to 400 px, and fades out under the header and above the transport. The header's scene name gives way with an ellipsis rather than wrapping the row. The layer's state is remembered per viewer and on by default.
- **Bucket eyes** live in `app.hiddenBuckets`: this page, this session (handover 9.5, Rob). They never reach the document or the mix.
- **Gain outside the ring's range** (D77): the ring sits at its end and the tip shows the real value. Turning it brings the sound into range.
- **Depth** runs to 1, not the prototype's 0.98: the command's range. A far ball still fits beside the panel.
- **Selection is Arrange's own,** separate from the Edit page's, which also selects dialogue. The clips themselves are shared.
- **Not done here:** double click and Enter opening a sound, the quick buttons and fades in the opened view (card C).

### D79 Unit tests for the UI's logic: vitest (2026-11-04)
- **Context:** the card asks for tests of snapping, lanes, the merge rule, settling and multi-select maths without a browser. The app had Playwright only.
- **Options:** Node's own test runner (needs `.ts` extensions in imports, which the app's other code doesn't use) or vitest.
- **Choice:** vitest 5 as a dev dependency (MIT), run by `npm test` and `scripts/check.sh`. The logic lives in `app/src/lib/sounds/` with no Svelte or browser in it. It shares the app's Vite toolchain.

### D80 The opened sound on Arrange (2026-11-04)
- **Spec:** handover "The opened sound", 5.6, 6 and 8, card C. Behaviour ported from the prototype; where it conflicts with the app, the app won. Logic in `lib/sounds/opened.ts`, tested without a browser.
- **The window.** A source up to 8 s shows whole, as in the prototype. A longer one shows the sound and a second each side. The prototype capped that at 12 s, which puts the bottom edge of a long bed out of reach. The window holds still through a gesture and fits itself after one if the sound came within 0.1 s of its edge.
- **Snapping** uses the spine's targets and the spine's 8 px, as the same distance on the view's own scale (the prototype used 10 px there).
- **A start trimmed onto a target hangs on it** (trim and move in one batch, so one undo step). A start trimmed anywhere else keeps its anchor with a new offset, so the audio stays where it was against the line. This differs on purpose from a move, which goes free when nothing snaps (D78): a trim doesn't say the sound has left its line.
- **Fades.** The fade in stops at the fade out; the fade out leaves a tenth of the sound and pushes the fade in back, as the core does. A press on a dot that goes nowhere sends nothing, so it can't turn auto off by accident. Auto fade off keeps the current value.
- **Quick buttons:** a line counts if it reaches more than 50 ms past the sound's start (prototype). Lines with no audio are skipped. With no such line, the rest of the source.
- **Close:** Esc, the close button, a click outside, hiding its bucket, the sound going, `S`, or leaving the page. The transport and the header don't count as outside, so play, Follow and the room can be used with the view open. Esc closes first, and a second Esc clears the selection.
- **Opening** scrubs to the sound (prototype) unless it's playing, which also moves the locator, as any scrub on this page does.
- **Waveforms:** the sound's whole source from `peaks_batch`, fetched once on opening. Until it arrives, the placement's own envelope. The dialogue column uses the takes' peaks, the same ones as the spine, in the same character colours.
- **Keyboard** (fits D78's map): ↑/↓ on a part step 100 ms, ⌥ 10 ms, and on the body ⌥↑/↓ slips. ⇧↑/↓, ←/→ and ⌥←/→ keep loudness, depth and pan. Key presses keep anchors, as all key moves do, and a burst is one command. Tab stays inside the view. Over the view, ⇧ scroll is the open sound's loudness, since it covers the balls behind it.
- **Changed from D78:** Enter on a ball opens it; ⇧ Enter selects. Enter with a mouse selection opens the last one selected, so it no longer plays the line in context while sounds are selected.

### D81 The room in the transport (Rob, 2026-11-04)
- **Rob:** a room's home is the project setup, Sounds tab, per scene (D76). The floating bar gets a small read-out and shortcut only.
- **"The context bar":** the app has no bar by that name. The transport is the floating bar with scene context, and it is the bar the prototype's hints sit near, so the read-out sits there. It shows only on Arrange with the sounds on: nothing else on the page needs it, and the Edit page has its own control bar.
- **One source of truth:** the chip reads `scene_rooms` and keeps nothing but a slider's value until it lands. The size slider sends the same `set_scene_room`, with damping and wet as the document has them, one undo step per release.
- **Room settings…** opens Import › Sounds, unfolds Rooms and focuses this scene's Size. The header's Back button returns to Arrange in that case.
- **Audible during playback:** it wasn't. `player::playhead` meant to reload the schedule when the document changed, but it asked `bus.refresh()` first, which is only true for another process's write, so the app's own edits were heard from the next Play. A size slider beside the transport is no use that way. It now compares the schedule and reloads when it differs, so a room, gain, fade or move is heard at the next poll, with the same short restart a seek has. A favorite, a note or anything else not heard restarts nothing, and an audition keeps playing what it plays. This is in the service, not the engine (out of scope for this card); the engine is untouched.
- **After a mouse drag** the slider gives up the focus, so Space plays straight away. Keyboard use keeps it.

### D82 Keyboard and screen readers across the sounds (2026-11-04)
- **Tab:** the layer is one Tab stop (roving `tabindex`). From there Tab and ⇧ Tab go through the balls in time, skipping hidden buckets, and past either end leave the layer. DOM order would follow the order sounds were placed in, and re-sorting the DOM would drop the focus mid nudge.
- **Multi-select:** ⇧ Enter adds or removes the ball in focus; ⌘A takes all visible sounds; then every key acts on the selection, as D78.
- **Announcements:** one polite live region. It says selection, opening and closing, what a burst or a gesture ended on, removals and buckets fading. Not during a drag, and not on undo, which the bridge doesn't describe yet.
- **Buttons and the page's keys:** Space and Enter on a focused button used to run the page's action instead. `data-own-keys` marks the opened view, the room chip and the panel: there a button reached with Tab does what it says. `:focus-visible` keeps Space playing after a mouse click. Elsewhere in the app it stays as it was, to not change everyday keys.
- **The wheel** ignores sliders so as not to scrub while dragging a value. The opened view's handles are sliders only for the keyboard, so they are exempt (`data-part`).
- **Reduced motion:** drops the column's and the sounds' glide, the panel's slide and the view's fade too. Card B had missed the first two because they are inline styles.
- **Edit page:** handover 10 asks for pan and depth to show there, and they didn't. The inspector gains Pan and Depth (Depth only for sounds in the room) and marks an automatic fade out.
- **Not done:** adding a sound from the bucket still needs a mouse drag. The card lists the controls to reach and this isn't one of them; a key on a bucket row that places at the playhead would be the natural way.

### D83 Sounds drag with the mouse, not HTML5 drag and drop (2026-11-04)
- **Found:** Rob couldn't drag a sound in. Every drag in the app used HTML5 drag and drop, and none of them can work in the desktop app: with `dragDropEnabled` on, Tauri's handler returns `true` for every drag (`tauri-runtime-wry`), so wry never passes it to the WKWebView and `dragover` and `drop` never reach the page. Chromium has no such layer, so every Playwright drag passed. The prototype used mouse events; card B's port changed that.
- **Options:** turn `dragDropEnabled` off, which ends Finder drops (a page can't read a dropped file's path); use Tauri's native drag position for page drags; or drag with the mouse. **Chose the mouse:** it's what the prototype does, the same in a browser and the app, and Finder drops keep working.
- **`lib/drag.svelte.ts`:** `grab` on a row's mousedown, `dropzone` on a target. Four pixels make a press a drag; Esc or losing the window calls it off. A pill follows the pointer until a zone shows the sound itself. All four sources (scene bucket, Epidemic results, both lists of the bucket drawer) and all three targets (the sounds layer, Edit lanes, Import › Sounds buckets) use it. A browser test fails if HTML5 drag comes back.
- **Rule 5.1 holds:** a mouse drag fires hover, which an HTML5 drag doesn't, so during a drag nothing in the layer takes the pointer but the field.
- **The bucket drawer's drag went nowhere even in a browser:** it set a type nothing read since the lanes moved to `application/x-cutaway-bucket`. It now drops on the lanes and keeps its cue (`cue_id` on `place_from_bucket`).
- **Finder drops on Arrange:** with the sounds layer on, audio went to `import_audio` as a recording, re-ran the file matching and left the page. Now it's a sound: into the library, placed if it landed by a line. Tauri calls the drop position physical, but on macOS wry reports points, so it is used as CSS pixels. Not tried by hand yet.
- **Dev note:** `tauri build` writes `target/debug/Cutaway`, which on a case-insensitive disk is the CLI's `target/debug/cutaway`. Delete it and `cargo build -p apc-cli` afterwards.

### D84 The buckets have their own preview player (Rob, 2026-11-04)
- **Rob:** Play in a bucket moved the timeline's playhead. A bucket gets a preview player; the main player is only for what sits on the timeline.
- **Two decks in `player.rs`:** a preview has its own engine and clock, so it never touches the timeline's position, schedule or scene. Same `Engine`, a second instance opened on first use, so no change to the audio engine. New methods `stop_sound` and `sound_playhead`; `play_sound` keeps its name; `stop` stops both, so closing an episode leaves nothing playing.
- **One at a time (Rob):** starting a preview stops the timeline, its playhead staying where it stopped, and starting the timeline stops the preview.
- **Space** while a preview plays stops the preview and doesn't start the timeline; otherwise stopping a sound by Space would set the scene off. The transport's button always means the timeline.
- **The player:** a strip at the foot of the bucket panels (scene bucket, Import › Sounds' panel and search): name, waveform to click in, time. It stays after the sound ends, to play it again. Leaving it stops the sound, so nothing plays that has no stop button on screen.

### D85 What a drop makes of a sound (Rob, 2026-11-04)
- **Rob:** a sound dropped from a bucket keeps its length under 6 s. Over 6 s it's cropped to 10 s, unless Loop is on: then it goes on as ambience. This replaces the prototype's rule (up to 3 s whole, 3 to 8 s cropped to 3, longer 20 s) and the full length the app used.
- **So a sound of 6 to 10 s is whole,** there being nothing to crop. Loop matters only past 6 s, so the Loop chip shows only there.
- **Loop lives on the bucket** (`SoundBucket.loops`, by asset or provider id): a show's sound isn't in the episode until placed, and a preview has no asset yet. Without a setting, ambience loops by default, which is the show's `room-tone` folder and anything else the library files as ambience. Rob can turn any of them off.
- **A looped drop** goes on the ambience lane even if dropped on an effects lane, starts with a 1 s fade, and repeats to the scene's end as the scene stands then. It doesn't follow later changes of the scene's length; its tail trims like any sound's. `Placement.looped` is the one exception to "never longer than the source".
- **Drops only:** `place_sound` (a cue's Place button) still places the whole sound. Finder drops follow the rule.
- **Music and vocals keep their full length (Rob):** he crops music by hand.
- **Export:** a clip can't outlast its file in an edit, so FCPXML gets the file repeated, cut end to end. The crossfade at the seam exists only in playback and the bounce.

### D86 A room tone per scene (Rob, 2026-11-04)
- **Found:** room tone was per location, switched on in Settings, with no way to pick a sound.
- **Rob:** set a looping room tone in Import › Sounds, per scene. `Scene.tone` (a sound and a level), `set_scene_tone`, and a picker and level in each Rooms row, with `cutaway sfx room-tone`. A scene without one still plays its location's, so nothing set before changes. Options: per location only, where the three scenes of one place couldn't differ. Rob asked for per scene.
- **The picker lists sounds marked Loop,** room tone buckets first. A show's sound joins the episode when picked.
- **Starts at −6 dB,** the location tone's default, with a range of −48 to +6 in the row.
- **Seams:** a loop jumped straight back to its start, a click on any file not cut to loop. The mixer now crossfades the last half second into the first (equal power, a quarter of the file at most) for room tone and looped sounds alike, in playback and the bounce. The loop's period is that much shorter, and the Edit page's seam marks follow. Not tried by ear on real room tone yet.

### D87 The Sounds toggle eases (Rob, 2026-11-04)
- **Rob:** on and off was too wild with its overshoot; it should be premium, eased and sleek. This replaces handover 6's "playful" toggle (shrink into the spine, overshoot on return).
- **Now:** the balls glide a fifth of the way toward the spine and fade, scaling only to 82 %. In: an expo-out, no overshoot, 0.7 s. Out: 0.4 s. The stagger is at most a quarter of a second in all, however many sounds. A browser test fails on an overshooting curve there.
- *Superseded in part by D89:* the column did swing past its mark, and the pop no longer overshoots.

### D88 ⇧ Space from a selected sound (Rob, 2026-11-04)
- **Rob:** with a sound selected on Arrange, ⇧ Space plays from its start.
- **Several selected:** from the first of them. Pressed during playback it starts there again, the way ⇧ Space does from the locator. The playhead and locator move there, as a scrub does.
- **Nothing selected:** the locator, as on every other page. A ball that only has the keyboard's focus counts as selected, as it does for the other sound keys. The opened sound counts too.

### D89 Arrange moves evenly (Rob, 2026-11-04)
- **Rob:** the animation on Arrange still overshoots. He wants an even, eased move.
- **Found:** D87 eased the balls, but the big overshoot was the dialogue column, and no easing could have fixed it. Its `left` ran over 0.85 s while its centring `translate(-50%)` ran over 0.3 s, and the two add up. Going 92 → 420 px it swung to −81, off the screen; coming back it reached 684. D87 said it never overshot, because its curves don't: that was wrong, and only measuring the move shows it.
- **Now:** the column has one horizontal motion, `left` and `width` in pixels, with no translate. The toggle's parts share `--ease-move` and `--dur-move` (0.7 s): the column's move across and up or down, and the panel. The balls use the same curve. The transition exists only during the toggle, so resizing the window doesn't animate.
- **A dropped sound's pop** fades in from 82 % on the same curve, with no overshoot. That was the last overshooting curve in the app. It replaces handover 6's "slight overshoot".
- **Tests:** a browser test measures every frame of the toggle and fails if anything turns back, and another fails on any overshooting curve in the code.
- **Left as they are:** the merged group's wobble and swirl, which are slow loops, not moves.

### D90 Voices in the room, and where wet lives (Rob, 2026-11-04)
- **Rob:** the people should have a little of the room too, per ambience, and the room's and the characters' wet and dry settings go to the Dev tab.
- **Reading:** "room tone" here means the scene's room, its reverb, not the looped bed: he asks for wet and dry on characters in the same breath. So each voice sends a little into its scene's room, and it sounds like the place it's in.
- **`Character.wet`,** 0 to 1, default 0.25, a send of −24 dB, four below a close sound. It's new: characters had no such setting. Per character, not per scene: the room's own Wet scales it, so the scene decides how much place there is and the character how much they take of it. Only the send: the voice keeps its level and brightness, so nothing changes in the editing of dialogue. Curve in `depth::voice_send`.
- **An actor's vocal sounds** on a dialogue lane get the same. A take auditioned alone stays dry, so takes compare fairly. Music and room tone stay dry.
- **This replaces D76's rule that dialogue never touches the room.** The guarantee that's left: a voice at wet 0 bounces exactly as it did, sample for sample.
- **Every scene now rings out** a little past its last line in playback and the bounce, as scenes with sounds already did. Bounces are per scene, so nothing downstream shifts. FCPXML is unchanged: it carries no room.
- *Superseded by D92:* wet lives in Import › Rooms, not in the Dev tab.

### D91 Search in the bucket, actions on Arrange (Rob, 2026-11-04)
- **Search:** a bar on the docked bucket (Arrange and Edit). It narrows this scene's buckets, by sound or bucket name and keywords, and lists matches from the rest of the library under them, up to 30, to play and drag. Esc clears it. Import › Sounds keeps its Epidemic search.
- **Rob:** on Arrange he needs to see the actions between the dialogue, to pick a sound to tag to them. They were one dim, cut-off phrase in the pause pill, and those before the first line or after the last didn't show at all, so a scene with no dialogue looked empty.
- **Now:** each action has its own row in the column, in full, with tags for the sounds hanging on it. They show with the sounds layer off too.
- **Tagging:** drop a sound on the action's row, or beside it in the field, where the row catches at its full height and the tooltip names it. A sound anchors to the action (M7) and follows it. A ball moved there hangs on it the same way.
- **An action has no time of its own:** several in one pause share an instant unless their spacers are on. So a sound hangs on its action with an offset, at the place in the pause where the row sits, which keeps the ball beside its action. `place_from_bucket` gained `offset_s` for this.
- **First layout:** the column now takes its place at once, without a glide. An action above the first line would otherwise slide it down each time Arrange opens.

### D92 Import › Rooms, and ambience isn't a bucket (Rob, 2026-11-04)
- **Rob:** the ambience and its assignment are janky, and a room's ambience shouldn't be a bucket among the SFX. A tab of its own for room setup, less crowded: dry and wet, the ambience (one per room), and how much wet goes into the character tracks.
- **The tab:** Files, Sounds, Rooms. A list of rooms; one room at a time, with Space (size, damping, wet) and Ambience; the voices beside it, as they go with the character, not the room. It replaces the Rooms panel on Sounds and Dev › Room mix (D90). One room per location: see D97.
- **Out of the buckets:** `studio::is_ambience_bucket`: the `room-tone` folder's bucket, and spotted "… ambience" buckets with nothing in their folder. Hidden from `buckets`, kept in the document so spotting still knows its cues. Loose ambience leaves Loose SFX too.
- **A trap in Episode 2:** spotting linked "Airfield ambience" and "Time Spring Command ambience" to `sfx/airfield` and `sfx/time-spring`, folders of real effects. Hiding by name alone would have lost those sounds, so a bucket with files in a folder of its own keeps them and shows as "Airfield".
- **The choices** (`ambience_sounds`): hidden buckets' sounds, what this scene's cues asked for first, then every other ambience in the library. "Add a file…" brings one into the room tone folder and straight into the room. Every choice plays on the preview player.
- **Loop chips stay** on bucket rows: D85's rule for a long sound dropped on the canvas is Rob's, and he didn't take it back.

### D93 Loops turn around inside the file (Rob, 2026-11-04)
- **Rob:** it should loop and crossfade from somewhere in the middle and somewhere not too close to the end. Replaces D86's seam from the file's very end to its very start: files often fade in and out there.
- **Now:** the file plays to 85 %, then a crossfade of up to 3 s goes back to 45 %, and that stretch repeats. It's cut from what follows the turn, so the last 15 % sets the limit on a short file. The first round is the file itself, unchanged.
- **For every loop:** room tone and looped sounds, in playback and the bounce. FCPXML still repeats the whole file (D85). The Edit page's seam marks follow.

### D94 Directions before the first line are on the timeline (Rob, 2026-11-04)
- **Rob:** they're part of the timeline, sounds can be placed there, and they count as break spacers.
- **Now:** a direction before the first line takes its spacer time from the start of the scene, and the line follows it. Before, the scene began on the first word. A spacer turned off takes none, and tightness applies as to any spacer.
- **What changes in Episode 2:** scenes grow by their opening directions, about a second each up to 2.5: scene 1 has 8 and scene 9 has 15. Dialogue moves later; anchored sounds move with it; sounds at a free time stay where they are, so they shift against the dialogue.
- **On Arrange** these rows have real time on the map, so a ball sits beside them and a sound on one starts at its start.

### D95 Filing a sound that comes in from the Finder (Rob, 2026-11-04)
- **Rob:** drop, and it guesses deterministically where the sound lives or proposes a new bucket. Enter, or correct the name (a new name is a new bucket), or click the right bucket's chip and Enter: it's in, and already in the scene. This replaced a first try with a zone per bucket, which didn't work well.
- **The sheet** (`FileSheet`) opens where the sound was dropped on Arrange with the layer on, Edit and Import › Sounds. Nothing is imported until Enter; Esc leaves nothing behind. Typing narrows the chips. Several files go to one bucket, and the first goes in the scene.
- **The guess:** `studio::bucket_by_name`. Words of a bucket's name score 3, its folder's 2, its keywords' 1, and the file's first word, the kind of sound in a library name, counts double. Ties go to the earlier bucket. With no match, `proposed_bucket`: what a library name opens with ("ES_Metal, Impact" → "Metal"), or the first two words. No AI and nothing stored, so the same name always gets the same answer.
- **In the scene:** on Arrange at the height it was dropped, snapping as any drop does, even over the column or the panel. On Edit at the playhead, as before. The drop rules (D85) apply.
- **Ambience** is a chip too, and the guess for a file named as one: it goes to the current scene's room, not the timeline.
- **Not tried by hand:** no OS drag can be sent from a session. The browser tests call what Tauri's drop event calls, through a hook that exists only in dev builds.

### D96 Less on screen (Rob, 2026-11-04)
- The "Cuts a word" tag is gone from Arrange's cards; the handles still turn amber.
- The hints under the transport are gone, and the one inside it. The help overlay (?) keeps the keys.

### D97 A place is one room (Rob, 2026-11-04)
- **Rob:** the base level for an ambience is −28 dB. Locations that are the same share the room tone, with an optional second one for day and night. He shouldn't have to fill everything in by hand. And ambience sounds get a drop zone.
- **Found:** he'd already set an ambience on eight scenes of Episode 2, with levels by ear. So nothing moves: `Scene.tone` stays, and sharing happens when it's read (`Document::room_tone_for`). A scene without one plays what its location has, for its time of day if set apart, then what the setting it belongs to has (Throughline's parents, so the Situation Room plays the TARMAC hallway's), then the location's room tone. Options: move ambience onto `Location`, cleaner but a migration of his work.
- **One row per location** in Import › Rooms. Choosing or levelling writes to every scene of the place in one step (`SetRoomAmbience`). The room's size, damping and wet are shared too, which replaces D76's room per scene: `SetSceneRoom` reaches every scene of the location, so the transport's size slider stays in step.
- **Night:** a scene is night if its time of day says night, evening, dusk or dark. Day and Night tabs show only where a place has both kinds of scene. With no night ambience the night scenes play the day's. "Continuous" and "Later" count as day; a heading doesn't say more.
- **−28 dB** is where a new ambience starts, and the default for location room tones. Scene 2 of Episode 2 is at −6, the old start, and stays there until Rob moves it.
- **Drop zone:** a dashed zone in the room's Ambience; the whole Rooms tab takes a drop for the room open. Elsewhere it's the Ambience chip (D95). The first file becomes the ambience; others wait among the choices. Files go in the show's room tone folder.

### D98 A direction's length (Rob, 2026-11-04)
- **Rob:** change the length of a direction, to fit the timing of its sounds.
- **Now:** the bottom edge of an action row on Arrange drags, like a pause, with its length beside it. It's the existing spacer (`set_spacer`), so a direction between lines widens that pause, and one before the first line pushes the scene on. Lines and anchored sounds after it follow. One undo step, 0.1 to 30 s, in steps of 0.05 s.
- **The row's height** follows its time, at the scale of the pause rows, so a long direction looks long.
- **A direction turned off** turns on when dragged: asking for length means it should take time.

### D99 Snapping off on Arrange (Rob, 2026-11-04)
- **Rob:** a button and holding ⌥ turn snapping off for sounds on Arrange, to position them precisely.
- **The button:** Snap in the header, beside Sounds and Follow, remembered per viewer. Off, nothing catches: moves, trims, drops and the opened sound.
- **⌥ was pan.** Both stay, by direction, the way a plain drag splits depth and time: ⌥ sideways pans, ⌥ up or down moves in time with no snap. Pressed during a move or a trim, it lets go there and then. This changes handover 4, where any ⌥ drag pans.
- **In the opened view** ⌥ on the edges trims without snapping; ⌥ on the middle still slips.
- **A sound put down free has no anchor,** so it stays at its time when the lines around it move.

### D100 One name at a time (Rob, 2026-11-04)
- **Rob:** the names under the balls clutter; show only the selected one, somewhere central.
- **Now:** no labels under balls or merged groups. The selected sound shows on a tab growing out of the transport's top edge, in the transport's own material and curved into it, with its bucket's colour, bucket and length, or "3 sounds selected". It slides up from behind the edge and down again. Screen readers keep each ball's full label.

### D101 Character search in the bucket (Rob, 2026-11-04)
*The bar and the drop are removed by D118. `search_performances` and what it finds stay, for the mapping editor (D104).*
- **Rob:** bucket search gets a sound mode and a character mode. Character mode searches the performances for words or grunts: type "paul huh" and it shows where he might have done that, pre-clipped. Adjust the trim, drag it in.
- **The bar:** Sounds and Characters switches above it, each with its own query, on the docked bucket (Arrange and Edit). The mode stays across pages. The first word or words are read as a name, by label or alias, with or without "The". With no name it searches everyone; a name alone lists that character's vocal sounds. Names show as chips until one is typed.
- **What it searches:** the cached transcript of each actor's own mic, all of it, takes and between takes, across the episode. `search_performances`, a read method, so the CLI and MCP have it too. Nothing is stored.
- **What counts:** the word itself first, then what the recogniser may have heard instead, marked "sounds close". For a grunt that's the sounds in `ALIKE` and of its vocal kind, and only standing alone: an "uh" inside a sentence is no "huh". Other words allow a misspelling at 80 %. Several words are a phrase, in order. Standing alone and louder rank higher. 30 show at most.
- **Grunts without words:** the recogniser hears none, so "grunt", "breath", "laugh", "sigh", "panting" and "running" also search what the vocal finder hears in the level, between takes, without the bucket's caps. It tells these sounds apart by length and level only, so they're candidates.
- **Not their voice:** left out if it's over 3 dB louder in another actor's mic of the session, or 18 dB under this mic's typical take level. The analysis's bleed margin is 10 dB; a grunt may be quieter than a line.
- **The clip:** word times snapped to the speech like a take's edges, 80 ms before and 150 ms after, stopping at the words around it. 1.5 s of audio shows either side. The edges drag, or step with the arrow keys (10 ms, ⇧ 100 ms). Playing it uses the preview player, on the source itself: nothing is cut to listen.
- **A drop** cuts the stretch into `Library/Vocals/<Character>/` as a `recorded` asset, keeps it in that character's vocal bucket, creating the bucket if needed, and places it, in one undo step (the file stays in the library, as an analysis's do). The same stretch again is the same sound. Source media isn't touched. It keeps its length, as vocals do (D85).
- **Lanes:** *replaced by D102.* It went on the character's dialogue lane, and a drop on Arrange didn't show there.
- **Lengths** on a hit read to the hundredth: `len` rounds to whole seconds, and a grunt is under one.
- **On Episode 2's transcripts** (a copy): "paul huh" gives 12, "paul grunt" 3, "paul breath" 332, the best 30 shown. Not listened to.

### D102 Adlibs have their own track (Rob, 2026-11-04)
*Removed by D118.*
- **Rob:** a character's adlib dragged out of a bucket is an adlib. It goes on an adlib track, and on Arrange it looks like a sound effect. It goes there and not somewhere else.
- **`Lane::Adlib`:** one track for the whole cast, stacking in layers when adlibs overlap, as SFX does (`adlib1`, `adlib2` in the mixer). `Placement::character` keeps whose voice it is. It replaces the dialogue lane as a vocal's home (the placement part of the vocals decision) and D80's "vocal sounds stay on the Edit page".
- **What counts as an adlib:** a sound kept in a character's vocal bucket, or a clip from the character search (D101).
- **Always there:** the lane a drop came in on is ignored, on the Edit page too. A dialogue or SFX lane doesn't take an adlib, and the Adlibs lane lights up while one is over any lane. The Adlibs lane takes nothing else.
- **On Arrange:** the vocal buckets are in the panel, after the scene's effects and before the whole show's, with their own colour and eye. Colours go to the effects first, so these don't shift. A dropped adlib is a ball like any sound: depth, pan, fades, anchors, one undo step.
- **In the room:** an adlib plays through the scene's room like an effect, so a ball's depth means something. That's also what "like a sound effect" gives. `Character.wet` doesn't apply to it.
- **Length and fades:** it keeps its length (D85) and its short vocal fades. Loop doesn't apply.
- **On the Edit page:** an Adlibs group between Dialogue and Sound effects, always shown, with each clip in its character's colour.
- **Export:** FCPXML has `ADLIB 1` … as lanes of their own, with the role `dialogue.Adlibs`. They're no longer on the actor's track. Options: a track per character, closer to the old export; Rob asked for an adlib track.
- **Vocals already on a dialogue lane stay there.** Nothing moves by itself. They keep working as before, and don't show on Arrange.
- **The file:** a project with an adlib in it needs this build or later. An older one can't read the lane.

### D103 Build output stays under 20 GB (Rob, 2026-11-05)
- **Rob:** the Rust build output filled the GitHub folder. Rebuilds can be slower, as long as it stays under about 20 GB.
- **The cause:** `cutaway/target` had reached 69 GB. macOS leaves debug info in `.o` files next to each build, 444,442 of them, every dependency was built with full debug info, and Cargo never prunes old builds.
- **Debug info:** `[profile.dev]` is now `debug = "line-tables-only"` and dependencies are `debug = false`. A full `check.sh` from clean now leaves 6.9 GB and 4,860 `.o` files, and took 4 minutes. Backtraces keep file and line for our crates. A debugger can't inspect variables any more, or step into dependencies.
- **The cleaner:** `scripts/clean-target.sh` counts `target/` across the main checkout and every worktree, and deletes until the total is under the limit: other worktrees' `target/` (least recently built first), the main incremental cache, the main compiled output, then the release build. `--dry-run` shows what would go, `--limit` changes the 20 GB.
- **Always kept:** `target/debug/cutaway`, because the MCP server launches from it, plus `cutaway-app` and the sherpa-onnx download.
- **Compiled output goes in one step:** a fingerprint left behind would describe output that's gone.
- **A running build skips the run.** A running app doesn't, or it would rarely run, since `tauri dev` is often left open.
- **When it runs:** at the start of `check.sh`, and hourly once `scripts/install-cleaner.sh` has installed a LaunchAgent. Rob installs that himself, since it stays on the machine. The limit is checked at each run, so the total can pass it between runs.
- **Alternatives:** one shared `CARGO_TARGET_DIR` was rejected, because parallel builds would wait on each other and overwrite the binary the MCP server runs from. `cargo-sweep` was rejected as a new tool for what a short script does.

### D104 The mapping editor (Rob, 2026-11-05)
- **Rob:** sometimes the mapping is wrong despite everything. A way to pick the take by hand: M on Options, the waveform with the words on it, in and out points, scroll into the vicinity, a search bar for sentences, a button makes a new take. A plus between lines adds a line the same way. An X deletes a segment, with a question first, and ⌘Z undoes it.
- **M, on Options only:** opens on the picked take of the current line, on its mic, with its marks. A line without a take opens near the takes around it. It renders inside the Options page; the other pages have no M.
- **Reading a mic:** `recording` gives all its words and takes once; scrolling only asks for peaks. A strip on top shows the whole file. Scroll moves, ⌥ or ⌘ scroll zooms. Words sit under the waveform on up to three rows, and what doesn't fit is left out until you zoom in.
- **Words are what the recogniser heard,** not the script. Where it misheard, the word is wrong, and the waveform and the ears decide.
- **Marking:** drag on the waveform, drag the marks, I and O at the locator, ⇧ click moves the nearer mark, double click marks a word, arrow keys on a mark (10 ms, ⇧ 100 ms). Space plays the marks, ⇧ Space plays from the locator, both on the preview player, on the source itself.
- **`TakeOrigin::Marked`:** a take marked here is heard exactly as marked. It gets no handles (`add_handles`), or the 150 ms handle would bring back the word Rob just cut off. Options: a flag on `Take`, but that's a field on every take for what the origin already says.
- **Creating a take:** it goes last, is picked, and takes the comp, word edits, head and tail trims and splits of the line with it: those belonged to the old take. The other takes stay. A mute stays, since it's a choice about the line. One undo step. Its loudness and level are measured like any take's. It's on a character's mic or it's refused, as a dev mark's take is.
- **Which words a take holds:** those with their midpoint between the marks.
- **Search:** `search_performances` gets a `character`. Words are read as they stand on that character's mic first, since a line can start with a name ("Paul, wait"), then with a leading name as a name. From three words a search is a sentence: 70 % of its words in order are enough, marked "sounds close". This reaches the bucket's search too, so D101's "several words are a phrase" now allows a misheard word. Below 70 % nothing shows: at 60 %, a three-word search matches two common words all over a session.
- **A new line:** `ScriptLine::adlib`, as a line from the unmatched bin is, with a Marked take, after an item by id, not by index. It starts on the speaker of the line above, and its text from what was heard, which can be typed over. With nothing heard the text has to be typed; the words are then spread over the stretch. It's marked "added", and exports like any line.
- **Deleting:** `ScriptLine::removed`, off the scene's order. Options: erase the line, as a new script version does. But the matcher would then give its audio to the lines around it, and a new script version would bring it back. So the aligner still sees it, its takes stay, and `apply_version` keeps it out. It's gone from the timeline, Options, the exports, the scene counts and the spotting. Directions delete the same way.
- **What hung on a deleted line:** sounds and cues go to the start of the next line, where the line was, or the end of the one before; with no line left, sounds stay where they play. Speech it used from the unmatched bin can be used again.
- **Re-running the analysis:** Marked takes stay, and so do the takes of adlib lines, which the matcher doesn't look at. Those used to be deleted by a full run. What stays is numbered after the run's takes.
- **Older builds:** a project with a Marked take needs this build or later. An older one can't read the origin.
- **On Episode 2 (a copy):** a 50-minute mic is 2058 words, 247 kB, no noticeable wait. A sentence search takes a few ms, and finds a line with two words wrong unless that's over 30 % of it. A take made from real audio came out exact (743.11 to 746.23), with its own loudness, and undo gave back the 11 takes before it. Not listened to.
- **In the real webview:** on a demo, with in-page events and no sound: 28 checks of the editor, the plus and the question, on WKWebView. Keys, wheel, drags, focus and undo work. A real mouse and hover, real typing and the sound on a real device wait for Rob.

### D105 Takes can be changed and deleted, and a line reads as it was said (Rob, 2026-11-05)
- **Rob:** not only a new take: the current take can be edited, and delete deletes the take. Fields sometimes show wrongly attributed text, so show what was transcribed, with a field to put the right word in. A longer crop should show the whole sentence, not just "the". The delete pop-up asks take or line.
- **Why it read "the":** D72 shows the script's words, and only those matched. Words heard outside the script had no place, so a crop longer than the line lost them.
- **Replaces D72.** Every card, the box editor, Arrange and Edit show the take's own words. `lib/words.ts` lost the spreading of script words over heard ones. **The cost:** the transcription errors D72 hid are back on the cards, on every take. The way out is the text field. Options: script spelling where a heard word matches a script word, heard words elsewhere. Rob asked for the transcription, so this stays simple until he's seen it on Episode 2.
- **Three actions in the editor:** Save take N (⌘⏎) changes it where it stands, Save as new take (⏎), Delete…. Enter stays on the one that loses nothing. Numbers along the top and the T chips on the waveform move the editor to another take of the line.
- **`edit_take`:** the number, favorite, pick and notes stay. New marks or another mic make it `Marked`. New text alone makes it `Corrected`: the matcher's edges and handles stay, because fixing a word mustn't change what's heard. Options: everything `Marked`, but a spelling fix would then clip the line's handles. New marks clear what was cut to the old stretch: trims, comp, splits, word edits.
- **The text:** stored in the take's words, so karaoke, comps, word delete and the SRT follow with no second copy. `timed` aligns what was typed to what was heard: a word that matches keeps its time, the others share the time around them by length, so a corrected word takes the place of the misheard ones.
- **A correction belongs to the take.** The transcript and the search don't change; the script doesn't. In the editor a take's own words lie over the mic's, so a correction stays when the crop grows. Text typed in the field doesn't follow the marks after that: Use what's heard fills it again.
- **Cut list and FCPXML names stay on the script's text:** they say which line it is.
- **`delete_take`:** the line stays, the next take is picked, the numbers close up. With the last take gone the line shows "no takes".
- **A re-run doesn't undo either.** `BoxState::dismissed` remembers a deleted take by id and stretch, and more than half of the same stretch on the same mic counts as the same performance. An edited take keeps its id, so the matcher finding it again changes nothing.
- **The pop-up:** one, from the card's × and the editor's Delete…: Delete take N, with the focus, or Delete line. Rob's earlier "just deletes" would have been no question at all; he then asked for the question. A line without takes and a direction only offer themselves.


### D106 The arrows walk the bucket chips (Rob, 2026-11-05)
- **Rob:** when a dropped sound almost has a match, cycle through the chips with the arrows, then Enter, or Shift to complete, once the right one is selected.
- **Arrows:** all four, round the ends, from the chip the name already is. ↑ and ↓ alone would do, but the chips are a row. The cost: ← and → no longer move the caret in this field.
- **Enter** files into the chip lit, and the line under the field names it. **Shift** alone, on release so Shift and a letter stays a capital, completes the name from the chip lit, or from the first match while typing. Options: Tab, the usual completion key; Rob asked for Shift.
- **Nothing is lit until an arrow is pressed,** and typing lets go. A first match lit by itself would take Enter away from a new bucket's name.
- It works wherever the sheet opens, not only on Arrange: it's one sheet.

### D107 ⌘D duplicates sounds (Rob, 2026-11-05)
- **Rob:** ⌘D should just duplicate SFX, on Arrange and Edit.
- **Where a copy goes:** right after its sound. A selection follows on from its last sound with its spacing kept, so a run of footsteps repeats as a run. One undo step, and the selection moves to the copies, so ⌘D again goes on.
- **What it keeps:** everything about how it's set: loudness, pan, depth, fades, trim. On a free layer, as any placed sound.
- **What it doesn't:** the anchor and the cue. The original hangs on a word; a copy after it has no word to hang on, so it stands at its own time and doesn't follow when the dialogue moves. It answers no cue.
- **Any selected sound,** adlibs and music too. Dialogue clips stay as they are: they're takes of a line, not sounds.
- `duplicate_sounds` on the CLI and MCP.

### D108 Exported scenes lock their timing (Rob's handover, 2026-11-05)
- **Brief:** `tasks/export-tab`. After delivery a stray take swap would silently break everything downstream, so an exported scene refuses timing edits until it's unlocked.
- **Where the rule lives:** in the bus, not in the commands. It takes a print of each locked scene's timing before a command and after it, and rolls back on a difference. The handover lists what a lock blocks and ends with "anything else that changes the scene's length or moves something inside it", and a check on the result covers exactly that, including commands that don't exist yet, `set_config` on pacing, `execute_fn`, and a batch with one bad edit in it. The other option was a list of commands to refuse, which every new command would have to remember.
- **What the print holds:** each item's place, each piece's source and in, out and start points, each sound's asset, place, length, in point and loop. Gain, pan, depth, fades, mutes, notes and favorites aren't in it, so they stay free. A favorite that would change the default take does move something, and is refused for that reason.
- **The one exception:** a new script version keeps the lock, sets `script_changed` on scenes whose lines it touched, and lists them in the change summary, as the handover asks.
- **A whole re-analysis** is refused at its start: the bus would refuse it anyway, but only after the long part. Scoped re-analysis goes through and is refused only if it moves something.
- **Undo:** `lock_scene` is undoable, as the handover asks, so ⌘Z or `undo` removes a lock without the question that `unlock_scene` asks. Undo is linear, so a timing edit from before a lock can't be undone past it, and a lock ends the redo. **Known gaps, left as they are:** undoing a lock doesn't ask, and reverting to a backup restores whatever locks that backup had.
- **Open question 2 (locks and non-timing changes):** gain and pan flag the scene *changed since v003*, as proposed, and aren't blocked.
- **Open question 4 (default of the checkbox):** on, as proposed.

### D109 The Export page (Rob's handover, 2026-11-05)
- **Fourth page, key 4,** replacing the export sheet and the header's Export button. ⌘E goes to the page, and on it exports. The single-format exports (`export_fcpxml` and friends) stay for the CLI and MCP.
- **Conflict with the handover:** "the scenes picked on the other pages": the app has no scene selection, so Range starts on the current scene.
- **Conflict:** the show file is `.cutawayshow`, not `.apcshow`.
- **The popups are the system's,** invisible `<select>`s over a label, as the header's scene picker already does. So Format is one list of useful combinations rather than three controls: it's what the mockup draws.
- **Blocked edits** say so at the pointer, or under the header when the edit came from the keyboard. The handover's "where the edit was attempted"; the toast would have been at the bottom of the screen.
- **Presets** are named settings in the project config (`export.presets`), so they travel with the episode. A preset menu appears beside the title once there is one.
- **Lock by hand:** the scene rail on Options and Arrange, and a button beside the scene picker on Edit, whose list is a native popup that can't hold one.
- **Muted and soloed lanes** shape exports, as they always have (bounces go through the same schedule). The page says so, because a solo left on empties a delivery.

### D110 Stems come from the one render path (Rob's handover, 2026-11-05)
- **How:** `Renderer` takes a `Part`: whole, dry (before the room send), or wet (the room's 100 % return). A stem is one part of some of the clips. The room is linear and its modulation depends only on the frame, so separate returns add up. Measured better than −100 dB on a scene with something on every lane, and the test fails at −22 dB when one return is misrouted.
- **Scene length is where the last sound has rung out,** not where the last line ends. Otherwise every stem would cut the reverb tail off. All the files of a scene share it, to the sample.
- **The groups:** a character's lane and adlibs are dialogue. **Room tone goes on Ambience:** the handover names no stem for it, and it has to land on one for the sum to hold.
- **An ambience in the room rings on Room · effects:** "Room · effects carries only the room reverb of the effects" can't quite hold, because ambience takes a send and the sum must come out whole. Depth applies as it does in the mix: a far effect's dry stem is quieter.
- **Loudness:** the mix alone, never past −1 dBFS. When the target would clip, it stops short and the manifest and the page say so. The manifest keeps the gain, so the null can still be checked against a loudness-matched mix.
- **Individual clips** are what was recorded with their handles: no gain, no fades, the source's rate and channels. Those stay on the clip in the FCPXML. Room tone isn't a clip. Ticked room tracks and the mix still render whole beside them.

### D111 Audio formats (Rob's handover, 2026-11-05)
- **Open question 5 (MP3 and FLAC):** FLAC anywhere, MP3 only for Mix only. MP3 pads its start and end, so it can't hold rule 1, every file exactly the scene's length. The page leaves it out, and the service refuses it.
- **FLAC and MP3 go through ffmpeg,** as the MP3 bounce already did: no new dependency, and FLAC stems were read back and are sample-exact. Without ffmpeg the page offers WAV only.
- **32-bit is float, WAV only.** No dither at 16 bits: a silent stem has to stay digital silence, and stems have to sum. Rob may want dither on a 16-bit mix one day.
- **Other rates** resample through the mixer's linear resampler, like everything else today. Fine when sessions are at 48 kHz, as Rob's are. An export at another rate deserves a better one.

### D112 Versions live on disk (Rob's handover, 2026-11-05)
- **The history is the manifests,** not a table in the document. A version can't then disagree with what's on disk, and undo or revert can't lose it or reuse a number. The other option, recording each export through the bus, would have put exports in the undo list.
- **Counters run across both destinations,** so v003 means one thing wherever its files landed.
- **Written hidden, then renamed,** so a failed export leaves no version, and a sync tool never picks up half of one. When a later scene of an export fails, the finished ones stay and are locked, and the error names them.
- **Timeline hash:** what's heard plus what the timeline files say (the schedule and the scene's timeline), so a note or a gain counts as a change. blake3 throughout, as elsewhere in the app.
- **A whole-episode export** writes every scene's version as usual, then `Ep02/Episode/v002 …/` with the episode's own FCPXML, SRT and cut list, and a manifest of which scene versions it holds. Those files point at the original recordings.
- **Open question 3 (show-level delivery number):** not built. Versions per scene and per episode, as the handover specifies.

### D113 FCPXML in an export (Rob's handover, 2026-11-05)
- **Room lanes need rendered room tracks.** "One lane per character plus two lanes for the room tracks" and "FCPXML only writes without rendering audio" can't both hold: FCPXML only has nothing to point at. So the room lanes appear when the room tracks are in the same export.
- **Rendered stems as the source** replace the clips with one full-length clip per stem, leaving out the mix, which would double everything. The scene and note markers stay.
- **Exported clips as the source** point each clip at its file, starting after its handle.
- **In the existing writer:** `write_with` takes the files behind a timeline's clips and where its markers come from. `write` is unchanged.

### D114 The pipeline folder (Rob's handover, 2026-11-05)
- **Stored in the show file** (`pipelineFolder`), relative to the root when it's inside, so it holds on the other machines of the Syncthing share.
- **The search** looks in the show and the two folders above it for `Sound`, `Sounds`, `Audio` or `Ton`, ranking folders in a pipeline (a show file, `*.jetplane.json`, `Episodes`) first, within a budget of folders, and skipping `Episodes` so an episode's own `Sound` isn't taken for it. On tm-2610 it proposes `tm-2610/Sound`, the show's library, and exports would land in `Sound/Exports`, beside `sfx` and `music`. **Rob should check that's the right place** (open question 1).
- **Never to a guessed path:** nothing writes there until the path is stored, and `set_pipeline_folder` needs `confirm: true`, because an agent could otherwise confirm its own proposal.
- **A show export** opens each episode in turn, as background jobs do. The Range reads episodes straight from their files, read-only, so looking changes nothing.

### D115 A bucket called Ambience broke the filing sheet (Rob, 2026-11-05)
- **Rob:** dragging in a sound doesn't work.
- **Cause:** the sheet for a Finder drop always offers an Ambience chip, and the chips are keyed by name. Episode 2 has a bucket called Ambience, so the key came twice and Svelte stopped rendering the sheet: one chip, an empty name, no guess. The demo has no such bucket, so every test passed, and only a run on a copy of the real episode in the real webview showed it.
- **Choice:** one chip per name, ignoring case, the fixed chip first. A bucket called Ambience was already out of reach by name: that name means the room.
- **Ruled out on the way:** bucket drags work on the real data, in every scene, with Snap on and off, and in the real webview. A scene locked by an export refuses a sound, as it should, but the lock came after the report.
- **Still silent:** a sound dropped on the dialogue cards, left of the spine, does nothing and says nothing. Left as it is for Rob to decide.
- **Also fixed:** a pipeline folder that is itself called `Exports` no longer gets `Exports/Exports`.

### D116 Bars step down instead of folding (Rob, 2026-11-05)
- **Rob:** the layout and tool bars should work on smaller screens and not fold into each other, and the scrollbar on the left is ugly: think of something else.
- **What was wrong:** the header wrapped to two or three rows over the page below about 1300 px, and on Arrange ran past the window. The Edit bar ended up behind it. The scene rail disappeared under 1000 px and the bucket under 1100 px, so scenes and sounds were out of reach.
- **`lib/fit.ts`:** a bar steps down until it fits, by measuring. Breakpoints can't do it: how wide the header is depends on the page, the Dev tab and the names in it. It measures on resize, when its children change size, and in the next frame after its contents change. Never during Svelte's update: reading a size there forces a style pass, and Arrange's column then committed its move early and lost its slide.
- **Header:** one row. Tighter, no key hints, a ⋯ menu for Size, Settings, Keyboard shortcuts and History, short labels, and Follow into the menu last. Snap and Follow keep their words: alone, each is the same dot.
- **Edit bar:** tighter, a shorter read-out, then Snap, Ripple, Lock, Wave and zoom into a ⋯ menu.
- **Scene rail:** no scrollbar. It sits between the header and the transport, and the numbers tighten until every scene shows: 16 scenes fit a 700 px window. Past the smallest size that reads, it pages, with steppers at its ends that are there only while something is out of sight, a fade on the cut edge, the wheel, and the current scene kept in view. The other options were a hidden scrollbar alone, which gives no sign of more, and a popup list, which costs a click every time.
- **Bucket under 1100 px:** an overlay, shut until asked for (B, or the tab on the edge), that fades away during a drag so the field is free. Its open state is separate from the wide window's, so resizing doesn't throw one over the timeline. Arrange gains the tab that Edit had.
- **The lock banner** moved below the Edit bar, which it was lying on.

### D117 The default export goes straight into the episode's Exports (Rob, 2026-11-05)
- **Rob:** the default path should be the project's dedicated export folder. Choosing a folder made it redundant, and in Episode 2 there shouldn't be a subfolder also called Episode 2. Default settings, and custom settings that create the export folder.
- **Conflict with the handover,** which draws `Exports/Ep02/Sc02 Canteen/…` for both destinations ("files look the same wherever they land"). Rob's word goes for the episode: `Exports/Sc02 Canteen/v003 2026-11-05/`, and `Exports/Episode/…` for a whole-episode version.
- **A custom (pipeline) folder keeps `Exports/Ep02/`:** it holds every episode, so there the level tells them apart, and it creates its `Exports` unless the folder is one already (D115).
- **File names keep `E02_S02_v003_…`:** they say where they're from once they leave the folder.
- **Nothing to choose for the default:** Save to starts on Episode exports, and a folder is asked for only when the pipeline is picked. The popup no longer repeats `Ep02`.
- **Versions already in `Exports/Ep02/` aren't counted:** the layout lasted hours and the only real export was deleted by hand.

### D118 Vocal adlibs aren't sounds any more (Rob, 2026-11-06)
- **Rob:** the vocal adlibs per person get exported into `Library/Vocals`. Remove having them as SFX, now that takes can go in between the script lines (D104).
- **Gone:** the analysis no longer looks for vocals or cuts anything. Also gone: the vocal buckets ("Paul · vocals"), `Lane::Adlib` and the Adlibs lane on Edit, adlib balls on Arrange, `ADLIB` lanes and the `dialogue.Adlibs` role in FCPXML, the Adlibs stem, and the Sounds and Characters switch on the docked bucket with its drop (`place_from_bucket`'s `clip`). That drop was the other thing writing to `Library/Vocals`, and it could only make an adlib. Options: keep the search there and drop into a take, but the mapping editor already does that with the waveform in front of it.
- **Stays:** `ScriptLine::adlib`, a line added by hand or from the unmatched bin. That's the replacement. `search_performances` stays too, with the grunt finder in `apc-analysis::vocals`: it's how a grunt with no words is found in the mapping editor. A sound dropped on a character's dialogue lane stays as well (D47): it's any bucket sound, nothing is cut for it.
- **Older files, read around rather than rewritten:** no schema change, no migration.
  - A placement with `"lane": "adlib"` reads as `Lane::Dialogue` with its character (a serde alias). It stays audible, on that character's lane, and now plays outside the room.
  - A bucket row with a `character` isn't taken into the document. That's in `Document::put_value`, so a load, an undo and a backup all go through it.
  - A `recorded` asset nothing uses is left out when a document loads, so the cuts don't fill Library and search. One placed, kept in a bucket or used as a room tone stays.
  - Options: a schema 3 migration that deletes the rows. It'd be cleaner in the file, but it can't be taken back in Rob's production file, an older build couldn't open that file any more, and undo steps and backups would bring the rows back anyway.
- **Left on disk:** files in `Library/Vocals` are never deleted by this. Nothing refers to the unused ones any more, so Rob can delete the folder by hand. On Episode 2 (a copy, read only) that's 188 cuts, 36 MB, in 7 buckets, none of them placed.
- **Trade-off:** the dead rows stay in the `.cutaway` file. They're small, and a future migration can clear them.

### D119 A scene's mark (Rob, 2026-11-06)
- **Rob:** mark a scene as in progress, review or finished, so he doesn't go into scenes and get lost. It's the colour of the scene's cell on the left. Unmarked scenes stay neutral, and a right click on the rail sets it.
- **`Scene.status`:** optional, `in_progress`, `review` or `finished`, left out of the file when unset, so older files and their hashes stay as they are. It survives a new script version like the room does.
- **`set_scene_status`:** one undo step, on the CLI as `cutaway mark` and on MCP. It says nothing about timing, so a locked scene takes it. A mark and a lock are separate things: "finished" doesn't lock, and a lock doesn't finish.
- **The colour:** blue, violet and green, as tokens in both themes. Amber was left out because the flagged dot already means that. The current scene keeps its ring, so it still reads when it's marked.
- **The menu** opens beside the rail with the mark that's set ticked. The context menu key opens it too, and the arrows, Enter and Esc work in it.

### D120 The room chip leaves the transport (Rob, 2026-11-06)
- **Rob:** remove the room and wet indicator from the transport on Arrange. He doesn't need that setting there.
- **Gone:** the chip and its quick size slider, with the helpers only it used. This replaces the shortcut of D81.
- **Stays:** every room setting on Import › Rooms, and `scene_rooms` and `set_scene_room` for the CLI and MCP. The way from Arrange to the settings moves to the pan pop-out (D122).

### D121 A longer direction is a bigger block (Rob, 2026-11-06)
- **Rob:** making a direction longer on Arrange should make the block visibly bigger.
- **Why it didn't:** D98 set the time as a *minimum* height, 14 px a second. A row's text already takes about 28 px, more when it wraps, so a direction under about 2 s, or a long one in wrapped text, didn't move at all.
- **Now:** the time adds to the text, at the same 14 px a second, so every tenth of a second shows. Directions take a little more room in the column than they did: the default of 0.6 to 2.5 s adds 8 to 35 px each.
- **The drag** goes at the same scale, so the edge stays under the mouse. It was 26 px a second and ran ahead of the row.
- **A direction turned off** takes no time, so it has no extra height, until it's dragged (D98).

### D122 Where the characters stand (Rob, 2026-11-06)
- **Rob:** set the pan of the characters per room, from a small pop-out behind an icon in the Arrange transport. The pans inherit per room, and when he changes one, that scene has a custom one.
- **Two values:** `Scene.room_pan` is the room's, and every scene of a location holds the same copy, the way the room itself does (D97). `Scene.voice_pan` is the scene's own. A scene plays its own, or else the room's, or else the centre. Both are left out of the file when empty.
- **The pop-out** lists the characters who speak in the scene. *This scene* writes the scene's own value, and an arrow takes it back to the room's. *Whole room* writes what the place's scenes inherit. It opens on This scene, which is what he asked a change to do. The other option was to have a first change write to the room, but then nothing shows which of the two a slider is setting.
- **A room's value doesn't touch a scene with its own,** or a custom value wouldn't stay custom.
- **`set_character_pan`,** one undo step per release, `scene_pans` to read them, on the CLI as `sfx voice-pan`, on MCP. It isn't timing, so a locked scene takes it, as sound pan already does (D108).
- **In the mix:** on the voice's clips in `PlaySchedule`, so playback, bounces and stems all have it. It adds to the lane pan on Edit, which stays for the whole episode, the way a sound's pan adds to its lane. A sound on an actor's dialogue lane stands with them. A take auditioned alone stays centred so takes compare fairly (D90's reason).
- **FCPXML carries no pan,** for voices or for sounds, as before. Pan set here won't reach Resolve through it. Stems and the mix have it.
- **New scenes:** a scene that appears in a new script version takes the room's values from a scene in the same place.
- **Room settings…** moved here from the room chip (D120), the way from Arrange to Import › Rooms.

### D123 Glue: a sound stays on the moment it was put (Rob, 2026-11-06)
- **Rob:** sounds should glue to the exact moment of the block where he puts them, even when things change later. He turns glue off per sound in the opened sound if he needs to.
- **What was wrong:** a sound only followed the dialogue if it caught a snap target: a line's start or end, a pause in a line, or a direction. Anywhere else, or with Snap off or ⌥ held, it was pinned to a time and stayed behind when the lines moved. Dragging an anchored sound off its target freed it too (D78, D99).
- **Now Snap decides where a sound lands, and glue whether it follows.** A drop or a move on Arrange hangs the sound on the word being said at that moment, with an offset for exactly how far into the word it is. Outside a word it hangs on the line's start, and between lines on the nearer edge. In a direction with time of its own it hangs on that direction. Where two lines overlap, on the one that started last. The rule is `timeline::glue_point`, so UI, CLI and MCP share it. This replaces D78's "a move that doesn't snap frees the sound" and D99's "a sound put down free has no anchor".
- **The exact offset isn't rounded to milliseconds,** so turning glue on doesn't move a sound, and a locked scene's timing doesn't change.
- **`Placement.unglued`:** glue turned off by hand, with the Glue button in the opened sound or Pin to time on Edit (`set_sound_glue`, on the CLI as `sfx glue`). The sound stays at its time, and moving it, even onto a snap target, doesn't hang it on anything again. Without this flag a move would glue it straight back.
- **What takes glue:** sounds on SFX lanes and dialogue lanes. Ambience, music and loops run under the scene, with nothing to follow.
- **`glue` on `move_sound`, `place_sound` and `place_from_bucket`:** a time with `glue` hangs the sound there. Without it, `at_s` is still a plain time, so scripts that mean a time keep working.
- **A copy (⌘D) hangs on the moment it lands on.** This replaces D107's "a copy stands at its own time". It still answers no cue.
- **A word the take doesn't have:** a take swap could leave an anchor with no word to find, and the sound fell back to a time from an older cut. It now takes the nearest word of the line, then the line's edge. It only falls back to the time if the line has left the timeline.
- **Edit:** drops there glue too, and Follows dialogue / Pin to time are the same command. Moving a sound on Edit keeps what it had: an anchored one takes a new offset, a pinned one stays pinned.
- **Sounds already in a file aren't changed.** One that was pinned stays pinned until it's moved on Arrange or its Glue is turned on, because rewriting placements in a production file without being asked isn't safe. Rob decides whether to convert the ones in Episode 2: a batch of `set_sound_glue` does it, undoably.

### D124 The opened sound's scrubber (Rob, 2026-11-06)
- **Rob:** the SFX editor should have a scrubber and I/O for trimming, close on Enter too, and play the sound alone when it's focused.
- **The scrubber** is a rail left of the sound, with a line across it. A drag on the rail, or on the dimmed source, scrubs. A click on the sound puts it there and doesn't move the sound. ↑ and ↓ on the rail step 100 ms, ⌥ 10 ms, like the other parts. It reaches the whole source, so a trimmed sound can be heard past its ends.
- **The sound alone:** with the view in focus, Space plays the sound from the scrubber on the preview player, without the scene, and the timeline's playhead stays. It stops when the view closes. ⇧ Space stays the timeline. It plays the file, so fades and gain aren't heard and neither is depth; the trim is what this is for. A drag restarts the sound at most every 90 ms and stops with the mouse. This is a stepped scrub, not a varispeed one: the preview player has no such mode.
- **I and O** set the sound's in and out at the scrubber, one undo step each, through the same rules as the handles. Glue stays, with a new offset, as with any start trimmed off a target (D82). A locked scene refuses them with a message.
- **Enter closes,** as Esc does. A button reached with Tab still does what it says.

### D126 A panned voice sits further back (Rob, 2026-11-06)
- **Rob:** the left-right pan should move people the tiniest bit back into the room, and closer to the centre they're drier. Add the sensitivity to Settings.
- **How:** panning moves the voice back along the same depth curve the sounds use (`depth::voice_mix`). Three things change together: less direct sound, more send to the room, and slightly softer highs. One curve, so sounds and voices go back in the same way.
- **The amount:** a hard-panned voice goes back by up to 0.2 on the depth scale. At the default setting of 50 % that's 0.1: 1.4 dB less direct sound, 1.8 dB more room, and highs rolling off from about 16.6 kHz. At 100 %: 2.8 dB, 3.6 dB and 13.7 kHz. These are starting values, set for "the tiniest bit" and not yet listened to.
- **Squared with the pan,** so the middle stays close and only the sides go back: at a quarter of the way out it's a sixteenth of the effect.
- **The centre is untouched:** a voice at pan 0 gets the same numbers as before, so existing mixes without pans don't change. This reads "drier towards the centre" as the centre staying where the character's wet puts it, and the sides getting wetter. The other reading, drying the centre below that, would change every existing mix.
- **Which pan counts:** the whole pan of the voice, the scene's or room's plus the lane's on Edit.
- **A voice at wet 0** stays out of the room however it's panned, so D90's guarantee holds: nothing at wet 0 gets a send, or a change in tone.
- **`pacing.pan_room`,** 0 to 1, Settings › Pan into the room. It's per episode like the other settings there, and 0 turns it off. Heard in playback, bounces and stems. It's in `Pacing` because that already reaches both, and it has no effect on timing.

### D125 Side by side (Rob, 2026-11-06)
- **Rob:** drag a line next to another on Arrange, and they sit next to each other. He can offset one, and for now only two run in parallel.
- **Reading:** next to each other means heard together. Negative pauses already overlap lines, but only against the end of the line before, so a change to its length moves the overlap, and the rows stay stacked.
- **`BoxState.beside`:** the line it runs next to and an offset from that line's *start*, so the pair holds when either line is trimmed. The next line follows whichever of the two ends last. A negative offset starts the second one first.
- **On Arrange:** drag by the grip onto the right half of a card, which shows a dashed "Side by side" slot. The dragged line goes right after the other in the order and to its right. The pill on the second card drags it up or down, in 50 ms steps, and the card follows the mouse at 26 px a second. The × puts it back in line, or drag it out. The pair is one row for the spine, so sounds map onto it as onto a card.
- **Two lines at most:** a line in a pair offers no slot, and `set_beside` refuses a third. It's stored as one line pointing at the one before it, so more than two would need a different model.
- **What ends a pair:** moving either line out of order, and pinning the second one. Anything that leaves the two lines apart some other way, such as a new script version, doesn't delete the field: the layout only honours it while the partner is right before it, and otherwise the line is back in the chain.
- **Edit:** moving the second line there, and `set_gap` on it, changes its offset. There's no pause before it to change. Moved without ripple, the first line takes the second with it and the line after them stays.
- **Nothing else changes for the audio:** overlapping clips already played, bounced and exported. A pair of two characters is on two lanes. Two lines of one character overlap on one lane, as a negative pause does.
- **A locked scene refuses it:** the lock print sees the new timing, so no extra check was needed.
- **The transport map** now draws overlapping time once, in time order. It used to add the lengths up, so an overlap, by pause or pair, drifted the map off the playhead. A line that sits wholly inside another doesn't show on the map.


### D127 The Export sheet lies wide (Rob, 2026-11-06)
- **Rob:** the export panel is made for a vertical setup, rearrange it for a horizontal one.
- **Why it had to change:** the sheet was one 640 px column, 835 px tall. In the default 1440 × 900 window the Export button was cut off by the bottom edge, and over half the width stayed empty.
- **Options:** two columns, which leaves the left one tall and the sheet still high; three columns; or a wide sheet with the rows in a new order.
- **Choice:** three columns in the old row order, read left to right. Range and History, then Export as and Tracks, then Format, Also export and Save to. 1160 px wide at most, 484 px tall by default.
- **Labels go above their controls.** This changes the handover's "labels on the left, one control per row": the label gutter cost 150 px, and each column needs that width.
- **Tracks stays under Export as,** so the rows that a mode hides never empty a column.
- **The sheet stays at the top of the page,** not centred, so a mode that hides rows doesn't move the controls under the pointer.
- **Never taller than the window:** the columns scroll inside the sheet, and the header and footer stay, so Export is always in view.
- **Smaller windows step down, as in D116:** below 1200 px the padding tightens, and the stems picture and the tracks' side notes go. The app's smallest window, 900 × 640, still has three columns. Below 860 px, which only a browser on the bridge reaches, it's one column and the page scrolls.
- **History rows** say Episode or Pipeline, with the full name as a tooltip. The narrower column cut the long name off, and that's the part that tells two rows apart.
