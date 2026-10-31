<script lang="ts">
  import { app } from '../lib/app.svelte';
  const rows: [string, string[]][] = [
    ['Play / stop', ['Space']],
    ['Play from locator', ['⇧', 'Space']],
    ['Play line in context', ['Enter']],
    ['Previous / next line', ['↑', '↓']],
    ['Previous / next take', ['←', '→']],
    ['Extend passage', ['⇧↑', '⇧↓']],
    ['Apply take to passage', ['P']],
    ['Favorite take', ['F']],
    ['Box editor', ['E']],
    ['Inspector', ['I']],
    ['Unmatched bin', ['U']],
    ['Sound bucket', ['B']],
    ['Options / Arrange / Edit', ['1', '2', '3']],
    ['Box size', ['−', '+']],
    ['Shorter / longer pause', [',', '.']],
    ['Move line (Arrange)', ['⌥↑', '⌥↓']],
    ['In / out at locator (Arrange)', ['I', 'O']],
    ['Undo / redo', ['⌘Z', '⌘⇧Z']],
    ['Save version', ['⌘S']],
    ['Zoom / move scene bar', ['⌘ scroll', '⇧ scroll']],
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
</script>

<div class="wrap" role="presentation" onclick={() => (app.overlay = null)}>
  <div class="glass box" role="dialog" aria-label="Keyboard shortcuts" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
    <div class="h1">Keyboard</div>
    {#each rows as [l, ks] (l)}<div class="hk"><span>{l}</span><span>{#each ks as k (k)}<span class="kb">{k}</span> {/each}</span></div>{/each}
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
