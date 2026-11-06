<script lang="ts">
  // One note: who, where, what they said, and what's become of it. The same card in the review
  // margin and in the Notes tab of the scene sounds panel.
  import { app } from '../lib/app.svelte';
  import { fmt } from '../lib/format';
  import { day, quoted, roleLabel, tagLabel, TAGS, type Note, type NoteTag } from '../lib/review';
  import Icon from './Icon.svelte';

  let { note, compact = false, onjump, onbucket }: { note: Note; compact?: boolean; onjump?: (n: Note) => void; onbucket?: (n: Note) => void } = $props();

  let replying = $state(false);
  let reply = $state('');
  let editing = $state(false);
  let text = $state('');

  const resolved = $derived(note.status === 'resolved');
  /** The editing pages say who they are the first time they answer. */
  const as = (f: () => void) => app.whoFirst(f);

  function toggle() {
    as(() => void app.note(resolved ? 'reopen_note' : 'resolve_note', { note: note.id }));
  }

  async function sendReply() {
    const t = reply.trim();
    if (!t) return;
    replying = false;
    reply = '';
    as(() => void app.note('reply_note', { note: note.id, text: t }));
  }

  function saveEdit() {
    const t = text.trim();
    editing = false;
    if (t && t !== note.text) void app.note('edit_note', { note: note.id, text: t });
  }

  function retag(tag: NoteTag | null) {
    if (tag !== note.tag) void app.note('tag_note', { note: note.id, tag });
  }

  function fieldKey(e: KeyboardEvent, send: () => void, cancel: () => void) {
    e.stopPropagation();
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      send();
    } else if (e.key === 'Escape') cancel();
  }

  const focus = (el: HTMLElement) => el.focus();
</script>

<article class="nc" class:done={resolved} class:compact data-testid="note" data-note={note.id} data-status={note.status}>
  <div class="top">
    <span class="av" class:me={note.mine} title="{note.author.name} · {roleLabel(note.author.role)}">{note.author.initials}</span>
    <span class="who ellip" title={roleLabel(note.author.role)}>{note.author.name}</span>
    <button class="tc tn" title={note.detached ? 'Where it last was' : 'Go to this spot'} disabled={note.detached} onclick={() => onjump?.(note)} data-testid="note-time">{note.time}</button>
    <span class="grow"></span>
    {#if note.tag}<span class="tag" data-testid="note-tag">{tagLabel(note.tag)}</span>{/if}
    <button class="chk" class:on={resolved} role="checkbox" aria-checked={resolved} aria-label={resolved ? 'Reopen this note' : 'Resolve this note'} title={resolved ? 'Reopen' : 'Resolve'} onclick={toggle} data-testid="note-resolve">{#if resolved}<Icon name="check" size={11} stroke={3} />{/if}</button>
  </div>
  {#if note.quote}<div class="quote script ellip" title={note.quote}>{#if note.speaker}<span class="spk">{note.speaker}</span>{/if}{quoted(note)}</div>{/if}
  {#if editing}
    <textarea class="fieldx" aria-label="Note" bind:value={text} use:focus onkeydown={(e) => fieldKey(e, saveEdit, () => (editing = false))} onblur={saveEdit}></textarea>
    <div class="tags">
      {#each TAGS as t (t.id)}<button class="tg" class:on={note.tag === t.id} onmousedown={(e) => e.preventDefault()} onclick={() => retag(note.tag === t.id ? null : t.id)}>{t.label}</button>{/each}
    </div>
  {:else}
    <div class="text" data-testid="note-text">{note.text}</div>
  {/if}
  {#if note.detached}
    <div class="hint warn" data-testid="note-detached">Detached: this is no longer in the cut. It was at {fmt(note.written_s)}.</div>
  {:else if note.changed}
    <div class="hint" data-testid="note-changed">Changed since note</div>
  {/if}
  {#if note.resolved && resolved}
    <div class="hint" data-testid="note-done"><Icon name="check" size={11} stroke={2.6} /> Done by {note.resolved.by.name} · {day(note.resolved.at)}</div>
  {:else if note.status === 'reopened' && note.reopened}
    <div class="hint" data-testid="note-reopened">Reopened by {note.reopened.by.name} · {day(note.reopened.at)}</div>
  {/if}
  {#each note.replies as r (r.id)}
    <div class="reply" data-testid="note-reply"><span class="av small">{r.author.initials}</span><span class="rt"><b>{r.author.name}</b> {r.text}</span></div>
  {/each}
  {#if replying}
    <textarea class="fieldx" aria-label="Reply" placeholder="Reply" bind:value={reply} use:focus onkeydown={(e) => fieldKey(e, sendReply, () => (replying = false))} data-testid="reply-field"></textarea>
  {:else}
    <div class="acts">
      <button onclick={() => (replying = true)} data-testid="note-reply-open">Reply</button>
      {#if note.mine && !editing}<button onclick={() => { text = note.text; editing = true; }} data-testid="note-edit">Edit</button>{/if}
      {#if onbucket && (note.tag === 'sound' || note.tag === 'music')}<button onclick={() => onbucket?.(note)} data-testid="note-bucket">Find in bucket</button>{/if}
      {#if compact && note.round_number}<span class="muted rd">Round {note.round_number}</span>{/if}
    </div>
  {/if}
</article>

<style>
  .nc {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13.5px;
    line-height: 1.4;
    user-select: text;
    -webkit-user-select: text;
  }
  .nc.done > :not(.top, .acts, .hint) {
    opacity: 0.55;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .grow {
    flex: 1;
  }
  .av {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    font-weight: 700;
    background: var(--glass-active);
    color: var(--text-strong);
  }
  .av.me {
    background: var(--accent);
    color: var(--text-on-pill);
  }
  .av.small {
    width: 18px;
    height: 18px;
    font-size: 8px;
  }
  .who {
    font-size: 12.5px;
    font-weight: 600;
    min-width: 0;
  }
  .tc {
    flex: none;
    font-size: 11px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 6px;
    background: rgb(var(--accent-rgb) / 0.18);
    color: var(--link);
  }
  .tc:disabled {
    cursor: default;
    background: var(--glass-fill-3);
    color: var(--text-4);
  }
  .tag {
    flex: none;
    height: 20px;
    padding: 0 8px;
    border-radius: var(--radius-pill);
    display: inline-flex;
    align-items: center;
    font-size: 11px;
    font-weight: 600;
    background: var(--glass-fill-4);
    color: var(--text-3);
  }
  .chk {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: inset 0 0 0 1.5px rgb(var(--veil-rgb) / 0.3);
    color: var(--text-on-pill);
  }
  .chk:hover {
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .chk.on {
    background: var(--accent);
    box-shadow: none;
  }
  .quote {
    font-size: 12.5px;
    color: var(--text-3);
    padding-left: 9px;
    box-shadow: inset 2px 0 0 var(--glass-active);
  }
  .spk {
    margin-right: 5px;
    font-family: var(--font-ui, inherit);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text-4);
  }
  .text {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: var(--link);
  }
  .hint.warn {
    color: var(--warn);
  }
  .reply {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .rt {
    font-size: 12.5px;
    color: var(--text-3);
    overflow-wrap: anywhere;
  }
  .rt b {
    font-weight: 600;
    color: var(--text-2);
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 16px;
    font-size: 11.5px;
    color: var(--text-4);
  }
  .acts button:hover {
    color: var(--text-1);
  }
  .rd {
    margin-left: auto;
  }
  .fieldx {
    box-sizing: border-box;
    width: 100%;
    min-height: 44px;
    resize: none;
    border: 0;
    outline: none;
    border-radius: 12px;
    padding: 9px 11px;
    font: inherit;
    font-size: 13px;
    color: var(--text-strong);
    background: var(--glass-fill-3);
    box-shadow: inset 0 0 0 1.5px rgb(var(--accent-rgb) / 0.7);
  }
  .tags {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
  }
  .tg {
    height: 22px;
    padding: 0 8px;
    border-radius: var(--radius-pill);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-3);
    box-shadow: inset 0 0 0 1px var(--glass-outline);
  }
  .tg.on {
    background: rgb(var(--accent-rgb) / 0.22);
    color: var(--text-strong);
    box-shadow: none;
  }
</style>
