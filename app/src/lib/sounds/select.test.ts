import { describe, expect, it } from 'vitest';
import type { SnapTarget } from './snap';
import { announce, autoFadeOut, ballsInBand, bandSelect, clickSelect, dbOfRing, dbText, depthCommands, gainCommands, groupDelta, moveCommands, normRect, oneStep, panCommands, panText, pressSelect, removeCommands, ringAt, ringOf, timeDrag, toggleHidden, zone, type SoundState } from './select';

const s = (id: string, over: Partial<SoundState> = {}): SoundState => ({ id, start_s: 5, len_s: 1, offset_s: 0, anchor: null, depth: 0.5, pan: 0, gain_db: 0, ...over });
const target: SnapTarget = { t: 6, anchor: { line_id: 'l', word: 2, edge: 'end' }, start: 'x', end: 'y' };

describe('selection', () => {
  it('a click selects one sound', () => {
    expect(clickSelect(['a', 'b'], 'c', false)).toEqual(['c']);
  });

  it('⇧ click adds and takes out', () => {
    expect(clickSelect(['a'], 'b', true)).toEqual(['a', 'b']);
    expect(clickSelect(['a', 'b'], 'a', true)).toEqual(['b']);
  });

  it('pressing a selected ball keeps the group for the drag', () => {
    expect(pressSelect(['a', 'b'], 'a')).toEqual(['a', 'b']);
    expect(pressSelect(['a', 'b'], 'c')).toEqual(['c']);
  });

  it('the band finds what it touches, in any drag direction', () => {
    const balls = new Map([
      ['in', { x: 50, y: 50, r: 10 }],
      ['edge', { x: 105, y: 50, r: 10 }],
      ['corner', { x: 108, y: 108, r: 10 }],
      ['out', { x: 200, y: 50, r: 10 }],
    ]);
    expect(ballsInBand(balls, normRect(100, 100, 0, 0))).toEqual(['in', 'edge']);
  });

  it('a band replaces the selection; with ⇧ it adds to it', () => {
    expect(bandSelect(['a'], ['b'], false)).toEqual(['b']);
    expect(bandSelect(['a', 'b'], ['b', 'c'], true)).toEqual(['a', 'b', 'c']);
  });
});

describe('moving a group', () => {
  it('stops the whole group at a limit, keeping the offsets', () => {
    expect(groupDelta([0.2, 0.8], 0.5, 0, 1)).toBeCloseTo(0.2);
    expect(groupDelta([0.2, 0.8], -0.5, 0, 1)).toBeCloseTo(-0.2);
    expect(groupDelta([0.2, 0.8], 0.1, 0, 1)).toBe(0.1);
  });

  it('a value past the limit does not stop the others coming back', () => {
    expect(groupDelta([-30, -10], 5, -24, 6)).toBe(5);
    expect(groupDelta([-30, -10], -5, -24, 6)).toBe(0);
    expect(groupDelta([-30, 8], 5, -24, 6)).toBe(0);
  });

  it('has no group to move with nothing selected', () => {
    expect(groupDelta([], 1, 0, 1)).toBe(0);
  });

  it('follows the grabbed ball, snap included', () => {
    const r = timeDrag([s('a'), s('b', { start_s: 7 })], 'a', 5.9, target);
    expect(r.delta).toBeCloseTo(1);
    expect(r.target).toBe(target);
  });

  it('moves freely when nothing snapped', () => {
    const r = timeDrag([s('a')], 'a', 5.4, null);
    expect(r.delta).toBeCloseTo(0.4);
    expect(r.target).toBe(null);
  });

  it('stops at the start of the scene, and a snap past it is off', () => {
    const r = timeDrag([s('a', { start_s: 5 }), s('b', { start_s: 1 })], 'a', 0, { ...target, t: 2 });
    expect(r.delta).toBeCloseTo(-1);
    expect(r.target).toBe(null);
  });

  it('knows nothing of a ball that is gone', () => {
    expect(timeDrag([s('a')], 'gone', 1, null)).toEqual({ delta: 0, target: null });
  });
});

describe('what a gesture commits', () => {
  it('hangs the grabbed sound on its target', () => {
    expect(moveCommands([s('a')], 1, 'a', target)).toEqual([{ cmd: 'move_sound', placement_id: 'a', anchor: target.anchor, offset_s: 0 }]);
  });

  it('leaves a sound with its glue off free, also on a target', () => {
    expect(moveCommands([s('a', { unglued: true })], 1, 'a', target)).toEqual([{ cmd: 'move_sound', placement_id: 'a', at_s: 6, glue: true }]);
  });

  it('asks for glue where it lands when nothing snapped', () => {
    const anchored = s('a', { anchor: { line_id: 'l', word: null, edge: 'start' } });
    expect(moveCommands([anchored], 0.25, 'a', null)).toEqual([{ cmd: 'move_sound', placement_id: 'a', at_s: 5.25, glue: true }]);
  });

  it('keeps the others at their distance, anchored ones on their line', () => {
    const anchored = s('b', { anchor: { line_id: 'l', word: null, edge: 'start' }, offset_s: 0.2 });
    const c = moveCommands([s('a'), anchored, s('c', { start_s: 9 })], 1, 'a', target);
    expect(c[1]).toEqual({ cmd: 'move_sound', placement_id: 'b', offset_s: 1.2 });
    expect(c[2]).toEqual({ cmd: 'move_sound', placement_id: 'c', at_s: 10, glue: true });
  });

  it('a nudge with nothing grabbed keeps every anchor', () => {
    const anchored = s('b', { anchor: { line_id: 'l', word: null, edge: 'start' } });
    expect(moveCommands([anchored], 0.1, null, null)).toEqual([{ cmd: 'move_sound', placement_id: 'b', offset_s: 0.1 }]);
  });

  it('rounds to the millisecond the core keeps', () => {
    expect(moveCommands([s('a', { start_s: 0.1 })], 0.2, 'a', null)[0].at_s).toBe(0.3);
  });

  it('applies depth, pan and gain as a delta, inside their ranges', () => {
    expect(depthCommands([s('a', { depth: 0.9 })], 0.5)[0].depth).toBe(1);
    expect(panCommands([s('a', { pan: -0.9 })], -0.5)[0].pan).toBe(-1);
    expect(gainCommands([s('a', { gain_db: 5 })], 3)[0].gain_db).toBe(6);
  });

  it('is one command, so one undo step, however many sounds', () => {
    const cmds = removeCommands(['a', 'b', 'c']);
    expect(oneStep(cmds)).toEqual({ cmd: 'batch', commands: cmds });
    expect(oneStep(cmds.slice(0, 1))).toEqual(cmds[0]);
    expect(oneStep([])).toBe(null);
  });
});

describe('bucket eyes', () => {
  const all = ['a', 'b', 'c'];

  it('a click fades one bucket and shows it again', () => {
    expect(toggleHidden([], all, 'a', false)).toEqual(['a']);
    expect(toggleHidden(['a', 'b'], all, 'a', false)).toEqual(['b']);
  });

  it('⌥ click shows only that bucket', () => {
    expect(toggleHidden([], all, 'b', true)).toEqual(['a', 'c']);
    expect(toggleHidden(['b'], all, 'b', true)).toEqual(['a', 'c']);
  });

  it('⌥ click again restores all', () => {
    expect(toggleHidden(['a', 'c'], all, 'b', true)).toEqual([]);
  });
});

describe('the automatic fade', () => {
  it('matches the rule in the core', () => {
    expect(autoFadeOut(0.5)).toBeCloseTo(0.15);
    expect(autoFadeOut(8)).toBe(0.3);
    expect(autoFadeOut(8.01)).toBe(2);
  });
});

describe('the loudness ring', () => {
  it('runs from −24 dB at the top round to +6', () => {
    expect(ringOf(-24)).toBe(0);
    expect(ringOf(6)).toBe(1);
    expect(dbOfRing(ringOf(-9))).toBeCloseTo(-9);
  });

  it('stays on the ring for values only the Edit page reaches', () => {
    expect(ringOf(-40)).toBe(0);
    expect(ringOf(12)).toBe(1);
  });

  it('follows the pointer clockwise from the top', () => {
    expect(ringAt(10, 0, 0, 0, 0.2)).toBeCloseTo(0.25);
    expect(ringAt(0, 10, 0, 0, 0.4)).toBeCloseTo(0.5);
    expect(ringAt(-10, 0, 0, 0, 0.7)).toBeCloseTo(0.75);
  });

  it('stops at the end it came from instead of jumping across the top', () => {
    expect(ringAt(-1, -10, 0, 0, 0.05)).toBe(0);
    expect(ringAt(1, -10, 0, 0, 0.95)).toBe(1);
  });
});

describe('words', () => {
  it('reads like the prototype', () => {
    expect(dbText(-6.4)).toBe('−6 dB');
    expect(dbText(3)).toBe('+3 dB');
    expect(dbText(0)).toBe('0 dB');
    expect(panText(0.01)).toBe('Pan center');
    expect(panText(-0.5)).toBe('Pan L 50');
    expect([zone(0.1), zone(0.5), zone(0.9)]).toEqual(['Close', 'Room', 'Far']);
  });

  it('announces everything a ball shows', () => {
    const a = announce({ name: 'Foil', bucket: 'Food', start_s: 3, len_s: 1.5, gain_db: -6, pan: 0.5, depth: 0.25 }, () => '00:03.0');
    expect(a).toBe('Foil, Food, starts 00:03.0, 1.5 seconds, loudness −6 dB, Pan R 50, depth Close 25 percent');
  });
});
