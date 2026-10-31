# Audioplay Cutter: UI Round 2, Rationale and Usage

Covers the four Round 2 screens on the design canvas:
- **Import › Files**
- **Import › Sounds**
- **Edit · bucket docked**
- **Edit · full screen**

They refine Import A, Bucket C and Edit A from Round 1. Requirement IDs refer to `audioplay-cutter-requirements-edit-import-bucket.md`.

---

## 1. Principles across all four screens

- **Calm by default.** Show what is needed to decide. No summary sentences such as "20 cues in 6 groups". Counts appear only where they drive an action.
- **Colour carries meaning, nothing else.**
  - Character colours (kaki, asagi, matcha, fuji…) are the only colours on the Import screen.
  - In the editor, gold means sound effects, blue grey means room tone and ambience, and indigo means music.
  - Status is shown by fill: solid means real or downloaded, outline or grey means a suggestion or preview.
- **Claude acts only on request.** Every AI step is a button the user presses: *Redo match*, *Suggest*, *Spot script*, *Search*, *Generate*. Nothing is matched, spotted or downloaded silently.
- **One playhead, one selection, one undo** across Options, Arrange and Edit (EDT-01, EDT-06).
- **Keyboard first, with Logic conventions** where they exist.

---

## 2. Import › Files

### Why
The old import had four steps, a dense table and many colours. What actually matters is whether **every recording sits on the right character** and **every director room file is clipped to the recordings it belongs to**. The screen is built around that check.

It also has to work regardless of how the session was recorded:
- **Ensemble:** all actors at once, one director room file for everyone.
- **Sequential:** one actor at a time, one director room file per actor.
- **Mixed:** both, in the same episode.

So the model is two links, nothing more:
1. **Recording → character.** By file name, alias, or transcript (IMP-14, IMP-16).
2. **Director room file → the recordings it overlaps in time.** By timecode, or by envelope cross-correlation when there is none (IMP-15, IMP-17, IMP-18).

There are no session letters. A room file shared by four actors simply appears on four cards.

### Layout, top to bottom
1. **Sub-tabs:** `Files | Sounds`. The sound bucket lives under Import (see 3).
2. **Two drop zones:**
   - **Script:** a `.fountain` file (IMP-01).
   - **Audio:** WAV or MP3 files or a whole folder (IMP-10).

   Once filled, each collapses to one line with *Replace* or *Add files*. The audio line shows the split: recordings and director room files.
3. **Cast header:**
   - A legend for the two file types:
     - **Recording:** solid light chip, mic icon. The actor's own mic.
     - **Director room:** outlined chip, speaker icon. Talkback and notes.
   - On the right, *Matched 2 min ago* and **Redo match**.
4. **Needs you:** only what failed or is unsure (IMP-19). Each row has its own one-click fix:
   - a file whose type is unclear: *Recording* or *Director room*?
   - a recording with no character: *Assign* or *Suggest*
   - a director room file not linked to any recording: *Link to*
   - a character in the script with no recording: *Pick file*
5. **Character cards:** one per character, in its colour tag. Inside, each recording is paired with its director room file:
   - **Recording row:** file name, a waveform in the character colour, length.
   - **Director room row:** indented under it with a connector line. File name, sync offset, and whether it is shared ("Ensemble · shared with 3 others") or "Only Paul".
6. **Footer:**
   - *Remember my fixes for the whole show* (IMP-20, stored as aliases).
   - **Start analysis** (IMP-30).

### Usage
- Drag the script and the audio folder in. Press *Redo match* if you want a fresh pass.
- Work through **Needs you** until it is empty. The cards are for checking; most of the time nothing there needs touching.
- The *Recording* and *Director room* labels are buttons. Click one to switch a file's type if detection got it wrong.
- *Suggest* asks Claude for a character based on the transcript. It only runs when pressed.
- *Start analysis* queues transcription, alignment and notes scene by scene (IMP-30, IMP-31).

### Not shown yet
- Script version diff (IMP-05, IMP-06). Round 1 Import C has a version of it. It should open as a sheet over this screen when a new script is dropped.
- Analysis progress per scene (IMP-31). Round 1 Import B shows one approach.

---

## 3. Import › Sounds (the full bucket)

### Why
Sound sourcing is a different job from editing. It needs room to browse, compare and decide, so the full bucket is a page under Import. The editor only carries a small, scene-filtered version (see 4).

The key distinction is **preview vs downloaded**. Search results from Epidemic Sound are ideas until you choose one, so a download happens only when you decide to use a sound.

### Layout
- **Toolbar:**
  - Sub-tabs `Files | Sounds`.
  - Sort buckets by **Scene**, **Tag** or **Show** (SND-11).
  - **Spot script:** an explicit AI run that proposes buckets from the script (SND-20).
  - **New bucket.**
- **Bucket grid.** Each card shows:
  - name and scene chips,
  - keyword chips (these *are* the search query),
  - a strip of up to four sounds: **gold, solid** = downloaded; **grey, outlined** = preview.
- **New bucket card:**
  - name and keywords,
  - an *Advanced* section: length range, what to leave out (e.g. music, voices), and which scenes it belongs to.
  - *Create and search* turns the keywords into a provider search and fills the bucket with previews.
- **Side panel**, opened by clicking a bucket:
  - Provider tabs: **Epidemic Sound**, **ElevenLabs** (generate), **Library**.
  - The query as editable keyword chips, plus *Search*.
  - **In this bucket:** downloaded sounds. Gold play button and waveform, a check mark, *Remove*.
  - **Previews, not downloaded:** grey waveform you can click into to audition from any point, *Download*, *Dismiss*.
  - **Load 12 more** for further results.

### Usage
- Create or spot buckets. Refine keywords in the bucket or in the panel until the previews are close.
- Audition previews: click the waveform to jump in. Space plays, Left/Right steps through results (SND-51).
- *Download* only what you want to keep. Downloaded sounds go into `Show/Library/SFX/<category>/` with their license info (SND-33).
- *ElevenLabs* tab: write or adjust a prompt and generate variations. The prompt is stored with each kept file (SND-31).
- **Back to Edit** (top right, `⇧B`) returns to the editor at the same playhead.

### To check
- The preview and download model assumes Epidemic Sound's API serves streamable previews and counts a download as a use. Confirm against their current API terms before building.

---

## 4. Edit page

### Why
Edit A from Round 1 was the clearest: one scene at a time, all lane groups visible, the scene's bucket on the side. Round 2 keeps that and adds two things:
1. **Logic-style controls,** so the editor feels familiar to anyone who cuts audio.
2. A way to **give the timeline the full screen** when the bucket is not needed.

### Shared layout (both states)
- **Control bar,** modelled on Logic:
  - Left: Inspector toggle (`I`), Mixer toggle (`X`).
  - Tools: Pointer, Split, Fade, Gain, Marquee.
  - Centre: go to start, Play (`Space`), Cycle (`C`), and a time display showing scene, lane and clip.
  - Right: *Snap: words* (EDT-33), *Ripple* (EDT-20), zoom, **Bucket** toggle (`B`).
- **Cycle range** in the ruler, as in Logic.
- **Script strip** under the ruler (EDT-14):
  - dialogue in the character colour, the line at the playhead highlighted,
  - action lines in grey italics below.
- **Lane groups**, top to bottom (EDT-11):
  1. Dialogue, one lane per character, with mute and solo.
  2. Sound effects. Extra layers are added when sounds overlap (EDT-12).
  3. Ambience: room tone (looping, with visible loop seams) and ambience (EDT-13).
  4. Music.
- **Anchors:** a dashed gold line runs from each effect to its script line or dialogue word (EDT-40 to EDT-42). Anchored clips move with the dialogue.
- **Scene boundary** marker. Room tone and ambience switch with the scene's location.

### Edit · bucket docked
- The right panel shows **only buckets relevant to this scene** (Food and foil, Walla and crowd, Props, Machines).
- A legend at the top: **Downloaded** (gold) and **Preview, downloads on drop** (grey).
- Sounds have a drag handle. Dragging a **preview** onto a lane:
  - places it, snapped to its anchor (SND-43),
  - shows it as a grey clip with a gold progress bar while it downloads ("Downloading 64% · snaps to 'Paul takes a bite.'").

  When the download finishes, the clip turns gold.
- Each bucket has *More like these* for fresh previews without leaving the editor.
- Panel header:
  - **Full bucket** (`⇧B`) jumps to Import › Sounds.
  - **Hide** (`B`) collapses the panel.

### Edit · full screen
- The bucket is hidden. A slim **Bucket** tab on the right edge brings it back (`B`).
- The **Inspector** (left, `I`) shows the selection, Logic-style:
  - **Clip:** name, source, gain, fade in, fade out, curve (EDT-31, EDT-32).
  - **Anchor:** what it is anchored to, offset, *Follows dialogue*, *Pin to time* (EDT-43).
  - **Lane:** volume and pan, plus a small meter.
- The selected clip shows fade handles and a gain line directly on the clip.

### Usage
- `Space` play/stop. `⇧Space` play from the locator.
- `B` show or hide the scene bucket. `⇧B` switch between the editor and the full bucket.
- `I` inspector, `X` mixer, `C` cycle.
- `⌥←/→` nudge 10 ms, `⇧⌥←/→` 100 ms (EDT-30). Option-drag moves one clip alone (EDT-21).
- Drag from the bucket to place a sound. Drop files from Finder straight onto a lane (SND-42).

---

## 5. Moving between bucket and editor

| From | Action | To |
|---|---|---|
| Edit, bucket docked | `B` or Hide | Edit, full screen |
| Edit, full screen | `B` or right-edge tab | Edit, bucket docked |
| Edit, either state | `⇧B` or *Full bucket* | Import › Sounds |
| Import › Sounds | `⇧B` or *Back to Edit* | Edit, same playhead and scene |

The scene in the editor sets the bucket filter. The full bucket opens with that scene's buckets highlighted.

---

## 6. Open questions

1. **Room tone settings.** Round 1 Edit B (popover on the scene marker) and Edit C (per-location inspector) show two options. Round 2 needs one home for them, probably the inspector when a room tone clip is selected.
2. **Mixer (`X`).** The button exists, but the mixer view is not designed yet.
3. **Episode view.** Round 2 is per scene. Round 1 Edit C (scene bands, room tone by location) could become the zoomed-out state.
4. **Preview licensing.** Confirm how Epidemic Sound previews and downloads are counted and cached.
