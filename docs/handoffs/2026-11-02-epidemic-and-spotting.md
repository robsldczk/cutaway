# Hand-off: Spot script on demand + Epidemic Sound search (2026-11-02)

For the agent testing and integrating this work. Merged into `dev` as `5a5ec5b`, built on branch `epidemic` (worktree `../cutaway-epidemic`, commits `28ca782` backend and `4d79ed3` UI). Decision record: **D60** in `docs/DECISIONS.md`. Not pushed.

State at merge: `scripts/check.sh` green, Playwright 14/14.

## What Rob asked for

1. Sound spotting (Claude reading the script for SFX) never runs by itself. There is a button to start it and an option to re-spot.
2. Epidemic searches show 5 results per bucket, with "Load more".
3. A free search bar for Epidemic. Results can be dragged into a bucket, or onto a card that creates a new bucket.

## What it does now

### Spotting (`crates/apc-app/src/buckets.rs` → `spot`, `spot_status`)
- Nothing spots automatically. The audio analysis never called spotting, and still doesn't.
- `spot_buckets { scene?, all? }`:
  - no `scene`, no `all`: spots only scenes that are **not spotted yet or whose script changed**;
  - `all: true`: re-spots every scene that has lines;
  - `scene`: always spots that one scene.
  - One AI call per scene. If a scene fails, it goes into `failed: [{scene, error}]` and the rest continue. The method only returns an error if every scene failed.
- `start_spotting { scene?, all? }` runs the same thing as a background job (`jobs.spawn("spotting", …)`). The UI polls `job_status`; the job's `message` reads "Scene N (i of n)".
- Change detection: `Command::ApplySpotting` now writes a `Setting` `spot:<scene_id>` = `{ "hash": … }` (`crates/apc-core/src/commands.rs`). The hash comes from `Document::scene_script_hash` (`crates/apc-core/src/model.rs`), which covers heading, location, and each line's id, kind, character and text. `Document::scene_spotted(id)` compares it. Undoing the spotting also undoes the hash.
- Re-spotting drops cue ids that no longer exist from the buckets it touches.
- `buckets` view now returns `spot: { scenes, pending }`.

### Bucket search (`buckets::search`, `keep_result`)
- `PAGE = 5`. A fresh search sets `search_offset = 0` and keeps **pinned** previews; "more" reads on from `search_offset`.
- New field `SoundBucket.search_offset` (serde default 0). It counts how far into the provider's results the search has read, so results hidden by filters no longer shift the next page.
- It reads up to 4 provider pages of 15 to fill a page of 5. Returns `{ added, previews, more }`.

### Free search and drops
- `search_epidemic { query, offset }` (Read, also exposed over MCP): pages of 10, nothing stored. Each result lists the `buckets` it is already in. Returns `{ results, next_offset, more }`.
- `bucket_add_preview { bucket_id, preview }`: adds a result as a **pinned preview**. Nothing is downloaded (the free API plan has 50 downloads). Dropping the same sound twice adds it once. A drop un-dismisses a dismissed sound. Vocal (character) buckets refuse drops; an already kept sound returns `added: false`.
- `create_bucket { …, previews: [Preview] }`: a new bucket that starts with pinned previews.
- New field `Preview.pinned` (serde default false).

### UI
- `app/src/pages/ImportSounds.svelte`:
  - **Search bar** in the top bar, disabled until Epidemic is connected.
  - **Spot button**, a split button labelled "Spot script" / "Spot N changed scenes" / "Re-spot script". Its menu offers "Re-spot all N scenes" and shows the number of AI calls.
  - **Drop targets:** bucket cards, plus a "New bucket" card that appears while search results are open. While a drag is in progress, cards show a dashed gold outline.
  - New buckets created by a drop are named after the search, with the query as keyword and the current scene.
- `app/src/components/EpidemicSearch.svelte`: the results panel, shown in the bucket panel's slot. Rows are draggable (HTML5, MIME `application/x-cutaway-epidemic`, constant `EPIDEMIC_DRAG` in `app/src/lib/types.ts`).
- `BucketPanel.svelte`: button now reads "Load more" (was "Load 12 more"), and it says when nothing more is found.
- `Icon.svelte`: new `search` icon.

### CLI
`cutaway sfx spot-script [--scene N] [--all]` and `cutaway sfx search <query> [--offset N]`.

## How to test

```
scripts/check.sh                                          # everything
cargo test -p apc-app --test spotting_and_free_search     # spot modes, job, pages of 5, pinned drops
cargo test -p apc-app --test buckets_and_import           # existing bucket flow (numbers now 5/10/8)
cargo test -p apc-app --test epidemic_connect             # key handling
cd app && npx playwright test                             # 14 specs, incl. tests/epidemic.spec.ts
```

- **Playwright setup changed.** `app/playwright.config.ts` now starts a third web server, `app/tests/epidemic-standin.mjs` (a fake Partner API on port 7359). `app/tests/serve-demo.sh` sets `CUTAWAY_SECRETS=memory` and `CUTAWAY_EPIDEMIC_URL=http://127.0.0.1:7359`, and unsets the real keys, so UI tests never touch the Keychain.
- **Specs share one server, in alphabetical order:**
  - `epidemic.spec.ts` connects, creates a bucket "creak", then disconnects at the end.
  - `round2.spec.ts` expects the panel to say "Connect Epidemic Sound". It also now waits for the button to read "Re-spot script", because spotting runs as a job.
- **Manual checks that remain** (Rob, or you with Rob's key):
  1. Settings → Connections: paste the real `epidemic_live_…` key.
  2. Import › Sounds: search, drag a result onto a bucket and onto "New bucket", then play a preview.
  3. Open the bucket and press Search. Expect 5 results plus the dropped ones; "Load more" adds 5.
  4. Spot script, then press it again: the second press should do nothing ("Nothing new to spot").
  5. Run the drag test **inside the Tauri window** (`npm run tauri dev`), not just Chromium.

## Known issues, most important first

1. **Never run against the real Epidemic API.** Endpoints, headers and response shapes match Epidemic's OpenAPI spec (`partner-content-api.epidemicsound.com/docs/spec.json`, checked 2026-11-02), but no real key has been used. The mocks assume `length` is in seconds; the spec says seconds for tracks, and sound effects are only typed as a number.
2. **Licensing.** A free Developer Portal key allows 50 downloads and carries **no commercial licence**. Downloads made under Rob's own Epidemic subscription would need Epidemic Connect (OAuth/PKCE), which is not implemented. This is Rob's decision; it is not a bug.
3. **Bug: the old per-scene spotting hides scenes from Spot script.** The B-key drawer (`app/src/components/Bucket.svelte`, `App.svelte:114`) and `cutaway sfx spot` call `spot_sound_effects`. That runs `ApplySpotting`, which now records the scene hash, but it does **not** group cues into buckets. Afterwards Spot script treats the scene as spotted and creates no buckets for it; only "Re-spot all" recovers. Two possible fixes:
   - record the hash only from `buckets::spot` (a flag on `ApplySpotting`, or a separate `Setting` write in the same command);
   - or have `spot_sound_effects` group into buckets too.
   Not fixed yet.
4. **Drag-and-drop in Tauri is untested.** `app/src-tauri/tauri.conf.json` has `dragDropEnabled: true`, which lets the window take Finder file drops. On Windows that setting is known to block HTML5 drag-and-drop inside the webview; macOS should be fine. The existing bucket-to-timeline drag on the Edit page has the same exposure. If drops don't fire, consider a pointer-event drag instead of HTML5.
5. **Possible lost update during a spotting job.** The job runs on its own connection (`Service::open(file, "pipeline")`). Each bucket goes through a read-modify-write `PutBucket`, which replaces the whole bucket. If the user drops a sound into a bucket, or edits its keywords, while the job is rewriting the same bucket, one change can overwrite the other. This depends on whether the worker re-reads the document before each `exec`; not verified. The download job and `bucket_search` already used the same pattern.
6. **"Load more" can be empty.** In the free search, `more` is a guess (`results.len() >= 10`), so a last page of exactly 10 shows "Load more" and it returns nothing. In the bucket panel the button shows whenever previews exist and ignores the `more` flag; it says "No more results…" when nothing was added.
7. **Re-spot leaves old state.** AI buckets that lose all their cues are kept, and old keywords stay (at most 6 per bucket).
8. **Script re-import.** If a re-import ever gives unchanged lines new ids, every scene counts as changed. On the demo, unchanged lines keep their ids (covered by the test).
9. **Cosmetic issues:**
   - the global toast (`App.svelte`, `top: 84px`, centred) covers the Spot button area of the Sounds top bar while it shows;
   - previews still use made-up waveforms, although the API returns a `waveformUrl` that could be used.
10. **Stale docs.** `docs/RUN_REPORT.md` still calls Epidemic a stub (D28). D37 and D60 supersede that.

## Integration notes

- **Decision numbers:** `editor` used D54–D59 and this work is D60. Any other branch adding decisions should start at D61.
- **Model changes are additive** (`#[serde(default)]`): `Preview.pinned`, `SoundBucket.search_offset`. Any new code that builds a `SoundBucket` literal needs `search_offset: 0`, and any `Preview` literal needs `pinned`.
- **`buckets::create` gained a final `previews: Vec<Preview>` argument.** `buckets::spot` now takes `(svc, scene, all, progress)`.
- **New methods** in `crates/apc-app/src/methods.rs`: `start_spotting` (not over MCP), `search_epidemic`, `bucket_add_preview`. `spot_buckets` gained `all`.
