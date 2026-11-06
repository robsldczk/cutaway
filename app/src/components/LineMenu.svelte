<script lang="ts">
  // A line's right click on Arrange: how far back it stands, and what it hands on to other lines.
  // With several lines selected, everything here is for all of them.
  import { onMount, tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { charLabel } from '../lib/colors';
  import { clamp } from '../lib/format';
  import { attrsText, DEPTH_STOPS, depthText } from '../lib/lines';

  let { lineId, ids, x, y, onclose }: { lineId: string; ids: string[]; x: number; y: number; onclose: () => void } = $props();

  /** Room to keep clear of the window's edges. */
  const MENU_W = 264;
  const MENU_H = 262;
  const left = $derived(clamp(x, 8, window.innerWidth - MENU_W - 8));
  const top = $derived(clamp(y, 8, window.innerHeight - MENU_H - 8));

  let el = $state<HTMLDivElement>();
  const line = $derived(app.lines.find((l) => l.line_id === lineId));
  const depth = $derived(line ? app.depthOf(line) : 0);
  const dry = $derived((app.view?.room_wet ?? 1) <= 0 && depth > 0);
  const near = (a: number, b: number) => Math.abs(a - b) < 0.005;

  const all = (d: number) => () => d;

  function slide(e: Event) {
    app.previewDepth(ids, all(Number((e.currentTarget as HTMLInputElement).value) / 100));
  }
  function release(e: Event) {
    void app.setLineDepth(ids, all(Number((e.currentTarget as HTMLInputElement).value) / 100));
  }
  function copy() {
    app.copyAttrs(line);
    onclose();
  }
  function paste() {
    void app.pasteAttrs(ids);
    onclose();
  }

  function key(e: KeyboardEvent) {
    const items = [...(el?.querySelectorAll<HTMLElement>('button:not(:disabled), input') ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') onclose();
    else if (e.key === 'ArrowDown') items[(i + 1) % items.length]?.focus();
    else if (e.key === 'ArrowUp') items[(i - 1 + items.length) % items.length]?.focus();
    else if (e.key === 'Enter' || e.key === ' ') (document.activeElement as HTMLElement | null)?.click();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  onMount(() => {
    void tick().then(() => el?.querySelector<HTMLElement>('input')?.focus());
    const away = (e: MouseEvent) => {
      if (!el?.contains(e.target as Node)) onclose();
    };
    window.addEventListener('mousedown', away, true);
    return () => window.removeEventListener('mousedown', away, true);
  });
</script>

{#if line}
  <!-- svelte-ignore a11y_interactive_supports_focus (its controls take the focus) -->
  <div class="menu" bind:this={el} role="menu" aria-label="Line {line.number}" style="left: {left}px; top: {top}px" onkeydown={key} data-testid="line-menu" data-own-keys>
    <div class="mhead muted">{ids.length > 1 ? `${ids.length} lines` : `${charLabel(app.view, line.character)} · line ${line.number}`}</div>
    <div class="place">
      <div class="row"><span>In the room</span><span class="tn val" data-testid="line-depth-value">{depthText(depth)}</span></div>
      <input type="range" min="0" max="100" step="1" value={Math.round(depth * 100)} oninput={slide} onchange={release} aria-label="How far back in the room" aria-valuetext={depthText(depth)} data-testid="line-depth" />
      <div class="stops">
        {#each DEPTH_STOPS as s (s.label)}
          <button role="menuitemradio" aria-checked={near(depth, s.depth)} class:on={near(depth, s.depth)} onclick={() => app.setLineDepth(ids, all(s.depth))} data-testid="depth-stop">{s.label}</button>
        {/each}
      </div>
      {#if dry}<p class="hint" data-testid="line-depth-dry">This scene’s room is dry, so the line only gets quieter.</p>{/if}
    </div>
    <div class="hr"></div>
    <button role="menuitem" onclick={copy} data-testid="copy-attrs">Copy attributes<span class="kb">⌥⌘C</span></button>
    <button role="menuitem" disabled={!app.lineAttrs} onclick={paste} data-testid="paste-attrs">
      <span class="two"><span>Paste attributes{ids.length > 1 ? ` on ${ids.length} lines` : ''}</span>{#if app.lineAttrs}<span class="what ellip">{attrsText(app.lineAttrs)}</span>{/if}</span>
      <span class="kb">⌥⌘V</span>
    </button>
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 35;
    width: 256px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px;
    border-radius: 12px;
    background: var(--panel-solid);
    backdrop-filter: var(--glass-blur);
    -webkit-backdrop-filter: var(--glass-blur);
    box-shadow: var(--glass-shadow), 0 14px 40px rgb(var(--shadow-rgb) / 0.4);
    pointer-events: auto;
  }
  .mhead {
    padding: 3px 8px 5px;
    font-size: 11px;
  }
  .place {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 8px 8px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-small);
    color: var(--text-2);
  }
  .val {
    color: var(--text-1);
  }
  input {
    width: 100%;
    margin: 0;
    accent-color: var(--playhead);
  }
  .stops {
    display: grid;
    grid-template-columns: 1fr 1.5fr 1fr 1fr;
    gap: 4px;
  }
  .stops button {
    height: 24px;
    padding: 0 2px;
    justify-content: center;
    font-size: 11px;
    text-align: center;
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.14);
  }
  .stops button.on {
    color: var(--text-1);
    box-shadow: inset 0 0 0 1.5px var(--playhead);
  }
  .hint {
    margin: 0;
    font-size: 11px;
    line-height: 1.35;
    color: var(--warn);
  }
  .hr {
    height: 1px;
    margin: 2px 6px 4px;
    background: rgb(var(--ink-rgb) / 0.1);
  }
  button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-height: 28px;
    padding: 0 8px;
    border-radius: 7px;
    font-size: var(--fs-small);
    color: var(--text-2);
    text-align: left;
  }
  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--glass-hover);
    color: var(--text-1);
  }
  button:disabled {
    opacity: 0.45;
  }
  .two {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: 3px 0;
  }
  .what {
    font-size: 11px;
    color: var(--text-4);
  }
  .kb {
    flex: none;
    font-size: 11px;
    color: var(--text-4);
  }
</style>
