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
  needs_rematch: boolean;
  ai_flag: string | null;
}

export interface LayoutWord {
  idx: number;
  text: string;
  take_id: string;
  start_s: number;
  end_s: number;
  deleted: boolean;
  script_idx: number | null;
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
  src_in: number;
  src_out: number;
  start_s: number;
  len_s: number;
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
  head_s: number;
  tail_s: number;
  comp: CompSegment[];
  edits: WordEdits;
  gain_db: number;
  low_confidence: boolean;
  adlib: boolean;
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
}

export interface CharacterView {
  id: string;
  label: string;
  color_index: number;
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
  breath_gap_s: number;
  jobs: Job[];
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
  lane: 'sfx' | 'ambience' | 'music';
  layer: number;
  start_s: number;
  len_s: number;
  anchored: boolean;
  anchor: Anchor | null;
  cue_id: string | null;
  fade_in_s: number;
  fade_out_s: number;
  gain_db: number;
  peaks: number[];
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

export interface EditView {
  scene_id: string;
  placements: PlacementView[];
  cues: CueView[];
}
