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
