<script lang="ts">
  // A sound dropped in from the Finder: where does it live? The name is already guessed, so
  // Enter files it and puts it in the scene. Type another name for a new bucket, or click a
  // chip and Enter. The arrows walk the chips: Enter takes the one lit, Shift alone completes the
  // name from it. Esc drops the whole thing, and nothing has been imported by then.
  import { tick, untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { call } from '../lib/bridge';
  import { stepChip } from '../lib/sounds/chips';
  import type { BucketsView } from '../lib/types';

  const AMBIENCE = 'Ambience';
  /** How a file says it's a bed, not an effect. */
  const AMBIENT = /ambien|room.?tone|atmo/i;

  const f = $derived(app.filing);
  let text = $state('');
  let typed = $state(false);
  let opening = 0;
  let buckets = $state<{ id: string; name: string }[]>([]);
  let box = $state<HTMLInputElement>();
  let chipsEl = $state<HTMLElement>();
  /** The chip the arrows are on, by name: narrowing the chips must not land on another. */
  let lit = $state<string | null>(null);
  /** Shift went down on its own, and nothing has been typed with it. */
  let shiftAlone = false;

  const fold = (s: string) => s.trim().toLowerCase();
  /** Typing narrows the chips; what was guessed leaves them all. */
  const chips = $derived([AMBIENCE, ...buckets.map((b) => b.name)].filter((n) => !typed || fold(n).includes(fold(text))));
  const chosen = $derived(lit != null && chips.includes(lit) ? lit : null);
  /** Where the sound goes: the chip the arrows are on, or the name in the field. */
  const wanted = $derived(chosen ?? text);
  const known = $derived(buckets.find((b) => fold(b.name) === fold(wanted)));
  const toRoom = $derived(fold(wanted) === fold(AMBIENCE));
  const place = $derived(app.view?.scene.place ?? 'this room');
  const stem = (p: string) => (p.split('/').pop() ?? p).replace(/\.\w+$/, '');

  $effect(() => {
    const paths = f?.paths;
    if (!paths) return;
    untrack(() => void open(paths[0]));
  });

  async function open(path: string) {
    const mine = ++opening;
    typed = false;
    text = '';
    lit = null;
    const [view, hit] = await Promise.all([call<BucketsView>('buckets', {}), call<{ name: string; bucket_id: string | null }>('suggest_bucket', { name: path })]);
    if (mine !== opening) return;
    buckets = view.buckets.filter((b) => !b.character).map((b) => ({ id: b.id, name: b.name }));
    // what's typed in the meantime stays: a guess never overwrites it
    if (typed) return;
    text = !hit.bucket_id && AMBIENT.test(path) ? AMBIENCE : hit.name;
    await tick();
    box?.focus();
    box?.select();
  }

  function pick(name: string) {
    text = name;
    typed = false;
    lit = null;
    box?.focus();
  }

  /** Moves along the chips; the first step starts from the chip the name already is. */
  async function step(by: 1 | -1) {
    const from = chosen ?? chips.find((n) => fold(n) === fold(text)) ?? null;
    lit = stepChip(chips, from, by);
    await tick();
    chipsEl?.querySelector('.lit')?.scrollIntoView({ block: 'nearest' });
  }

  /** Shift: the name of the chip lit, or of the first match while typing. */
  function complete() {
    const name = chosen ?? (typed ? chips[0] : undefined);
    if (!name) return;
    text = name;
    lit = null;
    // the chips stay as they were, narrowed by what was typed
    box?.setSelectionRange(name.length, name.length);
  }

  async function file() {
    const job = app.filing;
    const name = wanted.trim();
    if (!job || !name) return;
    app.filing = null;
    if (toRoom) {
      for (const [i, path] of job.paths.entries()) await app.method('import_ambience', { path, ...(i === 0 && app.sceneId ? { scene: app.sceneId } : {}) }, false);
      await app.refresh();
      app.say(`Ambience of ${place}`);
      return;
    }
    const bucket_id = known?.id ?? (await app.run(() => call<{ bucket_id: string }>('create_bucket', { name, keywords: [name.toLowerCase()] })))?.bucket_id;
    if (!bucket_id) return;
    let first: { id: string; file: string; duration_s: number } | undefined;
    for (const p of job.paths) {
      const r = await app.method<{ assets: { id: string; file: string; duration_s: number }[] }>('import_sounds', { paths: [p], bucket_id }, false);
      first ??= r?.assets[0];
    }
    if (first) {
      const sound = { asset_id: first.id, bucket_id, name: stem(first.file), duration_s: first.duration_s };
      if (app.page === 'arrange') await app.soundsUi?.placeFile(sound, job.x, job.y);
      else if (app.page === 'edit') {
        await app.method('place_from_bucket', { scene: app.sceneId, asset_id: first.id, bucket_id, at_s: +app.pos.toFixed(3) });
        await app.loadEdit();
      }
    }
    await app.refresh();
    app.say(`Into ${known?.name ?? name}`);
  }

  function key(e: KeyboardEvent) {
    shiftAlone = e.key === 'Shift' && !e.repeat;
    if (e.key === 'Enter') {
      e.preventDefault();
      void file();
    } else if (e.key === 'Escape') {
      e.stopPropagation();
      app.filing = null;
    } else if ((e.key === 'ArrowDown' || e.key === 'ArrowRight' || e.key === 'ArrowUp' || e.key === 'ArrowLeft') && !e.shiftKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      void step(e.key === 'ArrowDown' || e.key === 'ArrowRight' ? 1 : -1);
    }
  }

  // on release, so Shift with a letter stays a capital
  function keyUp(e: KeyboardEvent) {
    if (e.key === 'Shift' && shiftAlone) complete();
    shiftAlone = false;
  }

  const spot = $derived(f ? `left: ${Math.max(16, Math.min(f.x + 12, window.innerWidth - 396))}px; top: ${Math.max(76, Math.min(f.y - 20, window.innerHeight - 320))}px` : '');
</script>

{#if f}
  <div class="sheet glass" style={spot} role="dialog" aria-label="File this sound" data-testid="file-sheet">
    <div class="who ellip" title={f.paths[0]}>{f.paths.length > 1 ? `${f.paths.length} sounds` : stem(f.paths[0])}</div>
    <input
      class="field name"
      bind:this={box}
      bind:value={text}
      oninput={() => {
        typed = true;
        lit = null;
      }}
      onkeydown={key}
      onkeyup={keyUp}
      placeholder="Bucket"
      aria-label="Bucket"
      data-testid="file-bucket"
    />
    <div class="say muted" data-testid="file-say">
      {#if toRoom}Loops under {place}{:else if known}Into {chosen ? chosen : 'this bucket'}, and into the scene{:else if wanted.trim()}New bucket, and into the scene{:else}Name a bucket{/if}
      <span class="keys">{#if chosen}<span class="kb">⇧</span> {/if}<span class="kb">↵</span></span>
    </div>
    <div class="chips" bind:this={chipsEl}>
      {#each chips as n (n)}
        <button class="chip" class:on={!chosen && fold(n) === fold(text)} class:lit={n === chosen} onmousedown={(e) => e.preventDefault()} onclick={() => pick(n)} data-testid="file-chip">{n}</button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .sheet {
    position: absolute;
    z-index: 60;
    width: 380px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    border-radius: 18px;
    background: var(--panel-solid);
    box-shadow: var(--glass-shadow);
  }
  .who {
    font-size: 12px;
    color: var(--text-3);
  }
  .name {
    height: 34px;
    font-size: 14px;
    font-weight: 600;
  }
  .say {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11.5px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    max-height: 132px;
    overflow: auto;
    padding-top: 2px;
  }
  .chip {
    padding: 3px 9px;
    border-radius: var(--radius-pill);
    font-size: 11.5px;
    color: var(--text-2);
    box-shadow: inset 0 0 0 1px rgb(var(--ink-rgb) / 0.18);
  }
  .chip.on,
  .chip.lit {
    color: var(--on-sfx);
    background: var(--sfx);
    box-shadow: none;
  }
  .chip.lit {
    outline: 2px solid var(--text-strong);
    outline-offset: 1px;
  }
  .keys {
    display: flex;
    gap: 3px;
  }
</style>
