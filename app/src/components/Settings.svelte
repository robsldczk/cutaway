<script lang="ts">
  // Settings (⌘,), in tabs (D163): General for this computer, Pacing for the open episode and
  // scene, Connections for the services, then the imprint and the licenses. Beside the tabs: the
  // version, which a click copies.
  import { onDestroy } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { IMPRINT, IMPRINT_SOURCE } from '../lib/legal';
  import { versionLabel, versionLine } from '../lib/version';
  import Sheet from './Sheet.svelte';
  import Connections from './Connections.svelte';
  import Icon from './Icon.svelte';
  import Licenses from './Licenses.svelte';

  type Tab = 'general' | 'pacing' | 'connections' | 'imprint' | 'licenses';
  const TABS: { id: Tab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'pacing', label: 'Pacing' },
    { id: 'connections', label: 'Connections' },
    { id: 'imprint', label: 'Imprint' },
    { id: 'licenses', label: 'Licenses' },
  ];
  let tab = $state<Tab>('general');

  const v = $derived(app.view);
  let breath = $state(app.view?.breath_gap_s ?? 0.3);
  // The scene stores a pause multiplier; the slider shows its inverse on a log scale,
  // so right is tighter (×2 = half the pauses, twice the overlap).
  let tight = $state(Math.log2(1 / (app.view?.tightness ?? 1)));
  let model = $state('');
  let leadIn = $state(app.status?.handle_in_ms ?? 100);
  let tail = $state(app.status?.handle_out_ms ?? 150);
  let edgeFade = $state(app.status?.edge_fade_ms ?? 20);
  let panRoom = $state(Math.round((app.status?.pan_room ?? 0.5) * 100));

  async function setBreath() {
    await app.method('set_config', { key: 'pacing.breath_gap_s', value: +breath });
  }
  async function setTight() {
    if (!v) return;
    await app.cmd({ cmd: 'set_scene_tightness', scene_id: v.scene.id, tightness: 2 ** -tight });
  }
  async function setPacing(key: string, value: number) {
    await app.method('set_config', { key: `pacing.${key}`, value: +value });
  }
  async function level(on: boolean) {
    await app.method('set_config', { key: 'playback.level', value: on });
  }
  async function roomTone(on: boolean) {
    await app.method('set_config', { key: 'pacing.room_tone', value: on });
  }
  async function locTone(on: boolean) {
    if (!v) return;
    const tone = { ...v.room_tone, enabled: on };
    if (on && !tone.source_id) {
      // default: the quietest second before the first take of the scene on its source
      const first = v.items.find((i) => i.kind === 'line' && i.takes.length);
      const t = first?.takes[0];
      if (t) Object.assign(tone, { source_id: t.source_id, t0: Math.max(0, t.src_in - 1.2), t1: Math.max(0.5, t.src_in - 0.2) });
    }
    await app.cmd({ cmd: 'set_room_tone', location_id: v.location_id, tone });
  }
  async function setModel() {
    if (model.trim()) await app.method('set_config', { key: 'ai.model', value: model.trim() });
  }

  // ---- the version ----
  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyVersion() {
    try {
      await navigator.clipboard.writeText(versionLine);
      copied = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 1500);
    } catch {
      app.say(`Couldn't copy: ${versionLine}`);
    }
  }
  onDestroy(() => clearTimeout(copiedTimer));

  /** ← → between the tabs, as a tab list does. */
  function tabKey(e: KeyboardEvent) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    e.stopPropagation();
    const i = TABS.findIndex((t) => t.id === tab);
    tab = TABS[(i + (e.key === 'ArrowRight' ? 1 : TABS.length - 1)) % TABS.length].id;
    (document.querySelector(`[data-settings-tab="${tab}"]`) as HTMLElement | null)?.focus();
  }
</script>

{#snippet onoff(on: boolean, set: (v: boolean) => void, label: string, testid: string)}
  <div class="seg sw" role="radiogroup" aria-label={label} data-testid={testid} data-on={on}>
    <button class="tab" class:on role="radio" aria-checked={on} onclick={() => !on && set(true)}>On</button>
    <button class="tab" class:on={!on} role="radio" aria-checked={!on} onclick={() => on && set(false)}>Off</button>
  </div>
{/snippet}

<Sheet title="Settings" width={680}>
  <div class="bar">
    <div class="seg tabs" role="tablist" aria-label="Settings" tabindex="-1" onkeydown={tabKey}>
      {#each TABS as t (t.id)}
        <button class="tab" class:on={tab === t.id} role="tab" aria-selected={tab === t.id} tabindex={tab === t.id ? 0 : -1} onclick={() => (tab = t.id)} data-settings-tab={t.id} data-testid="settings-tab-{t.id}">{t.label}</button>
      {/each}
    </div>
    <button class="ver tn" onclick={copyVersion} title={copied ? 'Copied' : 'Copy version'} aria-label="Copy version {versionLabel}" data-testid="app-version">
      {versionLabel}<Icon name={copied ? 'check' : 'copy'} size={12} stroke={2.2} />
    </button>
  </div>
  <div class="fade" aria-hidden="true"></div>

  <div class="body" role="tabpanel" aria-label={TABS.find((t) => t.id === tab)?.label}>
    {#if tab === 'general'}
      <div class="rows">
        <div class="cap">This computer</div>
        <div class="row">
          <div class="l"><span class="ttl">Appearance</span></div>
          <div class="seg sw" role="radiogroup" aria-label="Theme">
            <button class="tab" class:on={app.theme === 'dark'} role="radio" aria-checked={app.theme === 'dark'} onclick={() => app.setTheme('dark')}>Dark</button>
            <button class="tab" class:on={app.theme === 'light'} role="radio" aria-checked={app.theme === 'light'} onclick={() => app.setTheme('light')} data-testid="theme-light">Light</button>
          </div>
        </div>
        <div class="row">
          <div class="l"><span class="ttl">Follow playhead</span><span class="hint">The current box moves with the playhead while playing. Off, it stays where you put it and Space plays from it again. Same as Follow in the header.</span></div>
          {@render onoff(app.follow, (on) => app.setFollow(on), 'Follow playhead', 'follow-setting')}
        </div>
        <div class="row">
          <div class="l"><span class="ttl">Developer</span><span class="hint">Shows the Dev page: the transcript under the hood, marks, re-analysis per character.</span></div>
          {@render onoff(app.dev, (on) => app.setDev(on), 'Developer', 'dev-switch')}
        </div>
        {#if app.status}
          <div class="cap">This episode</div>
          <div class="row">
            <div class="l"><span class="ttl">Playback level</span><span class="hint">Evens out roles recorded at different levels and limits loud peaks, so a hot mic doesn't blast. Playback only: bounces and exports stay as recorded.</span></div>
            {@render onoff(app.status.playback_level ?? true, level, 'Playback level', 'playback-level')}
          </div>
          <div class="row">
            <div class="l"><span class="ttl">Room tone</span><span class="hint">Plays room tone under the scenes.</span></div>
            {@render onoff(app.status.room_tone ?? true, roomTone, 'Room tone', 'room-tone-setting')}
          </div>
        {/if}
      </div>
    {:else if tab === 'pacing'}
      {#if v && app.status}
        <div class="grid">
          <div class="cap span">This scene · {v.scene.place || `Scene ${v.scene.number}`}</div>
          <label for="tight">Scene tightness</label>
          <div class="ctl"><input id="tight" type="range" min="-1.5" max="3.3" step="0.05" bind:value={tight} onchange={setTight} /><span class="val">×{(2 ** +tight).toFixed(2)}</span></div>
          <span class="hint2">Right is punchier: divides every default pause and spacer of this scene and deepens overlaps. Pauses you set by hand are kept.</span>
          <span class="lab">Room tone here</span>
          <div class="ctl">{@render onoff(v.room_tone.enabled ?? false, locTone, `${v.scene.place || 'This location'} has a room tone`, 'location-tone')}</div>
          <span class="hint2">Plays a room tone recorded at {v.scene.place || 'this location'} under every scene there.</span>

          <div class="cap span">The episode</div>
          <label for="breath">Breath gap</label>
          <div class="ctl"><input id="breath" type="range" min="-0.5" max="1.5" step="0.05" bind:value={breath} onchange={setBreath} /><span class="val">{+breath < 0 ? `${(-breath * 1000).toFixed(0)} ms overlap` : `${(+breath * 1000).toFixed(0)} ms`}</span></div>
          <span class="hint2">Below zero the next line comes in before the last one ends: first into its silent tail, then over the words.</span>
          <label for="leadin">Lead-in</label>
          <div class="ctl"><input id="leadin" type="range" min="0" max="500" step="10" bind:value={leadIn} onchange={() => setPacing('handle_in_ms', leadIn)} data-testid="lead-in" /><span class="val">{leadIn} ms</span></div>
          <label for="tail">Tail</label>
          <div class="ctl"><input id="tail" type="range" min="0" max="500" step="10" bind:value={tail} onchange={() => setPacing('handle_out_ms', tail)} data-testid="tail" /><span class="val">{tail} ms</span></div>
          <span class="hint2">Audio before and after every line, so it doesn't start or stop hard. Stops short of other words on the recording; the timing of the cut doesn't move. Heard and exported.</span>
          <label for="edgefade">Edge fade</label>
          <div class="ctl"><input id="edgefade" type="range" min="1" max="100" step="1" bind:value={edgeFade} onchange={() => setPacing('edge_fade_ms', edgeFade)} data-testid="edge-fade" /><span class="val">{edgeFade} ms</span></div>
          <span class="hint2">Fades every line in and out over this long, at the outer edge of its lead-in and tail. Longer is softer.</span>
          <label for="panroom">Pan into the room</label>
          <div class="ctl"><input id="panroom" type="range" min="0" max="100" step="1" bind:value={panRoom} onchange={() => setPacing('pan_room', panRoom / 100)} data-testid="pan-room-setting" /><span class="val">{panRoom} %</span></div>
          <span class="hint2">How far a voice panned to the side goes back into the room: a little less direct sound, a little more room. Around the centre it stays dry. Heard and exported.</span>
        </div>
      {:else}
        <p class="empty">Open an episode to set its pacing.</p>
      {/if}
    {:else if tab === 'connections'}
      <div class="rows">
        <div class="cap">Sound services</div>
        <Connections />
        {#if app.status}
          <div class="cap">AI</div>
          <div class="row top">
            <div class="l">
              <span class="ttl">Provider: {app.status.ai_provider}</span>
              {#if app.status.ai_warning}<span class="hint">Rules: {app.status.ai_warning}</span>{/if}
              <span class="hint">Only text (transcripts and script) leaves this machine, never audio.</span>
            </div>
            <span class="pair"><input class="field" placeholder="model, e.g. claude-sonnet-5-5" aria-label="AI model" bind:value={model} /><button class="btn" onclick={setModel}>Set</button></span>
          </div>
        {/if}
      </div>
    {:else if tab === 'imprint'}
      <div class="imprint" data-testid="imprint">
        {#each IMPRINT as block, i (block.language)}
          {#if i > 0}<div class="fade" aria-hidden="true"></div>{/if}
          <section>
            <div class="cap">{block.language}</div>
            {#each block.sections as s (s.heading)}
              <div class="sec">
                <h4>{s.heading}</h4>
                {#each s.lines as line (line)}<p>{line}</p>{/each}
              </div>
            {/each}
          </section>
        {/each}
        <p class="src">Also at {IMPRINT_SOURCE}</p>
      </div>
    {:else}
      <Licenses />
    {/if}
  </div>
</Sheet>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .tabs {
    flex: 1;
    min-width: 0;
  }
  .tabs .tab {
    flex: 1;
    height: 26px;
    padding: 0 10px;
  }
  .ver {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    background: var(--glass-fill-4);
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: var(--text-3);
    transition:
      background 0.15s var(--ease-out),
      color 0.15s var(--ease-out);
  }
  .ver:hover {
    background: var(--glass-hover);
    color: var(--text-1);
  }
  .fade {
    height: 1px;
    margin: 14px 0;
    background: linear-gradient(90deg, transparent, rgb(var(--veil-rgb) / 0.16) 10%, rgb(var(--veil-rgb) / 0.16) 90%, transparent);
  }
  /* a fixed height, so switching tabs doesn't make the sheet jump */
  .body {
    height: 470px;
    max-height: 62vh;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .body:has(:global([data-testid='licenses'])) {
    overflow: hidden;
  }
  .cap {
    font-size: 10px;
    line-height: 14px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-4);
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .rows .cap:not(:first-child) {
    margin-top: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
  }
  .row.top {
    align-items: flex-start;
  }
  .l {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .ttl {
    color: var(--text-1);
  }
  .hint,
  .hint2 {
    font-size: 11px;
    color: var(--text-4);
  }
  .sw {
    flex: none;
    display: inline-flex;
  }
  .sw .tab {
    height: 24px;
    min-width: 48px;
    padding: 0 12px;
    font-size: 12px;
  }
  .pair {
    display: flex;
    flex: none;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 12px 18px;
    align-items: center;
  }
  .grid .span {
    grid-column: 1 / -1;
  }
  .grid .span:not(:first-child) {
    margin-top: 10px;
  }
  label,
  .lab {
    color: var(--text-3);
  }
  .ctl {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .ctl input[type='range'] {
    flex: 1;
    accent-color: var(--text-strong);
  }
  .val {
    min-width: 108px;
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .hint2 {
    grid-column: 2;
  }
  .empty {
    color: var(--text-4);
  }
  .imprint {
    display: flex;
    flex-direction: column;
    user-select: text;
  }
  .imprint section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .sec h4 {
    margin: 0 0 2px;
    font-size: var(--fs-ui);
    font-weight: 600;
    color: var(--text-strong);
  }
  .sec p,
  .src {
    margin: 0;
    color: var(--text-3);
  }
  .src {
    margin-top: 18px;
    font-size: 11px;
    color: var(--text-5);
  }
</style>
