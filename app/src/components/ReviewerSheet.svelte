<script lang="ts">
  // Who's reviewing: asked once on entry, and when the chip in the header is pressed.
  import { app } from '../lib/app.svelte';
  import { EDITOR_ROLE, ROLES, roleLabel, type ReviewRole } from '../lib/review';
  import Icon from './Icon.svelte';

  const ask = $derived(app.asking);
  const who = $derived(app.reviewer);
  const round = $derived(app.review?.rounds.find((r) => r.id === app.review?.round) ?? null);
  /** Replying from an editing page, you're the editor. */
  const roles = $derived(app.inReview ? ROLES : [EDITOR_ROLE, ...ROLES]);

  let name = $state(app.reviewer?.current?.name ?? '');
  let role = $state<ReviewRole>(app.reviewer?.current?.role ?? (app.inReview ? 'director' : 'editor'));
  let keep = $state(app.reviewer?.remembered ?? true);

  const named = $derived(name.trim().length > 0);
  const editor = $derived(round?.created_by.name);

  async function start() {
    if (!named) return;
    const then = ask?.then;
    await app.setReviewer(name.trim(), role, keep);
    app.asking = null;
    then?.();
  }

  function useLast() {
    const l = who?.last;
    if (!l) return;
    name = l.name;
    role = l.role;
  }

  function key(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Enter') void start();
    else if (e.key === 'Escape' && ask?.closable) app.asking = null;
  }

  const focus = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };
</script>

<div class="scrim" role="presentation" onclick={() => ask?.closable && (app.asking = null)}></div>
<div class="glass who" role="dialog" tabindex="-1" aria-modal="true" aria-label="Who is reviewing" onkeydown={key} data-testid="who-sheet" data-own-keys>
  <span class="rv">Review</span>
  <h2>Who’s reviewing?</h2>
  <p class="muted">Your notes carry this name, so {editor ?? 'the editor'} knows who asked for what.</p>
  <label class="lab" for="rv-name">Name</label>
  <input id="rv-name" class="inp" bind:value={name} use:focus autocomplete="off" spellcheck="false" data-testid="who-name" />
  <div class="lab">Role</div>
  <div class="chips" role="radiogroup" aria-label="Role">
    {#each roles as r (r.id)}
      <button class="rc" class:on={role === r.id} role="radio" aria-checked={role === r.id} onclick={() => (role = r.id)} data-testid="who-role-{r.id}">{r.label}</button>
    {/each}
  </div>
  {#if round && app.inReview}
    <div class="lab">Scenes in this review</div>
    <div class="chips">
      {#each round.scenes as s (s.id)}<span class="sc">Sc. {s.number} {s.place}</span>{/each}
      <span class="muted by">{#if editor}picked by {editor}{/if}{#if round.cut} · cut {round.cut}{/if}</span>
    </div>
  {/if}
  <label class="keep"><input type="checkbox" bind:checked={keep} data-testid="who-keep" /><span class="box" class:on={keep}>{#if keep}<Icon name="check" size={11} stroke={3.6} />{/if}</span>Remember me on this computer</label>
  <div class="foot">
    <span class="muted last">{#if who?.last}<button onclick={useLast}>Last here: {who.last.name} · {roleLabel(who.last.role)}</button>{/if}</span>
    {#if ask?.closable}<button class="btnx" onclick={() => (app.asking = null)}>Cancel</button>{/if}
    <button class="btnx pri" onclick={start} disabled={!named} data-testid="who-start">{app.inReview ? 'Start review' : 'Continue'} <span class="key">↩</span></button>
  </div>
</div>

<style>
  .scrim {
    position: absolute;
    inset: 0;
    z-index: 44;
    background: rgb(var(--night-rgb) / 0.55);
  }
  .who {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    z-index: 45;
    width: min(500px, 94%);
    max-height: 92vh;
    overflow-y: auto;
    box-sizing: border-box;
    padding: 28px;
    border-radius: var(--radius-sheet);
    background: var(--panel-solid);
    font-size: 13px;
  }
  .rv {
    display: inline-flex;
    align-items: center;
    height: 24px;
    padding: 0 10px;
    border-radius: var(--radius-pill);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    background: rgb(var(--accent-rgb) / 0.2);
    color: var(--link);
    box-shadow: inset 0 0 0 1px rgb(var(--accent-rgb) / 0.5);
  }
  h2 {
    margin: 14px 0 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  p {
    margin: 4px 0 0;
  }
  .lab {
    display: block;
    margin: 18px 0 8px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--text-5);
  }
  .inp {
    box-sizing: border-box;
    width: 100%;
    height: 42px;
    padding: 0 13px;
    border: 0;
    outline: none;
    border-radius: 12px;
    font: inherit;
    font-size: 15px;
    color: var(--text-strong);
    background: var(--glass-fill-3);
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.7);
    user-select: text;
    -webkit-user-select: text;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .rc {
    height: 30px;
    padding: 0 12px;
    border-radius: var(--radius-pill);
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-3);
    background: var(--glass-fill-3);
  }
  .rc.on {
    background: var(--glass-active);
    color: var(--text-strong);
    font-weight: 600;
  }
  .sc {
    height: 26px;
    padding: 0 10px;
    border-radius: 8px;
    display: inline-flex;
    align-items: center;
    font-size: 12px;
    color: var(--text-2);
    background: var(--glass-fill-3);
  }
  .by {
    font-size: 12px;
    margin-left: 4px;
  }
  .keep {
    position: relative;
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 20px;
    cursor: pointer;
  }
  .keep input {
    position: absolute;
    opacity: 0;
    width: 16px;
    height: 16px;
    margin: 0;
  }
  .box {
    flex: none;
    width: 16px;
    height: 16px;
    border-radius: 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-on-pill);
    box-shadow: inset 0 0 0 1.5px rgb(var(--veil-rgb) / 0.3);
  }
  .box.on {
    background: var(--accent);
    box-shadow: none;
  }
  .keep input:focus-visible + .box {
    outline: 2px solid var(--link);
    outline-offset: 2px;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 24px;
    padding-top: 18px;
    border-top: 1px solid var(--glass-outline);
  }
  .last {
    flex: 1;
    min-width: 0;
    font-size: 12px;
  }
  .last button:hover {
    color: var(--text-1);
  }
  .btnx {
    height: 38px;
    padding: 0 16px;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-strong);
    background: var(--glass-hover);
  }
  .btnx.pri {
    background: var(--accent);
    color: var(--text-on-pill);
  }
  .btnx:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .key {
    display: inline-flex;
    align-items: center;
    height: 19px;
    padding: 0 5px;
    border-radius: 5px;
    font-size: 10.5px;
    background: rgb(var(--night-rgb) / 0.16);
  }
</style>
