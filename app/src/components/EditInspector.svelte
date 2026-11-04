<script lang="ts">
  // The Edit inspector (I), Logic-style: the selected clip, its anchor and its lane.
  import { app } from '../lib/app.svelte';
  import { charColor, charLabel } from '../lib/colors';
  import { db, laneLabel, laneRgb, mixOf, panLabel, placementLane, setMix } from '../lib/lanes';
  import { rgba } from '../theme';

  const v = $derived(app.view);
  const sel = $derived(app.editSel);
  const sound = $derived(sel?.kind === 'sound' ? app.edit?.placements.find((p) => p.id === sel.id) : undefined);
  const line = $derived(sel?.kind === 'line' ? app.lines.find((l) => l.line_id === sel.id) : undefined);
  const tone = $derived(sel?.kind === 'roomtone' ? app.edit?.room_tone : null);
  const part = $derived(sel?.kind === 'line' ? (sel.part ?? 0) : 0);
  const partView = $derived(line?.parts.find((p) => p.index === part));

  async function fades(fin: number, fout: number) {
    if (!line) return;
    await app.cmd({ cmd: 'set_clip_fades', line_id: line.line_id, fade_in_s: fin, fade_out_s: fout });
  }
  const laneKey = $derived(sound ? placementLane(sound) : line ? (line.character ?? '') : tone ? 'roomtone' : null);
  const mix = $derived(laneKey ? mixOf(laneKey) : null);

  const anchorItem = $derived(sound?.anchor ? v?.items.find((i) => i.line_id === sound.anchor!.line_id) : undefined);
  const anchorWord = $derived.by(() => {
    const a = sound?.anchor;
    if (!a || !anchorItem || a.word == null) return null;
    return anchorItem.tokens[a.word] ?? anchorItem.words.find((w) => w.script_idx === a.word)?.text ?? null;
  });

  async function patch(p: Record<string, unknown>) {
    if (!sound) return;
    await app.cmd({ cmd: 'update_placement', placement_id: sound.id, patch: p });
    await app.loadEdit();
  }

  /** Typed values accept "−4,5", "-4.5 dB" or "40 ms". */
  function num(e: Event): number | null {
    const n = parseFloat((e.currentTarget as HTMLInputElement).value.replace('−', '-').replace(',', '.'));
    return Number.isFinite(n) ? n : null;
  }
  const signed = (v: number, d = 1) => `${v < 0 ? '−' : ''}${Math.abs(v).toFixed(d)}`;

  async function follow(on: boolean) {
    if (!sound) return;
    if (!on) {
      await app.cmd({ cmd: 'unanchor_placement', placement_id: sound.id });
      return app.loadEdit();
    }
    // anchor to the word that starts nearest the sound
    let best: { line_id: string; word: number | null; d: number } | null = null;
    for (const it of v?.items ?? []) {
      if (it.kind === 'spacer') {
        const d = Math.abs(it.start_s - sound.start_s);
        if (!best || d < best.d) best = { line_id: it.line_id, word: null, d };
      }
      for (const w of it.words) {
        if (w.deleted || w.script_idx == null) continue;
        const d = Math.abs(w.start_s - sound.start_s);
        if (!best || d < best.d) best = { line_id: it.line_id, word: w.script_idx, d };
      }
    }
    if (!best) return;
    const at = (v?.items.find((i) => i.line_id === best!.line_id)?.words.find((w) => w.script_idx === best!.word)?.start_s ?? v?.items.find((i) => i.line_id === best!.line_id)?.start_s) ?? sound.start_s;
    await patch({ anchor: { line_id: best.line_id, word: best.word, edge: 'start' }, offset_s: +(sound.start_s - at).toFixed(3) });
  }

  async function setTone(gain: number) {
    if (!v) return;
    const own = app.edit?.room_tone?.own;
    if (own) await app.cmd({ cmd: 'set_scene_tone', scene_id: v.scene.id, tone: { asset_id: own, gain_db: gain } });
    else await app.cmd({ cmd: 'set_room_tone', location_id: v.location_id, tone: { ...v.room_tone, gain_db: gain } });
    await app.loadEdit();
  }

  /** Lane meter: the output level while a clip of the lane sounds at the playhead. */
  const laneActive = $derived.by(() => {
    if (!laneKey || !app.playing) return false;
    const t = app.pos;
    if (laneKey === 'roomtone') return !!app.edit?.room_tone;
    if (line) return app.lines.some((l) => l.character === laneKey && t >= l.start_s && t < l.end_s);
    return (app.edit?.placements ?? []).some((p) => placementLane(p) === laneKey && t >= p.start_s && t < p.start_s + p.len_s);
  });
  const outH = $derived(Math.round(Math.min(1, Math.sqrt(app.level)) * 100));
  const laneH = $derived(laneActive && !mix?.mute ? Math.round(outH * Math.min(1, Math.pow(10, (mix?.volume_db ?? 0) / 20))) : 0);
</script>

<aside class="glass insp" aria-label="Inspector">
  {#if sound}
    <div class="label">Clip</div>
    <div class="ttl ellip">{sound.name.replace(/\.[a-z0-9]+$/i, '')}</div>
    <div class="muted ellip src">{sound.name} · {sound.origin}</div>
    <label class="ir"><span class="muted">Gain</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={signed(sound.gain_db)} onchange={(e) => { const n = num(e); if (n != null) void patch({ gain_db: n }); }} aria-label="Gain (dB)" />dB</span></label>
    <label class="ir"><span class="muted">Fade in</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={Math.round(sound.fade_in_s * 1000)} onchange={(e) => { const n = num(e); if (n != null) void patch({ fade_in_s: Math.max(0, n) / 1000 }); }} aria-label="Fade in (ms)" />ms</span></label>
    <label class="ir"><span class="muted">Fade out{sound.auto_fade ? ' · auto' : ''}</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={Math.round(sound.fade_out_s * 1000)} onchange={(e) => { const n = num(e); if (n != null) void patch({ fade_out_s: Math.max(0, n) / 1000 }); }} aria-label="Fade out (ms)" />ms</span></label>
    <label class="ir"><span class="muted">Pan</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={signed(sound.pan * 100, 0)} onchange={(e) => { const n = num(e); if (n != null) void patch({ pan: n / 100 }); }} aria-label="Pan (−100 left to 100 right)" />L/R</span></label>
    {#if sound.lane === 'sfx' || sound.lane === 'ambience'}
      <label class="ir"><span class="muted">Depth</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={Math.round(sound.depth * 100)} onchange={(e) => { const n = num(e); if (n != null) void patch({ depth: n / 100 }); }} aria-label="Depth (0 close to 100 far)" />%</span></label>
    {/if}
    <label class="ir"><span class="muted">Curve</span>
      <select class="iv" value={sound.fade_curve} onchange={(e) => patch({ fade_curve: (e.currentTarget as HTMLSelectElement).value })} aria-label="Fade curve">
        <option value="equal_power">Equal power</option>
        <option value="linear">Linear</option>
      </select>
    </label>
    <div class="hr"></div>
    <div class="label">Anchor</div>
    {#if sound.anchor && anchorItem}
      <div class="ir"><span class="muted">To</span>
        <span class="ellip to">
          {#if anchorItem.kind === 'line'}<span class="dot" style="background: {rgba(charColor(v, anchorItem.character).rgb, 1)}"></span>{charLabel(v, anchorItem.character)}{anchorWord ? ` · “${anchorWord}”` : ''}
          {:else}<span class="script">{anchorItem.text}</span>{/if}
        </span>
      </div>
      <label class="ir"><span class="muted">Offset</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={(sound.offset_s >= 0 ? '+' : '') + signed(sound.offset_s, 2)} onchange={(e) => { const n = num(e); if (n != null) void patch({ offset_s: n }); }} aria-label="Offset (s)" />s</span></label>
    {:else}
      <div class="ir"><span class="muted">At</span><span class="iv tn">{sound.start_s.toFixed(2)} s</span></div>
    {/if}
    <label class="ir"><span class="muted">Follows dialogue</span><input type="checkbox" checked={sound.anchored} onchange={(e) => follow((e.currentTarget as HTMLInputElement).checked)} /></label>
    <label class="ir"><span class="muted">Pin to time</span><input type="checkbox" checked={!sound.anchored} onchange={(e) => follow(!(e.currentTarget as HTMLInputElement).checked)} /></label>
  {:else if line}
    <div class="label">Clip</div>
    <div class="ttl ellip">{charLabel(v, line.character)} · S{v?.scene.number}.{line.number}</div>
    <div class="muted src script two">{line.text}</div>
    <div class="ir"><span class="muted">Take</span><span class="iv tn">T{(line.take_index ?? 0) + 1} of {line.takes.length}</span></div>
    <label class="ir"><span class="muted">Gain</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={signed(line.gain_db)} onchange={(e) => { const n = num(e); if (n != null) void app.cmd({ cmd: 'set_clip_gain', line_id: line!.line_id, gain_db: n }); }} aria-label="Gain (dB)" />dB</span></label>
    {#if line.parts.length > 1}<div class="ir"><span class="muted">Part</span><span class="iv tn">{part + 1} of {line.parts.length}</span></div>{/if}
    <label class="ir"><span class="muted">Fade in</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={Math.round(line.fade_in_s * 1000)} onchange={(e) => { const n = num(e); if (n != null) void fades(Math.max(0, n) / 1000, line!.fade_out_s); }} aria-label="Fade in (ms)" />ms</span></label>
    <label class="ir"><span class="muted">Fade out</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={Math.round(line.fade_out_s * 1000)} onchange={(e) => { const n = num(e); if (n != null) void fades(line!.fade_in_s, Math.max(0, n) / 1000); }} aria-label="Fade out (ms)" />ms</span></label>
    <label class="ir"><span class="muted">Muted{line.parts.length > 1 ? ' (part)' : ''}</span><input type="checkbox" checked={!!partView?.muted} onchange={(e) => app.cmd({ cmd: 'set_clip_muted', line_id: line!.line_id, part, muted: (e.currentTarget as HTMLInputElement).checked })} /></label>
    <div class="hr"></div>
    <div class="label">Timing</div>
    <div class="ir"><span class="muted">{line.pinned ? 'At' : 'Pause before'}</span><span class="iv tn">{(line.pinned ? line.start_s : line.gap_before_s).toFixed(2)} s</span></div>
    <label class="ir"><span class="muted">Locked to script</span><input type="checkbox" checked={!line.pinned} onchange={(e) => app.cmd({ cmd: 'pin_clip', line_id: line!.line_id, at_s: (e.currentTarget as HTMLInputElement).checked ? null : line!.start_s })} /></label>
    <p class="muted hint">{line.pinned ? 'Free in time: the clip stays where you put it and nothing ripples around it.' : 'Follows the script order; moving it changes the pause before it.'}</p>
  {:else if tone}
    <div class="label">Room tone</div>
    <div class="ttl ellip">{tone.source || 'Room tone'}</div>
    <div class="muted src">Loops every {tone.loop_s.toFixed(1)} s · {v?.scene.place}</div>
    <label class="ir"><span class="muted">Gain</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={signed(tone.gain_db)} onchange={(e) => { const n = num(e); if (n != null) void setTone(n); }} aria-label="Room tone gain (dB)" />dB</span></label>
    <p class="muted hint">Choose the room tone passage on the Options page (room tone settings).</p>
  {:else}
    <div class="label">Clip</div>
    <p class="muted hint">Select a clip to see its gain, fades and anchor.</p>
  {/if}

  {#if laneKey && mix}
    <div class="hr"></div>
    <div class="label">Lane · {laneLabel(laneKey)}</div>
    <label class="ir"><span class="muted">Volume</span><span class="u"><input class="iv" type="text" inputmode="decimal" value={signed(mix.volume_db)} onchange={(e) => { const n = num(e); if (n != null) void setMix(laneKey!, { volume_db: n }); }} aria-label="Lane volume (dB)" title={db(mix.volume_db)} />dB</span></label>
    <label class="ir"><span class="muted">Pan</span><span class="u"><input type="range" min="-1" max="1" step="0.05" value={mix.pan} onchange={(e) => setMix(laneKey!, { pan: +(e.currentTarget as HTMLInputElement).value })} aria-label="Lane pan" class="pan" /><span class="iv tn">{panLabel(mix.pan)}</span></span></label>
  {/if}
  <div class="meters" aria-label="Meters">
    {#if laneKey}<div class="m"><div class="track"><span style="height: {laneH}%; background: linear-gradient(to top, {rgba(laneRgb(laneKey), 1)}, {rgba(laneRgb(laneKey), 0.45)})"></span></div><span class="muted ml">{laneLabel(laneKey)}</span></div>{/if}
    <div class="m"><div class="track"><span style="height: {outH}%; background: linear-gradient(to top, rgb(var(--room-rgb)), var(--text-strong))"></span></div><span class="muted ml">Out</span></div>
  </div>
</aside>

<style>
  .insp {
    position: absolute;
    top: 138px;
    bottom: 16px;
    left: 20px;
    width: 252px;
    box-sizing: border-box;
    border-radius: 22px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bar);
    z-index: 8;
    overflow-y: auto;
  }
  .ttl {
    font-size: 15px;
    font-weight: 600;
  }
  .src {
    font-size: 11.5px;
    margin-bottom: 6px;
  }
  .two {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .ir {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    min-height: 28px;
    font-size: 12.5px;
  }
  .iv {
    font: inherit;
    font-variant-numeric: tabular-nums;
    padding: 3px 8px;
    border-radius: 7px;
    background: var(--glass-fill-3);
    font-size: 12px;
    color: inherit;
    border: 0;
    outline: none;
    width: 74px;
    text-align: right;
    box-sizing: border-box;
  }
  select.iv {
    width: auto;
    text-align: left;
  }
  span.iv {
    width: auto;
  }
  .u {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-4);
  }
  .u .iv {
    width: 64px;
  }
  .to {
    font-size: 12px;
    max-width: 150px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .hr {
    height: 1px;
    background: var(--glass-outline);
    margin: 8px 0;
  }
  .hint {
    font-size: 12px;
    margin: 4px 0;
  }
  .pan {
    width: 70px;
    accent-color: var(--button-light);
  }
  input[type='checkbox'] {
    accent-color: var(--button-light);
  }
  .meters {
    margin-top: auto;
    display: flex;
    align-items: flex-end;
    gap: 10px;
    height: 120px;
    min-height: 120px;
    padding: 10px;
    border-radius: 14px;
    background: var(--lcd);
  }
  .m {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    height: 100%;
  }
  .track {
    flex: 1;
    width: 6px;
    border-radius: 3px;
    background: var(--glass-fill-4);
    position: relative;
  }
  .track span {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    border-radius: 3px;
    transition: height 0.06s linear;
  }
  .ml {
    font-size: 10px;
  }
  @media (max-width: 1100px) {
    .insp {
      display: none;
    }
  }
</style>
