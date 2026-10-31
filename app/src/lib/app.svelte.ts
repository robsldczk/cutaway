// UI state. Document state comes only from the core (view models); every change is a
// command sent through the bridge. Local state here is selection, page, playhead, overlays.
import { call, BridgeError } from './bridge';
import type { EditView, ItemView, SceneView, Status } from './types';

export type Page = 'options' | 'arrange' | 'edit';
export type Overlay = null | 'help' | 'editor' | 'settings' | 'history' | 'export' | 'import';
export type Drawer = null | 'inspector' | 'unmatched' | 'bucket';

class AppState {
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
  locator = $state(0);
  scale = $state(1);
  overlay = $state<Overlay>(null);
  drawer = $state<Drawer>(null);
  anim = $state({ n: 0, dir: 'R', id: '' });
  toast = $state<string | null>(null);
  busy = $state(false);
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
    } else {
      this.view = null;
      this.overlay = 'import';
    }
    this.watch();
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

  setPage(p: Page) {
    this.page = p;
    if (p === 'edit') void this.loadEdit();
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
    this.playing = true;
    this.pos = from;
    this.startPolling();
  }

  async stop() {
    await this.run(() => call('stop'));
    this.playing = false;
    this.stopPolling();
  }

  async togglePlay() {
    if (this.playing) return this.stop();
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
        const p = await call<{ position_s: number; playing: boolean }>('playhead');
        this.pos = p.position_s;
        const it = this.lineAt(p.position_s);
        if (it && it.line_id !== this.curId) this.curId = it.line_id;
        if (!p.playing) {
          this.playing = false;
          this.stopPolling();
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
