# Status

Read this at the start of each session. Update it at the end of each work block.

This file holds only what is open now. The history is in `docs/RUN_REPORT.md`, the plan in `docs/PLAN.md`, and the decisions in `docs/DECISIONS.md`.

## Open questions for Rob

Do not guess the answers. Add a question here when the docs do not answer it.

- **Bundle ID.** `com.cutaway.app` ends in `.app`, and Tauri warns about that on each build. Changing it also renames the Keychain entry (`crates/apc-sound/src/secrets.rs`), so saved API keys would need a migration. Change it before the first release, or keep it?
- **MPL-2.0 dependencies.** `THIRD_PARTY_LICENSES.md` lists 17 MPL-2.0 packages, used unchanged. D161 put MPL-2.0 on the allowed list, as Jetplane does. Is that acceptable?
- **Proprietary.** D161 made Cutaway proprietary: `LICENSE.md` says all rights reserved, © 2026 Robert Sladeczek. Is that the license you want?
- **CDLA-Permissive-2.0.** `webpki-roots` (the root certificates HTTPS checks against) is under it, a permissive data license. D161 allowed it. Agreed?
- **Privacy notice.** Neither Cutaway nor Jetplane has one. Cutaway sends script and transcript text to the AI provider, and API keys to Epidemic Sound and ElevenLabs. Add one to Settings, and with what text?
- **Imprint.** Settings › Imprint has § 5 DDG and the contact from robertsladeczek.com. The website's § 18 MStV line was left out (D161). Is that right for an app?

## Known problems

- **One exported file doesn't match its manifest** (found while reorganizing, D165): `Episode 2/Sound/Exports/Sc03_Colonels-Office/versions/v002_2026-11-07/E02_S03_v002_Dialogue_SARA.wav`, and its copy in `current/`, have 64,754 bytes after the end of the audio. It plays the same as the other voices. Was Sc03 exported on two machines at once around 11:57 on 2026-11-07? A new export of Sc03 replaces it in `current/`.

- **Albert's review package that wouldn't reopen (D159) wasn't reproduced.** The likely cause, running out of open files, is handled. Ask Albert whether it happens again. If it does, ask for the full message and the output of `ulimit -n` in Terminal.
- **The Script tag is new (D164).** A build from before it can't read a review file that has a Script note. Update every machine first, and restart any idle `cutaway mcp` servers.

- **Six UI tests fail in WebKit and pass in Chromium.** Each is marked with `test.fail(browserName === 'webkit', 'BUG: …')`, so CI stays green and reports when one starts to pass. Two kinds:
  - Escape doesn't close the line menu after a right click (`arrange-lines.spec.ts`) or the pan popover (`arrange-round.spec.ts`). A likely cause: WebKit doesn't focus a button on click, unlike Chromium. If so, users of the app hit it.
  - Three drags do nothing or land wrong (`arrange-round.spec.ts` pair offset, `review.spec.ts` note card, `tour.spec.ts` grip). These may be Playwright's mouse in WebKit, not the app. `docs/human-tasks.md` has the check by hand.
  - Tab skips the close button of the opened sound (`sounds-layer.spec.ts`). macOS WebKit tabs to buttons only with keyboard navigation turned on in System Settings.
- **`plays_and_reports_the_playhead` (apc-audio) can fail under load.** It plays 0.3 s and expects the stop within 600 ms of wall-clock time. It failed once in a full `check.sh` run on 2026-11-07 and passed in the next run, and 3 of 3 times alone.
- **An unhandled error in the sounds layer,** in Chromium and WebKit alike: `TypeError: Cannot read properties of null (reading 'asset_id')` at `app/src/components/SoundsLayer.svelte:1114`, where `opened` is read after it became null. The UI tests log it through Vite and still pass.

