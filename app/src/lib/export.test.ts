import { describe, expect, it } from 'vitest';
import { clock, day, fitFormat, footer, formatLabel, formatsFor, historyTip, lockLine, rangeCaption, savedCaption, showsFormat, showsTracks, size, unchangedCaption, unlockQuestion, version, type ExportPlan, type ExportRecord, type ExportScene, type ExportSettings, type ScenePlan } from './export';

const record = (v: number, date: string, destination: ExportRecord['destination'] = 'episode'): ExportRecord => ({ version: v, date, destination, folder: `/x/v${v}`, files: 9, bytes: 1000, locked: true });

const scene = (number: number, length_s: number, last: ExportRecord | null = null): ExportScene => ({
  id: `s${number}`,
  number,
  place: 'Canteen',
  heading: '',
  title: '',
  length_s,
  has_audio: true,
  lock: null,
  next_version: (last?.version ?? 0) + 1,
  last,
  history: last ? [last] : [],
  unchanged_since: null,
  changed_since: null,
});

const planned = (number: number, v: number, unchanged_since: number | null = null): ScenePlan => ({
  scene_id: `s${number}`,
  number,
  place: 'x',
  version: v,
  label: `Sc${String(number).padStart(2, '0')} ${version(v)}`,
  folder: '',
  files: [],
  bytes: 0,
  length_s: 1,
  unchanged_since,
  locked: false,
});

const plan = (scenes: ScenePlan[], episode_version: number | null = null): ExportPlan => ({ scenes, empty: [], episode_version, files: 18, bytes: 780e6 });

const settings = (s: Partial<ExportSettings>): ExportSettings => ({ mode: 'stems', tracks: { dialogue: true, room_voices: true, effects: true, room_effects: true, ambience: true, music: true, mix: true, split_dialogue: false }, codec: 'wav', sample_rate: 48000, bit_depth: 24, loudness: -16, fcpxml: true, fcpxml_source: 'original', subtitles: true, cut_list: null, lock_after: true, save_to: 'episode', ...s });

describe('the rows of the sheet', () => {
  it('follow what the export is', () => {
    expect(showsTracks('stems')).toBe(true);
    expect(showsTracks('clips')).toBe(true);
    expect(showsTracks('mix')).toBe(false);
    expect(showsTracks('fcpxml')).toBe(false);
    expect(showsFormat('mix')).toBe(true);
    expect(showsFormat('fcpxml')).toBe(false);
  });
});

describe('formats', () => {
  it('read like the mockup', () => {
    expect(formatLabel({ codec: 'wav', rate: 48000, bits: 24 })).toBe('WAV · 48 kHz · 24-bit');
    expect(formatLabel({ codec: 'wav', rate: 44100, bits: 32 })).toBe('WAV · 44.1 kHz · 32-bit float');
    expect(formatLabel({ codec: 'mp3', rate: 48000, bits: 24 })).toBe('MP3 · 48 kHz');
  });

  it('offer MP3 for the mix alone, and nothing that needs a missing ffmpeg', () => {
    expect(formatsFor('stems', true).some((f) => f.codec === 'mp3')).toBe(false);
    expect(formatsFor('clips', true).some((f) => f.codec === 'mp3')).toBe(false);
    expect(formatsFor('mix', true).some((f) => f.codec === 'mp3')).toBe(true);
    expect(formatsFor('mix', false).every((f) => f.codec === 'wav')).toBe(true);
  });

  it('fall back when the mode no longer allows them', () => {
    expect(fitFormat(settings({ mode: 'stems', codec: 'mp3' }), true).codec).toBe('wav');
    expect(fitFormat(settings({ mode: 'mix', codec: 'mp3' }), true).codec).toBe('mp3');
    expect(fitFormat(settings({ mode: 'mix', codec: 'flac', bit_depth: 16 }), false).codec).toBe('wav');
  });
});

describe('captions', () => {
  const now = new Date('2026-11-05T15:00:00');

  it('say the range the way the mockup does', () => {
    const r = rangeCaption([scene(2, 200, record(2, '2026-11-04T10:00:00')), scene(3, 57)], 6, now);
    expect(r).toBe('2 of 6 scenes · 4:17 · Sc. 2 was exported yesterday as v002');
    expect(rangeCaption([scene(1, 90), scene(2, 30)], 2, now)).toBe('All 2 scenes · 2:00');
  });

  it('count days by the calendar', () => {
    expect(day('2026-11-05T00:10:00', now)).toBe('today');
    expect(day('2026-11-04T23:50:00', now)).toBe('yesterday');
    expect(day('2026-10-30T12:00:00', now)).toBe('on 30 Oct');
  });

  it('say what the next versions are', () => {
    expect(savedCaption(plan([planned(2, 3), planned(3, 1)]))).toBe('Saved as Sc02 v003 and Sc03 v001. Versions count up on their own, nothing is overwritten.');
    const long = plan([1, 2, 3, 4, 5].map((n) => planned(n, 1)), 2);
    expect(savedCaption(long)).toBe('Saved as 5 scenes, each its next version. The episode becomes v002. Versions count up on their own, nothing is overwritten.');
  });

  it('warn of a delivery that would be a duplicate', () => {
    expect(unchangedCaption(plan([planned(2, 3)]))).toBe('');
    expect(unchangedCaption(plan([planned(2, 3, 2), planned(3, 1)]))).toBe('Sc. 2 is unchanged since v002.');
    expect(unchangedCaption(plan([planned(2, 3, 2), planned(3, 2, 1)]))).toBe('Sc. 2 and Sc. 3 are unchanged since their last versions.');
  });

  it('size the footer', () => {
    expect(footer(plan([]))).toBe('18 files · about 780 MB');
    expect(size(2.34e9)).toBe('2.3 GB');
    expect(size(40e3)).toBe('40 KB');
    expect(clock(3725)).toBe('1:02:05');
  });
});

describe('a lock', () => {
  it('reads like the handover', () => {
    const lock = { version: 3, at: '2026-11-05', script_changed: false };
    expect(lockLine(2, lock)).toBe("Sc. 2 is locked. Exported as v003 on 5 Nov. Timing can't change.");
  });

  it('says when it was never exported', () => {
    expect(lockLine(2, { version: null, at: '2026-11-05', script_changed: false })).toBe("Sc. 2 is locked. Timing can't change.");
  });

  it('says what happened since', () => {
    const lock = { version: 3, at: '2026-11-05', script_changed: true };
    expect(lockLine(2, lock, 3)).toBe("Sc. 2 is locked. Exported as v003 on 5 Nov. Timing can't change. Changed since v003: a new export is suggested. The script changed here.");
  });

  it('asks before it goes', () => {
    expect(unlockQuestion(4)).toBe("The exported files won't match anymore. The next export becomes v004.");
    expect(unlockQuestion(null)).toBe("The exported files won't match anymore.");
  });
});

it('lists a scene\'s history for its tooltip', () => {
  const s = scene(2, 10, record(2, '2026-11-04T10:00:00', 'pipeline'));
  s.history = [record(1, '2026-11-01T10:00:00'), record(2, '2026-11-04T10:00:00', 'pipeline')];
  expect(historyTip(s)).toBe('v001 2026-11-01 · episode\nv002 2026-11-04 · pipeline');
});
