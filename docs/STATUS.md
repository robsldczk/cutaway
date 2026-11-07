# Status

Read this at the start of each session. Update it at the end of each work block.

This file holds only what is open now. The history is in `docs/RUN_REPORT.md`, the plan in `docs/PLAN.md`, and the decisions in `docs/DECISIONS.md`.

## Open questions for Rob

Do not guess the answers. Add a question here when the docs do not answer it.

- **Bundle ID.** `com.cutaway.app` ends in `.app`, and Tauri warns about that on each build. Changing it also renames the Keychain entry (`crates/apc-sound/src/secrets.rs`), so saved API keys would need a migration. Change it before the first release, or keep it?
- **MPL-2.0 dependencies.** `THIRD_PARTY_LICENSES.md` lists MPL-2.0 packages. `CLAUDE.md` asks for a DECISIONS entry for GPL and LGPL only. Is MPL-2.0 acceptable as it is?

## Known problems

- **Six UI tests fail in WebKit and pass in Chromium.** Each is marked with `test.fail(browserName === 'webkit', 'BUG: …')`, so CI stays green and reports when one starts to pass. Two kinds:
  - Escape doesn't close the line menu after a right click (`arrange-lines.spec.ts`) or the pan popover (`arrange-round.spec.ts`). A likely cause: WebKit doesn't focus a button on click, unlike Chromium. If so, users of the app hit it.
  - Three drags do nothing or land wrong (`arrange-round.spec.ts` pair offset, `review.spec.ts` note card, `tour.spec.ts` grip). These may be Playwright's mouse in WebKit, not the app. `docs/human-tasks.md` has the check by hand.
  - Tab skips the close button of the opened sound (`sounds-layer.spec.ts`). macOS WebKit tabs to buttons only with keyboard navigation turned on in System Settings.
- **`plays_and_reports_the_playhead` (apc-audio) can fail under load.** It plays 0.3 s and expects the stop within 600 ms of wall-clock time. It failed once in a full `check.sh` run on 2026-11-07 and passed in the next run, and 3 of 3 times alone.
- **An unhandled error in the sounds layer,** in Chromium and WebKit alike: `TypeError: Cannot read properties of null (reading 'asset_id')` at `app/src/components/SoundsLayer.svelte:1114`, where `opened` is read after it became null. The UI tests log it through Vite and still pass.

