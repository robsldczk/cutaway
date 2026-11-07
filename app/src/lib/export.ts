// The Export page's words and rules. Mirrors apc_core::config and apc_app::exports.
import type { SceneLock, SceneStatus } from './types';

export type ExportMode = 'stems' | 'clips' | 'mix' | 'one_file' | 'fcpxml';
export type Codec = 'wav' | 'flac' | 'mp3';

export interface ExportTracks {
  dialogue: boolean;
  room_voices: boolean;
  effects: boolean;
  room_effects: boolean;
  ambience: boolean;
  music: boolean;
  mix: boolean;
  split_dialogue: boolean;
  /** The per-character files without pan: centred, one channel (D160). */
  split_mono: boolean;
}

export interface ExportSettings {
  mode: ExportMode;
  tracks: ExportTracks;
  codec: Codec;
  sample_rate: number;
  bit_depth: number;
  loudness: number | null;
  fcpxml: boolean;
  fcpxml_source: 'original' | 'stems';
  subtitles: boolean;
  cut_list: 'csv' | 'md' | null;
  lock_after: boolean;
  save_to: 'episode' | 'pipeline';
}

export interface ExportRecord {
  version: number;
  date: string;
  destination: 'episode' | 'pipeline';
  folder: string;
  files: number;
  bytes: number;
  locked: boolean;
}

export interface ExportScene {
  id: string;
  number: number;
  place: string;
  heading: string;
  title: string;
  status: SceneStatus | null;
  length_s: number;
  has_audio: boolean;
  lock: SceneLock | null;
  next_version: number;
  last: ExportRecord | null;
  history: ExportRecord[];
  unchanged_since: number | null;
  changed_since: number | null;
}

export interface Pipeline {
  show: string;
  path: string | null;
  proposal: string | null;
}

export interface ExportStatus {
  name: string;
  episode: string;
  scenes: ExportScene[];
  /** Versions of the episode itself, oldest first. */
  episode_versions: ExportRecord[];
  pipeline: Pipeline | null;
  presets: { name: string; settings: ExportSettings }[];
  defaults: ExportSettings;
  warnings: string[];
  ffmpeg: boolean;
}

export interface ShowExportEpisode {
  folder: string;
  label: string;
  episode?: string;
  scenes: ExportScene[];
}

export interface ShowExportStatus {
  name: string;
  episodes: ShowExportEpisode[];
  pipeline: Pipeline;
  defaults: ExportSettings;
  ffmpeg: boolean;
}

export interface ScenePlan {
  scene_id: string;
  number: number;
  place: string;
  version: number;
  label: string;
  folder: string;
  files: string[];
  bytes: number;
  length_s: number;
  unchanged_since: number | null;
  locked: boolean;
}

export interface OneFilePlan {
  version: number;
  label: string;
  folder: string;
  files: string[];
  bytes: number;
  length_s: number;
  unchanged_since: number | null;
}

export interface ExportPlan {
  scenes: ScenePlan[];
  /** Set instead of `scenes` when the range goes out as one file. */
  one_file: OneFilePlan | null;
  empty: number[];
  episode_version: number | null;
  files: number;
  bytes: number;
}

export interface ExportResult {
  scenes: { scene_id: string; number: number; version: number; folder: string; files: string[] }[];
  episode: ExportRecord | null;
  unchanged: { number: number; since: number }[];
  empty: number[];
  files: number;
  bytes: number;
  warnings: string[];
}

export const MODES: { id: ExportMode; label: string; caption: string }[] = [
  { id: 'stems', label: 'Aligned stems', caption: 'One file per track and scene, each from 0:00 to the end of the scene, so they line up when stacked. Other choices: individual clips, mix only, one file, FCPXML only.' },
  { id: 'clips', label: 'Individual clips', caption: 'One file per clip with its handles, named by scene, line, character and take. The FCPXML points at these files.' },
  { id: 'mix', label: 'Mix only', caption: 'One full mix per scene.' },
  { id: 'one_file', label: 'One file', caption: 'The range as a single mix, scene after scene with a short pause between them. It is a version of the episode: the scenes get none of their own.' },
  { id: 'fcpxml', label: 'FCPXML only', caption: 'Only the timeline, with nothing rendered, so it\'s ready at once. It points at the original recordings.' },
];

export const TRACKS: { key: keyof ExportTracks; name: string; sub: string; color: string }[] = [
  { key: 'dialogue', name: 'Dialogue', sub: 'dry', color: 'var(--track-dialogue)' },
  { key: 'room_voices', name: 'Room · voices', sub: 'reverb', color: 'var(--track-room-voices)' },
  { key: 'effects', name: 'Effects', sub: 'dry', color: 'var(--track-effects)' },
  { key: 'room_effects', name: 'Room · effects', sub: 'reverb', color: 'var(--track-room-effects)' },
  { key: 'ambience', name: 'Ambience', sub: '', color: 'var(--track-ambience)' },
  { key: 'music', name: 'Music', sub: '', color: 'var(--track-music)' },
  { key: 'mix', name: 'Full mix', sub: '', color: 'var(--track-mix)' },
  { key: 'split_dialogue', name: 'Split dialogue', sub: 'by character', color: '' },
];

export interface FormatChoice {
  codec: Codec;
  rate: number;
  bits: number;
}

const FORMATS: FormatChoice[] = [
  { codec: 'wav', rate: 48000, bits: 24 },
  { codec: 'wav', rate: 48000, bits: 16 },
  { codec: 'wav', rate: 48000, bits: 32 },
  { codec: 'wav', rate: 44100, bits: 24 },
  { codec: 'wav', rate: 44100, bits: 16 },
  { codec: 'wav', rate: 96000, bits: 24 },
  { codec: 'flac', rate: 48000, bits: 24 },
  { codec: 'flac', rate: 48000, bits: 16 },
  { codec: 'flac', rate: 44100, bits: 16 },
  { codec: 'mp3', rate: 48000, bits: 24 },
  { codec: 'mp3', rate: 44100, bits: 24 },
];

/** MP3 pads its ends, so it can't hold a stem or clip to the sample; FLAC and MP3 need ffmpeg. */
export function formatsFor(mode: ExportMode, ffmpeg: boolean): FormatChoice[] {
  return FORMATS.filter((f) => (f.codec === 'wav' || ffmpeg) && (f.codec !== 'mp3' || mode === 'mix' || mode === 'one_file'));
}

export const formatKey = (f: FormatChoice) => `${f.codec}/${f.rate}/${f.bits}`;

export function formatLabel(f: FormatChoice): string {
  const rate = `${f.rate / 1000} kHz`;
  if (f.codec === 'mp3') return `MP3 · ${rate}`;
  return `${f.codec.toUpperCase()} · ${rate} · ${f.bits === 32 ? '32-bit float' : `${f.bits}-bit`}`;
}

/** The format of settings, or the first that still fits after the mode changed. */
export function fitFormat(s: ExportSettings, ffmpeg: boolean): FormatChoice {
  const all = formatsFor(s.mode, ffmpeg);
  return all.find((f) => f.codec === s.codec && f.rate === s.sample_rate && f.bits === s.bit_depth) ?? all[0];
}

export const LOUDNESS: { value: number | null; label: string }[] = [
  { value: -16, label: 'Mix at −16 LUFS' },
  { value: -23, label: 'Mix at −23 LUFS' },
  { value: null, label: 'Mix not normalized' },
];

export const showsTracks = (m: ExportMode) => m === 'stems' || m === 'clips';
export const showsFormat = (m: ExportMode) => m !== 'fcpxml';

export function clock(s: number): string {
  const t = Math.round(s);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = String(t % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`;
}

export function size(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** `today`, `yesterday`, or `on 5 Nov`. */
export function day(iso: string, now = new Date()): string {
  const d = new Date(iso);
  const days = Math.round((startOfDay(now) - startOfDay(d)) / 86_400_000);
  if (days === 0) return 'today';
  if (days === 1) return 'yesterday';
  return `on ${d.getDate()} ${d.toLocaleDateString('en-GB', { month: 'short' })}`;
}

export const version = (v: number) => `v${String(v).padStart(3, '0')}`;

/** The scenes with a box to tick: one with nothing in it has nothing to export. */
export const tickable = (scenes: ExportScene[]) => scenes.filter((s) => s.has_audio);

/** The ticks with `ids` turned on or off, in script order. */
export function tick(scenes: ExportScene[], picked: string[], ids: string[], on: boolean): string[] {
  const next = new Set(picked);
  for (const id of ids) {
    if (on) next.add(id);
    else next.delete(id);
  }
  return scenes.filter((s) => next.has(s.id)).map((s) => s.id);
}

/** The scenes from one to the other, both included, whichever comes first. */
export function between(scenes: ExportScene[], a: string, b: string): string[] {
  const ids = scenes.map((s) => s.id);
  const [from, to] = [ids.indexOf(a), ids.indexOf(b)].sort((x, y) => x - y);
  return from < 0 ? [b] : ids.slice(from, to + 1);
}

/** Every box the other way round. */
export const inverted = (scenes: ExportScene[], picked: string[]) => tickable(scenes).filter((s) => !picked.includes(s.id)).map((s) => s.id);

export const withStatus = (scenes: ExportScene[], status: SceneStatus) => tickable(scenes).filter((s) => s.status === status).map((s) => s.id);

/** Every box ticked: that's the whole episode, with a version of its own. */
export const isWhole = (scenes: ExportScene[], picked: string[]) => {
  const open = tickable(scenes);
  return open.length > 0 && open.every((s) => picked.includes(s.id));
};

/** `2 of 6 scenes · 4:17 · Sc. 2 was exported yesterday as v002`. */
export function rangeCaption(picked: ExportScene[], total: number, joined?: number, now = new Date()): string {
  // one file also holds the pauses between its scenes
  const length = joined ?? picked.reduce((a, s) => a + s.length_s, 0);
  const count = picked.length === total ? `All ${total} scenes` : `${picked.length} of ${total} scenes`;
  const parts = [count, clock(length)];
  const last = picked.find((s) => s.last);
  if (last?.last) parts.push(`Sc. ${last.number} was exported ${day(last.last.date, now)} as ${version(last.last.version)}`);
  return parts.join(' · ');
}

function list(items: string[]): string {
  if (items.length < 2) return items.join('');
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`;
}

/** `Saved as Sc02 v003 and Sc03 v001.`, shortened for a long range. */
export function savedCaption(plan: ExportPlan): string {
  if (plan.one_file) return `Saved as ${plan.one_file.label}. Versions count up on their own, nothing is overwritten.`;
  const labels = plan.scenes.map((s) => s.label);
  const named = labels.length > 4 ? `${labels.length} scenes, each its next version` : list(labels);
  const episode = plan.episode_version ? ` The episode becomes ${version(plan.episode_version)}.` : '';
  return `Saved as ${named}.${episode} Versions count up on their own, nothing is overwritten.`;
}

/** What's the same as its last version: exporting it again delivers nothing new. */
export function unchangedCaption(plan: ExportPlan): string {
  const since = plan.one_file?.unchanged_since;
  if (since != null) return `These scenes are unchanged since ${version(since)}.`;
  const same = plan.scenes.filter((s) => s.unchanged_since != null);
  if (!same.length) return '';
  if (same.length === 1) return `Sc. ${same[0].number} is unchanged since ${version(same[0].unchanged_since as number)}.`;
  return `${list(same.map((s) => `Sc. ${s.number}`))} are unchanged since their last versions.`;
}

export const footer = (plan: ExportPlan) => `${plan.files} files · about ${size(plan.bytes)}`;

/** The quiet line on a locked scene. */
export function lockLine(number: number, lock: SceneLock, changedSince: number | null = null): string {
  const told = lock.version != null ? ` Exported as ${version(lock.version)} on ${lockDay(lock.at)}.` : '';
  const changed = changedSince != null ? ` Changed since ${version(changedSince)}: a new export is suggested.` : '';
  const script = lock.script_changed ? ' The script changed here.' : '';
  return `Sc. ${number} is locked.${told} Timing can't change.${changed}${script}`;
}

function lockDay(at: string): string {
  const d = new Date(`${at}T12:00:00`);
  if (Number.isNaN(d.getTime())) return at;
  return `${d.getDate()} ${d.toLocaleDateString('en-GB', { month: 'short' })}`;
}

/** What unlocking means, asked before it happens. */
export function unlockQuestion(nextVersion: number | null): string {
  const next = nextVersion ? ` The next export becomes ${version(nextVersion)}.` : '';
  return `The exported files won't match anymore.${next}`;
}

/** Export history in a tooltip. */
export function historyTip(s: ExportScene): string {
  return s.history.map((r) => `${version(r.version)} ${r.date.slice(0, 10)} · ${r.destination === 'pipeline' ? 'pipeline' : 'episode'}`).join('\n');
}
