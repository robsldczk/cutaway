// Mirrors apc-app::views (Rust). Read-only view models.

export interface WordView {
  text: string;
  t0: number;
  t1: number;
  script_idx: number | null;
}

export interface TakeView {
  id: string;
  take_no: number;
  take_type: 'full' | 'partial' | 'pickup' | 'micro';
  partial: boolean;
  favorite: boolean;
  favorite_origin: string | null;
  pick: number;
  is_default: boolean;
  direction: string;
  feedback: string;
  direction_raw: string;
  feedback_raw: string;
  tags: string[];
  duration: number;
  score: number;
  lufs: number | null;
  heard: string;
  source_id: string;
  source_name: string;
  src_in: number;
  src_out: number;
  tc_in: number;
  words: WordView[];
  peaks: number[];
  /** Length of the source file (how far a clip can be lengthened). */
  source_len: number;
  needs_rematch: boolean;
  ai_flag: string | null;
}

/** One step of the room strip, in recording order. */
export type RoomItem =
  | { kind: 'talk'; key: string; summary: string; raw: string }
  | { kind: 'take'; key: string; label: string; own: boolean }
  | { kind: 'break'; key: string };

export interface LayoutWord {
  idx: number;
  text: string;
  take_id: string;
  start_s: number;
  end_s: number;
  deleted: boolean;
  script_idx: number | null;
  part: number;
  muted: boolean;
}

export interface CompSegment {
  take_id: string;
  from_word: number;
  to_word: number;
  nudge_s: number;
}

export interface WordEdits {
  deleted: number[];
  keep_pause: number[];
  pause_trims: Record<string, number>;
}

export interface PieceView {
  take_id: string;
  source_id: string;
  src_in: number;
  src_out: number;
  start_s: number;
  len_s: number;
  part: number;
  muted: boolean;
}

/** A part of a line between splits (the whole line without splits), in scene time. */
export interface LayoutPart {
  index: number;
  split: number | null;
  start_s: number;
  end_s: number;
  muted: boolean;
  /** Ripple-deleted: no length, sits where it was cut out. */
  dropped: boolean;
}

export interface ItemView {
  kind: 'line' | 'spacer';
  line_id: string;
  number: number;
  script_index: number;
  moved: boolean;
  character: string | null;
  paren: string | null;
  text: string;
  tokens: string[];
  start_s: number;
  end_s: number;
  gap_before_s: number;
  gap_overridden: boolean;
  pinned: boolean;
  edited_in_edit: boolean;
  take_id: string | null;
  take_index: number | null;
  takes: TakeView[];
  words: LayoutWord[];
  pieces: PieceView[];
  parts: LayoutPart[];
  /** Every part muted / every part ripple-deleted. */
  muted: boolean;
  dropped: boolean;
  fade_in_s: number;
  fade_out_s: number;
  head_s: number;
  tail_s: number;
  comp: CompSegment[];
  edits: WordEdits;
  gain_db: number;
  low_confidence: boolean;
  adlib: boolean;
  /** The line's takes and one take either side, with the room talk between them. */
  room: RoomItem[];
  spacer_enabled: boolean;
  spacer_duration_s: number | null;
}

export interface SceneSummary {
  id: string;
  number: number;
  heading: string;
  place: string;
  time_of_day: string;
  lines: number;
  lines_with_takes: number;
  flagged: number;
  /** Beat title from the writers (empty for plain Fountain). */
  title: string;
  act: string;
  synopsis: string;
}

export interface CharacterView {
  id: string;
  label: string;
  color_index: number;
  /** The writers' notes in one line. */
  profile: string;
}

/** Story context of the open scene (from Throughline or Fountain synopses). */
export interface SceneStoryView {
  title: string;
  act: string;
  synopsis: string;
  cast: string[];
  tone: number | null;
  setting: string;
  setting_description: string;
  setting_parent: string | null;
  parent_description: string;
}

export interface UnmatchedView {
  id: string;
  text: string;
  t0: number;
  t1: number;
  source_id: string;
  origin: string;
  used: boolean;
}

export interface RoomTone {
  enabled: boolean;
  source_id: string | null;
  t0: number;
  t1: number;
  asset_id: string | null;
  gain_db: number;
}

export interface SceneView {
  episode: string;
  scene: SceneSummary;
  location_id: string;
  tightness: number;
  room_tone: RoomTone;
  duration_s: number;
  items: ItemView[];
  unmatched: UnmatchedView[];
  scenes: SceneSummary[];
  characters: CharacterView[];
  story: SceneStoryView;
  rev: number;
  can_undo: boolean;
  can_redo: boolean;
  breath_gap_s: number;
}

export interface Status {
  app: string;
  name: string;
  path: string;
  script_version: number | null;
  scenes: number;
  lines: number;
  recorded_role_lines: number;
  lines_with_takes: number;
  takes: number;
  picks: number;
  sources: number;
  missing_sources: number;
  unmatched: number;
  flagged: number;
  rev: number;
  can_undo: boolean;
  can_redo: boolean;
  ai_provider: string;
  ai_warning: string | null;
  room_tone: boolean;
  /** Recordings play leveled and limited (never exported). */
  playback_level: boolean;
  breath_gap_s: number;
  /** Lead-in and tail around every line, and the fade at its edges (heard and exported). */
  handle_in_ms: number;
  handle_out_ms: number;
  edge_fade_ms: number;
  jobs: Job[];
  /** The show the episode lives in (studio mode), or null for a standalone episode. */
  show: { name: string; root: string; episode: string | null; colors: string[] } | null;
  /** Said once when the episode opens (open on another machine, sync conflicts). */
  warnings: string[];
}

/** An episode folder of a show. */
export interface ShowEpisode {
  folder: string;
  number: number | null;
  title: string | null;
  label: string;
  dir: string;
  /** The episode file, when the episode has a Cutaway project. */
  file: string | null;
  open: boolean;
}

/** The show home (studio mode). */
export interface ShowView {
  name: string;
  root: string;
  file: string;
  sound: string;
  episodes: ShowEpisode[];
  theme: { colors: string[]; backdrop: string | null };
}

export interface Job {
  id: string;
  kind: string;
  state: 'running' | 'done' | 'failed';
  progress: number;
  stage: string;
  message: string;
  result: Record<string, unknown> | null;
}

export interface Anchor {
  line_id: string;
  word: number | null;
  edge: 'start' | 'end';
}

export interface PlacementView {
  id: string;
  asset_id: string;
  name: string;
  lane: 'sfx' | 'ambience' | 'music' | 'dialogue';
  /** Whose dialogue lane (lane 'dialogue': a character's vocal sound). */
  character: string | null;
  layer: number;
  start_s: number;
  len_s: number;
  anchored: boolean;
  anchor: Anchor | null;
  offset_s: number;
  cue_id: string | null;
  fade_in_s: number;
  fade_out_s: number;
  fade_curve: 'equal_power' | 'linear';
  gain_db: number;
  muted: boolean;
  /** Asset in point and the asset's length (trim limits). */
  in_s: number;
  asset_len_s: number;
  peaks: number[];
  origin: string;
}

export interface Asset {
  id: string;
  file: string;
  category: string;
  tags: string[];
  duration_s: number;
  source: string;
}

export interface SoundCue {
  id: string;
  scene_id: string;
  description: string;
  anchor: Anchor | null;
  cue_type: 'hard' | 'foley' | 'ambience' | 'walla' | 'music';
  tags: string[];
  priority: number;
  status: 'wanted' | 'candidates' | 'chosen' | 'placed';
  candidates: string[];
  chosen: string | null;
  ai_suggested: boolean;
  accepted: boolean;
}

export interface CueView {
  cue: SoundCue;
  anchor_text: string | null;
  candidates: Asset[];
}

export interface LaneMix {
  id: string;
  volume_db: number;
  pan: number;
  mute: boolean;
  solo: boolean;
}

export interface RoomToneBed {
  source: string;
  loop_s: number;
  gain_db: number;
  seams: number[];
}

export interface EditView {
  scene_id: string;
  duration_s: number;
  placements: PlacementView[];
  cues: CueView[];
  lane_mix: LaneMix[];
  room_tone: RoomToneBed | null;
}

// ---- Import › Files ----

export interface ImportFile {
  source_id: string;
  file: string;
  duration_s: number;
  role: 'character' | 'room' | 'unknown';
  character: string | null;
  confidence: number;
  locked: boolean;
  tc_start_s: number | null;
  peaks: number[];
}

export interface RoomLink {
  source_id: string;
  file: string;
  duration_s: number;
  locked: boolean;
  sync_s: number | null;
  by: 'timecode' | 'folder';
}

export interface Recording extends ImportFile {
  rooms: RoomLink[];
  shared_with: string[];
  session: string | null;
}

export interface CastCard {
  id: string;
  label: string;
  color_index: number;
  lines: number;
  recordings: Recording[];
}

export interface Fix {
  kind: 'role' | 'character' | 'link' | 'no_recording' | 'missing_file';
  source_id?: string;
  file?: string;
  character?: string;
  label?: string;
  message: string;
}

export interface ImportView {
  script: { number: number; name: string; file: string; summary: Record<string, unknown>; characters: number; lines: number } | null;
  files: { total: number; recordings: number; rooms: number; unknown: number };
  matched_at: string | null;
  analyzed: boolean;
  fixes: Fix[];
  cast: CastCard[];
  unassigned: ImportFile[];
  room_files: ImportFile[];
}

// ---- Sound buckets ----

export interface BucketSound {
  id: string;
  name: string;
  file: string;
  duration_s: number;
  provider: string | null;
  license: string | null;
  source: string;
  category: string;
  peaks: number[];
  tags: string[];
  /** What the recogniser heard ("Mm.") for a character's vocal. */
  heard: string | null;
  /** In this bucket because its file name answers the bucket's cues or keywords. */
  matched?: boolean;
}

export interface Preview {
  provider: string;
  remote_id: string;
  title: string;
  duration_s: number;
  /** Dropped in by hand from the free search; a fresh keyword search keeps it. */
  pinned?: boolean;
}

/** A result of the free Epidemic Sound search, with the buckets it already sits in. */
export interface SearchResult extends Preview {
  buckets: string[];
}

/** Drag type of a free-search result (a `Preview` as JSON) on its way into a bucket. */
export const EPIDEMIC_DRAG = 'application/x-cutaway-epidemic';

export interface SearchPage {
  results: SearchResult[];
  next_offset: number;
  more: boolean;
}

export interface Bucket {
  id: string;
  name: string;
  /** In a show: the bucket's folder in the show's library (`sfx/doors`). */
  folder?: string | null;
  keywords: string[];
  scenes: string[];
  scene_ids: string[];
  kept: BucketSound[];
  previews: Preview[];
  length_min_s: number | null;
  length_max_s: number | null;
  exclude: string[];
  ai_suggested: boolean;
  anchors: Anchor[];
  /** A character's vocal bucket: the actor's own recordings, found by the analysis. */
  character: string | null;
  color_index: number | null;
  /** Belongs to no scene, so every scene can use it. */
  show_wide?: boolean;
}

/** What Clean up proposes: nothing moves until the moves are confirmed. */
export interface CleanupPlan {
  moves: { file: string; name: string; to: string; new_folder: boolean; reason: string }[];
  stay: string[];
}

export interface BucketsView {
  buckets: Bucket[];
  /** Sound effects straight in `sfx/`, in no folder of their own. */
  loose?: number;
  connected: { epidemic: boolean; elevenlabs: boolean };
  /** Scenes with script lines, and those not spotted yet or changed since. */
  spot: { scenes: number; pending: number };
}

/** A bucket sound dragged onto the timeline (a kept asset, or a preview to download). */
export interface DragSound {
  bucket_id: string;
  asset_id?: string;
  remote_id?: string;
  name: string;
  duration_s: number;
}
