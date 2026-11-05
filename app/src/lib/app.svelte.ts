// UI state. Document state comes only from the core (view models); every change is a
// command sent through the bridge. Local state here is selection, page, playhead, overlays.
import { call, BridgeError } from './bridge';
import type { ExportScene, ExportSettings, ExportStatus } from './export';
import type { DragSound, EditView, ItemView, Job, SceneSummary, SceneView, ShowView, SoundFace, SoundKey, SoundPreview, Status } from './types';

const soundId = (key: SoundKey) => key.asset_id ?? key.remote_id ?? key.clip?.id ?? '';

export type Page = 'import' | 'options' | 'arrange' | 'edit' | 'export' | 'dev';
export type ImportTab = 'files' | 'sounds' | 'rooms';
export type Tool = 'pointer' | 'split' | 'fade' | 'gain' | 'marquee';
export type Overlay = null | 'help' | 'editor' | 'mapping' | 'delete-line' | 'settings' | 'history';

/** What the mapping editor is for: a new take of a line, or a new line after an item. */
export type MappingTarget = { kind: 'take'; line_id: string } | { kind: 'line'; after: string | null };
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
  /** A show (opened by its folder), not an episode. */
  show?: boolean;
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

/** Snapping on Arrange: on unless switched off, remembered per viewer. */
function storedSoundSnap(): boolean {
  try {
    return localStorage.getItem('cutaway.soundSnap') !== '0';
  } catch {
    return true;
  }
}

/** Follow playhead: on unless switched off, remembered per viewer. */
function storedFollow(): boolean {
  try {
    return localStorage.getItem('cutaway.follow') !== '0';
  } catch {
    return true;
  }
}

/** The sounds layer on Arrange: on unless switched off, remembered per viewer. */
function storedSounds(): boolean {
  try {
    return localStorage.getItem('cutaway.sounds') !== '0';
  } catch {
    return true;
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
  /** While playing, the current box moves with the playhead (and the view with it); off, it stays put. */
  follow = $state(storedFollow());
  /** Sounds on Arrange catch on lines, pauses and directions. ⌥ lets go for one drag. */
  soundSnap = $state(storedSoundSnap());
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
  mapping = $state<MappingTarget | null>(null);
  /** The line the delete question is about. */
  deleting = $state<string | null>(null);
  anim = $state({ n: 0, dir: 'R', id: '' });
  toast = $state<string | null>(null);
  busy = $state(false);
  /** Import page sub-tab. */
  importTab = $state<ImportTab>('files');
  /** The page the room shortcut came from: Back returns there. */
  importBack = $state<Page | null>(null);
  /** Import › Sounds opens its Rooms on the current scene once. */
  roomsAsked = $state(false);
  /** What's selected in the sounds layer, for the tab on the transport. */
  soundPicked = $state<{ name: string; detail: string; tone: string | null } | null>(null);
  /** A sound from the Finder waiting to be told where it lives. */
  filing = $state<{ paths: string[]; x: number; y: number } | null>(null);
  /** A scene of the room open in Import › Rooms, and which of its ambiences. */
  roomPick = $state<{ scene: string; night?: boolean } | null>(null);
  // Edit page (Logic-style control bar)
  editBucket = $state(true);
  /** Under 1100 px there's no room beside the timeline: the bucket is an overlay, shut until
   *  asked for, and it steps aside during a drag. */
  narrow = $state(typeof window !== 'undefined' && window.innerWidth <= 1100);
  private bucketOver = $state(false);
  /** What the docked bucket's search looks through. */
  bucketSearch = $state<'sounds' | 'characters'>('sounds');
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
  /** Arrange: the sounds layer beside the dialogue (S). */
  soundsOn = $state(storedSounds());
  /** Arrange: selected sounds; the last one is the primary. */
  soundSel = $state<string[]>([]);
  /** Arrange: the sound open in its vertical editor. */
  soundOpen = $state<string | null>(null);
  /** Arrange: buckets faded out with their eye. View only: this page, this session, never
   *  the document or the mix. */
  hiddenBuckets = $state<string[]>([]);
  /** Keys and the wheel go to the sounds layer first, while it is on the page. */
  soundsUi: { key(e: KeyboardEvent): boolean; wheel(e: WheelEvent): boolean; placeFile(s: DragSound, x: number, y: number): Promise<void>; dropOnAction(line_id: string, s: DragSound): void } | null = null;
  /** Bucket the full bucket highlights or opens in its side panel. */
  openBucket = $state<string | null>(null);
  /** The buckets' preview player: the sound last auditioned, playing or not. */
  preview = $state<SoundPreview | null>(null);
  /** What's been exported, by scene id: for tooltips and "changed since". */
  exported = $state<Record<string, ExportScene>>({});
  /** The Export page's choices, kept while the episode is open. */
  exportSettings = $state<ExportSettings | null>(null);
  /** ⌘E on the Export page. */
  exportUi: { run(): void } | null = null;
  /** The scene asked about before unlocking. */
  unlocking = $state<string | null>(null);
  /** The show home's export sheet. */
  showExport = $state(false);
  /** A locked scene refused an edit: said where the edit was tried. */
  blocked = $state<{ text: string; x: number; y: number } | null>(null);
  /** Where the pointer last pressed, and when. */
  pointer = { x: 0, y: 0, at: 0 };
  /** The open show (studio mode): its home shows when no episode is open. */
  show = $state<ShowView | null>(null);
  /** The show's backdrop (a data URL), loaded once per show. */
  backdrop = $state<string | null>(null);
  private backdropOf = '';
  private warned = '';
  private lastRev = -1;
  private playTimer: ReturnType<typeof setInterval> | null = null;
  private previewTimer: ReturnType<typeof setInterval> | null = null;
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

  setSoundSnap(on: boolean) {
    this.soundSnap = on;
    try {
      localStorage.setItem('cutaway.soundSnap', on ? '1' : '0');
    } catch {
      /* private mode: the choice lasts for this session */
    }
  }

  setFollow(on: boolean) {
    this.follow = on;
    try {
      localStorage.setItem('cutaway.follow', on ? '1' : '0');
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
      if (e instanceof BridgeError && e.code === 'scene_locked') this.refuse(e.message);
      else this.say(e instanceof BridgeError ? e.message : String(e));
      return undefined;
    }
  }

  /** Says a locked scene's refusal at the pointer, when the edit came from it, or by the
   *  scene's banner when it came from the keyboard. */
  private refuse(text: string) {
    const recent = performance.now() - this.pointer.at < 1500;
    const note = { text, x: recent ? this.pointer.x : window.innerWidth / 2, y: recent ? this.pointer.y : 96 };
    this.blocked = note;
    setTimeout(() => {
      if (this.blocked === note) this.blocked = null;
    }, 3600);
  }

  get bucketOn(): boolean {
    return this.narrow ? this.bucketOver : this.editBucket;
  }

  setBucket(on: boolean) {
    if (this.narrow) this.bucketOver = on;
    else this.editBucket = on;
  }

  /** The scene on screen, when it's locked. */
  get sceneLocked(): SceneSummary | null {
    const s = this.view?.scenes.find((x) => x.id === this.sceneId);
    return s?.lock ? s : null;
  }

  /** What's been exported, for the scene list's tooltips and the lock banner. */
  async loadExports() {
    try {
      const s = await call<ExportStatus>('export_status');
      this.exported = Object.fromEntries(s.scenes.map((x) => [x.id, x]));
    } catch {
      /* the next look tries again; nothing here stops an edit */
    }
  }

  async lockScene(id: string) {
    await this.method('lock_scene', { scene: id });
    await this.loadExports();
  }

  async unlockScene(id: string) {
    await this.method('unlock_scene', { scene: id, confirm: true });
    await this.loadExports();
  }

  /** Opens a version's folder in the Finder. */
  reveal(folder: string) {
    void this.run(() => call('reveal_export', { folder }));
  }

  async boot() {
    try {
      this.status = await call<Status>('project_status');
      this.noProject = false;
      if (this.status.path) remember({ name: this.status.show ? `${this.status.show.name} · ${this.status.name}` : this.status.name, path: this.status.path });
      if (this.status.show) await this.loadShow();
      else this.show = null;
      if (this.status.warnings.length && this.warned !== this.status.path) {
        this.warned = this.status.path;
        this.say(this.status.warnings.join(' '));
      }
    } catch (e) {
      if (e instanceof BridgeError && e.code === 'no_project') {
        this.noProject = true;
        await this.loadShow();
        return;
      }
      this.say(String(e));
      return;
    }
    if (this.status && this.status.scenes > 0) {
      await this.loadScene(this.sceneId || '1');
      if (this.status.takes === 0) this.page = 'import';
      void this.loadExports();
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
    this.resetEpisode();
  }

  /** Forgets the open episode's view state (the core has closed or replaced it). */
  private resetEpisode() {
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
    this.soundSel = [];
    this.soundOpen = null;
    this.hiddenBuckets = [];
    this.openBucket = null;
    this.overlay = null;
    this.drawer = null;
    this.pos = 0;
    this.locator = 0;
    this.page = 'options';
    this.importTab = 'files';
    this.exported = {};
    this.exportSettings = null;
    this.noProject = true;
  }

  /** The open show's home (null when no show is open), and its backdrop once. */
  async loadShow() {
    try {
      this.show = await call<ShowView>('show_view');
    } catch {
      this.show = null;
      this.backdrop = null;
      this.backdropOf = '';
      return;
    }
    const key = `${this.show.file}|${this.show.theme.backdrop ?? ''}`;
    if (key !== this.backdropOf) {
      this.backdropOf = key;
      const b = await this.run(() => call<{ url: string | null }>('show_backdrop'));
      this.backdrop = b?.url ?? null;
    }
  }

  /** Opens a show by its folder (or makes a production folder a show with `create`). */
  async openShow(path: string, create = false) {
    if (!this.noProject) await this.closeProject();
    const v = await this.run(() => call<ShowView>(create ? 'create_show' : 'open_show', create ? { root: path } : { path }));
    if (!v) return;
    remember({ name: v.name, path: v.root, show: true });
    await this.loadShow();
  }

  /** Opens an episode of the open show (its project is made when it has none). */
  async openEpisode(folder: string) {
    if (!this.noProject && this.status?.show?.episode === folder) return;
    if (this.playing) await this.stop();
    this.busy = true;
    const r = await this.run(() => call<{ created: boolean }>('open_episode', { episode: folder }));
    this.busy = false;
    if (!r) return;
    this.resetEpisode();
    await this.boot();
    if (r.created) this.say(`${folder}: a new project in its Sound folder`);
  }

  /** Back to the show's home: the episode closes, the show stays open. */
  async showHome() {
    await this.closeProject();
    await this.loadShow();
  }

  async closeShow() {
    if (this.playing) await this.stop();
    await this.run(() => call('close_show'));
    this.resetEpisode();
    this.show = null;
    this.backdrop = null;
    this.backdropOf = '';
    this.noProject = true;
  }

  async setBackdrop(path: string) {
    const r = await this.run(() => call('set_show_backdrop', { path }));
    if (r) await this.loadShow();
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
      this.soundSel = [];
      this.soundOpen = null;
    }
    if (this.wantsSounds) await this.loadEdit();
  }

  /** The Edit page, and Arrange with its sounds layer, read the placed sounds. */
  private get wantsSounds() {
    return this.page === 'edit' || (this.page === 'arrange' && this.soundsOn);
  }

  async loadEdit() {
    if (!this.sceneId) return;
    this.edit = (await this.run(() => call<EditView>('edit_view', { scene: this.sceneId }))) ?? null;
  }

  async refresh() {
    if (!this.sceneId) return this.boot();
    await this.loadScene(this.sceneId);
  }

  /** The last command still on its way (undo and redo wait for it). */
  private inflight: Promise<unknown> = Promise.resolve();

  async cmd(c: Record<string, unknown>) {
    const done = (async () => {
      const r = await this.run(() => call('command', c));
      await this.refresh();
      return r;
    })();
    this.inflight = done.catch(() => {});
    return done;
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
    if (p !== 'import') this.importBack = null;
    if (this.wantsSounds) void this.loadEdit();
  }

  toggleSounds() {
    this.soundsOn = !this.soundsOn;
    try {
      localStorage.setItem('cutaway.sounds', this.soundsOn ? '1' : '0');
    } catch {
      /* private mode: the choice lasts for this session */
    }
    if (this.wantsSounds) void this.loadEdit();
  }

  /** ⇧B: between the editor and the full bucket, keeping scene and playhead. */
  toggleFullBucket() {
    if (this.page === 'import') {
      if (this.view) this.setPage(this.importBack ?? 'edit');
    } else {
      this.importTab = 'sounds';
      this.setPage('import');
    }
  }

  /** The room's home: the Rooms tab of the project setup, at this scene. */
  openRooms() {
    this.importBack = this.page;
    this.roomsAsked = true;
    this.importTab = 'rooms';
    this.setPage('import');
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

  /** The mapping editor (Options only). */
  openMapping(target: MappingTarget) {
    if (this.page !== 'options') return;
    if (this.playing) void this.stop();
    this.mapping = target;
    this.overlay = 'mapping';
  }

  /** Makes a new take of a line from a stretch of a recording; it becomes the picked take. */
  async addTake(lineId: string, sourceId: string, t0: number, t1: number, text?: string) {
    const r = await this.method<{ take_no: number }>('add_take', { line_id: lineId, source_id: sourceId, t0, t1, text });
    if (!r) return false;
    this.curId = lineId;
    if (this.cur) this.pos = this.cur.start_s;
    this.say(`Take ${r.take_no} added`);
    return true;
  }

  /** Changes a take where it stands: its stretch, what it says, or both. */
  async editTake(takeId: string, change: { source_id?: string; t0?: number; t1?: number; text?: string }) {
    const r = await this.method<{ take_no: number }>('edit_take', { take_id: takeId, ...change });
    if (!r) return false;
    if (this.cur) this.pos = this.cur.start_s;
    this.say(`Take ${r.take_no} saved`);
    return true;
  }

  /** Adds a line after an item of the scene (first without one) and goes to it. */
  async addLine(after: string | null, character: string, sourceId: string, t0: number, t1: number, text: string) {
    const r = await this.method<{ line_id: string }>('add_line', { scene: this.sceneId, after, character, source_id: sourceId, t0, t1, text });
    if (!r) return false;
    this.curId = r.line_id;
    if (this.cur) this.pos = this.cur.start_s;
    this.say('Line added');
    return true;
  }

  askDelete(lineId: string) {
    this.deleting = lineId;
    this.overlay = 'delete-line';
  }

  async deleteLine(lineId: string) {
    if (this.playing) await this.stop();
    const r = await this.method<{ next: string | null }>('delete_line', { line_id: lineId, confirm: true });
    if (!r) return;
    if (this.curId === lineId) {
      this.curId = r.next;
      if (this.cur) this.pos = this.cur.start_s;
    }
    this.say('Line deleted. ⌘Z brings it back');
  }

  async deleteTake(takeId: string) {
    if (this.playing) await this.stop();
    if (await this.method('delete_take', { take_id: takeId, confirm: true })) this.say('Take deleted. ⌘Z brings it back');
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

  // a drop holds its preview until its command lands, so ⌘Z right after it must wait
  async undo() {
    await this.inflight;
    await this.method('undo');
  }

  async redo() {
    await this.inflight;
    await this.method('redo');
  }

  // ---- transport ----

  async play(from: number, until?: number) {
    const r = await this.run(() => call<{ playing: boolean }>('play', { scene: this.sceneId, from_s: from, until_s: until }));
    if (!r?.playing) return;
    this.audition = null;
    this.previewEnded();
    this.playing = true;
    this.pos = from;
    this.startPolling();
  }

  async playTake(takeId: string, from: number) {
    const r = await this.run(() => call<{ playing: boolean }>('play_take', { take_id: takeId, from_s: from }));
    if (!r?.playing) return;
    this.previewEnded();
    this.audition = takeId;
    this.playing = true;
    this.pos = from;
    this.startPolling();
  }

  /** Auditions a bucket sound (kept asset or preview) from a point in it, on the preview
   *  player. The backend stops the timeline for it; the playhead stays where it was. */
  async playSound(key: SoundKey, from: number, face: SoundFace) {
    const r = await this.run(() => call<{ playing: boolean; duration_s: number }>('play_sound', { ...key, from_s: from }));
    if (!r?.playing) return;
    this.transportStopped();
    this.preview = { ...face, id: soundId(key), key, duration_s: r.duration_s, pos: from, playing: true };
    this.startPreviewPolling();
  }

  /** Is this sound (asset or remote id) on the preview player, playing? */
  previewing(id: string) {
    return !!this.preview?.playing && this.preview.id === id;
  }

  toggleSound(key: SoundKey, face: SoundFace) {
    return this.previewing(soundId(key)) ? this.stopSound() : this.playSound(key, 0, face);
  }

  async stopSound() {
    await this.run(() => call('stop_sound'));
    this.previewEnded();
  }

  async stop() {
    await this.run(() => call('stop'));
    this.transportStopped();
    this.previewEnded();
  }

  private transportStopped() {
    this.playing = false;
    this.level = 0;
    this.stopPolling();
    if (this.audition) {
      // back on the scene timeline at the line being edited
      this.audition = null;
      if (this.cur) this.pos = this.cur.start_s;
    }
  }

  private previewEnded() {
    if (this.previewTimer) clearInterval(this.previewTimer);
    this.previewTimer = null;
    if (this.preview) this.preview = { ...this.preview, playing: false, pos: 0 };
  }

  private startPreviewPolling() {
    if (this.previewTimer) clearInterval(this.previewTimer);
    this.previewTimer = setInterval(async () => {
      try {
        const p = await call<{ position_s: number; playing: boolean }>('sound_playhead');
        if (!this.previewTimer || !this.preview) return;
        if (!p.playing) this.previewEnded();
        else this.preview.pos = p.position_s;
      } catch {
        this.previewEnded();
      }
    }, 33);
  }

  async togglePlay() {
    if (this.playing) return this.stop();
    if (this.page === 'edit' && this.cycle.on && this.cycle.to > this.cycle.from) {
      const from = this.pos >= this.cycle.from && this.pos < this.cycle.to ? this.pos : this.cycle.from;
      return this.play(from, this.cycle.to);
    }
    const c = this.cur;
    let from = this.pos;
    // with the sounds layer the playhead scrubs freely: a pause is where the sounds are
    const free = this.page === 'arrange' && this.soundsOn;
    if (!free && (!c || from < c.start_s - 0.001 || from >= c.end_s)) from = c?.start_s ?? 0;
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
        if (this.follow && !this.audition) {
          const it = this.lineAt(p.position_s);
          if (it && it.line_id !== this.curId) this.curId = it.line_id;
        }
        if (!p.playing) {
          this.playing = false;
          this.level = 0;
          this.stopPolling();
          const c = this.cycle;
          if (this.page === 'edit' && c.on && c.to > c.from && p.position_s >= c.to - 0.05) void this.play(c.from, c.to);
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
