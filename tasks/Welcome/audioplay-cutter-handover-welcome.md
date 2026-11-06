# Handover: Welcome screen (Shelf B)

**For:** the agent or developer building the welcome screen of Cutaway (Audioplay Cutter).
**Reference:** the interactive mockup `ShelfB.dc.html` is attached. It is a loose guide for layout and feel. Where it differs from this document, **this document wins** (see the click behavior in 4).

## 1. Ground rules: fit into what exists

- **Reuse what the app has:** theme tokens, glass panels, buttons, keycaps, the take deck icon, the stage dots, note count badges, sheets and file pickers. Only the hero area and the tile rows are new.
- **Same data, not a copy.** Recent items, shows, episodes, stages and note counts come from the app's existing project and recents data. Note counts come from the consolidated feedback set (see the review handover).
- **The codebase wins.** If something here conflicts with how the app already works, follow the app and log the decision in `DECISIONS.md`.

## 2. What the screen is

The first thing you see when Cutaway opens. In one place you can:
- create or open a **show** (a folder with a shared library and episodes),
- create or open an **episode**, inside a show or as a **single episode** without a show,
- open either of them in **Edit** mode or in **Review** mode (read-only, see the review handover).

## 3. Layout

- **Hero (top, about half the height).**
  - The selected item's cover image fills the background, darkened on the left and toward the bottom so the text stays readable.
  - On the left:
    - a small kicker (*TARMAC · Ep. 2*, *Single episode*, or *Show*),
    - the title, large,
    - one line of status (stage dots, stage name, last edited),
    - two buttons: **Edit** (↩) and **Review** (⌥↩). Review shows the open note count when there are notes.
- **Top right:** New episode (⌘N), New show (⇧⌘N), Open… (⌘O).
- **Row 1, Recent episodes:** a **+ New episode** tile first, then the most recent episodes as cover tiles (title, show and episode number). Single episodes appear here too.
- **Row 2, Shows:** a **+ New show** tile first, then the shows as cover tiles, then a **Single episodes** tile grouping all episodes without a show.
- **Footer:** one quiet line of hints (arrows, ↩, ⌥↩, drop files).

Keep the text to a minimum: titles, show and episode numbers, stage dots, and note counts only where there are notes.

## 4. Behavior

### Selecting and opening (differs from the mockup)

- **Clicking a tile selects it.** The hero changes to that item. **Nothing opens on click.**
- **Opening is always explicit:** use the hero's **Edit** or **Review** button, or press **↩** (Edit) or **⌥↩** (Review).
- **Hover only highlights.** It doesn't change the hero, so the large image stays calm while the pointer moves.
- **On launch,** the most recently opened item is selected.

### Shows and single episodes

- **Selecting a show:**
  - the hero shows the show (episode count, open notes);
  - the Recent episodes row switches to **that show's episodes** and its title changes to the show name;
  - **Edit** opens the show (its episodes and library), and **Review** opens the show's review space across its episodes.
- **Selecting *Single episodes*:** the first row lists the single episodes. The hero buttons stay disabled until you select one.
- **Going back:** selecting the Recent episodes row title, or pressing Esc, returns the first row to recent episodes across everything.

### Creating

- **+ New episode** or ⌘N opens a short sheet:
  - a name;
  - the show it belongs to, with **No show** for a single episode (pre-selected from the current selection);
  - an optional cover image.

  The new episode is created and selected. It opens only when Edit is chosen.
- **+ New show** or ⇧⌘N works the same way, with a name and an optional cover.
- **Dropping files** onto the window:
  - a script or recordings start a new episode with the name taken from the file;
  - an image dropped on the hero or a tile sets that item's cover;
  - a `.cutawayreview` file runs *Add review* on the matching project.

### Keyboard

| Key | Action |
|---|---|
| ← → | move within a row |
| ↑ ↓ | switch rows |
| ↩ | open the selected item in Edit |
| ⌥↩ | open the selected item in Review |
| ⌘N / ⇧⌘N / ⌘O | new episode / new show / open from disk |

### Opening transition

After Edit or Review, a short full-screen transition shows the item's cover (blurred), its title and a mode label (**Edit** or **Review · read-only**) while the project loads. Keep it under the actual load time; never add delay.

## 5. Cover images

- **Optional.** Any show or episode may have a cover. Without one, the tile and hero use a soft wash of the show's color (neutral for single episodes).
- **Stored in the package:** a `Cover.jpg` (or `.png`) in the show folder or the episode folder (D5), so it travels with the project.
- **Setting a cover:** from the New sheet, by dropping an image on the hero or a tile, or through a *Set cover…* menu item. Center-crop to the tile; an optional focal point can come later.
- **Readability:** always draw the dark gradient over the image behind text. Never place text directly on a bright image.
- **Episode covers:** an episode without its own cover may fall back to its show's cover.

## 6. Recent items and missing files

- **Source:** the app's recent documents list, newest first.
- **Missing items:** an item whose file has moved or gone is shown dimmed with *Locate…* instead of Edit and Review. Locating it relinks it.
- **Removing:** right-click → *Remove from recents*. It never deletes files.

## 7. Why it is designed this way

- **Click selects, buttons open.** Opening a project is heavy and choosing between Edit and Review matters, so a click should never commit to either. The hero becomes a calm confirmation step: you see what you picked and choose how to open it.
- **Hero and rows.** The one or two things you'll most likely open next get a big, clear spot. Everything else stays one row away, without a dense list.
- **Covers.** Shows and episodes are recognized faster by image than by name, especially across several productions.
- **Edit and Review side by side.** Review is a first-class way in for directors and producers, not a hidden option, and the note count tells the editor where feedback is waiting.
- **Single episodes as a group.** One-off audio plays don't need a show, but they shouldn't clutter the shows row either.

## 8. Done when

- Clicking a tile changes the hero and opens nothing. Edit, Review, ↩ and ⌥↩ open the selected item in the matching mode.
- Shows, episodes in shows, and single episodes can each be created and opened in Edit and Review.
- Selecting a show swaps the first row to its episodes. Selecting *Single episodes* lists them.
- Covers show where they exist and fall back to the color wash. Covers can be set from the sheet and by dropping an image.
- Review buttons show open note counts from the consolidated feedback set.
- Missing files show *Locate…*, and all keys in section 4 work.
- Every conflict with the existing app is logged in `DECISIONS.md`.
