// The room of a scene as the transport's read-out shows it. The document holds the room; the
// settings themselves are on Import › Rooms, and a quick change here is the same command.
import type { RoomRow, SceneRoom, RoomAmbience } from '../types';

const percent = (x: number) => Math.round(x * 100);

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

export const roomText = (r: SceneRoom) => `Room ${percent(r.size)} · wet ${percent(r.wet)}`;

export const roomLabel = (r: SceneRoom, decay_s: number) => `Room of this scene: size ${percent(r.size)}, wet ${percent(r.wet)}, ${decay_s.toFixed(1)} seconds decay`;

export const roomOfScene = (rows: RoomRow[], scene: string | null) => rows.find((r) => r.scene_id === scene) ?? null;

/** The command for a new size: the rest of the room stays as the document has it. */
export const sizeCommand = (row: RoomRow, size: number) => ({ cmd: 'set_scene_room', scene_id: row.scene_id, room: { ...row.room, size } });
