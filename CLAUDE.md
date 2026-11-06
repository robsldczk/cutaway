# Cutaway

A desktop editor that turns unslated audio play recording sessions (one WAV per role + a director room mic + the script) into script-matched rough cuts.

Owner: Rob. Read the specs before changing behaviour:
- `docs/specs/architecture.md`: stack, layers, folders, providers, MCP, CLI
- `docs/specs/how-it-works.md`: pipeline, data model, audio rules, Stage 0 learnings
- `docs/specs/frontend-design.md`: pages, hierarchy, visual direction, keyboard map
- `docs/BUILD_BRIEF.md`: milestones, acceptance tests, scope of the current run
- `docs/reference/stage0/`: the Python prototype that proved the matching. Port its logic, don't copy its shortcuts.

## Git workflow

- **Branches:**
  - Work only on `dev`. If `dev` doesn't exist, create it from `main` and push with `git push -u origin dev`.
  - Never commit to `main`, never push to `main`, never merge into `main`. Rob merges `dev` into `main` through a pull request.
  - Never force push, never rewrite pushed history, never delete branches.
- **Worktrees:**
  - Remove a worktree once its branch is merged, including ones made by hand: `git worktree remove <path>`. The branch stays. Swimlane does this for its own cards and sweeps merged ones.
  - Never share one Cargo target folder between checkouts. Parallel builds wait on each other and overwrite the binary the MCP server runs from.
- **Commits:**
  - Commit small, coherent steps.
  - Use Conventional Commits: `feat(core): ...`, `fix(aligner): ...`, `test: ...`, `docs: ...`, `chore: ...`.
  - Every commit builds. Run `scripts/check.sh` before each commit.
- **Pushing:**
  - Push to `origin dev` after every completed milestone, and at least every hour of work.
  - If a push fails (auth, network), keep committing locally, note it in `docs/PLAN.md`, and retry at the next milestone.
- **End of run:** open one pull request `dev -> main` with `gh pr create`, summarising the run. Do not merge it.

## Working mode

- **Plan first, then build everything in one run.** Phase 1 writes `docs/PLAN.md` (all milestones, tasks, interfaces, risks). Phase 2 implements the milestones in order without waiting for approval.
- **Keep the plan live.** Tick the checkboxes in `docs/PLAN.md` as tasks finish, and add a dated line to its progress log after each milestone.
- **Record decisions.** Any decision not settled by the specs (library choice, format version, a deviation) goes into `docs/DECISIONS.md`: one entry per decision with context, options, choice and reason.
- **Don't stop for questions.** When something is unclear, take the most reasonable reading of the specs, record it in `DECISIONS.md`, and continue.
- **Blocked:** if something is blocked (a missing API key, a model download failing, a GitHub auth problem), stub it behind its interface, add a test that is skipped with a clear reason, note it in `PLAN.md`, and continue.
- **Don't edit the specs silently.** If implementation shows a spec is wrong, record it in `DECISIONS.md` and add a short "Changed in implementation" note to the spec.
- **End of run:** write `docs/RUN_REPORT.md`:
  - what was built,
  - what wasn't, and why,
  - how to verify it,
  - the manual checks Rob needs to do (e.g. open the FCPXML in Resolve).

## Code rules

- **Rust workspace** for the core. Crates per layer, see the architecture spec:
  - `apc-core`: document, command bus, undo, project I/O
  - `apc-media`: decode, peaks, loudness, sync, BWF metadata
  - `apc-audio`: real-time engine and offline bounce
  - `apc-analysis`: ASR providers, aligner, take detection, room classification
  - `apc-ai`: AI provider interface, Claude Code provider, mock provider, prompts
  - `apc-sound`: library, bucket, sound providers
  - `apc-export`: FCPXML, SRT, cut list, mix bounce
  - `apc-cli`: the `cutaway` binary (name from `brand.json`)
  - `apc-mcp`: MCP server
- **App:** Tauri in `app/`, UI in TypeScript (Svelte preferred; React acceptable if recorded in DECISIONS.md).
- **Separation:**
  - The UI never touches audio and never mutates state directly. Every change is a command through the command bus.
  - The UI, CLI and MCP share the same commands.
- **Non-destructive:** source media is never modified.
- **Errors:** use `thiserror` in libraries and `anyhow` in binaries. No `unwrap()` outside tests.
- **Dependencies:**
  - Prefer well-maintained crates with permissive licenses (MIT, Apache-2.0, BSD). Record any GPL/LGPL dependency in DECISIONS.md before adding it.
  - Pin versions in `Cargo.lock` and the frontend lockfile.
- **Tests:**
  - Unit tests per module.
  - Integration tests against `fixtures/tarmac-ep2/` (see `docs/BUILD_BRIEF.md`). Tests that need fixtures or credentials skip with a message when those are absent.
- **AI calls in tests** use the mock provider. Live Claude Code calls run only in tests marked `#[ignore]`.
- **Formatting and lint:** `cargo fmt`, `cargo clippy -- -D warnings`, and for the UI `npm run lint` + `npm run check`. `scripts/check.sh` runs all of them plus the tests.
- **Secrets:** never commit secrets. API keys come from environment variables or the OS keychain. `.env` is gitignored.
- **Fixtures:** `fixtures/` contains unreleased production material. It is gitignored apart from its README. Never commit audio, scripts or transcripts from it.

## Commands

```
scripts/check.sh            fmt, clippy, tests, UI lint/typecheck
scripts/clean-target.sh     keep Rust build output under 20 GB (--dry-run, --limit <GB>)
scripts/install-cleaner.sh  run that hourly through a LaunchAgent (--uninstall)
cargo run -p apc-cli -- --help
cargo test --workspace
cd app && npm install && npm run tauri dev
```
