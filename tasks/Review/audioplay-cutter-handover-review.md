# Handover: Review mode

**For:** the agent or developer adding review mode to the Cutaway (Audioplay Cutter) codebase.
**Attached screens** (loose guides, not specs):
- *C · Margin notes, search, scene rail*
- *C · Entering review asks who you are*
- *C · Search across scenes, lines and notes*

## 1. Ground rules: fit into what exists

- **Reuse what the app already has.** The screens are loose guides. Build review mode from existing components and keep only the review-specific parts new:
  - Arrange line cards and the dialogue spine,
  - the scene rail on the right (numbered scene pills with status color, lock and dot),
  - the bottom context bar and transport,
  - the glass panels, tags, avatars, sheets and search patterns.
- **One document (D7).** Review mode is another way of looking at the live project, not a copy or an export. It reads the same episode data the editing pages use.
- **Every write is a command.** Creating, replying to, tagging, resolving and reopening notes go through the command bus, are undoable, and are available in the CLI and MCP (D8).
- **The codebase wins.** If something here conflicts with how the app already works, follow the app and log the decision in `DECISIONS.md`.

## 2. What review mode is

A lean, read-only way for people who don't edit (the director, a producer, a co-producer like Albert) to listen to scenes and leave notes exactly where they want changes. Like frame.io, but anchored to script lines, words, pauses and sounds instead of video frames.

Over time it becomes a standing **review space** for the show: every review round stays available, and you can go back to earlier rounds as more scenes are added.

## 3. Getting into review mode

There are two entry points; both open the same mode.

1. **Welcome screen:** a new option, **Open project package in review mode**, next to Open… It opens an episode or show package read-only for reviewing.
2. **Export page:** a new option, **Open these scenes for review**, next to the export settings.
   - It starts a **review round** for the selected scenes: it records which cut those scenes are in and flags the scenes as *in review*.
   - Flagged scenes show up in the review panel and on the scene rail.
   - Exporting and locking (see the Export handover) and opening for review can happen in one go, since a delivered cut is usually what gets reviewed.

**Who's reviewing.** On entry, ask once for the reviewer's name and role (Director, Producer, Writer, Actor, Client, Other) and offer *Remember me on this computer*. Every note carries that identity. A chip in the header shows who is reviewing and allows switching.

## 4. Read-only, live data

- **Live, not exported.** Review mode loads the live project data (lines, takes in use, timing, sounds, music, room tracks) and plays it through the normal audio engine. No exported files are needed.
- **No editing tools.**
  - Every command that changes the cut is unavailable in review mode, and the command bus rejects it with a clear reason.
  - The only commands allowed are note commands and reviewer identity.
- **Mode, not separate app.** Review mode is a mode of the same app, not a separate build, so it uses the same data and playback code.

## 5. The review screen (see screen C)

- **Center:** the scene as Arrange cards with the dialogue spine. Clicking the spine sets a marker; dragging along it marks a range. Ranges show as brackets beside the spine, markers as dots.
- **Margin:** notes hang to the right of the spine, linked to their spot. The note being written opens in place. Each note shows:
  - author and role, time code and quoted words,
  - text and tag (Performance, Timing, Sound, Music),
  - replies and the resolve state.
- **Filters** above the notes: All, Open, Mine, and by kind.
- **Scene rail** (right, reused from the app): the scenes in the current round are bright and the others dimmed. A dot means open notes, the lock means exported and locked, and the ring marks the current scene.
- **Context bar** (bottom, reused): transport and the current scene with note pins and the playhead.
- **Search** (header, ⌘K): one field for scenes, lines and notes, results grouped by those three. Enter jumps to the scene, line or note. Search covers all review rounds.
- **Rounds:** a round picker in the header lists the current round and every earlier one (date, scenes, cut, number of notes). An earlier round opens with its notes in place, and the current state is shown where things changed (5).

## 6. One consolidated feedback set

Notes from many reviewers and many rounds must not pile up as separate lists. They form **one feedback set per episode** that always reflects the current state.

- **Computed, not copied.** The consolidated view is a live query over all notes, not an exported document. It updates the moment anything changes, in review mode or on the editing pages.
- **Threads.** Notes from different reviewers on the same element, or overlapping ranges on it, appear as one thread with several voices. Replies, including the editor's, sit in the thread.
- **States:** open → resolved → (reopened). Each change records who made it and when.
- **Inside the editing pages:**
  - Open notes appear as markers on Options, Arrange and Edit.
  - Notes also appear as a **Notes** tab in the scene sounds panel, as on the Bokeh notes board. From there you can jump to a note, reply, and resolve it.
  - Sound and music notes offer a shortcut into the sound bucket.
- **Review sees edit, and edit sees review.**
  - When the editor resolves a note, the reviewer sees it as done right away, with who resolved it and when.
  - When a reviewer reopens it, it shows up open again in the editing pages.
- **A summary for each round:** how many notes are open, resolved or reopened, per scene and per reviewer. It is generated from the same data.

## 7. Notes stick to what they're about

Notes glue to elements exactly like sound effects do (the M7 anchoring and the Arrange sound handover).

- **Anchors:** a word, a range of words, a line, a pause, a sound placement, or a scene. Timeline positions are derived from the anchor, never stored as plain seconds.
- **Following the cut:** reordering lines, changing gaps, swapping takes or moving sounds moves the note with its element. The note's time code is always computed from where the element is now.
- **Changed since the note:** if the element changes after the note was written (new take, trimmed, sound moved), show a small *changed since note* hint, so the reviewer knows the fix is there to hear.
- **Deleted elements:** if the element is deleted, keep the note as *detached*. It is listed in its scene with the quoted words and the last known position, never silently dropped.
- **Locked scenes:** if a scene is locked (exported), notes still attach normally. A note on a locked scene is a strong hint to unlock and re-export, and the Export page should say so.

## 8. Data model

| Entity | Holds |
|---|---|
| **Reviewer** | name, role, initials, remembered per computer |
| **ReviewRound** | id, created by, date, scenes, cut reference (scene versions and timeline hashes), status |
| **Note** | id, round, author, anchor (element id + word range or offset), tag, text, status, created / resolved / reopened by and when |
| **NoteReply** | note, author, text, date |

Notes live in the episode package next to the cut, so the package carries its own review history. A show-level view can gather notes across episodes for the review space.

## 9. Getting reviews back: one format, two ways in

Reviewers work on their own computers. Their notes come back through a shared folder today, and as a portable file when there is no shared folder. Both use the **same file format and the same merge**, so there is only one code path.

### The `.cutawayreview` file

- **Contents:** a gzip-compressed JSON file.
  - A **manifest**: format version, project and episode ids, review rounds, the cut each round refers to (scene versions and timeline hashes), reviewer, device, number of events, and a checksum.
  - An **event log**: every note, reply, edit of a note, tag change, resolve and reopen, as one event.
- **Every event:**
  - has a globally unique id;
  - records its author, a hybrid logical clock timestamp and its round;
  - holds the note's anchor (element id plus word range or offset), not a time in seconds.
- **Append-only:** events are added, never edited.

### Merge rules

- **Idempotent.** Merging is the union of events by id. Importing the same file twice, an older copy, or a file that overlaps another changes nothing it shouldn't. That's what makes moving files around safe.
- **State is folded from events.** For each note, the latest event per field wins by its clock. Resolve and reopen are events too, so "resolved in edit" and "reopened in review" settle cleanly in either order.
- **Everyone writes their own file.** Each person writes only their own file, so two machines never write the same file and nothing conflicts.

### Shared folder (today)

```
TARMAC/                       show folder
  Reviews/
    anna-weber.cutawayreview      one rolling file per reviewer
    albert.cutawayreview
    rob.cutawayreview             the editor's replies, resolves and reopens
```

- **One rolling file per person, not per session.** Review mode appends that reviewer's events to their own file and compacts it from time to time (a snapshot plus the newest events), so it never splits into fragments.
- **The editing app watches the folder** and merges new events into the project database as they arrive. The project database stays the source of truth for the editor.
- **The editor's own actions** (replies, resolves, reopens) are written to the editor's file in the same folder, so reviewers see "done" without asking.
- **Location.** The folder sits in the show folder by default; the show settings can point it at another shared location.

### Add review (without a shared folder)

- **Exporting:** in review mode, *Send review* saves the reviewer's events as a single `.cutawayreview` file, ready to e-mail or hand over.
- **Importing:** the editor uses **Add review** (or drags the file onto the app). It is merged with the same rules as the shared folder.
- **Archiving:** after a successful import, the file moves to a hidden `Reviews/Imported/` archive with its date, so nothing piles up in view. Adding it again is harmless.
- **Mismatches:** if a file belongs to a different project, or refers to a cut the project no longer has, the import says so, and its notes attach as *detached* where their elements are missing.

### Overview

A **Review history** panel lists what happened, without opening any file:
- each round (date, scenes, cut, reviewers),
- each arrival (*"Albert, 12 new notes, 3 replies, via shared folder, Tue 14:02"* or *"Anna, added from file"*),
- open and resolved counts per round, scene and reviewer.

This panel and the consolidated feedback set (6) are both built from the merged events.

### Why this option

- **One mechanism.** A shared folder and Add review use the same file and merge, so behavior can't drift between them.
- **No conflicts.** A file per person avoids two machines writing the same file in a shared or synced folder.
- **Safe to repeat.** Events with ids and an idempotent merge make re-sending, duplicates and out-of-order arrival harmless.
- **No clutter.** Rolling per-person files plus archiving after import keep the folder clean and the overview in the app.
- **Room to grow.** A sync service later just moves the same events.

## 10. Why it is designed this way

- **Read-only, live data.** Reviewers hear the real current cut without anyone exporting first, and they can't change it by accident.
- **Anchored notes.** In an audio play the cut changes constantly. Notes tied to lines and words survive reordering and take swaps, where time codes would point at the wrong place after the first edit.
- **One consolidated set.** The editor works from a single to-do list that stays current, instead of reconciling messages from several people and rounds.
- **Rounds you can revisit.** Reviews come in waves as more scenes are finished. Keeping each round makes the history clear (who asked for what, when, and on which cut) while the consolidated set keeps day-to-day work simple.
- **Reused components.** Review should feel like the same app, so a reviewer who later opens the editor already knows the layout.

## 11. Open questions

1. **Simultaneous access:** what happens if the project is open in edit mode and review mode at the same time on one computer? SQLite WAL allows it, but the editing app needs to pick up note changes live.
2. **Who may resolve:** only the editor, or the reviewer too?
3. **Episode or show:** should a review round cover scenes across episodes, or one episode at a time?

## 12. Done when

- The welcome screen offers *Open project package in review mode*, and the Export page offers *Open these scenes for review*, which starts a round and flags the scenes.
- Review mode plays the live project with no editing commands available. The command bus rejects edits there.
- The reviewer is asked for name and role once, and every note carries them.
- Notes attach to words, ranges, lines, pauses, sounds and scenes, and they follow reordering, take swaps, gap changes and sound moves. Deleted elements leave detached notes, not lost ones.
- Resolving in the editing pages shows as done in review immediately, and reopening in review shows as open in the editing pages.
- All rounds stay available, search covers scenes, lines and notes across rounds, and the consolidated feedback set updates live.
- Notes, replies and resolve state are available through the CLI and MCP.
- Review files in the shared folder merge automatically. *Add review* imports a `.cutawayreview` file and archives it. Importing the same file twice changes nothing, and the Review history panel shows every arrival.
