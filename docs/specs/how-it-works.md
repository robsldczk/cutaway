# Cutaway: How It Works

## Purpose

Turn raw, unslated session recordings into script-matched, adjustable rough cuts without re-screening the material. The script is the spine; the audio fills it.

## Inputs

| Input | Format | Notes |
|---|---|---|
| Role tracks | One continuous WAV (BWF) per character | Isolated, clean, timecoded. Recorded as per-character run-throughs, no group recording. |
| Room track | One continuous WAV (BWF) | Director mic. Isolated, timecoded. Contains direction and possibly cue reading. |
| Script | Throughline file (Fountain based, proprietary flavor) | Parsed as Fountain, extensions kept as metadata. |

*(Changed in implementation, D61: a Throughline `.thrl` export is gzipped JSON, not Fountain. Cutaway reads it natively into the same parsed script, and keeps its story context (beat titles and summaries, acts, cast per scene, setting groups, character bios) per episode. Fountain files still work.)*

Session language: English. Takes are not slated and not cut. Take structure is derived from the script.

## Pipeline

1. **Ingest and validate**
   - Read BWF timecode, iXML, sample rate and bit depth.
   - Check that all files share one timecode base.
   - Create the project folder.

2. **Script parse**
   - Extract scenes, characters, dialogue lines, parentheticals and action lines.
   - Keep notes `[[ ]]`, sections `#` and synopses `=` as metadata.
   - Give every dialogue line a stable ID and tokenize it into words.

3. **Transcription**
   - Run English ASR with word timestamps on each track (WhisperX or mlx-whisper).
   - Feed script vocabulary (character names, invented words) as prompt and hotwords.
   - Use VAD for speech segments.

4. **Script alignment**
   - Per role track, align the transcript word stream against that character's lines in script order.
   - Use local word-level sequence alignment that tolerates omissions, substitutions and ad-libs.
   - Output per spoken segment: matched line IDs, word mapping, confidence.

5. **Take detection**
   - While the script position moves forward, it is one take. A backward jump starts a new take.
   - Classify each take as:
     - full run,
     - partial rerun,
     - pickup (short),
     - micro-repeat (single word or half sentence).
   - Silence gaps and room mic activity confirm the boundaries.

6. **Forced alignment**
   - Re-align the audio against the expected text for precise word boundaries.
   - Where the actor deviates from the script, keep the actually spoken words.

7. **Room mic classification**
   - Sort every speech segment into:
     - **cue reading:** matches other characters' lines; used as a timing reference, excluded from notes,
     - **direction:** everything else.
   - Flag notes spoken during a take.

8. **Note linking and summary**
   - Attach each note to the take before it (feedback) and the take after it (direction).
   - An LLM condenses each note to one sentence plus tags (faster, quieter, angrier, ad-lib, flub).
   - Detect explicit picks ("that's the one", "keep that").
   - *Changed in implementation (D73):* the room talk between two takes is one gap, read whole by the LLM (chatter included) and summarised in one or two full sentences. The same gap is the feedback of the take before it and the direction of the take after it.

9. **Default selection per line**
   - Director pick, else the last complete take, else the highest-confidence take.

10. **Timeline assembly per scene**
    - One box per dialogue line, in script order.
    - A default breath gap between boxes.
    - Spacers generated from action lines.
    - Scene pacing parameters applied.

11. **Unmatched bin**
    - Per scene, speech that matches no script line (ad-libs, chatter).

## Data model

| Entity | Holds |
|---|---|
| Project | Settings, source files, scenes, pacing defaults |
| SourceFile | Path, role, timecode start, sample rate, loudness cache, waveform peaks |
| Scene | Heading, ordered timeline items, pacing settings |
| ScriptLine | ID, character, text, words, parenthetical |
| Take | Role, source range, type (full, partial, pickup, micro), linked notes |
| Utterance | One performance of one line in one take: source file, in/out, word timings, confidence, LUFS |
| Note | Room mic range, transcript, summary, tags, linked takes, pick flag |
| Box | Timeline item for a line: selected utterance or comp, edit list, head/tail trim |
| CompSegment | Source utterance, word range, crossfade length |
| Edit | Deleted word ranges, pause trims, close-or-keep-pause flag |
| Spacer | Linked action line, duration, enabled flag |
| Gap | Duration between boxes, override flag |
| Split point | Edit page split of a box by source time; the part after it has an offset, trims, mute and drop *(added in implementation, M11 / D55)* |

Rules:

- **Fully non-destructive.** Everything references source files with in and out points. Original WAVs are never modified.
- **Default handles:** 1 s.
- **Storage:** project state in the episode SQLite file (see the architecture spec), plus a cache for transcripts, alignments and waveform peaks. *(Changed in implementation: was "project state as JSON"; the architecture spec and build brief settle on SQLite.)*
- **Undo:** per box and per scene.
- **Mute and ripple delete** *(added in implementation, M11 / D56)*: a muted box, part or sound keeps its time but is not heard (skipped in playback and bounce, disabled in FCPXML, no subtitle). A ripple-deleted part leaves the cut and the time closes up; it can be restored.
- **Pinned boxes** *(changed in implementation, M11 / D57)*: a box pinned to absolute time is out of the ripple chain; the next box follows the last unpinned one. The first unpinned box honours its pause.

*(Changed in implementation, D104: a take can be marked by hand in the mapping editor (`TakeOrigin::Marked`): it's heard exactly as marked, without handles, and a re-run leaves it alone. A line can be added by hand (`adlib`) or deleted (`removed`: off the scene's order, its takes kept, so the matcher still claims its audio and a new script version doesn't bring it back).)*

## Audio rules

- **Cut points:** snap to word boundaries, then to the nearest zero crossing.
- **Crossfades:** applied automatically on every splice, short and equal power.
- **Lead-in, tail and edge fade** *(added in implementation, D71)*: every line plays from up to `pacing.handle_in_ms` (100 ms) before its first kept sound to up to `pacing.handle_out_ms` (150 ms) after its last, fading in and out over `pacing.edge_fade_ms` (20 ms, was a fixed 5 ms). A handle stops 40 ms short of any other word on the same recording, at the file's edges and at the scene start. Only the heard audio grows: the layout, pauses and word times don't move. Playback, bounce and FCPXML all use it.
- **Splice warnings:** flag a splice that lands mid-phrase with little pause or a large pitch jump.
- **Audition loudness:** integrated LUFS is measured per utterance at ingest and used as a playback offset only, so takes are compared at equal loudness.
  - *Changed in implementation (D69):* scene playback levels per recording (the median take LUFS of each source), not per take, so performances keep their dynamics; take and sound auditions match per item. Leveling and an output safety limiter are on by default (`playback.level`) and never reach bounces or exports.
- *Changed in implementation (D75, D76):* sound effects and ambience of a scene play through that scene's one room, on a shared reverb bus; each scene has its own room. A sound's depth (0 close … 1 far) moves it towards the room and further back. Playback and the bounce run the same code and match; dialogue, music and room tone never reach the room. The room's tail rings on past the last clip in both.
- **Format:** export keeps the source sample rate and bit depth.

## Export

| Format | Content |
|---|---|
| FCPXML | For Resolve and Logic. One track per role, clips linked to the original WAVs with source timecode, handles and crossfades. Markers for scenes and notes. Per scene, or one timeline with scene markers. |
| WAV / MP3 | Bounce per scene or full, via ffmpeg. Optional loudness normalization. |
| SRT | From final word timings, character name optional. |
| As-cut transcript | Per line: character, text, take, source file, source TC in/out, note. Doubles as cut list. |
| AAF (later) | For Pro Tools mixers. |

Comped or edited boxes export as several clips on the same role track, with crossfades.

*Changed in implementation (D108–D114):* an export is a version folder per scene (`Exports/Sc02 Canteen/v003 2026-11-05/` in the episode's folder; `Exports/Ep02/Sc02 Canteen/…` in a pipeline folder, D117) with a manifest, never overwritten. Stems are every track at the full length of the scene, from one render path, and add up to the mix. Loudness applies to the mix alone. An exported scene locks its timing in the command bus.

## Tech stack

See `architecture.md` for the stack, the project folder, the importer and the AI provider. In short:
- **Core:** Rust.
- **App:** Tauri with a web UI.
- **Audio:** a native audio engine.
- **Transcription:** Parakeet via ONNX Runtime, local.
- **AI interpretation:** Claude Code CLI through a provider interface (model agnostic).

## Build stages

**Stage 0: pipeline proof, no UI**
- CLI: role WAVs + room WAV + Throughline file in, FCPXML rough cut and confidence report out.
- Success criteria:
  - take boundaries detected correctly,
  - 85% or more of lines matched,
  - notes linked to the right takes.

**Stage 1: MVP**
- Full pipeline
- Options view
- Arrange view (reorder)
- Box editor: comp mode in text-aligned swipe, edit mode with word delete, pause trim and restore
- Pacing controls
- All exports except AAF

**V2**
- Waveform-precise editing in the box editor
- Modifier-key cut modes
- Overlap mode beyond the Arrange page's negative pause *(changed for the Option C design: simple overlap by negative pause is in the MVP)*
- Cue-reading rhythm as timing template
- SFX and music lanes
- Volume automation and effects
- Horizontal timeline
- AAF export

## Sound effects (planned)

See the architecture spec for the details. In short:
- **Spotting:** an AI pass reads each scene and suggests sound cues anchored to script lines, which go into a per-scene wish list (the bucket).
- **Show library:** cues and sounds live at show level and are reused across scenes and episodes.
- **Sourcing:** sounds come from the library, from drag-in files, from search providers (e.g. Epidemic Sound) or from generation providers (e.g. ElevenLabs, Magnific).
- **Placement:** placed sounds anchor to dialogue, so they follow the cut when timing changes. *Changed in implementation (D123):* the anchor is the word at the moment the sound was put, plus an exact offset, and glue can be turned off per sound.

New data model entities:

| Entity | Holds |
|---|---|
| Show | Library database, aliases, locations, settings |
| LibraryAsset | File, hash, category, tags, duration, source (imported, licensed, generated), license, prompt |
| SoundCue | Scene, script anchor, description, type, tags, priority, status, candidates, chosen asset |
| Placement | Asset, lane and layer, anchor + offset (or absolute time), trim, fades, gain |

## Room tone (planned)

*Changed in implementation (D86, D92, D93):* a location is one room with one ambience, picked in Import › Rooms, and optionally a second for night; a scene without one plays what its place has. A loop plays the file to 85 %, then crossfades back to 45 % and repeats that stretch.

*Changed in implementation (D94):* directions before a scene's first line take their spacer time on the timeline, and the first line follows them.

- The script already defines locations: every scene heading names a room (e.g. TARMAC - CANTEEN, TIME SPRING COMMAND).
- Each location gets one room tone, set once (an extracted silence from the session or a library file).
- It plays automatically under every scene in that location and fills gaps, cuts and spacers.
- It exports as its own track.

## Stage 0 test results (Paul, TARMAC Ep. 2, 13 min)

Inputs: Paul track, room track, Fountain script, plus sentence-level transcript JSON for each track.

**What worked**
- **Script matching:** 36 takes on all 9 Paul lines in the recorded span. No line was placed wrongly.
- **Take boundaries:** found without slates, from repeated script lines.
- **Bleed:** aligning against *all* characters' lines makes other voices on the actor's mic match their own lines, so they drop out.
- **Pick detection:** the director's words right after a take found the right picks ("that was the one I liked", "that one was the right one", "we have the edible, definitely").
- **Cue reading:** the room track contains a reader for the other roles. It can be used as context audio, so the actor is heard in the scene.
- **Partial takes:** false starts are flagged ("I put this in the time machine").
- **Unmatched bin:** actor questions to the director end up there instead of in the timeline.

**What we learned**
- **Tracks are not fully isolated.** The actor mic carries the director and the reader, so bleed handling is required: match against all characters, then check loudness.
- **Sentence timestamps are not enough.** Clip edges were estimated and then snapped to loudness. Good enough for auditioning, not for export. Word-level ASR (WhisperX or similar) on the WAVs is required.
- **Speaker labels from the transcript are unreliable.** Ownership has to come from script matching plus loudness, never from diarization.
- **Room offset.** The room track ran about 0.25 s behind the actor track. Measure the sync per file pair (envelope cross-correlation) and correct for it.
- **Short lines need context rules.** One-word lines ("Lunch.", "Whoa!") only match when they stand alone or start a segment. Otherwise every "and" becomes a line.
- **Notes need an LLM for both directions.** Keyword summaries are not usable. Direction (before a take) and feedback (after it) both go through the LLM and come back as a few words.
- **Note windows.** Feedback is the first ~8 s of room speech after a take; direction is up to 30 s before it. Longer conversations in between are context, not notes.
  - *Changed in implementation (D73):* direction reaches back up to 60 s before a take, and a gap is read as a whole, so its note is one or two sentences rather than a few words.
- **Matching runs in two passes.** Pass 1 matches long lines across the whole script to find the active scenes. Pass 2 matches all lines, but only within those scenes and their neighbours.

## Risks

- **Micro-takes:** quick repeats of single words or half sentences create ambiguous takes.
- **Notes during takes:** direction spoken while the actor is performing.
- **Ad-libs:** lines that match no script line.
  *Changed in implementation (D118):* an actor's small sounds aren't collected or cut by the analysis. They're added as takes or lines in the mapping editor.
- **Script quirks:** proprietary Throughline extensions breaking the Fountain parse.

## Open items

- Word-level transcription on the original WAVs.
- LLM note condensing (direction and feedback).
- FCPXML export of the default picks, opened in Resolve as the next proof.
- A second actor track, to test assembling a scene from two real role tracks.
