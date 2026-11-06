import { describe, expect, it } from 'vitest';
import { autoFadeOut } from './select';
import { scrubLimits, sourceAt, autoFadeToggle, fadeLevel, fadesAt, keepWindow, maxLen, MIN_TRIM_S, nearestIn, openWindow, partAt, partText, quickLen, quickOn, readout, reshape, reshapeCommands, snapTol, tOf, yOf, type OpenState, type Win } from './opened';
import type { SnapTarget } from './snap';

const sound = (over: Partial<OpenState> = {}): OpenState => ({ id: 'a', start_s: 10, len_s: 2, offset_s: 0, anchor: null, depth: 0, pan: 0, gain_db: 0, in_s: 1, asset_len_s: 5, fade_in_s: 0.02, fade_out_s: 0.3, auto_fade: true, ...over });

const target = (t: number): SnapTarget => ({ t, anchor: { line_id: 'l', word: null, edge: 'end' }, start: `Start ${t}`, end: `End ${t}` });

const line = (start_s: number, end_s: number) => ({ kind: 'line' as const, start_s, end_s, pieces: [{}] as never[] });

describe('the window', () => {
  it('shows a short source whole, with a little room around it', () => {
    expect(openWindow(sound())).toEqual({ t0: 8.75, len: 5.5 });
  });

  it('shows the sound and a second each side of a long source', () => {
    expect(openWindow(sound({ asset_len_s: 60, len_s: 30 }))).toEqual({ t0: 9, len: 32 });
  });

  it('leaves a source of unknown length to the sound', () => {
    expect(openWindow(sound({ asset_len_s: 0 }))).toEqual({ t0: 8.75, len: 3.5 });
  });

  it('stays while the sound is inside, and follows one that reached the edge', () => {
    const w: Win = { t0: 9, len: 4 };
    expect(keepWindow(w, sound({ asset_len_s: 60 }))).toBe(w);
    expect(keepWindow(w, sound({ asset_len_s: 60, len_s: 2.95 }))).toEqual({ t0: 9, len: 4.95 });
    expect(keepWindow(w, sound({ asset_len_s: 60, start_s: 9.05 }))).toEqual({ t0: 8.05, len: 4 });
  });

  it('maps time to height and back', () => {
    const w: Win = { t0: 10, len: 5 };
    expect(yOf(w, 500, 12)).toBe(200);
    expect(tOf(w, 500, 200)).toBe(12);
  });
});

describe('snapping in the opened view', () => {
  const w: Win = { t0: 0, len: 4 };

  it('is the spine’s 8 px, in the view’s time', () => {
    expect(snapTol(w, 400)).toBeCloseTo(0.08);
    expect(snapTol({ t0: 0, len: 40 }, 400)).toBeCloseTo(0.8);
  });

  it('takes the nearest target inside the distance', () => {
    const ts = [target(1), target(1.1), target(3)];
    expect(nearestIn(ts, 1.04, 0.08)?.t).toBe(1);
    expect(nearestIn(ts, 1.07, 0.08)?.t).toBe(1.1);
  });

  it('lets go outside the distance', () => {
    expect(nearestIn([target(1)], 1.09, 0.08)).toBeNull();
    expect(nearestIn([], 1, 0.08)).toBeNull();
  });
});

describe('trimming', () => {
  it('keeps the audio where it was when the start moves', () => {
    const r = reshape('in', sound(), 10.5);
    expect(r.patch).toMatchObject({ start_s: 10.5, in_s: 1.5, len_s: 1.5 });
  });

  it('stops the start where the source begins', () => {
    expect(reshape('in', sound(), 5).at).toBe(9);
    expect(reshape('in', sound(), 5).patch.in_s).toBe(0);
  });

  it('stops the start at the start of the scene', () => {
    const r = reshape('in', sound({ start_s: 0.5, in_s: 3 }), -2);
    expect(r.at).toBe(0);
    expect(r.patch).toMatchObject({ in_s: 2.5, len_s: 2.5 });
  });

  it('leaves the shortest length at either edge', () => {
    expect(reshape('in', sound(), 11.95).patch.len_s).toBeCloseTo(MIN_TRIM_S);
    expect(reshape('out', sound(), 10.05).patch.len_s).toBeCloseTo(MIN_TRIM_S);
  });

  it('never makes a sound shorter than the rule by trimming a shorter one', () => {
    expect(reshape('out', sound({ len_s: 0.1 }), 10.02).patch.len_s).toBeCloseTo(0.1);
  });

  it('stops the end where the source does', () => {
    expect(maxLen(sound())).toBe(4);
    expect(reshape('out', sound(), 30).patch.len_s).toBe(4);
  });

  it('runs on freely when the source length is unknown', () => {
    expect(reshape('out', sound({ asset_len_s: 0 }), 30).patch.len_s).toBe(20);
  });

  it('reports a snap only when the edge got there', () => {
    expect(reshape('out', sound(), 0, target(11.5)).target?.t).toBe(11.5);
    expect(reshape('out', sound(), 0, target(40)).target).toBeNull();
    expect(reshape('in', sound(), 0, target(2)).target).toBeNull();
  });

  it('the fades follow as the core makes them', () => {
    expect(fadesAt(sound(), 0.5).fade_out_s).toBe(autoFadeOut(0.5));
    expect(fadesAt(sound({ auto_fade: false, fade_out_s: 1 }), 1).fade_out_s).toBe(0.5);
    expect(fadesAt(sound({ auto_fade: false, fade_in_s: 1, fade_out_s: 1 }), 1)).toEqual({ fade_in_s: 0.5, fade_out_s: 0.5 });
  });

  it('sends the end as one trim', () => {
    const s = sound();
    expect(reshapeCommands('out', s, reshape('out', s, 11.5))).toEqual([{ cmd: 'trim_placement', placement_id: 'a', edge: 'end', delta_s: -0.5 }]);
  });

  it('sends the start as a trim that keeps the anchor', () => {
    const s = sound({ anchor: { line_id: 'x', word: null, edge: 'end' } });
    expect(reshapeCommands('in', s, reshape('in', s, 10.5))).toEqual([{ cmd: 'trim_placement', placement_id: 'a', edge: 'start', delta_s: 0.5 }]);
  });

  it('hangs a start trimmed onto a target on it', () => {
    const s = sound();
    const t = target(10.4);
    expect(reshapeCommands('in', s, reshape('in', s, 0, t))).toEqual([
      { cmd: 'trim_placement', placement_id: 'a', edge: 'start', delta_s: 0.4 },
      { cmd: 'move_sound', placement_id: 'a', anchor: t.anchor, offset_s: 0 },
    ]);
  });

  it('sends nothing when nothing changed', () => {
    const s = sound();
    expect(reshapeCommands('in', s, reshape('in', s, s.start_s))).toEqual([]);
    expect(reshapeCommands('out', s, reshape('out', s, s.start_s + s.len_s))).toEqual([]);
  });
});

describe('slipping', () => {
  it('slides the audio inside the window: down brings earlier audio in', () => {
    const s = sound();
    const r = reshape('slip', s, partAt('slip', s) + 0.5);
    expect(r.patch).toEqual({ in_s: 0.5 });
  });

  it('leaves place and length alone', () => {
    const s = sound();
    const r = reshape('slip', s, partAt('slip', s) - 1);
    expect(r.patch).toEqual({ in_s: 2 });
    expect(r.patch.start_s).toBeUndefined();
    expect(r.patch.len_s).toBeUndefined();
  });

  it('stays inside the source at both ends', () => {
    const s = sound();
    expect(reshape('slip', s, partAt('slip', s) + 9).patch.in_s).toBe(0);
    expect(reshape('slip', s, partAt('slip', s) - 9).patch.in_s).toBe(3);
  });

  it('has nowhere to go when the window fills the source', () => {
    const s = sound({ in_s: 0, len_s: 5 });
    expect(reshape('slip', s, partAt('slip', s) - 1).patch.in_s).toBe(0);
  });

  it('sends the new in point once', () => {
    const s = sound();
    expect(reshapeCommands('slip', s, reshape('slip', s, partAt('slip', s) + 0.25))).toEqual([{ cmd: 'slip_sound', placement_id: 'a', in_s: 0.75 }]);
    expect(reshapeCommands('slip', s, reshape('slip', s, partAt('slip', s)))).toEqual([]);
  });
});

describe('moving the middle', () => {
  it('moves the whole sound, not the audio inside it', () => {
    expect(reshape('move', sound(), 12).patch).toEqual({ start_s: 12 });
  });

  it('stops at the start of the scene', () => {
    expect(reshape('move', sound(), -3).at).toBe(0);
  });

  it('hangs on a target, and goes free without one', () => {
    const s = sound({ anchor: { line_id: 'x', word: null, edge: 'start' } });
    const t = target(12);
    expect(reshapeCommands('move', s, reshape('move', s, 0, t))).toEqual([{ cmd: 'move_sound', placement_id: 'a', anchor: t.anchor, offset_s: 0 }]);
    expect(reshapeCommands('move', s, reshape('move', s, 12.5))).toEqual([{ cmd: 'move_sound', placement_id: 'a', at_s: 12.5, glue: true }]);
  });

  it('keeps the anchor on a key press', () => {
    const s = sound({ anchor: { line_id: 'x', word: null, edge: 'start' }, offset_s: 0.2 });
    expect(reshapeCommands('move', s, reshape('move', s, 10.1), true)).toEqual([{ cmd: 'move_sound', placement_id: 'a', offset_s: 0.3 }]);
  });

  it('takes an anchor at the same time, but not the one it has', () => {
    const t = target(10);
    expect(reshapeCommands('move', sound(), reshape('move', sound(), 0, t))).toHaveLength(1);
    const s = sound({ anchor: t.anchor });
    expect(reshapeCommands('move', s, reshape('move', s, 0, t))).toEqual([]);
  });
});

describe('fades', () => {
  it('sets the fade in from the corner dot, up to the fade out', () => {
    const s = sound();
    expect(reshape('fadeIn', s, 10.5).patch).toEqual({ fade_in_s: 0.5 });
    expect(reshape('fadeIn', s, 30).patch).toEqual({ fade_in_s: 1.7 });
    expect(reshape('fadeIn', s, 5).patch).toEqual({ fade_in_s: 0 });
  });

  it('a fade in leaves the auto fade on', () => {
    const s = sound();
    expect(reshape('fadeIn', s, 10.5).patch.auto_fade).toBeUndefined();
    expect(reshapeCommands('fadeIn', s, reshape('fadeIn', s, 10.5))).toEqual([{ cmd: 'set_sound_fades', placement_id: 'a', fade_in_s: 0.5 }]);
  });

  it('setting the fade out by hand turns auto off', () => {
    const s = sound();
    const r = reshape('fadeOut', s, 11);
    expect(r.patch).toMatchObject({ fade_out_s: 1, auto_fade: false });
    expect(reshapeCommands('fadeOut', s, r)).toEqual([{ cmd: 'set_sound_fades', placement_id: 'a', fade_out_s: 1 }]);
  });

  it('the fade out leaves a tenth of the sound and pushes the fade in back', () => {
    const r = reshape('fadeOut', sound({ fade_in_s: 1 }), 0);
    expect(r.patch.fade_out_s).toBeCloseTo(1.8);
    expect(r.patch.fade_in_s).toBeCloseTo(0.2);
  });

  it('a press that goes nowhere leaves the auto fade alone', () => {
    const s = sound();
    expect(reshapeCommands('fadeOut', s, reshape('fadeOut', s, partAt('fadeOut', s)))).toEqual([]);
  });

  it('shapes the level', () => {
    const s = sound({ fade_in_s: 1, fade_out_s: 0.5 });
    expect(fadeLevel(s, 10.5)).toBe(0.5);
    expect(fadeLevel(s, 11)).toBe(1);
    expect(fadeLevel(s, 11.75)).toBe(0.5);
    expect(fadeLevel(s, 9)).toBe(0);
    expect(fadeLevel(sound({ fade_in_s: 0, fade_out_s: 0 }), 11)).toBe(1);
  });
});

describe('auto fade', () => {
  it('follows the rule: 30 % up to 0.3 s, and 2 s for a bed over 8 s', () => {
    expect(autoFadeOut(0.5)).toBeCloseTo(0.15);
    expect(autoFadeOut(3)).toBe(0.3);
    expect(autoFadeOut(8)).toBe(0.3);
    expect(autoFadeOut(8.1)).toBe(2);
  });

  it('turned on, the fade out follows the length again', () => {
    const r = autoFadeToggle(sound({ auto_fade: false, fade_out_s: 1.5 }));
    expect(r.patch).toEqual({ auto_fade: true, fade_out_s: 0.3 });
    expect(r.cmd).toEqual({ cmd: 'set_sound_fades', placement_id: 'a', auto: true });
  });

  it('turned off, the fade stays as it was', () => {
    const r = autoFadeToggle(sound());
    expect(r.patch).toEqual({ auto_fade: false, fade_out_s: 0.3 });
    expect(r.cmd).toEqual({ cmd: 'set_sound_fades', placement_id: 'a', auto: false });
  });
});

describe('quick buttons', () => {
  const lines = [line(9, 10.5), line(11, 12.5), line(14, 15)];

  it('End with line: the end of the line it starts in', () => {
    expect(quickLen('line', sound(), lines)).toBe(0.5);
  });

  it('Until next line: the start of the next line', () => {
    expect(quickLen('next', sound(), lines)).toBe(1);
  });

  it('a line just starting counts as the one it is on', () => {
    expect(quickLen('next', sound({ start_s: 11 }), lines)).toBe(3);
    expect(quickLen('line', sound({ start_s: 11 }), lines)).toBe(1.5);
  });

  it('Full: the rest of the source', () => {
    expect(quickLen('full', sound(), lines)).toBe(4);
  });

  it('with no line left, the rest of the source', () => {
    expect(quickLen('line', sound({ start_s: 20 }), lines)).toBe(4);
    expect(quickLen('next', sound({ start_s: 20 }), lines)).toBe(4);
  });

  it('stays inside the source', () => {
    expect(quickLen('next', sound({ in_s: 4.5, len_s: 0.5, asset_len_s: 5 }), [line(14, 15)])).toBe(0.5);
  });

  it('skips lines with no audio', () => {
    expect(quickLen('line', sound(), [{ kind: 'line', start_s: 9, end_s: 10.5, pieces: [] }, line(11, 12.5)])).toBe(2.5);
  });

  it('copes with lines out of order', () => {
    expect(quickLen('next', sound(), [line(14, 15), line(11, 12.5)])).toBe(1);
  });

  it('keeps the length of a sound with no known source and no line', () => {
    expect(quickLen('full', sound({ asset_len_s: 0 }), [])).toBe(2);
  });

  it('lights up the one the sound already matches', () => {
    expect(quickOn('line', sound({ len_s: 0.5 }), lines)).toBe(true);
    expect(quickOn('next', sound({ len_s: 0.5 }), lines)).toBe(false);
    expect(quickOn('full', sound({ in_s: 0, len_s: 5 }), lines)).toBe(true);
  });
});

describe('what the view reads', () => {
  it('names length, source and fades', () => {
    expect(readout(sound())).toBe('2.0 of 5.0 s · fade in 0.02 · fade out 0.3 auto');
    expect(readout(sound({ asset_len_s: 0, auto_fade: false }))).toBe('2.0 s · fade in 0.02 · fade out 0.3');
  });

  it('says what a part did', () => {
    const s = sound();
    expect(partText('out', s, reshape('out', s, 0, target(11)))).toContain('End 11');
    expect(partText('out', s, reshape('out', s, 40))).toContain('Full length');
    expect(partText('fadeIn', s, reshape('fadeIn', s, 10.5))).toBe('Fade in 0.50 s');
    expect(partText('slip', s, reshape('slip', s, partAt('slip', s) + 0.5))).toContain('0.50');
    expect(partText('move', s, reshape('move', s, 0, target(11)))).toBe('Start 11');
  });
});

describe('the scrubber', () => {
  const sound = { start_s: 10, len_s: 2, in_s: 1, asset_len_s: 5 };

  it('reads a scene time as a place in the file', () => {
    expect(sourceAt(sound, 10)).toBe(1);
    expect(sourceAt(sound, 9)).toBe(0);
    expect(sourceAt(sound, 8)).toBe(0);
    expect(sourceAt(sound, 30)).toBe(5);
  });

  it('comes round on a loop', () => {
    expect(sourceAt({ ...sound, looped: true }, 15.5)).toBeCloseTo(1.5);
  });

  it('reaches the whole source, or a loop’s length', () => {
    expect(scrubLimits(sound)).toEqual({ from: 9, to: 14 });
    expect(scrubLimits({ ...sound, looped: true })).toEqual({ from: 9, to: 12 });
  });
});
