# Cutaway

Script-matched rough cuts from unslated audio play sessions: one WAV per role, a director room mic and the script go in; a cut you can flip through take by take comes out, with exports to Resolve and Logic.

## Starting the build run

1. **Create a private GitHub repo** (e.g. `cutaway`) and push this folder as the initial commit on `main`:
   ```
   git init && git add . && git commit -m "chore: initial package"
   git branch -M main && git remote add origin git@github.com:<you>/cutaway.git
   git push -u origin main
   ```
   The fixtures are gitignored and stay local.
2. **Check the tools on the Mac:**
   - `gh auth status` (logged in),
   - Rust (`rustup`),
   - Node 20+,
   - Xcode command line tools,
   - `claude` CLI (used by the app's AI provider).
3. **Optional API keys** in `.env` (gitignored): `ELEVENLABS_API_KEY`, `EPIDEMIC_SOUND_API_KEY`.
4. **Start Claude Code** in the repo root and paste the prompt from `docs/KICKOFF_PROMPT.md`.

## What's in here

| Path | Purpose |
|---|---|
| `CLAUDE.md` | Rules for Claude Code: git workflow, working mode, code rules |
| `docs/BUILD_BRIEF.md` | Scope, milestones M0 to M8, acceptance tests |
| `docs/KICKOFF_PROMPT.md` | The prompt that starts the run |
| `docs/specs/` | Architecture, pipeline, frontend design |
| `docs/DECISIONS.md` | Decisions so far; Claude Code appends new ones |
| `docs/reference/stage0/` | Python prototype and its match report |
| `fixtures/tarmac-ep2/` | Test session (local only) |
| `.claude/settings.json` | Permissions for the run (no pushes to main, no force push) |
| `scripts/check.sh` | Placeholder; Claude Code fills it in M0 |
