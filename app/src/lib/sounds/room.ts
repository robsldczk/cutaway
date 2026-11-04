// The room of a scene as the transport's read-out shows it. The document holds the room; the
// settings themselves are on Import › Sounds, and a quick change here is the same command.
import type { RoomRow, SceneRoom } from '../types';

const percent = (x: number) => Math.round(x * 100);

export const roomText = (r: SceneRoom) => `Room ${percent(r.size)} · wet ${percent(r.wet)}`;

export const roomLabel = (r: SceneRoom, decay_s: number) => `Room of this scene: size ${percent(r.size)}, wet ${percent(r.wet)}, ${decay_s.toFixed(1)} seconds decay`;

export const roomOfScene = (rows: RoomRow[], scene: string | null) => rows.find((r) => r.scene_id === scene) ?? null;

/** The command for a new size: the rest of the room stays as the document has it. */
export const sizeCommand = (row: RoomRow, size: number) => ({ cmd: 'set_scene_room', scene_id: row.scene_id, room: { ...row.room, size } });
