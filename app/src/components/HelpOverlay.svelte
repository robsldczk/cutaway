<script lang="ts">
  import { app } from '../lib/app.svelte';
  import { CHAPTERS, chapterFor } from '../lib/tour';
  const rows: [string, string[]][] = [
    ['Play / stop', ['Space']],
    ['Play from locator', ['⇧', 'Space']],
    ['Play line in context', ['Enter']],
    ['Previous / next line', ['↑', '↓']],
    ['Previous / next take (Options, Arrange)', ['←', '→']],
    ['Mute the line / restore a cut line', ['⌃M']],
    ['Extend passage', ['⇧↑', '⇧↓']],
    ['Apply take to passage', ['P']],
    ['Favorite take', ['F']],
    ['Box editor', ['E']],
    ['Mapping editor: mark the right take by hand (Options)', ['M']],
    ['Inspector', ['⌘I']],
    ['Unmatched bin', ['U']],
    ['Sound bucket', ['B']],
    ['Editor ⇄ full bucket (Import › Sounds)', ['⇧B']],
    ['Options / Arrange / Edit / Export', ['1', '2', '3', '4']],
    ['Export page; there, export', ['⌘E']],
    ['Box size', ['−', '+']],
    ['Shorter / longer pause (Options, Arrange)', [',', '.']],
    ['Move line (Arrange)', ['⌥↑', '⌥↓']],
    ['Select more lines (Arrange)', ['⌘ click', '⇧ click']],
    ['Move the line back or forward in the room (Arrange)', ['⌥←', '⌥→', 'right click']],
    ['Copy / paste a line’s attributes (Arrange)', ['⌥⌘C', '⌥⌘V']],
    ['In / out at the locator', ['I', 'O']],
    ['Undo / redo', ['⌘Z', '⌘⇧Z']],
    ['Save version', ['⌘S']],
    ['Fly through the lines (Options, Arrange)', ['⇧ scroll']],
    ['Zoom / move scene bar (over the bar)', ['⌘ scroll', '⇧ scroll']],
  ];
  const welcome: [string, string[]][] = [
    ['Move within a row / between the rows', ['←', '→', '↑', '↓']],
    ['Open the selected item in Edit / Review', ['↩', '⌥↩']],
    ['New episode / new show / open from disk', ['⌘N', '⇧⌘N', '⌘O']],
    ['Back to the recent episodes', ['Esc']],
  ];
  const sounds: [string, string[]][] = [
    ['Show / hide the sounds', ['S']],
    ['Scrub', ['scroll']],
    ['Select more sounds', ['⇧ click', 'drag on empty space']],
    ['Reach a sound, one after the other in time', ['Tab', '⇧Tab']],
    ['Select more sounds, or all', ['⇧Enter', '⌘A']],
    ['Duplicate the selected sounds (Arrange, Edit)', ['⌘D']],
    ['Open a sound', ['2×', 'Enter']],
    ['Opened sound: trim, fade or move the part in focus', ['Tab', '↑', '↓']],
    ['Opened sound: the same by 10 ms / slip the audio inside', ['⌥↑', '⌥↓', '⌥ drag']],
    ['Opened sound: hear it alone, from the scrubber', ['Space', 'drag the rail']],
    ['Opened sound: trim to the scrubber', ['I', 'O']],
    ['Close the opened sound', ['Enter', 'Esc']],
    ['Depth', ['drag sideways', '←', '→']],
    ['Pan', ['⌥ drag sideways', '⌥←', '⌥→']],
    ['Move or trim without snapping', ['⌥ drag']],
    ['Loudness', ['ring', '⇧ scroll', '⇧↑', '⇧↓']],
    ['Move 100 ms / 10 ms earlier or later', [',', '.', '⇧,', '⇧.']],
    ['Remove / clear the selection', ['⌫', 'Esc']],
    ['Play from the selected sound', ['⇧', 'Space']],
    ['Fade a bucket / show only it', ['eye', '⌥ eye']],
  ];
  const edit: [string, string[]][] = [
    ['Show / hide the scene bucket', ['B']],
    ['Inspector / mixer / cycle', ['I', 'X', 'C']],
    ['Previous / next clip', ['←', '→']],
    ['Playhead to previous / next word', [',', '.']],
    ['Nudge 10 ms / 100 ms', ['⌥←→', '⇧⌥←→']],
    ['Move one clip alone', ['⌥ drag']],
    ['Add to the selection', ['⇧ click', 'drag']],
    ['Select all following / all', ['⇧F', '⌘A']],
    ['Split at the playhead', ['⌘T']],
    ['Trim start / end to the playhead', ['⌥[', '⌥]']],
    ['Mute clip (stays in place)', ['⌃M']],
    ['Ripple delete (Ripple on) / mute (off); sounds: remove', ['⌫']],
    ['Cycle to the selection', ['⌘U']],
    ['Zoom in / out', ['⌘←', '⌘→', '⌥ scroll']],
    ['Zoom to selection and back', ['Z']],
    ['Lane height', ['⌘↑', '⌘↓']],
    ['Mute / solo the selected lane', ['M', 'S']],
    ['Tool menu, then the tool letter', ['T']],
    ['Back to the pointer / clear selection', ['Esc']],
  ];
  const review: [string, string[]][] = [
    ['Marker at the playhead', ['M']],
    ['Note on the current line', ['N']],
    ['Previous / next scene', ['←', '→']],
    ['Previous / next line', ['↑', '↓']],
    ['Search scenes, lines and notes', ['⌘K']],
    ['Add the note', ['↩']],
    ['Move a note', ['drag its dot, bracket or top row']],
    ['Scroll through the scene, faster', ['scroll', '⇧ scroll']],
    ['Undo your last note', ['⌘Z']],
  ];
  const ed: [string, string[]][] = [
    ['Play / play from locator', ['Space', '⇧ Space']],
    ['Nudge segment', ['←', '→']],
    ['Move playhead (no segment)', ['←', '→']],
    ['Next segment', ['Tab']],
    ['Words overlay', ['W']],
    ['Use whole take', ['1', '…', '5']],
    ['Reset comp', ['R']],
    ['Delete / restore words (Edit)', ['⌫']],
    ['Comp / Edit mode', ['M']],
    ['Close', ['Esc']],
  ];
  const chapter = $derived(!app.noProject && app.view ? chapterFor(app.page, app.inReview) : null);
  function takeTour() {
    app.overlay = null;
    if (chapter) void app.startTour(chapter);
  }
</script>

<div class="wrap" role="presentation" onclick={() => (app.overlay = null)}>
  <div class="glass box" role="dialog" aria-label="Keyboard shortcuts" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    {#if chapter}
      <button class="tour" onclick={takeTour} data-testid="take-tour"><b>Take the tour of {CHAPTERS[chapter].name}</b><span class="muted">Anything you try is put back at the end.</span></button>
    {/if}
    <div class="h1">Keyboard</div>
    {#each rows as [l, ks] (l)}<div class="hk"><span>{l}</span><span>{#each ks as k (k)}<span class="kb">{k}</span> {/each}</span></div>{/each}
    <div class="h2">Sounds on the Arrange page</div>
    {#each sounds as [l, ks] (l)}<div class="hk"><span>{l}</span><span>{#each ks as k (k)}<span class="kb">{k}</span> {/each}</span></div>{/each}
    <div class="h2">On the Edit page</div>
    {#each edit as [l, ks] (l)}<div class="hk"><span>{l}</span><span>{#each ks as k (k)}<span class="kb">{k}</span> {/each}</span></div>{/each}
    <div class="h2">On the welcome screen</div>
    {#each welcome as [l, ks] (l)}<div class="hk"><span>{l}</span><span>{#each ks as k (k)}<span class="kb">{k}</span> {/each}</span></div>{/each}
    <div class="h2">In review</div>
    {#each review as [l, ks] (l)}<div class="hk"><span>{l}</span><span>{#each ks as k (k)}<span class="kb">{k}</span> {/each}</span></div>{/each}
    <div class="h2">In the box editor</div>
    {#each ed as [l, ks] (l)}<div class="hk"><span>{l}</span><span>{#each ks as k (k)}<span class="kb">{k}</span> {/each}</span></div>{/each}
  </div>
</div>

<style>
  .wrap {
    position: absolute;
    inset: 0;
    z-index: 40;
    background: var(--scrim-light);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .box {
    width: min(680px, 92%);
    box-sizing: border-box;
    padding: 24px 28px;
    border-radius: var(--radius-sheet);
    background: var(--panel-solid);
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px 32px;
    font-size: var(--fs-ui);
    max-height: 86vh;
    overflow-y: auto;
  }
  .tour {
    grid-column: 1 / -1;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    border-radius: var(--radius-btn);
    background: rgb(var(--accent-rgb) / 0.16);
    box-shadow: inset 0 0 0 1px rgb(var(--accent-rgb) / 0.4);
    text-align: left;
  }
  .tour:hover {
    background: rgb(var(--accent-rgb) / 0.26);
  }
  .h1 {
    grid-column: 1 / -1;
    font-weight: 700;
    font-size: 15px;
  }
  .h2 {
    grid-column: 1 / -1;
    font-weight: 700;
    margin-top: 8px;
    color: var(--text-3);
  }
  .hk {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    color: var(--text-2);
  }
</style>
