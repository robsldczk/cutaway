# Cutaway: Frontend Design

> **Design source.** The visual design follows the clickable prototype **Option C, Arrange, Box editor** in `docs/design/prototype/` (open `Prototype.dc.html` through a local server). Where this spec and the prototype disagree on looks, the prototype wins; where the prototype is silent (Edit page, inspector, importer, bucket, settings, history), these screens reuse its components and tokens. Changes against the first version of this spec are listed at the end under **Changed for the Option C design**.

## Principles

- **The script is the spine.** The timeline is vertical and reads like a script.
- **Dialogue first.** The spoken line is the loudest thing on screen. Everything else is quieter, smaller or tucked away.
- **Calm by default.** Show only what is needed to decide. Details appear on demand.
- **Keyboard first.** Shortcuts follow Resolve conventions where they exist.
- **Two levels of work.** The main timeline handles boxes. The box editor (overlay) handles everything inside a box.
- **Context audition.** Every choice is judged with pre-roll from the surrounding lines.
- **Re-skinnable.** Components never hard-code a color, radius, font or shadow. They read design tokens (see **Design tokens**).

## Learnings from the Stage 0 test page

The mechanic works: script-matched takes, flipping through them, and playing the scene with your picks. The test page was too cluttered:
- Every take showed timecode, duration, match score, tags, take count, direction, feedback and full notes at once.
- Notes were transcript fragments, not instructions.
- Direction and feedback sat inside the dialogue box and competed with the line.
- Action lines were grey, but as large as the dialogue.
- Play and Stop were not clearly one control. Playback state was hard to read.

Everything below follows from that.

## Visual direction

- **Feel:** frosted glass on a deep night-blue stage. A polished iPhone app sitting in the middle of the screen, focused and quiet.
- **Stage:** a near-black background (`#0B0D12`) lit by three soft radial glows (indigo centre, persimmon lower left, asagi upper right). A faint ruled-paper texture (1 px lines every 7 px) runs down the centre column like a script page, fading out at the sides. A thin horizontal hairline marks the vertical centre, where the current line sits.
- **Glass:** panels use `backdrop-filter: blur(22px) saturate(140%)`, a 1 px inner top highlight, a 1 px inner outline and a deep drop shadow. No hard borders.
- **Boxes:** large radius (32 px for the current box, 24 px for the others), generous padding, colored by their character with a diagonal gradient that fades into the stage.
- **Dark mode first.** Each character gets its own color from a traditional Japanese palette, muted and harmonious:
  - kaki (persimmon) `#E08A5A`, asagi (pale blue-green) `#5EB8B0`, matcha `#9DB46A`, fuji (wisteria) `#A08FD0`,
  - then ai (indigo), kurenai (crimson), yamabuki (golden yellow) for further roles.

  Each color has a lighter *name* tint for the character label (e.g. persimmon `#EDAA82`). Colors are matched in lightness so no character dominates. A light theme comes second; the token set is built for it.
- **Take shades.** Takes of the same line are told apart by shades of the character color (lighter and darker steps: 0, +34 %, −32 %, +58 %, −52 %). Take pills, deck cards, comp segments and waveform fills all use the take's shade.
- **Typography:**
  - UI: **Figtree** (400–700), small sizes, tabular numerals for times.
  - Script: **Courier Prime** (screenplay mono). Dialogue 24 px in the current box, 18 px in the others; parentheticals italic, 14/13 px; action lines 13 px.
  - Character name: Figtree 600, 11–12 px, letter-spacing 0.14 em, upper case, in the character's name tint.
- **Column:** narrow and centred (current box `min(640px, 88%)`, Arrange `min(600px, 86%)`), scaled by the **Size** control (0.8–1.3).
- **Motion:** take flips slide 26 px sideways (0.45 s, `cubic-bezier(.2,.8,.2,1)`); the column scrolls so the current line sits on the centre hairline (0.34 s). Respect `prefers-reduced-motion`.

## Design tokens

All visual values live in `app/src/theme/`, separate from the components:

| File | Holds |
|---|---|
| `tokens.css` | CSS custom properties: stage colors and glows, text levels (`--text-1` … `--text-4`), glass fill/blur/shadow, radii, spacing, font families and sizes, accent colors (locator `#9FD6FF`, warning `#E6B84C`, playhead `#F3F1EC`), motion durations and curves |
| `palette.ts` | Character palette (RGB + name tint), take-shade steps, mixing base, helpers (`shade`, `rgba`, `mix`) |
| `index.ts` | The active theme: re-exports the palette and the theme name |

Rules:
- Components use `var(--token)` in CSS and the palette helpers in script. No literal colors in components.
- A new skin is a new folder (`app/src/theme/<skin>/`) with the same token names; switching skins changes one import (or the `data-theme` attribute for CSS-only skins).
- Light theme: `[data-theme="light"]` overrides the same tokens.

## Hierarchy

| Level | Element | Treatment |
|---|---|---|
| 1 | Dialogue line (what was actually said) | Largest, full contrast, inside the character box |
| 2 | Character name, take position | Small caps in the box header; take pills below the current box |
| 3 | Direction and feedback | Short notes beside the current box, small, muted |
| 4 | Action lines (spacers) | Small glass pills between boxes, one line, low contrast |
| 5 | Timecode, match score, file, full room transcript | Hidden; shown in the inspector on demand |

*Changed in implementation (D72):* boxes, strips, word lanes and the playing-word highlight show the **script's** words, not the recognizer's. Matched words keep their timing; words it missed or misheard are spread over the time they were spoken in, so the highlight runs on cleanly. Repeats and ad-libs ride on the word before them. The Inspector and the Dev page still show what was heard.

## Screen layout (all pages)

| Area | Content |
|---|---|
| Header (top, full width) | Left: episode name (600) and the current scene ("Sc. 2 · Canteen · Later", muted). Centre: the **page switch**, a glass pill with Options `1`, Arrange `2`, Edit `3`. Right: **Size** slider, **?** (keyboard help), **⚙** (settings sheet), **Export**. |
| Left rail | A floating glass rail with one 40 px button per scene (number). Current scene highlighted; an amber dot marks flagged items. Hover shows the heading; the rail expands into the scene list (heading, location, status, flagged count) on click of the expand control. Hidden below 1000 px. |
| Centre | The page (Options column, Arrange list, Edit timeline). |
| Right (on demand) | Inspector, unmatched bin, sound bucket: glass drawers that slide in. |
| Bottom | The **transport** (see below) and a one-line key hint under it. |

## Options page (default)

Pick the take per line while the rest of the scene stays in place.

**The column**
- All lines of the scene in script order. The current line sits on the centre hairline; the column scrolls to keep it there.
- Other lines are smaller glass boxes (24 px radius, 18 px dialogue) colored by their take's shade, fading with distance from the current line (−12 % opacity per step, minimum 35 %). Their header shows the name and `★ 2/3` (favorite, take position). Click selects; double-click opens the box editor.
- The edges of a box with several takes show one or two stacked slivers.
- The column fades out at the top and bottom (mask).
- Scrolling the wheel steps line by line. Shift + wheel flies through the lines (one per 30 px of scroll, no rate limit).

**The current box**
- A deck: up to three cards of the next takes sit behind the box, offset 16 px to the right each and slightly shorter, showing their take number.
- Header: character name, a ★ if the take is a favorite, and `2 / 5` on the right.
- Parenthetical (italic) above the dialogue, then the dialogue of the selected take at 24 px. During playback, spoken words are full white, the current word scales up (×1.22) and glows in the take's shade, upcoming words are dimmed. Words trimmed away are struck through.
- The box glows in its take's color; the glow grows while playing.
- Flipping takes slides the box in from the side.
- **Take pills** below the box: one pill per take (`1`, `2 ★`, …), the selected one filled with its shade, the others glass with a shade underline. Partial takes get a dashed outline.
- **Notes aside** (right of the box, 220 px): Direction and Feedback, each a small upper-case label and a 14 px note; below them a muted meta line (`3.4 s · match 94% · partial`). Hidden below 1100 px (then shown under the box).
  - *Changed in implementation (D73):* Direction and Feedback became one **room strip** ("In the room"). It lists the line's takes and one take either side in recording order, with the room talk between them as sentences. The current take sits on the centre line, the talk on both sides of it is at full strength, and further steps fade. Changing takes slides the strip. The raw transcript shows only with the developer switch on.
- **Time chip** (left of the box): the scene time of the playhead.

**Gaps and action lines**
- Between boxes, the pause is shown as space proportional to its length.
- An action line shows as a small glass pill with one line of script text (13 px, 50 % text); click expands it.

**Other Options features**
- **Loudness matched:** takes play back at equal loudness.
- **Passage mode:** Shift+Up/Down extends the selection over several boxes; P applies the current take number to all of them.
- **Edge trim:** handled in Arrange (waveform strip) and the box editor; Options shows the result (struck-through words).
- **Unmatched bin:** a drawer per scene (U). Drag an item into the column to use it as a box.
- **State markers:** favorite ★; partial take: dashed edge; no takes found: dimmed box showing the script text; low confidence: a small dot that opens the inspector; edited in Edit: a small marker that jumps to the clip on the Edit page.

## Arrange page

Reorder lines and spacers, set pauses, trim heads and tails.

- **Line rows** (92 px, 18 px radius): a grip `⠿` to drag, the character name (76 px column), the line text (one line, ellipsis), badges and the duration (`2.8 s`).
  - **Moved** badge (outlined) when a line is out of script order.
  - **Cuts a word** badge (amber `#E6B84C`) when a head or tail trim lands inside a word.
- **Waveform strip** under the text: the take's waveform in its shade. Trimmed head and tail are darkened. Two handles trim the head and tail (amber when they cut a word). Click sets the **locator** (blue flag `#9FD6FF`); I and O set in and out at the locator. The playhead is a white glowing line.
  - *Changed in implementation (D63):* the strip also shows a handle of source audio on each side of the take (darkened, like a trim). Dragging a handle out into it lengthens the clip; a dashed mark shows where the take was cut. The handle grows with each extension, up to the ends of the file. Comps keep the take-only strip.
- **Gap rows** between lines: a hairline and a centred pill showing the pause (`‖ 0.30 s`) and the action line that sits there. Drag the row up or down to change the pause in 50 ms steps; dragging below zero overlaps the two lines (`overlap 0.20 s`, shown with blended colors).
  - *Changed in implementation (D125):* a line dragged by its grip onto the right half of another runs beside it: two cards in one row, heard together, the second offset with its pill.
- **Reorder:** drag the grip; a glowing insertion line shows the drop point; ripple is the default. Alt+Up/Down moves the current line.
- The current row has a brighter gradient and a 1.5 px outline in its shade, and sits on the centre hairline.
- *Changed in implementation (D78):* the **sounds layer** sits beside the dialogue, shown or hidden with **Sounds** or `S`. The column moves left and narrows; beside it runs the spine (the dialogue as a vertical waveform in character colours), a hairline per sound, and the sounds as balls (size = length, x = depth, y = start). The Edit page's bucket floats on the right, with an eye per bucket. Scrolling moves the playhead continuously, with the column on the centre line. Each gesture is one undo step, a selection moves as one, and a ball is the same clip the Edit page shows. Spec: `tasks/Bokeh · live prototype-html/audioplay-cutter-handover-arrange-sounds.md`.
- *Changed in implementation (D128–D130):* a sound being moved shows on the blocks with a marker of its own, in its bucket's colour, and across the column in a pause. ⌘ and ⇧ click select several lines. A line's right click holds how far back in the room it stands and Copy and Paste attributes; a line moved back wears a chip and a fainter waveform.

## Edit page (full editor)

A horizontal multitrack timeline for one scene or the whole episode, built from the same parts as the box editor (lane labels 150 px, glass lane areas, waveforms in take shades, the same ruler, locator and playhead).

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
  - *Changed in implementation (D123):* every sound dropped or moved on Arrange hangs on the exact moment it was put, snapped or not: Snap decides where it lands, glue whether it follows. Glue in the opened sound turns it off for one sound.
- **Overlaps:** two sounds that overlap in one lane split onto separate layers automatically, so nothing hides another.
- **Dialogue moves:** dragging a dialogue clip changes its gap. With ripple on (default), everything after it follows; hold Option to move it alone, which lets it overlap its neighbours.

**Script strip:** the script runs along the top of the timeline, so you see which action line you are scoring.

*(Changed in implementation, M11 / D54–D59: the Edit page works as a minimal NLE. Fixed time scale; a playhead you drag (silent scrub) in a ruler with a cycle strip on top; a word row under the sentences that stays when groups collapse; collapsed groups preview their clips. Clips draw their own audio with word markers, have trim handles on both edges and fade handles on the corners. Dialogue splits into parts (⌘T); clips can be muted (⌃M, kept in place) or ripple-deleted (⌫ with Ripple on, restorable). A Lock switch and a lock per clip: unlocked dialogue moves freely in time and leaves the ripple chain. ←/→ select clips here, , and . step through words; the full key list is in the help overlay.)*

## Transport

A floating glass bar at the bottom centre (`min(820px, 94%)`, 30 px radius):
- **Play/Stop:** one 52 px round light button on the left; the same button toggles; Space does the same. A soft ring in the current take's shade shows that it is playing.
- **Times:** elapsed scene time, a middle hint (`Paul · take 2 of 3 · ★` on Options), total scene time.
- **Scene map:** a 24 px bar with one segment per line (in the line's take shade; past lines brighter than future ones) and per pause. Click or drag moves the playhead. ⌘ scroll zooms (up to ×16), ⇧ scroll moves the zoomed view; while playing, the view follows the playhead. The locator shows as a blue flag.
- **Buttons on the right:** Favorite (F), Box editor (E), Play from locator (⇧ Space).

**Playback states:**
- **Playing:** the current box glows in its take's color, the current word lights up, and the column auto-scrolls to keep it centred.
  - *Changed in implementation (D70):* **Follow** (header pill and Settings, on by default, remembered per viewer). On, the current box moves with the playhead on Options and Arrange, and the Edit page pages along and highlights the line in its scene strip; stopping leaves the box where playback stopped, so Space carries on. Off, the box stays where you put it and Space plays from it again. Follow never changes the Edit page's clip selection.
- **Stopped:** the playhead stays where it stopped.
  - *Changed in implementation (D118):* adlibs aren't sounds any more. No vocal buckets, no Adlibs lane, no Characters mode on the docked bucket: an adlib is a take or an added line, made in the mapping editor.
  - *Changed in implementation (D102, removed by D118):* a character's adlib always goes on an Adlibs track. On Arrange its bucket is in the panel and it's a ball like any sound; on Edit the Adlibs lane sits under Dialogue.
  - *Changed in implementation (D101, the bar removed by D118):* the docked bucket's search has two modes, Sounds and Characters. Characters finds what an actor said or did ("paul huh"), pre-clipped, to trim and drag in.
  - *Changed in implementation (D100):* the balls carry no labels; the selected sound shows on a tab on the transport's top edge.
  - *Changed in implementation (D99):* a Snap button on Arrange, and ⌥ up or down moves a sound without snapping; ⌥ sideways still pans.
  - *Changed in implementation (D98):* an action row's bottom edge sets how long the direction takes.
  - *Changed in implementation (D92–D96):* Import has a Rooms tab, one room per location: its size, damping, wet and its one ambience, optionally a second for night, with the voices' wet beside it. Ambience isn't a bucket. Dropping audio from the Finder opens a sheet with the bucket guessed by name: Enter files it and puts it in the scene. The hints under the transport and the Cuts a word tag are gone.
  - *Changed in implementation (D90, D91):* each voice takes a little of its scene's room. The docked bucket has a search bar. Arrange shows action lines in full between the cards, and a sound dropped on one hangs on it.
  - *Changed in implementation (D89):* the Sounds toggle is one even move on one curve, and nothing on Arrange overshoots, a dropped sound's pop included.
  - *Changed in implementation (D88):* with sounds selected on Arrange, ⇧ Space plays from the start of the first of them.
  - *Changed in implementation (D85–D87):* a dropped sound keeps its length under 6 s and is cropped to 10 s past it; with **Loop** on (bucket rows over 6 s) it repeats on the ambience lane to the scene's end. Each row of Rooms picks a room tone. The Sounds toggle glides and fades, with no overshoot.
  - *Changed in implementation (D84):* Play in a bucket doesn't use the transport. A preview player at the foot of the bucket panel shows it, on its own clock, and the playhead stays. One plays at a time: a preview stops the timeline, and the timeline stops a preview. Space stops a playing preview.

**Modes:**
- play the scene from the selected line (Space),
- play from the locator (⇧ Space),
- play the line in context (Enter: one line of pre-roll).

**Other controls:** gap/pacing and the room-tone toggle sit in the settings sheet (⚙), not in the main bar.

*(Changed in implementation, D120, D122: the room's read-out left the transport. D81 had put it there. Its settings are on Import › Rooms. On Arrange the transport has a pan icon instead: a pop-out with each character's pan in this scene or the whole room.)*

## Box editor (overlay)

**Opening:** E, double-click a box, or the pencil button in the transport. The page dims and blurs; a large glass dialog (`min(1220px, 96%)`, 32 px radius, tinted by the take's shade) holds the box.

**Header:** character name and `Line 3 · 5 takes · 4.1 s`; the comp's text in script type (21 px). Buttons: **Words** (W, toggles the word overlay), **Reset** (R), **Done** (Esc). A Comp / Edit switch selects the mode.

**Lanes** (label column 150 px, then the lane area):
- **Ruler:** time ticks every 0.5 s; click or drag places the locator and playhead.
- **Comp lane (84 px):** the current result. Each segment shows its source take's waveform in its shade, a chip (`T3`, `T3 +20 ms` when nudged) and a 3 px underline in its shade. Seams between segments are white; a seam that cuts a word is amber and glows.
- **Take lanes (58 px each):** one per take, label `T2 ★ · picked` plus its short note, waveform in its shade (stronger when the take is used in the comp), a small bar marking the stretches used in the comp. Partial takes have a dashed outline.
- **Word overlay:** each word sits centred over its stretch of waveform (Courier Prime 13–14 px, with a dark text shadow); the word under the playhead lights up.
- **Locator:** a dashed blue line; **playhead:** a white glowing line with a small square head.

**Comp mode**
- Click a take to listen from there. Drag across a take to paint that stretch into the comp; paint edges snap to word boundaries.
- Click a comp segment to select it; drag it or use Left/Right to nudge it (Shift = coarse); Tab steps through segments.
- 1…5 use a whole take; R resets the comp to the selected take.
- **Text-aligned:** painting is stored as a range of script words, so the same word is taken from the chosen take wherever it was said.
- **Splices:** the amber seam warns where a cut lands inside a word or where intonation is likely to jump.

**Edit mode**
- **Delete words:** click or drag across words in the comp lane, then Delete. Deleted words stay visible, struck through. Click to restore them.
- **Close or keep pause:** a per-cut toggle on the seam.
- **Pause trim:** drag the silence between words to shorten it.
- **Head and tail:** drag the start and end of the line.

**Footer:** key hints (⇧ Space from locator, ← → nudge, Tab next segment, the amber seam legend) and a **Play comp / Play T2** button (Space).

**Closing:** Esc or Done; the box updates and the timeline ripples.

## Inspector (on demand)

Opened with ⌘I or a long press on a box. A right-hand glass drawer that shows everything hidden from the main view:
- source timecode in/out, duration, match score,
- the full room transcript before and after the take,
- tags, favorite origin (director pick or you),
- the source file.

## Settings sheet (⚙)

A glass sheet with:
- **Global breath gap:** default 300 ms.
- **Per-scene tightness slider:** scales all gaps and spacers of the scene at once.
  - *Changed in implementation (D62):* the breath gap goes down to −500 ms (overlap), and the tightness slider reads right = tighter (up to ×10, i.e. a tenth of the pauses); tighter also deepens overlaps.
- **Local overrides:** individual gaps and spacers are marked and kept when the slider moves.
- **Room tone:** on/off and the tone per location.
- **AI:** provider and model (privacy note: only text leaves the machine).
- **Cue-reading template (V2):** use the director's cue-reading rhythm as the timing template.

## Room tone

- **Per location:** set once in the scene list or settings.
- **In the timeline:** shown as a faint texture behind the scene column, not as a separate lane (a lane on the Edit page).
- **Toggle:** on/off in the settings sheet.

## Sound bucket (panel)

A right-hand glass drawer (B), available on every page.

**Tabs:**
- **This scene:** the wish list of cues, in script order.
- **Show library:** all sounds, searchable, filterable by tag and category.

**Each cue row shows:** the description, its status (wanted, candidates, chosen, placed) as a pill, the anchor line, and the candidate sounds, each with a play button and a waveform thumbnail.

**Actions per cue:** Search, Generate, Choose, Drag onto the timeline.

**Other features:**
- **Drag in from Finder:** drop files on the bucket to import them into the show library (sorted into folders and tagged), or drop them on a cue to attach them as candidates.
- *Changed in implementation (D76):* Import › Sounds also holds the **Rooms** section: one row per scene with size, damping and wet. It stays folded until opened, and a slider sends one command on release.
- **Auditioning:** click to play; Space plays the selected sound; Left/Right steps through candidates. Sounds play loudness-matched, like takes.
- **Manual editing:** add, rename, retag, merge and delete cues. Duplicates across scenes can be merged into one show-level cue.
- **Spotting:** "Spot this scene" runs the AI suggestion pass. New cues are marked so you can accept or dismiss them.

## Importer

A glass sheet in three steps:
1. **Script:** drop the Fountain file; the parsed scene and line counts show.
2. **Audio:** drop a folder or files. A table shows each file, its role (character chip in the character's color, or Room) and how sure the app is; click to correct.
3. **Analysis:** progress per stage (sync, transcription, alignment, notes). Then the **summary card**: lines matched, takes found, picks found, flagged items.

## Save and history

- An autosave dot in the header (saved / saving).
- **Save Version** (⌘S) asks for a name.
- **History** (⌘⇧H): the list of backups and named versions with Revert.

*Changed in implementation (D131):* the app reopens what was open at the end, and each episode comes back on its page, scene, line, playhead and Edit zoom. The start screen shows after closing on purpose. Spots are kept per machine, not in the project.

## Export panel

- **Scope:** current scene, selected scenes, or everything.
- **Formats:** FCPXML, WAV, MP3, SRT, as-cut transcript (cut list).
- **Settings:** handles, naming, loudness normalization, room tone track on or off.

*Changed in implementation (D108, D109):* Export is the fourth page (`4`; ⌘E goes there, and on it exports), one sheet with Range, Export as, Tracks, Format, Also export and Save to. It replaces this panel and the header's Export button. An exported scene locks: a quiet banner on Options, Arrange and Edit, a lock in the scene list, no trim handles, and a refused edit says why at the pointer. From the show home, Export for the show saves to the confirmed pipeline folder.

*Changed in implementation (D133, D134):* Range is a list of boxes with All, None, Invert and the scene marks above it; every box ticked is the whole episode, so the Whole episode switch is gone. ⇧ click takes a run and a press sweeps. Export as has One file.

*Changed in implementation (D127):* the sheet lies wide: three columns read left to right (Range and History; Export as and Tracks; Format, Also export and Save to), labels above their controls. It's never taller than the window: the columns scroll inside it and the footer stays in view.

*Changed in implementation (D119):* a right click on a scene marks it in progress, review or finished, and the mark is the colour of its cell.

*Changed in implementation (D116):* below about 1300 px the header and the Edit bar step down instead of wrapping: tighter, no key hints, a ⋯ menu for the secondary tools. The scene rail has no scrollbar: it tightens to fit the height, and only past that gets steppers. Under 1100 px the bucket is an overlay opened with B, which fades away during a drag.

## Views: one timeline, three pages

Like the Cut and Edit pages in DaVinci Resolve, every page is a different display of the same timeline. There is one source of truth: the episode's timeline in the core document. A change in any page updates all the others immediately.

| Page | Purpose | Freedom |
|---|---|---|
| Options | Build the dialogue fast: pick takes per line | Guided: one lane, ripple, script order |
| Arrange | Reorder lines and spacers, pauses, head/tail | Guided: one lane, ripple, overlap by negative pause |
| Edit | Full multitrack editor: dialogue, sound effects, ambience, music | Free: any clip, any lane, any time |

**Rules that keep the pages in sync:**
- **One timing model.** Each dialogue clip stores its take, trims and its gap to the previous clip. Options and Arrange edit these values through simple gestures; Edit edits the same values directly.
- **Free edits show up in the simple pages.** Moving a dialogue clip in Edit changes its gap, and the Arrange page shows the new pause. Trimming in Edit changes the box length.
- **Edits the simple pages can't express are marked, never lost.** Examples: a line split into two clips, a clip pinned to absolute time. The box shows a small "edited in Edit" marker, and clicking it jumps to the clip there. The simple pages still let you swap the take; the free edits are kept wherever they still fit the new take.
- **Ripple is relative.** Take swaps and gap changes ripple everything after them, including sound effects anchored to dialogue. Clips pinned to absolute time in Edit stay put.
- **Shared playhead, locator and selection.** Switching pages keeps them.
- **One undo history** across all pages.

## Processing and review

- **Ingest:** progress shown per pipeline stage.
- **Summary card after processing:** lines matched, takes found, picks found, flagged items.
- **Flagged items:** listed in the scene rail/list, not in the column.

*(Changed in implementation, D104: the mapping editor, on Options. M opens a character's mic on the current line's take: the waveform with the heard words on it, in and out marks, a strip of the whole file, a sentence search and a button that makes a new take. A plus on hover between two lines opens it for a new line. An × on hover deletes a line or direction, after asking; ⌘Z brings it back. Inside it: I / O mark at the locator, Space plays the marks, ⇧ Space from the locator, Enter creates, / searches, Esc closes.)*

*(Changed in implementation, D105: cards read the take's own words, as said, not the script's (this replaces D72). The mapping editor has a text field to correct them, with the script beside it, and saves to the current take (⌘⏎), to a new take (⏎), or deletes. The × and Delete… ask: delete take, or delete line.)*

## Keyboard map

| Key | Action |
|---|---|
| Up / Down, scroll | Previous / next line (⇧ scroll: fast) |
| Shift+Up / Shift+Down | Extend the passage selection |
| Left / Right | Previous / next take |
| Space | Play / stop |
| Shift+Space | Play from locator |
| Enter | Play line in context |
| F | Favorite take |
| E | Open box editor |
| M | Open the mapping editor (Options) |
| Esc | Close box editor / inspector / drawers |
| I / O | In / out at the locator (Options, Arrange) |
| ⌘I | Inspector |
| P | Apply take to selected passage |
| U | Unmatched bin |
| B | Sound bucket |
| , / . | Shorter / longer pause after the current line (100 ms steps) |
| Alt+Up / Alt+Down | Move line (Arrange) |
| ⌘ click, ⇧ click | Select more lines (Arrange, D130) |
| Option+Left/Right | The line further back or forward in the room, with no sounds selected (Arrange, D129) |
| ⌥⌘C / ⌥⌘V | Copy / paste a line's attributes (Arrange, D130) |
| − / + | Box size |
| 1 / 2 / 3 | Options / Arrange / Edit |
| ⌘Z / ⌘⇧Z | Undo / redo |
| ⌘S | Save version |
| Option+Left/Right | Nudge selected clip 10 ms (Edit page; Shift+Option 100 ms) |
| ⌥ scroll (or ⌘ scroll) | Zoom the timeline around the mouse (Edit page) |
| ? | Keyboard help |
| ⌘ scroll / ⇧ scroll | Zoom / move the scene map |
| S | Show or hide the sounds (Arrange) |
| Scroll | Scrub, with the sounds on (Arrange) |
| Tab, ⇧ Tab | Reach the balls, one after the other in time (Arrange) |
| Enter, double click | Open the sound (Arrange) |
| ⇧ Enter, ⌘A | Add the ball in focus to the selection; select all (Arrange) |
| Up / Down | On a part of the opened sound: trim, fade or move 100 ms; ⌥ 10 ms, on the body ⌥ slips (Arrange) |
| ⇧ click, drag on empty space | Select more sounds (Arrange) |
| Left / Right | Depth, with sounds selected (Arrange) |
| Option+Left/Right | Pan, with sounds selected (Arrange) |
| Shift+Up / Shift+Down | Loudness, with sounds selected (Arrange) |
| , / . | Move 100 ms, with sounds selected; ⇧ 10 ms (Arrange) |
| ⇧ scroll over a ball | Loudness (Arrange) |
| ⌫ / Esc | Remove / clear the selected sounds; Esc closes an opened sound first (Arrange) |
| Space / I / O / Enter | In an opened sound: play it alone from the scrubber / trim its in or out to the scrubber / close it (Arrange, D124) |
| ⌘D | Duplicate the selected sounds, right after themselves (Arrange, Edit) |

**In the box editor:** Space play/stop, ⇧ Space from locator, ← → nudge segment (or move the playhead when no segment is selected; Shift coarse), Tab next segment, W words overlay, 1…5 use whole take, R reset comp, Delete delete selected words (Edit mode), Esc close.

## Out of scope for MVP

- Waveform-precise editing and modifier-key cut modes *(Changed in implementation, M11: trim handles, waveform zoom to 10 ms and Logic key commands are in; sample-level editing is not)*
- Volume automation beyond clip gain, effects
- AAF export
- Light theme (tokens are prepared)

## Changed for the Option C design

Recorded in `docs/DECISIONS.md` (D11–D16). In short:
- **Look:** frosted glass on a night-blue stage, Figtree + Courier Prime, take shades of the character color (was: generic "soft depth, no borders").
- **Take position:** numbered take pills under the current box and a deck of take cards behind it (was: dots).
- **Favorites:** ★ marks a favorite take (F). Director picks found by the analysis become favorites with origin "director" (was: a separate pick star).
- **Page switch:** in the header with keys 1/2/3 (was: bottom bar).
- **Transport:** adds the scene map with zoom, the locator and play-from-locator, and favorite/editor buttons.
- **Scene list:** a compact numbered rail that expands into the list (was: a full sidebar).
- **Arrange:** waveform strips with head/tail trim, I/O at the locator, pause rows you drag, and overlap by negative pause (overlap was V2).
- **Box editor:** comp by painting across take lanes with nudgeable segments and seam warnings; Edit mode keeps word delete and pause tools in the same lanes.
- **Keyboard:** adds F, ⇧ Space, , and ., Alt+Up/Down, −/+, ?, U; I/O set in and out everywhere (as in the prototype and Resolve), the inspector moves to ⌘I.
