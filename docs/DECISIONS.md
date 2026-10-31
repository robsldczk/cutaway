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
