# Kickoff prompt

Paste this into Claude Code, started in the repo root:

---

You are building Cutaway from this repository.

1. Read `CLAUDE.md`, `docs/BUILD_BRIEF.md`, all files in `docs/specs/`, `docs/DECISIONS.md`, and `docs/reference/stage0/` (the prototype and its report). Look at the fixtures in `fixtures/tarmac-ep2/`.
2. Check git: make sure you are on `dev` (create it from `main` and push it if missing) and that `origin` is reachable. If pushing isn't possible, continue locally and note it in `docs/PLAN.md`.
3. **Phase 1:** write `docs/PLAN.md` as specified in the build brief. Commit and push.
4. **Phase 2:** implement milestones M0 to M8 in order, in this one run, without asking for approval. After each milestone:
   - run `scripts/check.sh` until green,
   - tick the tasks in `docs/PLAN.md` and add a progress log line,
   - commit with Conventional Commits,
   - push to `origin dev`.
5. Record every decision the specs don't settle in `docs/DECISIONS.md`. When something is blocked, stub it behind its interface, skip its test with a reason, note it, and keep going.
6. **At the end:**
   - write `docs/RUN_REPORT.md`,
   - push,
   - open a pull request from `dev` to `main` with `gh pr create`. Do not merge it.

Never push to `main`, never force push.
