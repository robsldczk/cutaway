# Audioplay Cutter: Frontend Design

## Principles

- **The script is the spine.** The timeline is vertical and reads like a script.
- **Dialogue first.** The spoken line is the loudest thing on screen. Everything else is quieter, smaller or tucked away.
- **Calm by default.** Show only what is needed to decide. Details appear on demand.
- **Keyboard first.** Shortcuts follow Resolve conventions where they exist.
- **Two levels of work.** The main timeline handles boxes. The box editor (overlay) handles everything inside a box.
- **Context audition.** Every choice is judged with pre-roll from the surrounding lines.

## Learnings from the Stage 0 test page

The mechanic works: script-matched takes, flipping through them, and playing the scene with your picks. The test page was too cluttered:
- Every take showed timecode, duration, match score, tags, take count, direction, feedback and full notes at once.
- Notes were transcript fragments, not instructions.
- Direction and feedback sat inside the dialogue box and competed with the line.
- Action lines were grey, but as large as the dialogue.
- Play and Stop were not clearly one control. Playback state was hard to read.

Everything below follows from that.

## Visual direction

- **Feel:** a polished iPhone app sitting in the middle of the screen, focused and quiet.
- **Boxes:** large corner radius, soft depth, generous padding, no borders.
- **Dark mode first.** Each character gets its own color from a traditional Japanese palette, muted and harmonious. For example:
  - ai (indigo),
  - asagi (pale blue-green),
  - kaki (persimmon),
  - matcha,
  - fuji (wisteria),
  - kurenai (crimson),
  - yamabuki (golden yellow).

  Colors are matched in lightness so no character dominates. A light theme comes second.
- **Typography:** script conventions.
  - Character name in caps above the line.
  - Dialogue in a screenplay mono (Courier Prime or similar), large.
  - Parentheticals smaller and italic.
- **Column:** narrow, centered, like a script page. Nothing else competes in the middle.

## Hierarchy

| Level | Element | Treatment |
|---|---|---|
| 1 | Dialogue line (what was actually said) | Largest, full contrast, inside the character box |
| 2 | Character name, take position | Small caps, in the box header |
| 3 | Direction and feedback | Short note beside the box, small, muted |
| 4 | Action lines (spacers) | Small, low contrast, between boxes, one line, expandable |
| 5 | Timecode, match score, file, full room transcript | Hidden; shown in an inspector on demand |

## Dialogue box

**Contains only:**
- the character name,
- the spoken line (from the selected take; words light up during playback),
- the take position, shown as small dots (filled for current, accent for a director pick).

**Interaction:**
- Swipe or Left/Right to flip takes. The box animates sideways like a card carousel.
- The neighbouring takes peek at the edges.

**State markers:**
- **Pick:** a small star in the corner.
- **Partial take:** a subtle dashed edge.
- **No takes found:** the box is dimmed and shows the script text instead.
- **Low confidence:** a small dot that leads to the inspector.

## Notes beside the box

- **Position:** the side of the box (right on desktop, below on narrow screens). Never inside the box.
- **Content:** LLM-condensed, a few words each.
  - **Direction:** what was asked before the take, e.g. "quieter, to yourself".
  - **Feedback:** the reaction after it, e.g. "liked it", or "needs more oomph on 'science'".
- **Updates:** the notes change with the take.
- **Full room transcript:** only in the inspector.

## Action lines

- **Look:** small, low-contrast script text between boxes, truncated to one line.
- **Expand:** click to show the full action.
- **Spacer:** each action line also acts as a spacer; its height reflects its pause length.

## Transport

**One clear Play/Stop control:**
- one large button, bottom center like a music app,
- the same button toggles between play and stop,
- Space does the same.

**Playback states:**
- **Playing:** the current box glows in its character color, the current word lights up, and the column auto-scrolls to keep it centered.
- **Stopped:** the playhead stays where it stopped.

**Modes:**
- play scene from the selected line (default),
- play line in context (Enter).

**Other controls:** gap/pacing and the room-tone toggle sit in a compact settings sheet, not in the main bar.

## Inspector (on demand)

Opened with I or a long press on a box. It shows everything hidden from the main view:
- source timecode in/out, duration, match score,
- the full room transcript before and after the take,
- tags,
- the source file.

## Layout

| Area | Content |
|---|---|
| Left (collapsible) | Scene list: heading, location, status, flagged count |
| Center | Script column with dialogue boxes, notes beside them, action lines between |
| Right (on demand) | Inspector |
| Bottom | Transport (Play/Stop), page switch (Options / Arrange / Edit), settings sheet |

## Views

### One timeline, three pages

Like the Cut and Edit pages in DaVinci Resolve, every view is a different display of the same timeline. There is one source of truth: the episode's timeline in the core document. A change in any view updates all the others immediately.

| Page | Purpose | Freedom |
|---|---|---|
| Options | Build the dialogue fast: pick takes per line | Guided: one lane, ripple, script order |
| Arrange | Reorder lines and spacers | Guided: one lane, ripple |
| Edit | Full multitrack editor: dialogue, sound effects, ambience, music | Free: any clip, any lane, any time |

**Rules that keep the pages in sync:**
- **One timing model.** Each dialogue clip stores its take, trims and its gap to the previous clip. Options and Arrange edit these values through simple gestures; Edit edits the same values directly.
- **Free edits show up in the simple pages.** Moving a dialogue clip in Edit changes its gap, and the Options page shows the new spacing, with the override dot. Trimming in Edit changes the box length.
- **Edits the simple pages can't express are marked, never lost.** Examples: overlapping dialogue, a line split into two clips, a clip pinned to absolute time. The box shows a small "edited in Edit" marker, and clicking it jumps to the clip there. The simple pages still let you swap the take; the free edits are kept wherever they still fit the new take.
- **Ripple is relative.** Take swaps and gap changes in Options ripple everything after them, including sound effects anchored to dialogue. Clips pinned to absolute time in Edit stay put.
- **Shared playhead and selection.** Switching pages keeps the playhead and the selected line.
- **One undo history** across all pages.

**Options view (default)**
- Pick the take per line while the rest of the scene stays in place.
- **Loudness matched:** takes play back at equal loudness.
- **Passage mode:** select several boxes and apply one take to all of them.
- **Edge trim:** drag the top or bottom edge of a box. It snaps to words, and the following boxes ripple.
- **Unmatched bin:** a drawer per scene. Drag an item into the timeline to use it.

**Arrange view**
- Drag boxes and spacers to reorder them; ripple is the default.
- The script panel marks lines moved out of script order.
- **Overlap (V2):** Option-drag overlaps two boxes, shown as blended character colors.

**Edit view (full editor)**

A horizontal multitrack timeline for one scene or the whole episode. It is the most open way to work. It's mostly used for sound, but dialogue is fully editable here too.

Lanes are grouped, top to bottom, and each group is visually separated by color and spacing:

| Group | Lanes | Behaviour |
|---|---|---|
| Dialogue | One lane per character | Fully editable: move, trim, split, overlap, swap take (right-click or Left/Right on a selected clip) |
| Sound effects | As many layers as needed; layers are added automatically when sounds overlap | Free placement |
| Ambience | Room tone and atmosphere per location | Auto-filled from the location, adjustable |
| Music | Music cues | Free placement |

**Placing and moving:**
- **From the bucket:** drag a sound into an SFX lane. It snaps to the script anchor it belongs to (the action line or word), then nudge freely.
- **Nudge:** Option+Left/Right moves 10 ms, Shift+Option moves 100 ms. Clips snap to dialogue word boundaries when close.
- **Edges and fades:** trim the clip edges, drag the fade handles, drag the gain line per clip.
- **Anchors:** a thin line connects each sound to its anchor in the dialogue. If the dialogue moves, the sound moves with it.
- **Overlaps:** two sounds that overlap in one lane split onto separate layers automatically, so nothing hides another.
- **Dialogue moves:** dragging a dialogue clip changes its gap. With ripple on (default), everything after it follows; hold Option to move it alone, which lets it overlap its neighbours.

**Script strip:** the script runs along the top of the timeline, so you see which action line you are scoring.

## Sound bucket (panel)

A side panel available in the Edit view, and from the scene list.

**Tabs:**
- **This scene:** the wish list of cues, in script order.
- **Show library:** all sounds, searchable, filterable by tag and category.

**Each cue row shows:**
- the description,
- its status (wanted, candidates, chosen, placed),
- the anchor line,
- the candidate sounds, each with a play button and a waveform thumbnail.

**Actions per cue:**
- **Search:** finds sounds in the library and the connected services.
- **Generate:** creates variations through a generation provider.
- **Choose:** picks one candidate.
- **Drag:** drag a candidate onto the timeline to place it.

**Other features:**
- **Drag in from Finder:** drop files on the bucket to import them into the show library (sorted into folders and tagged), or drop them on a cue to attach them as candidates.
- **Auditioning:** click to play; Space plays the selected sound; Left/Right steps through candidates. Sounds play loudness-matched, like takes.
- **Manual editing:** add, rename, retag, merge and delete cues. Duplicates across scenes can be merged into one show-level cue.
- **Spotting:** "Spot this scene" runs the AI suggestion pass. New cues are marked so you can accept or dismiss them.

## Box editor (overlay)

**Opening:** Enter or double-click on a box (or a selected passage). The timeline dims; you work on that box only.

**Comp mode**
- **Comp lane (top):** the current result, color-striped by source take.
- **Take lanes (below):** one per take of the passage, each with its short note.
- **Text-aligned:** the same script word sits in the same column across all lanes.
- **Picking:** swipe words to promote them into the comp, or drag a chunk up into the comp lane.
- **Splices:** a warning marker where intonation is likely to jump.

**Edit mode**
- **Delete words:** select text and delete. Deleted words stay visible, struck through. Click to restore them.
- **Close or keep pause:** a per-cut toggle.
- **Pause trim:** drag the silence between words to shorten it.
- **Head and tail:** drag the start and end of the line.

**Closing:** Esc closes the overlay; the box updates and the timeline ripples.

## Room tone

- **Per location:** set once in the scene list or settings.
- **In the timeline:** shown as a faint texture behind the scene column, not as a separate lane.
- **Toggle:** on/off in the settings sheet.

## Pacing controls (settings sheet)

- **Global breath gap:** default 300 ms.
- **Per-scene tightness slider:** scales all gaps and spacers of the scene at once.
- **Local overrides:** individual gaps and spacers, marked, and kept when the slider moves.
- **Cue-reading template (V2):** use the director's cue-reading rhythm as the timing template.

## Keyboard map (MVP)

| Key | Action |
|---|---|
| Up / Down | Previous / next line |
| Left / Right | Previous / next take |
| Space | Play / stop |
| Enter | Play line in context |
| E | Open box editor |
| Esc | Close box editor / inspector |
| I | Inspector |
| P | Apply take to selected passage |
| Cmd+Z | Undo |
| 1 / 2 / 3 | Options / Arrange / Edit |
| Option+Left/Right | Nudge selected clip 10 ms (Edit view) |
| B | Toggle sound bucket |

## Processing and review

- **Ingest:** progress shown per pipeline stage.
- **Summary card after processing:**
  - lines matched,
  - takes found,
  - picks found,
  - flagged items.
- **Flagged items:** listed in the scene sidebar, not in the column.

## Export panel

- **Scope:** current scene, selected scenes, or everything.
- **Formats:** FCPXML, WAV, MP3, SRT, as-cut transcript (cut list).
- **Settings:** handles, naming, loudness normalization, room tone track on or off.

## Out of scope for MVP

- Overlap mode
- Waveform-precise editing and modifier-key cut modes
- Edit view, sound bucket and music lanes (planned for build stage 5)
- Volume automation beyond clip gain, effects
- AAF export
