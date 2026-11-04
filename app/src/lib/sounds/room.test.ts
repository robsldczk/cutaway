import { describe, expect, it } from 'vitest';
import { roomLabel, roomOfScene, roomText, sizeCommand } from './room';
import type { RoomRow } from '../types';

const row = (scene_id: string): RoomRow => ({ scene_id, number: 1, heading: '', place: '', room: { size: 0.35, damping: 0.5, wet: 0.48 }, decay_s: 0.8, predelay_ms: 10, tone: null, place_tone: false, sounds: 2 });

describe('the room read-out', () => {
  it('reads size and wet', () => {
    expect(roomText(row('a').room)).toBe('Room 35 · wet 48');
    expect(roomLabel(row('a').room, 0.84)).toBe('Room of this scene: size 35, wet 48, 0.8 seconds decay');
  });

  it('is the current scene’s', () => {
    expect(roomOfScene([row('a'), row('b')], 'b')?.scene_id).toBe('b');
    expect(roomOfScene([row('a')], 'c')).toBeNull();
    expect(roomOfScene([row('a')], null)).toBeNull();
  });

  it('a quick change keeps the rest of the room', () => {
    expect(sizeCommand(row('a'), 0.7)).toEqual({ cmd: 'set_scene_room', scene_id: 'a', room: { size: 0.7, damping: 0.5, wet: 0.48 } });
  });
});
