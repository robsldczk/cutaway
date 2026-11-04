import { describe, expect, it } from 'vitest';
import { placesOf, roomLabel, roomOfScene, roomText, sizeCommand } from './room';
import type { RoomRow } from '../types';

const row = (scene_id: string): RoomRow => ({ scene_id, number: 1, heading: '', place: '', room: { size: 0.35, damping: 0.5, wet: 0.48 }, decay_s: 0.8, predelay_ms: 10, tone: null, own_tone: false, location_id: 'loc', night: false, place_tone: false, sounds: 2 });

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

describe('places', () => {
  const tone = (name: string) => ({ asset_id: name, name, gain_db: -28 });

  it('makes one room of a location, however many scenes', () => {
    const rows = [
      { ...row('a'), number: 2, location_id: 'canteen', place: 'Canteen', tone: tone('hum') },
      { ...row('b'), number: 3, location_id: 'office', place: 'Office' },
      { ...row('c'), number: 11, location_id: 'canteen', place: 'Canteen', tone: tone('hum') },
    ];
    const p = placesOf(rows);
    expect(p.map((x) => x.name)).toEqual(['Canteen', 'Office']);
    expect(p[0].numbers).toEqual([2, 11]);
    expect(p[0].day?.name).toBe('hum');
    expect(p[0].mixed).toBe(false);
  });

  it('offers a night ambience only where a place has both, and only when set apart', () => {
    const day = { ...row('a'), location_id: 'farm', tone: tone('birds'), own_tone: true };
    const dark = { ...row('b'), location_id: 'farm', night: true, tone: tone('birds') };
    expect(placesOf([day, dark])[0]).toMatchObject({ mixed: true, night: null });
    const set = { ...dark, tone: tone('crickets'), own_tone: true };
    expect(placesOf([day, set])[0].night?.name).toBe('crickets');
    expect(placesOf([dark])[0].mixed).toBe(false);
  });
});

