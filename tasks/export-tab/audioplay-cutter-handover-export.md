# Handover: Export page

**For:** the agent or developer adding the Export page to the Cutaway (Audioplay Cutter) codebase.
**Reference:** the static mockup `ExportTab.dc.html` is attached. It shows the layout and the default state. Behavior that a static picture can't show is described here.

## 1. Ground rules: fit into what exists

- **The codebase wins.** Build on the existing export code, command bus, undo log, audio engine (offline render), project folders (D5), versions and backups (D6), and FCPXML export (D9). Where this document conflicts with what the app already does, follow the app and log the decision in `DECISIONS.md`.
- **Extend, don't duplicate.** The app already has `export_fcpxml`, `export_mix`, `export_srt` and `export_cut_list`, scoped to one scene or the whole episode. The Export page is a front end for those commands plus the new pieces below (stems, room tracks, locking, versioning, show exports). Each option on the page must also be reachable from the CLI and MCP (D8).
- **Port behavior, not code.** The mockup is a single file built on a canvas runtime with fake data. Use the app's own components for popups, checkboxes, chips and buttons.

## 2. The page

Export is the fourth page after Options, Arrange and Edit. It shows one calm panel in the style of a macOS export sheet: labels on the left, one control per row, details tucked into popups.

| Row | Default | Choices and notes |
|---|---|---|
| **Range** | Scenes: the scenes picked on the other pages, as removable chips | Whole episode, or Scenes. Caption shows count, length and the last export of each scene (*"Sc. 2 was exported yesterday as v002"*). Checkbox **Lock these scenes after export** (see 5). |
| **Export as** | Aligned stems | Aligned stems · Individual clips · Mix only · FCPXML only |
| **Tracks** | all on except Split dialogue | Dialogue (dry), Room · voices, Effects (dry), Room · effects, Ambience, Music, Full mix, Split dialogue by character |
| **Format** | WAV · 48 kHz · 24-bit; Mix at −16 LUFS | WAV, MP3, FLAC; sample rate and bit depth; loudness off, −16 or −23 LUFS |
| **Also export** | FCPXML and Subtitles on, Cut list off | FCPXML for Resolve and Logic, SRT, cut list (CSV or Markdown) |
| **Save to** | Episode exports | Episode exports or the show's pipeline folder (see 4). Caption shows the version each scene will get. |
| **Footer** | | File count and size, Save preset, Export (⌘E) |

The **Export as** popup changes which rows show. FCPXML only hides Tracks and Format. Mix only hides Tracks.

## 3. Audio rules

1. **Aligned stems.** In stem mode, every file of a scene starts at the scene's 0:00 and is exactly as long as the scene, to the sample. That holds for dialogue, both room tracks, effects, ambience, music and the full mix. A track with nothing in it is still written as a silent full-length file, so a stack in Resolve or Logic is always complete.
2. **Dialogue is merged.** All characters go into one stereo dialogue file per scene. *Split dialogue by character* writes one full-length file per character instead.
3. **Voices and effects stay dry.**
   - **Room · voices** carries only the room reverb of the voices, and **Room · effects** only the room reverb of the effects.
   - In the render graph, the dry stems take the signal before the room send, and the room stems are the 100 % wet returns of those sends.
4. **The stems add up to the mix.** Dry dialogue + room voices + dry effects + room effects + ambience + music should null against the full mix before loudness. Add this as a test.
5. **Loudness applies to the full mix only.** Stems stay at mix level so they still add up.
6. **Individual clips.**
   - One file per clip with handles, named by scene, line, character and take.
   - In this mode, the FCPXML references the exported clips.
7. **FCPXML**
   - One lane per character (dry), plus two lanes for the room tracks.
   - Source: the original recordings (default) or the rendered stems.
   - Crossfades, scene markers and note markers, as the existing exporter already does.

## 4. Episode exports and show exports

Two kinds of export, two destinations.

- **Episode export.**
  - **Started from:** inside an episode (whole episode or some of its scenes).
  - **Goes to:** the episode's `Exports/` folder, next to the project file (D5). This is the working area: rough cuts, checks, sending a scene to someone.
- **Show export.**
  - **Started from:** the show level, the deliverable for the production. The Range row then lists episodes and their scenes across the show.
  - **Goes to:** the show's **pipeline folder**. It sits somewhere inside a sound folder of the production pipeline on Rob's machine.
  - **Finding that folder:**
    1. Look for a path stored in the show settings (`.apcshow`).
    2. If none, search the production folders above the show folder for a sound folder (names like `Sound`, `Audio`, `Ton`, any case) inside a pipeline structure, and propose the best match.
    3. Ask Rob to confirm it once, then store it in the show settings. Never write to a guessed path without that confirmation.
  - **Changing it later:** possible in the show settings.
- **Both destinations** use the same folder structure and versioning (below), so files look the same wherever they land.

## 5. Locking exported scenes

Once a scene is delivered, its timing must not drift, or the files already handed on stop lining up.

- **How a scene gets locked**
  - Automatically: the checkbox *Lock these scenes after export*, on by default, locks every exported scene.
  - By hand: lock and unlock from the scene list on any page.
  - Each is one command on the command bus (`lock_scene`, `unlock_scene`), undoable, and available in the CLI and MCP.
- **What a lock blocks:** every command that would change timing in that scene:
  - take swaps and comps,
  - trims and word deletes,
  - gap and pause changes, and reordering lines,
  - moving or trimming sounds, and adding or removing sounds,
  - anything else that changes the scene's length or moves something inside it.
- **What stays allowed:** non-timing changes such as gain, pan, depth, fades that don't change length, notes and favorites.
  - These mark the scene *changed since v003*, so a new export is suggested.
  - The timing stays identical.
- **Making Rob aware**
  - **Opening a locked scene** on any page shows a quiet banner: *"Sc. 2 is locked. Exported as v003 on 5 Nov. Timing can't change. Unlock"*.
  - **On episode open,** if it has locked scenes, say so once: *"2 scenes are locked because they were exported."*
  - **A blocked edit** shows a short inline message where the edit was attempted. It never fails silently.
  - **Unlocking asks first** and says what it means: *"The exported files won't match anymore. The next export becomes v004."*
- **Visible everywhere:** a lock icon on the scene in every scene list, and on the timeline the handles of a locked scene are hidden.
- **Agents too:** MCP and CLI edits on a locked scene are refused with a clear error naming the lock, unless they unlock first. Unlocking via MCP needs `confirm: true`, like other destructive tools.
- **Script changes:** if a new script version changes lines inside a locked scene, keep the lock, flag the scene, and list it in the change summary.

## 6. Automatic versioning

Nothing is ever overwritten. Exporting a scene again creates its next version automatically. There is no overwrite dialog for exports anymore.

- **Per scene.** Each scene has its own counter: v001, v002, v003. A whole-episode export gets an episode counter and records which scene versions it contains.
- **Folders** (same in the episode folder and the pipeline folder):

  ```
  Exports/
    Ep02/
      Sc02 Canteen/
        v003 2026-11-05/
          E02_S02_v003_Dialogue.wav
          E02_S02_v003_RoomVoices.wav
          E02_S02_v003_Effects.wav
          E02_S02_v003_RoomEffects.wav
          E02_S02_v003_Ambience.wav
          E02_S02_v003_Music.wav
          E02_S02_v003_Mix.wav
          E02_S02_v003.fcpxml
          E02_S02_v003.srt
          manifest.json
  ```

  The version is in every file name, so files stay identifiable once they leave the folder.
- **Manifest** per version: date, app version, export settings (preset), timeline hash of the scene, file list with durations and checksums, and whether the scene was locked.
- **Unchanged scenes.** If the timeline hash matches the previous version, still allow the export but say so (*"Unchanged since v002"*). This avoids accidental duplicate deliveries.
- **History.** The scene's export history (versions, dates, destinations) is visible on the Export page and in the scene list tooltip. Revealing a version opens its folder.

## 7. Why it is designed this way

- **One sheet, few choices.** Exports happen at stressful moments: a mixer waiting, a deadline. A single column with good defaults, like Apple's export sheets, means the usual export is one look and one click. Details live in popups and presets instead of on screen.
- **Aligned stems.** Mixers and editors stack stems in Resolve, Logic or Pro Tools. Full-length files that all start at 0:00 drop in without any lining up. Writing silent files for empty tracks keeps every stack complete and predictable.
- **Dry stems plus room tracks.** A mixer needs to rebalance or replace the reverb. Keeping voices and effects dry, with the room as separate wet tracks (one for voices, one for effects), keeps that possible without losing the room the director heard.
- **Loudness on the mix only.** Normalizing stems separately would break the sum. Only the deliverable mix needs a loudness target.
- **FCPXML only.** Often Rob only needs the timeline in Resolve or Logic to keep working, and rendering audio would be a wait for nothing.
- **Locking.** After delivery, a stray take swap would silently break everything downstream. Locking makes delivered timing safe by default, and the banner keeps the lock from becoming a mystery.
- **Episode and show destinations.** Working exports belong with the episode. Deliverables belong where the production pipeline expects them. Two clear destinations avoid copying files around by hand.
- **Automatic versions.** Overwriting delivered files is the classic post-production mistake. Counting up on its own, with the version in every file name, makes every delivery traceable and removes a decision from the export moment.

## 8. Open questions for Rob

1. The exact pipeline folder path and its naming convention, confirmed on first show export (4).
2. Should locked scenes also block non-timing changes (gain, pan), or just flag them as proposed?
3. Version per scene only, or also a show-level delivery number?
4. Default for *Lock these scenes after export*: on (as proposed) or off?
5. MP3 and FLAC: needed for stems, or only for the mix?

## 9. Done when

- In stem mode, every exported file of a scene has exactly the scene's length and starts at 0:00, including silent ones.
- The dry stems plus room tracks plus ambience and music null against the full mix before loudness.
- FCPXML only writes just the FCPXML (and the chosen extras), without rendering audio.
- Episode exports land in the episode's `Exports/`, and show exports in the confirmed pipeline folder.
- A second export of the same scene creates the next version with a manifest and never overwrites.
- An exported, locked scene refuses timing edits from the UI, CLI and MCP with a clear message. Opening it shows the lock banner. Unlocking asks first.
- All options are available in the CLI and MCP, and every conflict with the existing app is logged in `DECISIONS.md`.
