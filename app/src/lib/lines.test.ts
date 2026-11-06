import { describe, expect, it } from 'vitest';
import { attrsOf, attrsText, depthCommands, depthDelta, depthText, pasteCommand, rangeLines, sourceAt, timeAt, toggleLine, wordAt } from './lines';
import type { PieceView } from './types';

const piece = (src_in: number, src_out: number, start_s: number, over: Partial<PieceView> = {}): PieceView => ({ take_id: 't', source_id: 'mic', src_in, src_out, start_s, len_s: src_out - src_in, part: 0, muted: false, ...over });

describe('a moment on a block', () => {
  it('runs straight through a line nobody cut', () => {
    const line = { pieces: [piece(10, 12, 100)] };
    expect(sourceAt(line, 100.5, 'mic')).toBe(10.5);
    expect(timeAt(line, 10.5, 'mic')).toBe(100.5);
  });

  it('jumps a cut word: past it the take is further on than the scene', () => {
    // a second of the take cut out at 11
    const line = { pieces: [piece(10, 11, 100), piece(12, 13, 101)] };
    expect(sourceAt(line, 101.5, 'mic')).toBe(12.5);
    expect(timeAt(line, 12.5, 'mic')).toBe(101.5);
  });

  it('lands where the line goes on when asked for something cut', () => {
    const line = { pieces: [piece(10, 11, 100), piece(12, 13, 101)] };
    expect(timeAt(line, 11.5, 'mic')).toBe(101);
  });

  it('waits at the end of a part through the pause after it', () => {
    const line = { pieces: [piece(10, 11, 100), piece(11, 12, 103, { part: 1 })] };
    expect(sourceAt(line, 102, 'mic')).toBe(11);
  });

  it('takes the newer piece where two overlap at a splice', () => {
    const line = { pieces: [piece(10, 11, 100), piece(12, 13, 100.99)] };
    expect(sourceAt(line, 100.995, 'mic')).toBeCloseTo(12.005);
  });

  it('has no answer for a stretch of another recording', () => {
    const line = { pieces: [piece(10, 11, 100, { source_id: 'other' })] };
    expect(sourceAt(line, 100.5, 'mic')).toBe(null);
    expect(timeAt(line, 10.5, 'mic')).toBe(null);
  });

  it('stays on the block before the line starts and past its end', () => {
    const line = { pieces: [piece(10, 11, 100)] };
    expect(sourceAt(line, 99, 'mic')).toBe(10);
    expect(sourceAt(line, 105, 'mic')).toBe(11);
    expect(timeAt(line, 20, 'mic')).toBe(101);
  });
});

describe('the word at a moment', () => {
  const w = (idx: number, text: string, start_s: number, end_s: number, deleted = false) => ({ idx, text, start_s, end_s, deleted, muted: false, part: 0 });
  const line = { words: [w(0, 'open', 1, 1.4), w(1, 'the', 1.5, 1.6, true), w(2, 'door', 1.7, 2)] };

  it('names it, and nothing between words or for a cut one', () => {
    expect(wordAt(line, 1.2)).toBe('open');
    expect(wordAt(line, 1.45)).toBe(null);
    expect(wordAt(line, 1.55)).toBe(null);
    expect(wordAt(line, 2)).toBe('door');
  });
});

describe('selecting lines', () => {
  const order = ['a', 'b', 'c', 'd'];

  it('⌘ click starts from the current line', () => {
    expect(toggleLine([], 'a', 'c')).toEqual(['a', 'c']);
  });

  it('⌘ click takes a line back out', () => {
    expect(toggleLine(['a', 'c'], 'a', 'c')).toEqual(['a']);
  });

  it('⌘ click with nothing current selects just that line', () => {
    expect(toggleLine([], null, 'c')).toEqual(['c']);
  });

  it('⇧ click selects the run, in either direction', () => {
    expect(rangeLines(order, 'b', 'd')).toEqual(['b', 'c', 'd']);
    expect(rangeLines(order, 'd', 'b')).toEqual(['b', 'c', 'd']);
  });

  it('⇧ click with no line to start from selects the one', () => {
    expect(rangeLines(order, null, 'b')).toEqual(['b']);
    expect(rangeLines(order, 'gone', 'b')).toEqual(['b']);
  });
});

describe('depth', () => {
  it('reads as a place', () => {
    expect(depthText(0)).toBe('Close');
    expect(depthText(0.4)).toBe('Back 40 %');
  });

  it('a group moves as one and stops at the ends', () => {
    expect(depthDelta([0.2, 0.9], 0.5)).toBeCloseTo(0.1);
    expect(depthDelta([0.2, 0.9], -0.5)).toBeCloseTo(-0.2);
  });

  it('is one command per line', () => {
    expect(depthCommands(['a', 'b'], (id) => (id === 'a' ? 0.1234 : 0.5))).toEqual([
      { cmd: 'set_clip_depth', line_id: 'a', depth: 0.123 },
      { cmd: 'set_clip_depth', line_id: 'b', depth: 0.5 },
    ]);
  });
});

describe('attributes', () => {
  const a = { depth: 0.4, gain_db: -3, fade_in_s: 0.1, fade_out_s: 0 };

  it('are how a line sounds and nothing else', () => {
    expect(attrsOf({ ...a, head_s: 1, line_id: 'x' } as never)).toEqual(a);
  });

  it('say what they are', () => {
    expect(attrsText(a)).toBe('Back 40 % · −3.0 dB · fades');
    expect(attrsText({ depth: 0, gain_db: 0, fade_in_s: 0, fade_out_s: 0 })).toBe('Close');
  });

  it('paste as one undo step on every line', () => {
    const c = pasteCommand(['x', 'y'], a) as { cmd: string; commands: { cmd: string; line_id: string }[] };
    expect(c.cmd).toBe('batch');
    expect(c.commands.map((x) => x.cmd)).toEqual(['set_clip_depth', 'set_clip_gain', 'set_clip_fades', 'set_clip_depth', 'set_clip_gain', 'set_clip_fades']);
    expect(c.commands[3].line_id).toBe('y');
  });

  it('paste on nothing is nothing', () => {
    expect(pasteCommand([], a)).toBe(null);
  });
});
