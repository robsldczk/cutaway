<script lang="ts">
  // One field for scenes, lines and notes, across every round. Enter goes to the one lit.
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { quoted, type Note, type SearchResults } from '../lib/review';
  import Icon from './Icon.svelte';

  let { onnote }: { onnote: (n: Note) => void } = $props();

  let q = $state('');
  let open = $state(false);
  let lit = $state(0);
  let found = $state<SearchResults | null>(null);
  let input = $state<HTMLInputElement>();

  type Hit = { key: string; go: () => void };
  const hits = $derived.by(() => {
    const out: Hit[] = [];
    if (!found) return out;
    for (const s of found.scenes) out.push({ key: `s${s.id}`, go: () => void app.loadScene(s.id) });
    for (const l of found.lines) out.push({ key: `l${l.id}`, go: () => void goLine(l.scene_id, l.id) });
    for (const n of found.notes) out.push({ key: `n${n.id}`, go: () => onnote(n) });
    return out;
  });
  const nothing = $derived(!!found && !hits.length && q.trim().length > 0);

  async function goLine(scene: string, line: string) {
    await app.loadScene(scene);
    app.select(line);
  }

  $effect(() => {
    const term = q.trim();
    if (!term) {
      found = null;
      return;
    }
    const wait = setTimeout(async () => {
      const r = await app.run(() => call<SearchResults>('review_search', { q: term }));
      if (q.trim() === term) {
        found = r ?? null;
        lit = 0;
      }
    }, 120);
    return () => clearTimeout(wait);
  });

  export function focus() {
    input?.focus();
    input?.select();
    open = true;
  }

  function pick(h: Hit | undefined) {
    if (!h) return;
    h.go();
    open = false;
    input?.blur();
  }

  function key(e: KeyboardEvent) {
    // ⌘K is the page's; everything else belongs to the field
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') return;
    e.stopPropagation();
    if (e.key === 'Escape') {
      if (q) q = '';
      else {
        open = false;
        input?.blur();
      }
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      lit = Math.min(hits.length - 1, lit + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      lit = Math.max(0, lit - 1);
    } else if (e.key === 'Enter') pick(hits[lit]);
  }

  /** The text with what was searched for lit. */
  function parts(text: string) {
    const words = q.trim().split(/\s+/).filter(Boolean).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
    if (!words.length) return [{ t: text, on: false }];
    return text.split(new RegExp(`(${words.join('|')})`, 'gi')).map((t, i) => ({ t, on: i % 2 === 1 }));
  }
  const index = (key: string) => hits.findIndex((h) => h.key === key);
</script>

{#snippet lit_(text: string)}{#each parts(text) as p, i (i)}{#if p.on}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}{/snippet}

<div class="sw">
  <label class="srch" class:on={open}>
    <Icon name="search" size={16} />
    <input bind:this={input} bind:value={q} onfocus={() => (open = true)} onblur={() => setTimeout(() => (open = false), 120)} onkeydown={key} placeholder="Search scenes, lines and notes" aria-label="Search scenes, lines and notes" spellcheck="false" autocomplete="off" data-testid="review-search" />
    <span class="key">⌘K</span>
  </label>
  {#if open && (hits.length || nothing)}
    <div class="dd" role="listbox" aria-label="Search results" data-testid="search-results">
      {#if nothing}<div class="none muted">Nothing for “{q.trim()}”</div>{/if}
      {#if found?.scenes.length}
        <div class="dh">Scenes</div>
        {#each found.scenes as s (s.id)}
          <button class="di" class:lit={hits[lit]?.key === `s${s.id}`} role="option" aria-selected={hits[lit]?.key === `s${s.id}`} onmouseenter={() => (lit = index(`s${s.id}`))} onmousedown={(e) => e.preventDefault()} onclick={() => pick(hits[index(`s${s.id}`)])} data-testid="search-scene">
            <span class="num tn">{s.number}</span>
            <span class="col"><span class="h ellip">{@render lit_(s.title || s.place)}</span><span class="sub ellip">{@render lit_(s.text)}</span></span>
            {#if s.open}<span class="cnt">{s.open}</span>{/if}
          </button>
        {/each}
      {/if}
      {#if found?.lines.length}
        <div class="dh">Lines</div>
        {#each found.lines as l (l.id)}
          <button class="di" class:lit={hits[lit]?.key === `l${l.id}`} role="option" aria-selected={hits[lit]?.key === `l${l.id}`} onmouseenter={() => (lit = index(`l${l.id}`))} onmousedown={(e) => e.preventDefault()} onclick={() => pick(hits[index(`l${l.id}`)])} data-testid="search-line">
            <span class="col"><span class="sub ellip who">{l.who} · Sc. {l.scene_number}</span><span class="h script ellip">{@render lit_(l.text)}</span></span>
          </button>
        {/each}
      {/if}
      {#if found?.notes.length}
        <div class="dh">Notes</div>
        {#each found.notes as n (n.id)}
          <button class="di" class:lit={hits[lit]?.key === `n${n.id}`} role="option" aria-selected={hits[lit]?.key === `n${n.id}`} onmouseenter={() => (lit = index(`n${n.id}`))} onmousedown={(e) => e.preventDefault()} onclick={() => pick(hits[index(`n${n.id}`)])} data-testid="search-note">
            <span class="av">{n.author.initials}</span>
            <span class="col"><span class="h ellip">{@render lit_(n.text)}</span><span class="sub ellip">{n.scene_number ? `Sc. ${n.scene_number}` : 'No scene'}{n.round_number ? ` · Round ${n.round_number}` : ''}{n.quote ? ` · ${quoted(n)}` : ''}</span></span>
            {#if n.status === 'resolved'}<Icon name="check" size={13} />{/if}
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .sw {
    position: relative;
    width: min(440px, 100%);
  }
  .srch {
    height: 38px;
    border-radius: 12px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    box-sizing: border-box;
    background: var(--glass-fill-3);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
    font-size: 13.5px;
    color: var(--text-3);
  }
  .srch.on {
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.8);
  }
  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: none;
    font: inherit;
    color: var(--text-strong);
    user-select: text;
    -webkit-user-select: text;
  }
  input::placeholder {
    color: var(--text-5);
  }
  .key {
    display: inline-flex;
    align-items: center;
    height: 19px;
    padding: 0 5px;
    border-radius: 5px;
    background: var(--glass-hover);
    font-size: 10.5px;
    font-weight: 600;
    color: var(--text-3);
    white-space: nowrap;
  }
  .dd {
    position: absolute;
    left: 0;
    right: 0;
    top: 44px;
    z-index: 20;
    max-height: 70vh;
    overflow-y: auto;
    border-radius: 18px;
    background: var(--panel-solid);
    padding: 8px;
    box-sizing: border-box;
    box-shadow: inset 0 0 0 1px var(--glass-outline), 0 30px 70px rgb(var(--shadow-rgb) / 0.6);
  }
  .dh {
    padding: 8px 10px 4px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-5);
  }
  .none {
    padding: 12px 10px;
    font-size: 13px;
  }
  .di {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 10px;
    border-radius: 11px;
    box-sizing: border-box;
    text-align: left;
  }
  .di.lit {
    background: var(--glass-fill-4);
  }
  .num {
    flex: none;
    width: 34px;
    height: 28px;
    border-radius: 9px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12.5px;
    font-weight: 700;
    background: rgb(var(--accent-rgb) / 0.18);
    color: var(--link);
  }
  .col {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .h {
    font-size: 13.5px;
    font-weight: 600;
  }
  .h.script {
    font-weight: 400;
  }
  .sub {
    font-size: 11.5px;
    color: var(--text-3);
  }
  .sub.who {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .cnt {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    box-sizing: border-box;
    border-radius: 9px;
    background: rgb(var(--accent-rgb) / 0.85);
    color: var(--text-on-pill);
    font-size: 10.5px;
    font-weight: 700;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .av {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 700;
    background: var(--glass-active);
    color: var(--text-strong);
  }
  mark {
    background: rgb(var(--accent-rgb) / 0.35);
    color: var(--text-strong);
    border-radius: 3px;
    padding: 0 1px;
  }
</style>
