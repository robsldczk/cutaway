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
