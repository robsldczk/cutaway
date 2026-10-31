# Round 2 screens (as implemented)

Designs and rationale: `tasks/` (four screens and `audioplay-cutter-ui-round2-rationale.md`). Decisions D40–D43 in `docs/DECISIONS.md`. This page supersedes the *Edit page*, *Sound bucket* and *Importer* sections of `docs/specs/frontend-design.md`, which is read-only in this checkout and was left unchanged.

**Header:** `Import | Options | Arrange | Edit`. Import and Edit have no scene rail; the Edit header's scene label is a scene picker.

## Import › Files
- Sub-tabs `Files | Sounds`.
- Two drop zones (script, audio) that collapse to one line with *Replace* / *Add files*.
- Cast header with the two file types (Recording: solid chip with a mic; Director room: outlined chip with a speaker), *Matched … ago* and **Redo match**.
- **Needs you**, one-click fixes: type unclear → Recording / Director room; no character → Assign / Suggest; room not linked → Link to (pick recordings); character without recording → Pick file; missing file → Find.
- Character cards: each recording with its director room file (connector, sync offset, "Ensemble · shared with N others" / "Only X"). The type chips are buttons that switch a file's type.
- Footer: *Remember my fixes for the whole show* and **Start analysis** (progress in place).
- A new script version opens a diff sheet (changed, new, deleted lines) with *Undo this version*.

## Import › Sounds
- Sort by Scene, Tag or Show; **Spot script**; **New bucket** (name, keyword chips, Advanced: length, leave out, scenes; *Create and search*).
- Bucket cards: name, scene chips, keyword chips, four tiles (gold = downloaded, outlined grey = preview). The editor's scene is outlined in gold.
- Side panel: provider tabs (Epidemic Sound, ElevenLabs, Library), the keyword query and *Search*, **In this bucket** (gold play, waveform, ✓, remove), **Previews · not downloaded** (click the waveform to audition from there, download, dismiss), *Load 12 more*. Space plays, arrows step through results.
- **Back to Edit ⇧B** in the header.

## Edit
- **Control bar** (Logic-style): Inspector I, Mixer X; tools Pointer, Split, Fade, Gain, Marquee; go to start, Play, Cycle C, a time display with scene, lane and clip; Snap: words; Ripple; zoom; Bucket B.
- **Ruler** with the cycle range (drag in the ruler sets it, a click seeks).
- **Script strip:** dialogue in the character colour (the line at the playhead highlighted), action lines in grey italics below.
- **Lane groups:** Dialogue, Sound effects (layers added on overlap), Ambience (room tone with loop seams, ambience), Music. Each lane has M and S; groups fold. Gold = sound effects, blue grey = room tone and ambience, indigo = music.
- **Anchors:** dashed gold lines from the effect to its word or action line.
- **Bucket docked (B):** the scene's buckets with a legend; drag a sound onto a lane. A preview downloads on drop: a grey clip with a gold progress bar and "Downloading 64% · snaps to …"; gold when done. *More like these* per bucket; *Full bucket ⇧B*; *Hide*.
- **Full screen:** a slim Bucket tab on the right edge brings the bucket back.
- **Inspector (I):** Clip (name, source, gain, fade in, fade out, curve), Anchor (to, offset, *Follows dialogue*, *Pin to time*), Lane (volume, pan) and meters. Room tone clips show their loop and gain here (rationale open question 1). The selected clip shows fade handles and its gain line.
- **Tools:** Split cuts the clicked clip at the pointer; Fade drags a fade in from the left half or out from the right half; Gain drags up and down; Marquee drags a range that becomes the cycle.
- **Mixer (X):** a strip per lane (volume fader, pan, M, S). Not designed in Round 2 (D42).

## Keys
On the Edit page I is the inspector, X the mixer, C the cycle, B the docked bucket, Esc returns to the pointer tool. ⇧B switches between the editor and Import › Sounds everywhere. I/O stay in/out on Options and Arrange.
