// UI state. Document state comes only from the core (view models); every change is a
// command sent through the bridge. Local state here is selection, page, playhead, overlays.
import { call, BridgeError } from './bridge';
import type { EditView, ItemView, Job, SceneView, Status } from './types';

export type Page = 'import' | 'options' | 'arrange' | 'edit' | 'dev';
export type ImportTab = 'files' | 'sounds';
export type Tool = 'pointer' | 'split' | 'fade' | 'gain' | 'marquee';
export type Overlay = null | 'help' | 'editor' | 'settings' | 'history' | 'export';
export type Drawer = null | 'inspector' | 'unmatched' | 'bucket';

export type Theme = 'dark' | 'light';

/** A clip on the Edit page: a dialogue part (`part`, 0 without splits), a sound or the room tone. */
export interface EditSel {
  kind: 'line' | 'sound' | 'roomtone';
  id: string;
  part?: number;
}

export interface RecentEpisode {
  name: string;
  path: string;
}

/** Recently opened episodes (this viewer only), newest first. */
export function recentEpisodes(): RecentEpisode[] {
  try {
    const v = JSON.parse(localStorage.getItem('cutaway.recent') ?? '[]');
    return Array.isArray(v) ? v.filter((r) => r && typeof r.path === 'string') : [];
  } catch {
    return [];
  }
}

function remember(r: RecentEpisode) {
  try {
    const list = [r, ...recentEpisodes().filter((x) => x.path !== r.path)].slice(0, 6);
    localStorage.setItem('cutaway.recent', JSON.stringify(list));
  } catch {
    /* private mode: no recent list */
  }
}

/** The developer switch (dev panel, scoped re-analysis), remembered per viewer. */
function storedDev(): boolean {
  try {
    return localStorage.getItem('cutaway.dev') === '1';
  } catch {
    return false;
  }
}

function storedTheme(): Theme {
  try {
    return localStorage.getItem('cutaway.theme') === 'light' ? 'light' : 'dark';
  } catch {
    return 'dark';
  }
}

class AppState {
  /** Dark first; the choice is remembered per viewer. */
  theme = $state<Theme>(storedTheme());
  dev = $state(storedDev());
  status = $state<Status | null>(null);
  view = $state<SceneView | null>(null);
  edit = $state<EditView | null>(null);
  noProject = $state(false);
  page = $state<Page>('options');
  sceneId = $state('');
  curId = $state<string | null>(null);
  passageTo = $state<string | null>(null);
  playing = $state(false);
  pos = $state(0);
  /** Take auditioned alone in the box editor (`pos` is then seconds into the take). */
  audition = $state<string | null>(null);
  locator = $state(0);
  /** Output level while playing (0..1, decays). */
  level = $state(0);
  scale = $state(1);
  overlay = $state<Overlay>(null);
  drawer = $state<Drawer>(null);
  anim = $state({ n: 0, dir: 'R', id: '' });
  toast = $state<string | null>(null);
  busy = $state(false);
  /** Import page sub-tab. */
  importTab = $state<ImportTab>('files');
  // Edit page (Logic-style control bar)
  editBucket = $state(true);
  editInspector = $state(false);
  mixer = $state(false);
  tool = $state<Tool>('pointer');
  snapWords = $state(true);
  ripple = $state(true);
  cycle = $state<{ on: boolean; from: number; to: number }>({ on: false, from: 0, to: 0 });
  /** What the edit page has selected (shared with its inspector and the LCD): the clip
   *  clicked last, and everything selected with it (⇧-click, rubber band, ⇧F). */
  editSel = $state<EditSel | null>(null);
  editMany = $state<EditSel[]>([]);
  /** Edit page time scale in px per second; 0 until the page first fits the scene. */
  editPx = $state(0);
  /** Edit page lane height and waveform height (⌘↑/↓, the vertical zoom). */
  laneScale = $state(1);
  waveGain = $state(1);
  /** Dialogue clips follow the script order and pauses; off, they move freely in time. */
  lockScript = $state(true);
  /** Bucket the full bucket highlights or opens in its side panel. */
  openBucket = $state<string | null>(null);
  /** Sound being auditioned from a bucket (asset id or remote id). */
  sound = $state<string | null>(null);
  private lastRev = -1;
  private playTimer: ReturnType<typeof setInterval> | null = null;
  private revTimer: ReturnType<typeof setInterval> | null = null;

  get lines(): ItemView[] {
    return this.view?.items.filter((i) => i.kind === 'line') ?? [];
  }

  get cur(): ItemView | undefined {
    return this.lines.find((l) => l.line_id === this.curId) ?? this.lines[0];
  }

  get curIndex(): number {
    const c = this.cur;
    return c ? this.lines.indexOf(c) : 0;
  }

  /** Lines in the passage selection (current line to `passageTo`). */
  get passage(): ItemView[] {
    const a = this.curIndex;
    const bi = this.lines.findIndex((l) => l.line_id === this.passageTo);
    if (bi < 0) return [];
    const [lo, hi] = a <= bi ? [a, bi] : [bi, a];
    return this.lines.slice(lo, hi + 1);
  }

  setTheme(t: Theme) {
    this.theme = t;
    document.documentElement.dataset.theme = t;
    try {
      localStorage.setItem('cutaway.theme', t);
    } catch {
      /* private mode: the choice lasts for this session */
    }
  }

  say(msg: string) {
    this.toast = msg;
    setTimeout(() => {
      if (this.toast === msg) this.toast = null;
    }, 3200);
  }

  async run<T>(f: () => Promise<T>): Promise<T | undefined> {
    try {
      return await f();
    } catch (e) {
      this.say(e instanceof BridgeError ? e.message : String(e));
      return undefined;
    }
  }

  async boot() {
    try {
      this.status = await call<Status>('project_status');
      this.noProject = false;
      if (this.status.path) remember({ name: this.status.name, path: this.status.path });
    } catch (e) {
      if (e instanceof BridgeError && e.code === 'no_project') {
        this.noProject = true;
        return;
      }
      this.say(String(e));
      return;
    }
    if (this.status && this.status.scenes > 0) {
      await this.loadScene(this.sceneId || '1');
      if (this.status.takes === 0) this.page = 'import';
    } else {
      this.view = null;
      this.page = 'import';
    }
    this.watch();
  }

  /** Back to the start screen: the core snapshots unsaved changes and closes the episode. */
  async closeProject() {
    if (this.playing) await this.stop();
    const r = await this.run(() => call<{ snapshot: string | null }>('close_project'));
    if (!r) return;
    if (this.revTimer) clearInterval(this.revTimer);
    this.revTimer = null;
    this.lastRev = -1;
    this.status = null;
    this.view = null;
    this.edit = null;
    this.sceneId = '';
    this.curId = null;
    this.passageTo = null;
    this.editSel = null;
    this.editMany = [];
    this.openBucket = null;
    this.overlay = null;
    this.drawer = null;
    this.pos = 0;
    this.locator = 0;
    this.page = 'options';
    this.importTab = 'files';
    this.noProject = true;
  }

  /** Picks up changes made elsewhere (CLI, MCP, a background job). */
  watch() {
    if (this.revTimer) return;
    this.revTimer = setInterval(async () => {
      try {
        const s = await call<Status>('project_status');
        this.status = s;
        if (s.rev !== this.lastRev && this.view) await this.refresh();
        else if (s.rev !== this.lastRev && s.scenes > 0) await this.loadScene('1');
      } catch {
        /* the next tick retries */
      }
    }, 1500);
  }

  async loadScene(key: string) {
    const v = await call<SceneView>('scene_view', { scene: key });
    this.view = v;
    this.lastRev = v.rev;
    if (this.sceneId !== v.scene.id) {
      this.sceneId = v.scene.id;
      const first = v.items.find((i) => i.kind === 'line' && i.takes.length > 0) ?? v.items.find((i) => i.kind === 'line');
      this.curId = first?.line_id ?? null;
      this.pos = first?.start_s ?? 0;
      this.locator = this.pos;
      this.passageTo = null;
    }
    if (this.page === 'edit') await this.loadEdit();
  }

  async loadEdit() {
    if (!this.sceneId) return;
    this.edit = (await this.run(() => call<EditView>('edit_view', { scene: this.sceneId }))) ?? null;
  }

  async refresh() {
    if (!this.sceneId) return this.boot();
    await this.loadScene(this.sceneId);
  }

  async cmd(c: Record<string, unknown>) {
    const r = await this.run(() => call('command', c));
    await this.refresh();
    return r;
  }

  async method<T = unknown>(name: string, params: object = {}, refresh = true): Promise<T | undefined> {
    const r = await this.run(() => call<T>(name, params));
    if (refresh) await this.refresh();
    return r;
  }

  setDev(on: boolean) {
    this.dev = on;
    try {
      localStorage.setItem('cutaway.dev', on ? '1' : '0');
    } catch {
      /* private mode: the switch lasts for this session */
    }
    if (!on && this.page === 'dev') this.page = 'options';
  }

  setPage(p: Page) {
    this.page = p;
    if (p === 'edit') void this.loadEdit();
  }

  /** ⇧B: between the editor and the full bucket, keeping scene and playhead. */
  toggleFullBucket() {
    if (this.page === 'import') {
      if (this.view) this.setPage('edit');
    } else {
      this.importTab = 'sounds';
      this.setPage('import');
    }
  }

  /** Waits for a background job, reporting progress; resolves with its final state. */
  async waitJob(id: string, onProgress?: (j: Job) => void): Promise<Job | undefined> {
    for (;;) {
      const jobs = await this.run(() => call<Job[]>('job_status'));
      const j = jobs?.find((x) => x.id === id);
      if (!j) return undefined;
      onProgress?.(j);
      if (j.state !== 'running') return j;
      await new Promise((r) => setTimeout(r, 250));
    }
  }

  select(lineId: string, keepPassage = false) {
    const it = this.lines.find((l) => l.line_id === lineId);
    if (!it) return;
    this.curId = lineId;
    if (!keepPassage) this.passageTo = null;
    if (!this.playing) {
      this.pos = it.start_s;
      this.locator = it.start_s;
    }
  }

  go(d: number) {
    const i = Math.max(0, Math.min(this.lines.length - 1, this.curIndex + d));
    const l = this.lines[i];
    if (l) this.select(l.line_id);
  }

  extendPassage(d: number) {
    const from = this.passageTo ? this.lines.findIndex((l) => l.line_id === this.passageTo) : this.curIndex;
    const i = Math.max(0, Math.min(this.lines.length - 1, from + d));
    this.passageTo = this.lines[i]?.line_id ?? null;
  }

  async stepTake(d: number) {
    const c = this.cur;
    if (!c || c.takes.length < 2) return;
    this.anim = { n: this.anim.n + 1, dir: d > 0 ? 'R' : 'L', id: c.line_id };
    await this.cmd({ cmd: 'step_take', line_id: c.line_id, delta: d });
    if (this.cur) this.pos = this.cur.start_s;
  }

  async pickTake(lineId: string, takeId: string) {
    const it = this.lines.find((l) => l.line_id === lineId);
    const from = it?.take_index ?? 0;
    const to = it?.takes.findIndex((t) => t.id === takeId) ?? 0;
    this.anim = { n: this.anim.n + 1, dir: to >= from ? 'R' : 'L', id: lineId };
    await this.cmd({ cmd: 'select_take', line_id: lineId, take_id: takeId });
  }

  async toggleFav() {
    const c = this.cur;
    if (!c?.take_id) return;
    await this.cmd({ cmd: 'toggle_favorite', take_id: c.take_id });
  }

  async applyPassage() {
    const c = this.cur;
    const p = this.passage;
    if (!c || c.take_index == null || p.length < 2) return;
    const takeNo = c.takes[c.take_index].take_no;
    await this.cmd({ cmd: 'apply_take_to_passage', line_ids: p.map((l) => l.line_id), take_no: takeNo });
    this.say(`Take ${takeNo} on ${p.length} lines`);
  }

  async nudgeGap(d: number) {
    const c = this.cur;
    if (!c || this.curIndex === 0) return;
    await this.cmd({ cmd: 'nudge_gap', line_id: c.line_id, delta_s: d });
  }

  async undo() {
    await this.method('undo');
  }

  async redo() {
    await this.method('redo');
  }

  // ---- transport ----

  async play(from: number, until?: number) {
    const r = await this.run(() => call<{ playing: boolean }>('play', { scene: this.sceneId, from_s: from, until_s: until }));
    if (!r?.playing) return;
    this.audition = null;
    this.sound = null;
    this.playing = true;
    this.pos = from;
    this.startPolling();
  }

  async playTake(takeId: string, from: number) {
    const r = await this.run(() => call<{ playing: boolean }>('play_take', { take_id: takeId, from_s: from }));
    if (!r?.playing) return;
    this.audition = takeId;
    this.playing = true;
    this.pos = from;
    this.startPolling();
  }

  /** Auditions a bucket sound (kept asset or preview) from a point in it. */
  async playSound(key: { asset_id?: string; remote_id?: string }, from = 0) {
    const r = await this.run(() => call<{ playing: boolean }>('play_sound', { ...key, from_s: from }));
    if (!r?.playing) return;
    this.audition = null;
    this.sound = key.asset_id ?? key.remote_id ?? null;
    this.playing = true;
    this.pos = from;
    this.startPolling();
  }

  async stop() {
    await this.run(() => call('stop'));
    this.playing = false;
    this.sound = null;
    this.stopPolling();
    if (this.audition) {
      // back on the scene timeline at the line being edited
      this.audition = null;
      if (this.cur) this.pos = this.cur.start_s;
    }
  }

  async togglePlay() {
    if (this.playing) return this.stop();
    if (this.page === 'edit' && this.cycle.on && this.cycle.to > this.cycle.from) {
      const from = this.pos >= this.cycle.from && this.pos < this.cycle.to ? this.pos : this.cycle.from;
      return this.play(from, this.cycle.to);
    }
    const c = this.cur;
    let from = this.pos;
    if (!c || from < c.start_s - 0.001 || from >= c.end_s) from = c?.start_s ?? 0;
    if (from >= (this.view?.duration_s ?? 0) - 0.05) from = c?.start_s ?? 0;
    await this.play(from);
  }

  async playFromLocator() {
    if (this.playing) await this.stop();
    await this.play(this.locator);
  }

  /** Enter: the line with one line of pre-roll. */
  async playLineInContext() {
    const c = this.cur;
    if (!c) return;
    const prev = this.lines[this.curIndex - 1];
    if (this.playing) await this.stop();
    await this.play(prev ? prev.start_s : c.start_s, c.end_s + 0.2);
  }

  /** Silent scrub: moves the playhead without restarting playback. */
  scrubTo(t: number) {
    this.pos = t;
    this.locator = t;
    const it = this.lineAt(t);
    if (it) this.curId = it.line_id;
  }

  /** ⌃M on Options and Arrange: mutes the current line, or restores it when it was cut. */
  async toggleMuteCurrent() {
    const c = this.cur;
    if (!c || c.takes.length === 0) return;
    if (c.dropped) await this.cmd({ cmd: 'set_clip_dropped', line_id: c.line_id, dropped: false });
    else await this.cmd({ cmd: 'set_clip_muted', line_id: c.line_id, muted: !c.muted });
  }

  seek(t: number) {
    this.pos = t;
    this.locator = t;
    const it = this.lineAt(t);
    if (it) this.curId = it.line_id;
    if (this.playing) void this.play(t);
  }

  lineAt(t: number): ItemView | undefined {
    const ls = this.lines.filter((l) => l.takes.length > 0 && l.end_s > l.start_s);
    for (let i = 0; i < ls.length; i++) {
      const next = ls[i + 1];
      if (t < (next ? next.start_s : Infinity)) return ls[i];
    }
    return ls[ls.length - 1];
  }

  private startPolling() {
    this.stopPolling();
    this.playTimer = setInterval(async () => {
      try {
        const p = await call<{ position_s: number; playing: boolean; level?: number }>('playhead');
        this.pos = p.position_s;
        this.level = Math.max(p.level ?? 0, this.level * 0.85);
        if (!this.audition && !this.sound) {
          const it = this.lineAt(p.position_s);
          if (it && it.line_id !== this.curId) this.curId = it.line_id;
        }
        if (!p.playing) {
          this.playing = false;
          this.level = 0;
          this.stopPolling();
          const c = this.cycle;
          if (this.page === 'edit' && c.on && !this.sound && c.to > c.from && p.position_s >= c.to - 0.05) void this.play(c.from, c.to);
          if (this.sound) this.sound = null;
        }
      } catch {
        this.playing = false;
        this.stopPolling();
      }
    }, 33);
  }

  private stopPolling() {
    if (this.playTimer) clearInterval(this.playTimer);
    this.playTimer = null;
  }
}

export const app = new AppState();
