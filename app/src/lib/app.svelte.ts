// UI state. Document state comes only from the core (view models); every change is a
// command sent through the bridge. Local state here is selection, page, playhead, overlays.
import { brand } from './brand';
import { call, choosePath, BridgeError } from './bridge';
import type { ExportScene, ExportSettings, ExportStatus } from './export';
import { attrsOf, attrsText, depthCommands, depthDelta, pasteCommand, type LineAttrs } from './lines';
import type { Note, NoteTag, ReviewerView, ReviewRole, ReviewView } from './review';
import { oneStep } from './sounds/select';
import type { DragSound, EditView, ItemView, Job, SceneSummary, SceneView, ShowView, SoundFace, SoundKey, SoundPreview, Status } from './types';

const soundId = (key: SoundKey) => key.asset_id ?? key.remote_id ?? key.clip?.id ?? '';

import { flushSpot, keepLast, keepSpot, landing, lastOpen, queueSpot, spotOf, type Spot } from './resume';

export type Page = 'import' | 'options' | 'arrange' | 'edit' | 'export' | 'dev' | 'review';
export type ImportTab = 'files' | 'sounds' | 'rooms';
export type Tool = 'pointer' | 'split' | 'fade' | 'gain' | 'marquee';
export type Overlay = null | 'help' | 'editor' | 'mapping' | 'delete-line' | 'settings' | 'history' | 'review-history';

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

/** How many items the shelf remembers. */
const RECENT = 24;

/** Recently opened episodes and shows (this viewer only), newest first. */
export function recentEpisodes(): RecentEpisode[] {
  try {
    const v = JSON.parse(localStorage.getItem('cutaway.recent') ?? '[]');
    return Array.isArray(v) ? v.filter((r) => r && typeof r.path === 'string') : [];
  } catch {
    return [];
  }
}

function keepRecent(list: RecentEpisode[]) {
  try {
    localStorage.setItem('cutaway.recent', JSON.stringify(list.slice(0, RECENT)));
  } catch {
    /* private mode: no recent list */
  }
}

export function remember(r: RecentEpisode) {
  keepRecent([r, ...recentEpisodes().filter((x) => x.path !== r.path)]);
}

/** Off the list. The files stay where they are. */
export function forgetRecent(paths: string[]) {
  keepRecent(recentEpisodes().filter((r) => !paths.includes(r.path)));
}

/** An item found again in a new place keeps its place in the list. */
export function relinkRecent(from: string, to: string) {
  keepRecent(recentEpisodes().filter((r) => r.path !== to).map((r) => (r.path === from ? { ...r, path: to } : r)));
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
  /** The episode that has landed on its remembered spot: until then nothing overwrites it. */
  private resumed = $state('');
  /** The spot the scene being loaded lands on. */
  private arriving: Spot | null = null;
  /** A launch reopens what was left open, once. */
  private reopened = false;
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
  /** Arrange: where the sound being moved sits, for the marker on the blocks (D128). */
  soundCue = $state<{ t: number; tone: string } | null>(null);
  /** Arrange: lines selected together (⌘ or ⇧ click). Empty is the current line on its own. */
  lineSel = $state<string[]>([]);
  /** What was copied from a line, for as long as the episode is open. */
  lineAttrs = $state<LineAttrs | null>(null);
  /** Depths on their way to the document, by line. */
  depthPreview = $state<Record<string, number>>({});
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
  /** What the review screen shows around the scene. */
  review = $state<ReviewView | null>(null);
  /** The round looked at; the current one until another is picked. */
  reviewRound = $state<string | null>(null);
  /** Who's writing notes here. */
  reviewer = $state<ReviewerView | null>(null);
  /** "Who's reviewing?" is up. `then` carries on with what asked. */
  asking = $state<{ then?: () => void; closable: boolean } | null>(null);
  /** The page editing was on before review. */
  private reviewFrom: Page = 'arrange';
  /** The note to bring into view, wherever notes show. */
  noteFocus = $state<string | null>(null);
  /** The scene sounds panel: its sounds, or the scene's notes. */
  panelTab = $state<'sounds' | 'notes'>('sounds');
  /** Keys on the review screen. */
  reviewUi: { marker(): void; note(): void; search(): void; cancel(): boolean; wheel(e: WheelEvent): boolean } | null = null;
  /** The welcome screen sees keys and drops first, while it's up. */
  welcomeUi: { key(e: KeyboardEvent): boolean; drop(paths: string[], x: number, y: number): Promise<void> } | null = null;
  /** What's opening, shown over everything for as long as it takes. `image` is a CSS background. */
  opening = $state<{ title: string; kicker: string; mode: 'edit' | 'review'; image: string } | null>(null);
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
      if (e instanceof BridgeError && (e.code === 'scene_locked' || e.code === 'review_only')) this.refuse(e.message);
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
      // a package opened for review is no place to come back to as an editor
      if (this.status.path && !this.status.review.package) remember({ name: this.status.show ? `${this.status.show.name} · ${this.status.name}` : this.status.name, path: this.status.path });
      if (this.status.show) await this.loadShow();
      else this.show = null;
      if (this.status.warnings.length && this.warned !== this.status.path) {
        this.warned = this.status.path;
        this.say(this.status.warnings.join(' '));
      }
    } catch (e) {
      if (e instanceof BridgeError && e.code === 'no_project') {
        if (await this.reopen()) return;
        this.noProject = true;
        await this.loadShow();
        return;
      }
      this.say(String(e));
      return;
    }
    void this.loadReviewer();
    if (this.status?.review.active) {
      await this.bootReview();
    } else if (this.status && this.status.scenes > 0) {
      await this.resume();
      if (this.status.takes === 0) this.page = 'import';
      void this.loadExports();
    } else {
      this.view = null;
      this.page = 'import';
    }
    this.resumed = this.status?.path ?? '';
    if (this.status?.path && !this.status.review.package) keepLast({ name: this.status.name, path: this.status.path });
    this.watch();
  }

  // ---- review ----

  /** The bus takes notes only, here or in a package opened for review. */
  get inReview(): boolean {
    return !!this.status?.review.active;
  }

  /** Opens an episode or a show read-only, to review it. */
  async openForReview(path: string) {
    if (!this.noProject) await this.closeProject();
    this.busy = true;
    const r = await this.run(() => call<{ file?: string; show?: ShowView }>('open_review', { path }));
    this.busy = false;
    if (!r) return false;
    if (r.show) await this.loadShow();
    else await this.boot();
    return true;
  }

  /** Opens an episode file to edit it. */
  async openFile(path: string) {
    if (!this.noProject) await this.closeProject();
    const r = await this.run(() => call('open_project', { path }));
    if (!r) return false;
    await this.boot();
    return true;
  }

  /** The synthetic demo session, made where it's asked for and opened. */
  async demo() {
    const parent = await choosePath({ directory: true, title: 'Folder for the demo session' });
    if (!parent) return;
    this.busy = true;
    const r = await this.run(() => call('create_demo', { parent, name: `${brand.displayName} demo` }));
    this.busy = false;
    if (r) await this.boot();
  }

  /** Into review mode, from the Export page or the episode menu. */
  async enterReview() {
    if (this.page !== 'review') this.reviewFrom = this.page;
    if (this.playing) await this.stop();
    await this.method('enter_review', {}, false);
    this.status = await call<Status>('project_status');
    await this.bootReview();
  }

  private async bootReview() {
    this.drawer = null;
    this.overlay = null;
    this.page = 'review';
    await this.loadReviewer();
    if (!this.status?.scenes) {
      this.say('This episode has no scenes to review yet');
      return;
    }
    // a package opens on the round's first scene, not on whatever comes first in the script
    if (!this.sceneId) {
      const v = await this.run(() => call<ReviewView>('review_view'));
      await this.loadScene(v?.rounds.find((r) => r.id === v.round)?.scenes[0]?.id ?? '1');
    } else {
      await this.loadScene(this.sceneId);
    }
    this.askOnEntry();
  }

  async loadReviewer() {
    this.reviewer = (await this.run(() => call<ReviewerView>('reviewer'))) ?? null;
  }

  /** Asked once: someone remembered on this computer goes straight in. */
  private askOnEntry() {
    if (!this.reviewer?.current) this.asking = { closable: false };
  }

  /** Done: back to editing, or out of a package altogether. */
  async leaveReview() {
    if (this.playing) await this.stop();
    if (this.status?.review.package) {
      await this.closeProject();
      return;
    }
    await this.method('leave_review', {}, false);
    this.status = await call<Status>('project_status');
    this.review = null;
    this.reviewRound = null;
    this.setPage(this.reviewFrom);
    await this.refresh();
  }

  async loadReview() {
    const scene = this.sceneId;
    if (!scene) return;
    const v = await this.run(() => call<ReviewView>('review_view', { scene, round: this.reviewRound ?? undefined }));
    // the scene may have changed while it was on its way
    if (v && scene === this.sceneId) this.review = v;
  }

  async pickRound(id: string) {
    this.reviewRound = id;
    await this.loadReview();
    const first = this.review?.rounds.find((r) => r.id === id)?.scenes[0];
    if (first && !this.review?.rounds.find((r) => r.id === id)?.scenes.some((s) => s.id === this.sceneId)) await this.loadScene(first.id);
  }

  /** The next scene of the round, or of the episode when there's no round. */
  stepScene(d: number) {
    const all = this.view?.scenes ?? [];
    const on = this.review?.scenes.filter((s) => s.in_round).map((s) => s.id) ?? all.map((s) => s.id);
    const i = on.indexOf(this.sceneId);
    const next = i < 0 ? on[0] : on[i + d];
    if (next) void this.loadScene(next);
  }

  /** Editing pages reply and resolve too: the first time, they say who's doing it. */
  whoFirst(then: () => void) {
    if (this.reviewer?.current) then();
    else this.asking = { then, closable: true };
  }

  async setReviewer(name: string, role: ReviewRole, remember: boolean) {
    this.reviewer = (await this.run(() => call<ReviewerView>('set_reviewer', { name, role, remember }))) ?? this.reviewer;
    if (this.inReview) await this.loadReview();
  }

  /** Writes a note, a reply, a resolve; the view comes back from the core. */
  async note<T = Note>(method: string, params: object) {
    const r = await this.run(() => call<T>(method, params));
    await this.refresh();
    return r;
  }

  addNote(target: object, text: string, tag: NoteTag | null) {
    return this.note('add_note', { ...target, text, tag: tag ?? undefined, round: this.reviewRound ?? undefined });
  }

  /** The scenes go out for review, and review opens on them. */
  async startReview(scenes: string[]) {
    await this.loadReviewer();
    // the round says who picked its scenes
    if (!this.reviewer?.current) {
      this.asking = { then: () => void this.startReview(scenes), closable: true };
      return;
    }
    const r = await this.run(() => call<{ round: string; number: number }>('start_review_round', { scenes }));
    if (!r) return;
    this.reviewRound = null;
    await this.refresh();
    if (!scenes.includes(this.sceneId) && scenes[0]) await this.loadScene(scenes[0]);
    await this.enterReview();
  }

  /** From a marker on an editing page: to the note, in the scene's panel. */
  openNote(id: string) {
    this.noteFocus = id;
    this.panelTab = 'notes';
    if (this.page !== 'edit' && this.page !== 'arrange') this.setPage('arrange');
    if (this.page === 'arrange' && !this.soundsOn) this.toggleSounds();
    this.setBucket(true);
  }

  /** At launch, opens what was open at the end. True when an episode took over the boot. */
  private async reopen() {
    if (this.reopened) return false;
    this.reopened = true;
    const last = lastOpen();
    if (!last) return false;
    // off the record until it has opened: a file that brings the app down isn't tried at every launch
    keepLast(null);
    this.busy = true;
    try {
      await call(last.show ? 'open_show' : 'open_project', { path: last.path });
    } catch (e) {
      this.say(`${last.name} couldn't be reopened. ${e instanceof BridgeError ? e.message : String(e)}`);
      return false;
    } finally {
      this.busy = false;
    }
    if (last.show) {
      keepLast(last);
      return false;
    }
    await this.boot();
    return true;
  }

  /** The first load of an episode lands where it was left. After that it stays on its scene. */
  private async resume() {
    const path = this.status?.path ?? '';
    const spot = this.resumed === path ? null : spotOf(path);
    if (!spot) return this.loadScene(this.sceneId || '1');
    this.page = spot.page === 'dev' && !this.dev ? 'options' : spot.page;
    this.importTab = spot.tab;
    this.editPx = spot.zoom;
    this.arriving = spot;
    try {
      await this.loadScene(spot.scene);
    } catch {
      // the scene has gone since: the page is still the right one
      await this.loadScene('1');
    } finally {
      this.arriving = null;
    }
  }

  /** Where Rob is in this episode, or null until it has landed. */
  get spot(): Spot | null {
    if (!this.status?.path || this.resumed !== this.status.path || !this.sceneId) return null;
    // a review isn't a place in the episode to come back to, and a package isn't Rob's to mark
    if (this.status.review.package) return null;
    return {
      page: this.page === 'review' ? this.reviewFrom : this.page,
      tab: this.importTab,
      scene: this.sceneId,
      line: this.curId,
      at: this.audition ? this.locator : this.pos,
      zoom: this.editPx,
    };
  }

  /** Called whenever the spot changes. */
  noteSpot(spot: Spot | null) {
    if (spot && this.status?.path) queueSpot(this.status.path, spot);
  }

  /** Back to the start screen: the core snapshots unsaved changes and closes the episode. */
  async closeProject() {
    if (this.playing) await this.stop();
    const r = await this.run(() => call<{ snapshot: string | null }>('close_project'));
    if (!r) return;
    const show = this.status?.review.package ? null : this.status?.show;
    this.resetEpisode();
    // closed on purpose: the next launch comes back to the show's home, or to the start screen
    keepLast(show ? { name: show.name, path: show.root, show: true } : null);
  }

  /** Forgets the open episode's view state (the core has closed or replaced it). */
  private resetEpisode() {
    const spot = this.spot;
    if (spot && this.status?.path) keepSpot(this.status.path, spot);
    flushSpot();
    this.resumed = '';
    this.editPx = 0;
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
    this.soundCue = null;
    this.lineSel = [];
    this.lineAttrs = null;
    this.review = null;
    this.reviewRound = null;
    this.reviewer = null;
    this.asking = null;
    this.noteFocus = null;
    this.panelTab = 'sounds';
    this.depthPreview = {};
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
    if (!v) return false;
    remember({ name: v.name, path: v.root, show: true });
    keepLast({ name: v.name, path: v.root, show: true });
    await this.loadShow();
    return true;
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
    keepLast(null);
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
        // what others said comes in here, and what's said here goes out
        await call('review_sync');
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
      this.lineSel = [];
      // with the view, not after it: the pages read the playhead as they appear
      if (this.arriving?.scene === v.scene.id) {
        const { at, line } = landing(this.arriving, v.duration_s, this.lines);
        this.pos = at;
        this.locator = at;
        this.curId = line ?? this.lineAt(at)?.line_id ?? this.curId;
      }
    }
    if (this.wantsSounds) await this.loadEdit();
    if (this.inReview) await this.loadReview();
  }

  /** The Edit page, Arrange with its sounds layer and the review screen read the placed sounds. */
  private get wantsSounds() {
    return this.page === 'edit' || this.page === 'review' || (this.page === 'arrange' && this.soundsOn);
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
    this.lineSel = [];
    if (!keepPassage) this.passageTo = null;
    if (!this.playing) {
      this.pos = it.start_s;
      this.locator = it.start_s;
    }
  }

  /** What a line command acts on: the lines selected, or the current one. */
  get pickedLines(): ItemView[] {
    const sel = this.lines.filter((l) => this.lineSel.includes(l.line_id));
    const cur = this.cur;
    return sel.length ? sel : cur ? [cur] : [];
  }

  depthOf(l: ItemView) {
    return this.depthPreview[l.line_id] ?? l.depth;
  }

  previewDepth(ids: string[], depth: (id: string) => number) {
    const next = { ...this.depthPreview };
    for (const id of ids) next[id] = depth(id);
    this.depthPreview = next;
  }

  /** Sends depths as one undo step, and holds the preview until the view has them. */
  async setLineDepth(ids: string[], depth: (id: string) => number) {
    this.previewDepth(ids, depth);
    const c = oneStep(depthCommands(ids, depth));
    try {
      if (c) await this.cmd(c);
    } finally {
      const next = { ...this.depthPreview };
      for (const id of ids) delete next[id];
      this.depthPreview = next;
    }
  }

  private depthBurst: { base: Map<string, number>; delta: number; timer: ReturnType<typeof setTimeout> } | null = null;

  /** ⌥← / ⌥→: the keys come in bursts, and a burst is one undo step. A group moves as one. */
  nudgeLineDepth(delta: number) {
    const lines = this.pickedLines.filter((l) => l.takes.length > 0);
    if (!lines.length) return;
    const same = this.depthBurst && lines.every((l) => this.depthBurst?.base.has(l.line_id));
    if (!same) this.flushLineDepth();
    const b = this.depthBurst ?? { base: new Map(lines.map((l) => [l.line_id, this.depthOf(l)])), delta: 0, timer: setTimeout(() => {}) };
    clearTimeout(b.timer);
    b.delta = depthDelta([...b.base.values()], b.delta + delta);
    b.timer = setTimeout(() => this.flushLineDepth(), 400);
    this.depthBurst = b;
    this.previewDepth([...b.base.keys()], (id) => (b.base.get(id) ?? 0) + b.delta);
  }

  flushLineDepth() {
    const b = this.depthBurst;
    if (!b) return;
    this.depthBurst = null;
    clearTimeout(b.timer);
    void this.setLineDepth([...b.base.keys()], (id) => (b.base.get(id) ?? 0) + b.delta);
  }

  /** ⌥⌘C: how a line sounds, for other lines. */
  copyAttrs(line: ItemView | undefined) {
    if (!line) return;
    this.lineAttrs = attrsOf({ ...line, depth: this.depthOf(line) });
    this.say(`Copied: ${attrsText(this.lineAttrs)}`);
  }

  /** ⌥⌘V: on every line given, as one undo step. */
  async pasteAttrs(ids: string[]) {
    if (!this.lineAttrs) {
      this.say('Copy a line’s attributes first: ⌥⌘C');
      return;
    }
    const c = pasteCommand(ids, this.lineAttrs);
    if (!c) return;
    await this.cmd(c);
    this.say(ids.length > 1 ? `Pasted on ${ids.length} lines` : 'Pasted');
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
