// The rooms of an episode as Import › Rooms lists them.
import type { RoomRow, RoomAmbience } from '../types';

/** One location: however many scenes happen there, it's one room. */
export interface Place {
  id: string;
  name: string;
  scenes: RoomRow[];
  numbers: number[];
  /** It has night scenes and others, so it can have an ambience for each. */
  mixed: boolean;
  /** What its scenes play by day, or at all when it isn't mixed … */
  day: RoomAmbience | null;
  /** … and at night, when that's set apart. */
  night: RoomAmbience | null;
}

export function placesOf(rows: RoomRow[]): Place[] {
  const by = new Map<string, RoomRow[]>();
  for (const r of rows) by.set(r.location_id, [...(by.get(r.location_id) ?? []), r]);
  return [...by.entries()].map(([id, scenes]) => {
    const dark = scenes.filter((s) => s.night);
    const mixed = dark.length > 0 && dark.length < scenes.length;
    const first = (scenes.find((s) => !s.night) ?? scenes[0]);
    const night = mixed ? (dark.find((s) => s.own_tone)?.tone ?? null) : null;
    return { id, name: first.place || first.heading, scenes, numbers: scenes.map((s) => s.number), mixed, day: first.tone, night };
  });
}
