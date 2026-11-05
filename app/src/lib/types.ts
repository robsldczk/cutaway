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
  /** How long it takes: that, or the default for its length. */
  spacer_s: number;
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

/** The room a scene's sounds share. Each setting runs 0 … 1. */
export interface SceneRoom {
  size: number;
  damping: number;
  wet: number;
}

/** A scene's room on the Sounds tab. */
export interface RoomAmbience {
  asset_id: string;
  name: string;
  gain_db: number;
}

export interface RoomRow {
  scene_id: string;
  number: number;
  heading: string;
  place: string;
  room: SceneRoom;
  /** What the settings come to, from the curves the audio uses. */
  decay_s: number;
  predelay_ms: number;
  /** The ambience it plays, its own or what its place shares. */
  tone: RoomAmbience | null;
  /** It's set on this scene. */
  own_tone: boolean;
  /** Scenes of one location are one room. */
  location_id: string;
  night: boolean;
  /** Without one it plays its location's, if that has any. */
  place_tone: boolean;
  sounds: number;
}

/** A character's voice in the room. */
export interface VoiceRoom {
  id: string;
  label: string;
  color_index: number;
  wet: number;
}

export interface PlacementView {
  id: string;
  asset_id: string;
  name: string;
  lane: 'sfx' | 'ambience' | 'music' | 'dialogue' | 'adlib';
  /** Whose voice (lanes 'dialogue' and 'adlib'). */
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
  /** -1 left … +1 right. */
  pan: number;
  /** 0 close … 1 far. */
  depth: number;
  /** The fade out follows the length; setting it by hand turns this off. */
  auto_fade: boolean;
  /** Order the sounds were added in (0 for sounds from before it was kept). */
  added: number;
  /** Asset in point and the asset's length (trim limits). */
  in_s: number;
  asset_len_s: number;
  /** Repeats its file, so it may outlast it. */
  looped: boolean;
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
  /** Its library sound; null for a stretch of a recording. */
  own: string | null;
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
  /** Dropped, a sound over 6 s with this on repeats on the ambience lane. */
  loop: boolean;
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
  /** Set in a bucket; see `BucketSound.loop`. */
  loop?: boolean;
}

/** A result of the free Epidemic Sound search, with the buckets it already sits in. */
export interface SearchResult extends Preview {
  buckets: string[];
}

/** Drag type of a free-search result (a `Preview` as JSON) on its way into a bucket. */

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
  /** Absent for a sound of the library that sits in no bucket. */
  bucket_id?: string;
  asset_id?: string;
  remote_id?: string;
  /** A stretch of a recording, cut when it's dropped (the character search). */
  clip?: PerformanceClip;
  character?: string;
  /** The cue it answers, for a candidate. */
  cue_id?: string;
  name: string;
  duration_s: number;
}

/** A sound for the preview player. */
export interface SoundKey {
  asset_id?: string;
  remote_id?: string;
  clip?: PerformanceClip & { id: string };
}

/** A stretch of a recording, in source-file seconds. */
export interface PerformanceClip {
  source_id: string;
  t0: number;
  t1: number;
  text: string;
}

/** A place a character said it: `t0`–`t1` is the clip, `w0`–`w1` the audio around it. */
export interface PerformanceHit {
  id: string;
  source_id: string;
  character: string;
  name: string;
  color_index: number;
  t0: number;
  t1: number;
  w0: number;
  w1: number;
  peaks: number[];
  text: string;
  before: string;
  after: string;
  /** The kind of sound, for one without words. */
  tag: string | null;
  close: boolean;
  alone: boolean;
  at: string;
}

export interface PerformanceResults {
  cast: { id: string; label: string; color_index: number }[];
  character: string | null;
  hits: PerformanceHit[];
  total: number;
  /** There were transcripts to look in. */
  heard: boolean;
}

/** A character with their mics, in the mapping editor. */
export interface MappingVoice {
  id: string;
  label: string;
  color_index: number;
  /** Session time = file time − offset_s. */
  mics: { id: string; name: string; duration_s: number; offset_s: number; session_id: string | null }[];
}

/** Where the mapping editor opens. */
export interface MappingOpen {
  cast: MappingVoice[];
  character: string | null;
  source_id: string | null;
  /** Where to look first, in source-file seconds. */
  at_s: number;
  /** The line's take, when it has one. */
  t0: number | null;
  t1: number | null;
}

export interface RecordingWord {
  text: string;
  t0: number;
  t1: number;
}

export interface RecordingTake {
  id: string;
  line_id: string;
  take_no: number;
  t0: number;
  t1: number;
  text: string;
}

/** One mic as heard. */
export interface Recording {
  source_id: string;
  name: string;
  character: string | null;
  duration_s: number;
  /** It has a transcript. */
  heard: boolean;
  words: RecordingWord[];
  takes: RecordingTake[];
  overview: number[];
}

/** What the preview player shows of it. */
export interface SoundFace {
  name: string;
  duration_s: number;
  peaks: number[];
}

/** The buckets' preview player. `pos` is seconds into the sound; the timeline's `pos` stays. */
export interface SoundPreview extends SoundFace {
  id: string;
  key: SoundKey;
  pos: number;
  playing: boolean;
}
