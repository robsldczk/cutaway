import { expect, test, type Locator, type Page } from '@playwright/test';
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// The sounds layer on Arrange, on the demo session: real commands through the bridge.

const bridge = 'http://127.0.0.1:7358';

async function rpc<T = unknown>(method: string, params: object = {}): Promise<T> {
  const r = await fetch(`${bridge}/rpc`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  const j = (await r.json()) as { ok: boolean; result: T; error?: { message: string } };
  if (!j.ok) throw new Error(j.error?.message ?? method);
  return j.result;
}

/** A mono 16-bit WAV of noise. */
function wav(path: string, seconds: number) {
  const rate = 48000;
  const n = Math.round(rate * seconds);
  const b = Buffer.alloc(44 + n * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + n * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(rate, 24);
  b.writeUInt32LE(rate * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.round((Math.random() * 2 - 1) * 6000), 44 + i * 2);
  writeFileSync(path, b);
}

type Sound = { id: string; lane: string; start_s: number; offset_s: number; len_s: number; depth: number; pan: number; gain_db: number; anchor: { line_id: string; word: number | null; edge: string } | null; fade_out_s: number; auto_fade: boolean };
const list = () => rpc<Sound[]>('list_sounds', { scene: '1' });

let asset = '';
let bucket = '';

test.beforeAll(async () => {
  const dir = mkdtempSync(join(tmpdir(), 'sounds-'));
  const file = join(dir, 'Foil_crackle.wav');
  wav(file, 2.5);
  const r = await rpc<{ assets: { id: string }[] }>('import_sounds', { paths: [file] });
  asset = r.assets[0].id;
  const sc = await rpc<{ id: string }[]>('list_scenes');
  // a failed test restarts the worker, and this runs again
  const have = await rpc<{ buckets: { id: string; name: string }[] }>('buckets', {});
  bucket = have.buckets.find((b) => b.name === 'Layer test sounds')?.id ?? (await rpc<{ bucket_id: string }>('create_bucket', { name: 'Layer test sounds', keywords: ['foil'], scenes: [sc[0].id] })).bucket_id;
  await rpc('bucket_keep', { bucket_id: bucket, asset_id: asset });
});

test.beforeEach(async () => {
  for (const s of await list()) await rpc('remove_sound', { placement_id: s.id });
});

async function place(at_s: number, depth: number): Promise<string> {
  const r = await rpc<{ id: string }>('place_from_bucket', { scene: '1', asset_id: asset, bucket_id: bucket, at_s, depth });
  return r.id;
}

async function open(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByRole('button', { name: 'Arrange', exact: false }).click();
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
}

/** The demo session is shared by every spec: other buckets may come first. */
const foil = (page: Page) => page.getByRole('region', { name: 'Layer test sounds' });

async function bucketRow(page: Page) {
  const sec = foil(page);
  await expect(sec).toBeVisible();
  const row = sec.getByTestId('bucket-sound');
  if (!(await row.count())) await sec.locator('button[aria-expanded]').click();
  await expect(row.first()).toBeVisible();
  await row.first().scrollIntoViewIfNeeded();
  return row.first();
}

const ballOf = (page: Page, id: string) => page.locator(`[data-sound="${id}"]`);

async function centre(l: Locator) {
  const b = (await l.boundingBox())!;
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

const undoSteps = async () => (await rpc<{ state: string }[]>('history', { limit: 100000 })).filter((r) => r.state === 'done').length;
const sound = async (id: string) => (await list()).find((s) => s.id === id)!;

/** Runs a gesture and checks that it was one undo step, whose undo brings back what was. */
async function oneStep(page: Page, gesture: () => Promise<void>) {
  const before = await list();
  const steps = await undoSteps();
  await gesture();
  await expect.poll(undoSteps).toBe(steps + 1);
  const after = await list();
  expect(after).not.toEqual(before);
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(list).toEqual(before);
  await page.keyboard.press('ControlOrMeta+Shift+z');
  await expect.poll(list).toEqual(after);
  return after;
}

/** A drag in steps, as a hand does it. */
async function drag(page: Page, from: { x: number; y: number }, dx: number, dy: number) {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  for (let i = 1; i <= 6; i++) await page.mouse.move(from.x + (dx * i) / 6, from.y + (dy * i) / 6);
  await page.mouse.up();
}

test('sounds sit beside the dialogue: spine, lines, balls and the bucket', async ({ page }) => {
  const a = await place(0.5, 0.1);
  await place(1.5, 0.8);
  await open(page);
  await expect(page.getByTestId('sound-ball')).toHaveCount(2);
  await expect(ballOf(page, a)).toBeVisible();
  await bucketRow(page);
  await page.waitForTimeout(800);
  await page.screenshot({ path: 'test-results/sounds-layer.png' });
});

test('add: a bucket row dropped beside a line snaps, and the drop sets the depth', async ({ page }) => {
  await open(page);
  const row = page.getByTestId('arrange-row').nth(1);
  const r = (await row.boundingBox())!;
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  await (await bucketRow(page)).dragTo(page.getByTestId('sounds-field'), { targetPosition: { x: 330, y: r.y - field.y + 1 } });
  await expect.poll(async () => (await list()).length).toBe(1);
  const [s] = await list();
  expect(s.anchor).toMatchObject({ edge: 'start' });
  expect(s.depth).toBeGreaterThan(0.3);
  expect(s.depth).toBeLessThan(0.9);
  expect(s.auto_fade).toBe(true);
  await expect(page.getByTestId('sound-ball')).toHaveCount(1);
  await expect(page.getByTestId('loudness')).toBeVisible();
});

/** The desktop webview: Tauri's file drop takes every drag, so the page never sees one. */
const noHtmlDrag = (page: Page) =>
  page.addInitScript(() => {
    for (const t of ['dragstart', 'dragenter', 'dragover', 'drop'])
      window.addEventListener(
        t,
        (e) => {
          e.preventDefault();
          e.stopImmediatePropagation();
        },
        true,
      );
  });

test('add: a bucket row lands where the desktop webview sees no HTML drag at all', async ({ page }) => {
  await noHtmlDrag(page);
  await open(page);
  const from = await centre(await bucketRow(page));
  const r = (await page.getByTestId('arrange-row').nth(1).boundingBox())!;
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  await drag(page, from, field.x + 330 - from.x, r.y + 1 - from.y);
  await expect.poll(async () => (await list()).length).toBe(1);
  await expect(page.getByTestId('sound-ball')).toHaveCount(1);
  await expect(page.getByTestId('drag-ghost')).toHaveCount(0);
});

test('add: on its way the sound follows the pointer, Esc calls it off, and the balls stay still', async ({ page }) => {
  const id = await place(1.5, 0.5);
  await open(page);
  const from = await centre(await bucketRow(page));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x - 30, from.y + 10);
  await expect(page.getByTestId('drag-ghost')).toContainText('Foil_crackle');
  // over a ball nothing wakes up (5.1)
  const ball = await centre(ballOf(page, id));
  await page.mouse.move(ball.x, ball.y);
  await expect(page.getByTestId('drag-ghost')).toHaveCount(0);
  await expect(page.locator('.line.hover')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect(page.getByTestId('drag-ghost')).toHaveCount(0);
  await page.waitForTimeout(150);
  expect(await list()).toHaveLength(1);
});

test('add: a press on a row with no drag is no drag, and its Play button stays a button', async ({ page }) => {
  await open(page);
  await (await bucketRow(page)).click();
  await page.waitForTimeout(150);
  expect(await list()).toHaveLength(0);
  await expect(page.getByTestId('drag-ghost')).toHaveCount(0);
});

test('add: a bucket row drops onto an Edit lane', async ({ page }) => {
  await noHtmlDrag(page);
  await open(page);
  await page.keyboard.press('3');
  const lane = page.locator('[data-testid="edit-lane"][data-lane="sfx1"]').first();
  await expect(lane).toBeVisible();
  await (await bucketRow(page)).dragTo(lane, { targetPosition: { x: 200, y: 12 } });
  await expect.poll(async () => (await list()).length).toBe(1);
});

test('no HTML drag and drop anywhere: it never fires in the desktop app', () => {
  const files = readdirSync('src', { recursive: true, encoding: 'utf8' }).filter((f) => f.endsWith('.svelte'));
  const used = files.filter((f) => /draggable=|ondrag\w*=|ondrop=/.test(readFileSync(join('src', f), 'utf8')));
  expect(used).toEqual([]);
});

test('preview: Play in a bucket has its own player, and the timeline stays where it is', async ({ page }) => {
  await open(page);
  await page.getByTestId('arrange-row').nth(2).click();
  const world = page.getByRole('group', { name: 'Sounds of the scene' });
  const row = await bucketRow(page);
  await page.waitForTimeout(400);
  const at = await world.getAttribute('style');
  const head = (await rpc<{ position_s: number }>('playhead')).position_s;

  await row.getByRole('button', { name: /^Play / }).click();

  const player = page.getByTestId('preview-player');
  await expect(player).toContainText('Foil_crackle');
  await expect(row.getByRole('button', { name: /^Play / })).toBeVisible();
  // it runs on its own clock …
  await expect.poll(async () => (await rpc<{ position_s: number }>('sound_playhead')).position_s).toBeGreaterThan(0.3);
  await expect(page.getByTestId('preview-time')).not.toContainText('0:00 /');
  // … and the main transport never heard of it
  await expect(page.getByRole('button', { name: 'Play or stop (Space)' })).toHaveAttribute('data-playing', 'false');
  expect(await world.getAttribute('style')).toBe(at);
  const main = await rpc<{ position_s: number; playing: boolean }>('playhead');
  expect(main.playing).toBe(false);
  expect(main.position_s).toBe(head);

  // Space stops the preview, and only then does it play the timeline. The sound is short:
  // from its start again, so it can't run out first
  const w = (await page.getByTestId('preview-wave').boundingBox())!;
  await page.mouse.click(w.x + 1, w.y + w.height / 2);
  await expect(player.getByRole('button', { name: /^Stop preview/ })).toBeVisible();
  await page.keyboard.press('Space');
  await expect.poll(async () => (await rpc<{ playing: boolean }>('sound_playhead')).playing).toBe(false);
  expect((await rpc<{ playing: boolean }>('playhead')).playing).toBe(false);
  await expect(player.getByRole('button', { name: /^Play preview/ })).toBeVisible();
  await page.keyboard.press('Space');
  await expect.poll(async () => (await rpc<{ playing: boolean }>('playhead')).playing).toBe(true);
  await rpc('stop');
});

test('preview: the transport takes over, the player seeks, and hiding the bucket stops it', async ({ page }) => {
  await open(page);
  const row = await bucketRow(page);
  const heard = async () => (await rpc<{ playing: boolean; position_s: number }>('sound_playhead'));
  await row.getByRole('button', { name: /^Play / }).click();
  await expect.poll(async () => (await heard()).playing).toBe(true);

  // a click on the player's waveform lands in the sound
  const w = (await page.getByTestId('preview-wave').boundingBox())!;
  await page.mouse.click(w.x + w.width * 0.8, w.y + w.height / 2);
  await expect.poll(async () => (await heard()).position_s).toBeGreaterThan(1.9);

  // the main Play stops it
  const play = page.getByRole('button', { name: 'Play or stop (Space)' });
  await play.click();
  await expect(play).toHaveAttribute('data-playing', 'true');
  await expect.poll(async () => (await heard()).playing).toBe(false);
  await play.click();
  await expect(play).toHaveAttribute('data-playing', 'false');

  // and a preview stops the main Play, leaving the playhead where it stopped
  await play.click();
  await expect(play).toHaveAttribute('data-playing', 'true');
  await row.getByRole('button', { name: /^Play / }).click();
  await expect(play).toHaveAttribute('data-playing', 'false');
  await expect.poll(async () => (await heard()).playing).toBe(true);

  await page.keyboard.press('b');
  await expect.poll(async () => (await heard()).playing).toBe(false);
});

test('loop: a long sound with Loop on drops as ambience, and is cropped with it off', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'loops-'));
  const file = join(dir, 'Rain_on_glass.wav');
  wav(file, 8);
  const r = await rpc<{ assets: { id: string }[] }>('import_sounds', { paths: [file], bucket_id: bucket });
  const long = r.assets[0].id;
  await rpc('bucket_set_loop', { bucket_id: bucket, asset_id: long, on: false });
  await open(page);
  await bucketRow(page);
  const row = foil(page).getByTestId('bucket-sound').filter({ hasText: 'Rain_on_glass' });
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  const y = (await page.getByTestId('arrange-row').nth(1).boundingBox())!.y;
  const toField = { targetPosition: { x: 330, y: y - field.y + 1 } };

  const loop = row.getByTestId('bucket-loop');
  await expect(loop).toHaveAttribute('aria-pressed', 'false');
  await row.dragTo(page.getByTestId('sounds-field'), toField);
  await expect.poll(async () => (await list()).length).toBe(1);
  expect((await rpc<{ looped: boolean }[]>('list_sounds', { scene: '1' }))[0].looped).toBe(false);

  await loop.click();
  await expect(loop).toHaveAttribute('aria-pressed', 'true');
  await row.dragTo(page.getByTestId('sounds-field'), toField);
  await expect.poll(async () => (await list()).length).toBe(2);
  const bed = (await rpc<{ looped: boolean; lane: string; len_s: number }[]>('list_sounds', { scene: '1' })).find((x) => x.looped)!;
  expect(bed.lane).toBe('ambience');
  await rpc('bucket_remove', { bucket_id: bucket, asset_id: long });
});

test('the Sounds toggle is one even move: nothing goes past where it is going', async ({ page }) => {
  await place(1, 0.5);
  await open(page);
  await expect(page.getByTestId('sound-ball')).toHaveCount(1);
  await page.waitForTimeout(1000);
  for (let i = 0; i < 4; i++) {
    // every frame of the move, measured: two motions can each ease and still add up to a swing
    const paths = await page.evaluate(
      () =>
        new Promise<Record<string, number[]>>((done) => {
          const row = document.querySelector('[data-testid="arrange-row"]')!;
          const panel = document.querySelector('aside.panel')!;
          const out: Record<string, number[]> = { left: [], top: [], width: [], panel: [] };
          const t0 = performance.now();
          window.dispatchEvent(new KeyboardEvent('keydown', { key: 's', bubbles: true }));
          const frame = () => {
            const b = row.getBoundingClientRect();
            out.left.push(b.left);
            out.top.push(b.top);
            out.width.push(b.width);
            out.panel.push(panel.getBoundingClientRect().left);
            if (performance.now() - t0 < 1000) requestAnimationFrame(frame);
            else done(out);
          };
          requestAnimationFrame(frame);
        }),
    );
    for (const [name, p] of Object.entries(paths)) {
      const way = Math.sign(p[p.length - 1] - p[0]);
      for (let k = 1; k < p.length; k++) expect((p[k] - p[k - 1]) * way, `${name} turned back at frame ${k}: ${p.join(' ')}`).toBeGreaterThanOrEqual(-0.5);
    }
    expect(Math.abs(paths.left[0] - paths.left[paths.left.length - 1])).toBeGreaterThan(100);
    await page.waitForTimeout(300);
  }
});

test('no easing in the app overshoots', () => {
  const files = readdirSync('src', { recursive: true, encoding: 'utf8' }).filter((f) => /\.(svelte|css|ts)$/.test(f));
  const over: string[] = [];
  for (const f of files)
    for (const m of readFileSync(join('src', f), 'utf8').matchAll(/cubic-bezier\(([^)]+)\)/g)) {
      const [, y1, , y2] = m[1].split(',').map(Number);
      if (y1 > 1 || y2 > 1 || y1 < 0 || y2 < 0) over.push(`${f}: ${m[0]}`);
    }
  expect(over).toEqual([]);
});

test('play: ⇧ Space plays from the start of the selected sound, the first of several', async ({ page }) => {
  const a = await place(2.5, 0.2);
  const b = await place(1.2, 0.7);
  await open(page);
  const head = () => rpc<{ position_s: number; playing: boolean }>('playhead');
  const play = page.getByRole('button', { name: 'Play or stop (Space)' });

  await ballOf(page, a).click();
  await page.keyboard.press('Shift+Space');
  await expect(play).toHaveAttribute('data-playing', 'true');
  let h = await head();
  expect(h.playing).toBe(true);
  expect(h.position_s).toBeGreaterThanOrEqual(2.5);
  expect(h.position_s).toBeLessThan(3.5);

  // again while it plays: back to the start of the sound
  await page.waitForTimeout(500);
  await page.keyboard.press('Shift+Space');
  await expect.poll(async () => (await head()).position_s).toBeLessThan(3);
  await page.keyboard.press('Space');
  await expect(play).toHaveAttribute('data-playing', 'false');

  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.press('Shift+Space');
  await expect(play).toHaveAttribute('data-playing', 'true');
  h = await head();
  expect(h.position_s).toBeGreaterThanOrEqual(1.2);
  expect(h.position_s).toBeLessThan(2.2);
  await page.keyboard.press('Space');

  // with nothing selected it's the locator's again
  await page.keyboard.press('Escape');
  await expect(ballOf(page, b)).not.toHaveClass(/sel/);
  await page.keyboard.press('Shift+Space');
  await expect(play).toHaveAttribute('data-playing', 'true');
  await rpc('stop');
});

test('search: the bucket narrows down, the library comes in, and a result drags like any sound', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'finds-'));
  const file = join(dir, 'Zebra_crossing.wav');
  wav(file, 1);
  await rpc('import_sounds', { paths: [file] });
  await open(page);
  await bucketRow(page);
  const box = page.getByTestId('bucket-search');

  // a word of a sound's name: its row stays, and other buckets step aside
  await box.fill('foil_cr');
  await expect(foil(page).getByTestId('bucket-sound').first()).toContainText('Foil_crackle');
  await expect(foil(page).getByTestId('bucket-sound').filter({ hasNotText: 'Foil_crackle' })).toHaveCount(0);
  await box.fill('zzz nothing');
  await expect(foil(page)).toHaveCount(0);
  await expect(page.getByText('No sound by that name.')).toBeVisible();

  // a sound in no bucket of this scene comes from the library, and plays and drags
  await box.fill('zebra');
  const found = page.getByTestId('library-sound').filter({ hasText: 'Zebra_crossing' });
  await expect(found).toHaveCount(1);
  await found.getByRole('button', { name: /^Play / }).click();
  await expect(page.getByTestId('preview-player')).toContainText('Zebra_crossing');
  await page.keyboard.press('Space');
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  const y = (await page.getByTestId('arrange-row').nth(1).boundingBox())!.y;
  await found.dragTo(page.getByTestId('sounds-field'), { targetPosition: { x: 330, y: y - field.y + 1 } });
  await expect.poll(async () => (await list()).length).toBe(1);

  // Esc clears it, and the buckets come back
  await box.focus();
  await page.keyboard.press('Escape');
  await expect(box).toHaveValue('');
  await expect(foil(page)).toBeVisible();
});

test('actions: they read between the lines, and a sound dropped on one hangs on it', async ({ page }) => {
  await open(page);
  const acts = page.getByTestId('action-row');
  await expect(acts.first()).toContainText('A kettle whistles');
  await expect(acts.last()).toContainText('The door opens');
  const spoken = (await rpc<{ line_id: string }[]>('list_lines', { scene: '1' })).map((l) => l.line_id);

  // on the action itself
  const row = acts.last();
  await (await bucketRow(page)).dragTo(row);
  await expect.poll(async () => (await list()).length).toBe(1);
  const [s] = await list();
  const action = s.anchor!.line_id;
  expect(spoken).not.toContain(action);
  await expect(row.getByTestId('action-tag')).toContainText('Foil_crackle');
  await expect(page.getByTestId('sound-ball')).toHaveCount(1);

  // beside it counts too, and the tooltip says what it's on
  await rpc('remove_sound', { placement_id: s.id });
  await expect(row.getByTestId('action-tag')).toHaveCount(0);
  // to the middle first: at the end of a long scene it can sit under the transport
  await row.click();
  await page.waitForTimeout(400);
  const b = (await row.boundingBox())!;
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  const from = await centre(await bucketRow(page));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x - 40, from.y);
  await page.mouse.move(field.x + 330, b.y + b.height / 2);
  await expect(page.getByTestId('sound-tip')).toContainText('On “The door opens');
  await page.mouse.up();
  await expect.poll(async () => (await list())[0]?.anchor?.line_id).toBe(action);
});

const fileDrag = (page: Page, call: string, ...args: unknown[]) =>
  page.evaluate(([c, a]) => (window as unknown as Record<string, Record<string, (...x: unknown[]) => unknown>>).cutawayFileDrag[c as string](...(a as unknown[])), [call, args] as const);

test('finder: a bucket called Ambience does not break the sheet', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const { bucket_id } = await rpc<{ bucket_id: string }>('create_bucket', { name: 'Ambience', keywords: ['ambience'] });
  const dir = mkdtempSync(join(tmpdir(), 'finder-'));
  const file = join(dir, 'Foil wrap crinkle.wav');
  wav(file, 1);
  await open(page);
  await bucketRow(page);
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  const y = (await page.getByTestId('arrange-row').nth(1).boundingBox())!.y;

  await fileDrag(page, 'drop', [file], field.x + 330, y + 1);
  const name = page.getByTestId('file-bucket');
  await expect(name).toBeFocused();
  await expect(name).not.toHaveValue('');
  await expect(page.getByTestId('file-chip').filter({ hasText: /^Ambience$/ })).toHaveCount(1);
  expect(errors).toEqual([]);
  await page.keyboard.press('Escape');
  await rpc('delete_bucket', { bucket_id, confirm: true });
});

test('finder: drop, Enter: the name guesses the bucket, and the sound is filed and in the scene', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'finder-'));
  const byName = join(dir, 'Foil wrap crinkle.wav');
  wav(byName, 1);
  await open(page);
  await bucketRow(page);
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  const y = (await page.getByTestId('arrange-row').nth(1).boundingBox())!.y;

  await fileDrag(page, 'drop', [byName], field.x + 330, y + 1);
  const name = page.getByTestId('file-bucket');
  await expect(name).toHaveValue('Layer test sounds');
  await expect(name).toBeFocused();
  await expect(page.getByTestId('file-say')).toContainText('Into this bucket');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('file-sheet')).toHaveCount(0);
  await expect.poll(async () => (await list()).length).toBe(1);
  expect((await list())[0].anchor).toMatchObject({ edge: 'start' });
  const kept = await rpc<{ buckets: { id: string; kept: { name: string }[] }[] }>('buckets', {});
  expect(JSON.stringify(kept.buckets.find((b) => b.id === bucket)?.kept)).toContain('Foil wrap crinkle');
});

test('finder: a name no bucket has proposes one, and typing another makes that bucket', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'finder-'));
  const file = join(dir, 'ES_Whistle, Referee, Short - Epidemic Sound.wav');
  wav(file, 1);
  await open(page);
  await bucketRow(page);
  const c = await centre(page.getByTestId('sounds-field'));
  await fileDrag(page, 'drop', [file], c.x, c.y);
  const name = page.getByTestId('file-bucket');
  await expect(name).toHaveValue('Whistle');
  await expect(page.getByTestId('file-say')).toContainText('New bucket');
  await name.fill('Whistles of the test');
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await list()).length).toBe(1);
  const v = await rpc<{ buckets: { id: string; name: string; kept: unknown[] }[] }>('buckets', {});
  const made = v.buckets.find((b) => b.name === 'Whistles of the test')!;
  expect(made.kept).toHaveLength(1);
  await rpc('delete_bucket', { bucket_id: made.id, confirm: true });
});

test('finder: a chip picks the bucket, typing narrows the chips, and Esc leaves nothing behind', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'finder-'));
  const file = join(dir, 'Unheard of.wav');
  wav(file, 1);
  await open(page);
  await bucketRow(page);
  const c = await centre(page.getByTestId('sounds-field'));
  const before = await rpc<{ id: string }[]>('list_library');

  // Esc: nothing imported, nothing placed
  await fileDrag(page, 'drop', [file], c.x, c.y);
  const name = page.getByTestId('file-bucket');
  await expect(name).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('file-sheet')).toHaveCount(0);
  expect(await list()).toHaveLength(0);
  expect(await rpc<{ id: string }[]>('list_library')).toHaveLength(before.length);

  await fileDrag(page, 'drop', [file], c.x, c.y);
  await expect(name).toHaveValue('Unheard of');
  await name.fill('layer te');
  const chips = page.getByTestId('file-chip');
  await expect(chips).toHaveCount(1);
  await chips.first().click();
  await expect(name).toHaveValue('Layer test sounds');
  await expect(name).toBeFocused();
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await list()).length).toBe(1);
});

test('finder: the arrows walk the chips and Enter takes the one lit', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'finder-'));
  const file = join(dir, 'Unheard of.wav');
  wav(file, 1);
  await open(page);
  await bucketRow(page);
  const c = await centre(page.getByTestId('sounds-field'));
  await fileDrag(page, 'drop', [file], c.x, c.y);
  const name = page.getByTestId('file-bucket');
  await expect(name).toHaveValue('Unheard of');
  const lit = page.locator('.chip.lit');
  await expect(lit).toHaveCount(0);

  await page.keyboard.press('ArrowDown');
  await expect(lit).toHaveText('Ambience');
  await expect(page.getByTestId('file-say')).toContainText('Loops under');
  await page.keyboard.press('ArrowRight');
  await expect(lit).not.toHaveText('Ambience');
  await page.keyboard.press('ArrowUp');
  await expect(lit).toHaveText('Ambience');
  // round the end
  await page.keyboard.press('ArrowLeft');
  await expect(lit).toHaveText((await page.getByTestId('file-chip').last().textContent())!);
  await expect(name).toHaveValue('Unheard of');

  // typing lets go of the chip: a new name still goes through with Enter
  await name.pressSequentially('!');
  await expect(lit).toHaveCount(0);

  await name.fill('layer te');
  await expect(page.getByTestId('file-chip')).toHaveCount(1);
  await page.keyboard.press('ArrowDown');
  await expect(page.getByTestId('file-say')).toContainText('Into Layer test sounds');
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await list()).length).toBe(1);
  const v = await rpc<{ buckets: { id: string; kept: { name: string }[] }[] }>('buckets', {});
  expect(JSON.stringify(v.buckets.find((b) => b.id === bucket)?.kept)).toContain('Unheard of');
});

test('finder: Shift alone completes the name, Shift with a letter is a capital', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'finder-'));
  const file = join(dir, 'Unheard of.wav');
  wav(file, 1);
  await open(page);
  await bucketRow(page);
  const c = await centre(page.getByTestId('sounds-field'));
  await fileDrag(page, 'drop', [file], c.x, c.y);
  const name = page.getByTestId('file-bucket');
  await expect(name).toHaveValue('Unheard of');

  await name.fill('layer');
  await name.press('Shift+T');
  await expect(name).toHaveValue('layerT');

  await name.fill('layer te');
  await page.keyboard.press('Shift');
  await expect(name).toHaveValue('Layer test sounds');
  await expect(page.getByTestId('file-say')).toContainText('Into this bucket');

  // with a chip lit it's that one
  await name.fill('a');
  await page.keyboard.press('ArrowDown');
  const want = await page.locator('.chip.lit').textContent();
  await page.keyboard.press('Shift');
  await expect(name).toHaveValue(want!);
  await page.keyboard.press('Escape');
});

test('finder: the Ambience chip gives the sound to the room, and nothing to the timeline', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'finder-'));
  const file = join(dir, 'Ambience, Empty Hall.wav');
  wav(file, 2);
  await open(page);
  await bucketRow(page);
  const c = await centre(page.getByTestId('sounds-field'));
  await fileDrag(page, 'drop', [file], c.x, c.y);
  const name = page.getByTestId('file-bucket');
  await expect(name).toHaveValue('Ambience');
  await expect(page.getByTestId('file-say')).toContainText('Loops under');
  await page.keyboard.press('Enter');
  const rooms = () => rpc<{ tone: { name: string; gain_db: number } | null }[]>('scene_rooms');
  await expect.poll(async () => (await rooms())[0].tone?.name).toContain('Empty Hall');
  expect((await rooms())[0].tone?.gain_db).toBe(-28);
  expect(await list()).toHaveLength(0);
  await rpc('set_scene_tone', { scene: '1' });
});

test('directions: the ones before the first line take time, and a sound can sit on them', async ({ page }) => {
  await open(page);
  const first = (await rpc<{ start_s?: number }[]>('list_lines', { scene: '1' }))[0];
  expect(first).toBeTruthy();
  const row = page.getByTestId('action-row').first();
  await expect(row).toContainText('A kettle whistles');
  const b = (await row.boundingBox())!;
  const lineTop = (await page.getByTestId('arrange-row').first().boundingBox())!.y;
  expect(b.y).toBeLessThan(lineTop);
  await (await bucketRow(page)).dragTo(row);
  await expect.poll(async () => (await list()).length).toBe(1);
  const [s] = await list();
  expect(s.start_s).toBe(0);
  expect(s.anchor).toMatchObject({ edge: 'start' });
});

test('directions: the bottom edge sets how long one takes, and what follows moves with it', async ({ page }) => {
  const id = await place(0, 0.5);
  const spoken = (await rpc<{ line_id: string }[]>('list_lines', { scene: '1' }))[0];
  await rpc('move_sound', { placement_id: id, anchor_line_id: spoken.line_id, offset_s: 0 });
  await open(page);
  const before = (await sound(id)).start_s;
  const row = page.getByTestId('action-row').first();
  const handle = row.getByTestId('action-len');
  const was = parseFloat((await handle.textContent()) ?? '');
  await page.waitForTimeout(400);
  const steps = await undoSteps();
  const c = await centre(handle.locator('.asec'));
  const tall = (await row.boundingBox())!.height;
  await page.mouse.move(c.x, c.y);
  await page.mouse.down();
  for (let i = 1; i <= 4; i++) await page.mouse.move(c.x, c.y + 7 * i);
  await expect(handle).toContainText(`${(was + 2).toFixed(1)} s`);
  // the block grows with it, however short it was: the edge stays under the mouse
  expect((await row.boundingBox())!.height).toBeCloseTo(tall + 28, -0.5);
  await page.mouse.up();

  await expect.poll(undoSteps).toBe(steps + 1);
  await expect.poll(async () => (await sound(id)).start_s).toBeGreaterThan(before + 1.9);

  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await sound(id)).start_s).toBeCloseTo(before, 2);
  await expect(handle).toContainText(`${was.toFixed(1)} s`);
});

test('names: no labels under the balls, only the selection, on a tab on the transport', async ({ page }) => {
  const a = await place(1, 0.3);
  const b = await place(2.5, 0.7);
  await open(page);
  await expect(page.getByTestId('sound-ball')).toHaveCount(2);
  const field = page.getByRole('group', { name: 'Sounds of the scene' });
  await expect(field.getByText('Foil_crackle')).toHaveCount(0);

  const tab = page.getByTestId('sound-tab');
  await expect(tab).not.toHaveClass(/on/);
  await ballOf(page, a).click();
  await expect(tab).toHaveClass(/on/);
  await expect(tab).toContainText('Foil_crackle');
  await expect(tab).toContainText('Layer test sounds');
  await page.waitForTimeout(450);

  // it stands on the transport's top edge
  const t = (await tab.boundingBox())!;
  const bar = (await page.getByRole('button', { name: 'Play or stop (Space)' }).locator('xpath=..').boundingBox())!;
  expect(Math.abs(t.y + t.height - bar.y)).toBeLessThan(1);
  await page.screenshot({ path: 'test-results/sound-tab.png' });

  await page.keyboard.press('ControlOrMeta+a');
  await expect(tab).toContainText('2 sounds selected');
  await page.keyboard.press('Escape');
  await expect(tab).not.toHaveClass(/on/);
  await expect(ballOf(page, b)).toHaveAttribute('aria-label', /Foil_crackle/);
});

test('nothing along the bottom: no hints under the transport, no word tags on the cards', async ({ page }) => {
  await open(page);
  await expect(page.getByText('Cuts a word')).toHaveCount(0);
  await expect(page.getByText(/all keys/)).toHaveCount(0);
  await expect(page.getByText(/scroll to zoom/)).toHaveCount(0);
  await expect(page.getByText(/to reorder/)).toHaveCount(0);
});

test('depth: sideways is depth, and small vertical motion leaves the timing alone', async ({ page }) => {
  const a = await place(1, 0.1);
  await open(page);
  const start = (await sound(a)).start_s;
  const after = await oneStep(page, () => centre(ballOf(page, a)).then((c) => drag(page, c, 120, 8)));
  const s = after.find((x) => x.id === a)!;
  expect(s.depth).toBeGreaterThan(0.2);
  expect(s.start_s).toBe(start);
});

/** Drags a ball onto the next line's start; `hold` keeps ⌥ down. */
async function ontoLine(page: Page, id: string, hold = false) {
  const row = (await page.getByTestId('arrange-row').nth(1).boundingBox())!;
  const c = await centre(ballOf(page, id));
  await page.mouse.move(c.x, c.y);
  if (hold) await page.keyboard.down('Alt');
  await page.mouse.down();
  for (let i = 1; i <= 8; i++) await page.mouse.move(c.x, c.y + ((row.y + 1 - c.y) * i) / 8);
  const tip = await page.getByTestId('sound-tip').textContent();
  await page.mouse.up();
  if (hold) await page.keyboard.up('Alt');
  return tip ?? '';
}

test('snap: ⌥ lets a sound go exactly where it is put, for one move', async ({ page }) => {
  const a = await place(0.2, 0.3);
  await open(page);
  await page.waitForTimeout(400);
  const tip = await ontoLine(page, a, true);
  expect(tip).toContain('Starts at');
  await expect.poll(async () => (await sound(a)).start_s).toBeGreaterThan(0.2);
  // no snap, but it still hangs on what's said there
  expect((await sound(a)).anchor).not.toBeNull();
  expect((await sound(a)).pan).toBe(0);
});

test('snap: the button turns it off, and it stays off', async ({ page }) => {
  const a = await place(0.2, 0.3);
  await open(page);
  const snap = page.getByTestId('snap-toggle');
  await expect(snap).toHaveAttribute('aria-pressed', 'true');
  await snap.click();
  await expect(snap).toHaveAttribute('aria-pressed', 'false');
  await page.waitForTimeout(400);
  expect(await ontoLine(page, a)).toContain('Starts at');
  await expect.poll(async () => (await sound(a)).start_s).toBeGreaterThan(0.2);
  expect((await sound(a)).anchor).not.toBeNull();

  await page.reload();
  await expect(page.getByTestId('snap-toggle')).toHaveAttribute('aria-pressed', 'false');
  await page.getByTestId('snap-toggle').click();
});

test('move: a ball snaps to a line, names it, and hangs on it', async ({ page }) => {
  const a = await place(0.2, 0.3);
  await open(page);
  const row = (await page.getByTestId('arrange-row').nth(1).boundingBox())!;
  const c = await centre(ballOf(page, a));
  await page.mouse.move(c.x, c.y);
  await page.mouse.down();
  for (let i = 1; i <= 8; i++) await page.mouse.move(c.x, c.y + ((row.y + 1 - c.y) * i) / 8);
  await expect(page.getByTestId('sound-tip')).toContainText('On “');
  await page.mouse.up();
  await expect.poll(async () => (await sound(a)).anchor?.edge).toBe('start');
  const lines = await rpc<{ line_id: string }[]>('list_lines', { scene: '1' });
  expect((await sound(a)).anchor?.line_id).toBe(lines[1].line_id);
});

test('glue: dropped where nothing snaps, a sound stays on that moment when the dialogue moves', async ({ page }) => {
  const a = await place(0.5, 0.3);
  await rpc('move_sound', { placement_id: a, anchor_line_id: (await rpc<{ line_id: string }[]>('list_lines', { scene: '1' }))[1].line_id, offset_s: 0.4 });
  await open(page);
  // the distance below is free of targets only from where the ball comes to rest
  let y = 0;
  await expect
    .poll(
      async () => {
        const last = y;
        y = (await centre(ballOf(page, a))).y;
        return Math.abs(y - last);
      },
      { intervals: [150] },
    )
    .toBeLessThan(0.5);
  const after = await oneStep(page, () => centre(ballOf(page, a)).then((c) => drag(page, c, 0, 44)));
  const put = after.find((x) => x.id === a)!;
  expect(put.anchor).not.toBeNull();

  // the pause before its line grows: the sound goes with the line and keeps its spot on it
  const lines = await rpc<{ line_id: string }[]>('list_lines', { scene: '1' });
  expect(lines[0].line_id).not.toBe(put.anchor!.line_id);
  await rpc('set_gap', { line_id: put.anchor!.line_id, gap_s: 3 });
  const now = await sound(a);
  expect(now.start_s).toBeGreaterThan(put.start_s + 1);
  expect(now.anchor).toEqual(put.anchor);
  expect(now.offset_s).toBe(put.offset_s);
  await rpc('undo');
});

test('glue: off in the opened sound, it stays at its time and through a move, and comes back', async ({ page }) => {
  const a = await place(1, 0.3);
  await rpc('set_sound_glue', { placement_id: a, glued: true });
  await open(page);
  await ballOf(page, a).dblclick();
  const glue = page.getByTestId('opened-glue');
  await expect(glue).toHaveAttribute('aria-pressed', 'true');
  const at = (await sound(a)).start_s;

  await glue.click();
  await expect(glue).toHaveAttribute('aria-pressed', 'false');
  await expect.poll(async () => (await sound(a)).anchor).toBeNull();
  expect((await sound(a)).start_s).toBeCloseTo(at, 3);

  await page.keyboard.press('Escape');
  await ontoLine(page, a);
  await expect.poll(async () => (await sound(a)).start_s).toBeGreaterThan(at);
  expect((await sound(a)).anchor).toBeNull();

  await ballOf(page, a).dblclick();
  await glue.click();
  await expect.poll(async () => (await sound(a)).anchor).not.toBeNull();
});

test('trim: the tail trims, the fade follows, the rest of the source shows dashed', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  const tail = page.getByTestId('line-tail');
  await expect(tail).toBeVisible();
  await expect(page.getByTestId('unused-source')).toHaveAttribute('d', '');
  const after = await oneStep(page, () => centre(tail).then((c) => drag(page, c, 0, -30)));
  const s = after.find((x) => x.id === a)!;
  expect(s.len_s).toBeLessThan(2.5);
  expect(s.auto_fade).toBe(true);
  expect(s.fade_out_s).toBeLessThanOrEqual(0.3);
  await ballOf(page, a).click();
  await expect(page.getByTestId('unused-source')).not.toHaveAttribute('d', '');
});

test('loudness: the ring, and ⇧ scroll as one step', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  await expect(page.getByTestId('loudness')).toContainText('Loudness 0 dB');
  await oneStep(page, async () => {
    const c = await centre(ballOf(page, a));
    await page.mouse.move(c.x, c.y);
    await page.keyboard.down('Shift');
    for (let i = 0; i < 5; i++) await page.mouse.wheel(0, 40);
    await page.keyboard.up('Shift');
  });
  expect((await sound(a)).gain_db).toBeLessThan(-5);
  const knob = page.getByTestId('loudness-knob');
  await page.waitForTimeout(900);
  const before = (await sound(a)).gain_db;
  await oneStep(page, () => centre(knob).then((c) => drag(page, c, 60, 60)));
  expect((await sound(a)).gain_db).not.toBe(before);
});

test('pan: ⌥ drag on a ball', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  const after = await oneStep(page, async () => {
    await page.keyboard.down('Alt');
    await drag(page, await centre(ballOf(page, a)), 50, 0);
    await page.keyboard.up('Alt');
  });
  const s = after.find((x) => x.id === a)!;
  expect(s.pan).toBeGreaterThan(0.2);
  expect(s.depth).toBe(0.3);
});

test('delete: ⌫ removes the selected sound, undo brings it back', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  await page.keyboard.press('Backspace');
  await expect(page.getByTestId('sound-ball')).toHaveCount(0);
  await page.keyboard.press('ControlOrMeta+z');
  await expect(page.getByTestId('sound-ball')).toHaveCount(1);
});

test('multi-select: ⇧ click, then one drag moves all, keeps their offsets, one undo step', async ({ page }) => {
  const a = await place(0.5, 0.2);
  const b = await place(2, 0.4);
  const c = await place(4, 0.6);
  await open(page);
  await ballOf(page, a).click();
  await ballOf(page, b).click({ modifiers: ['Shift'] });
  await expect(page.locator('.ball.sel')).toHaveCount(2);
  const [a0, b0, c0] = [await sound(a), await sound(b), await sound(c)];
  const others = await ballOf(page, c).boundingBox();
  const start = await centre(ballOf(page, a));
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  for (let i = 1; i <= 6; i++) await page.mouse.move(start.x + i * 10, start.y + i * 4);
  // during the drag only the selected move
  expect(await ballOf(page, c).boundingBox()).toEqual(others);
  await page.mouse.up();
  await expect.poll(async () => (await sound(a)).depth).toBeGreaterThan(a0.depth);
  const [a1, b1, c1] = [await sound(a), await sound(b), await sound(c)];
  expect(b1.depth - a1.depth).toBeCloseTo(b0.depth - a0.depth, 2);
  expect(b1.start_s - a1.start_s).toBeCloseTo(b0.start_s - a0.start_s, 2);
  expect(c1).toEqual(c0);
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(() => sound(a)).toEqual(a0);
  expect(await sound(b)).toEqual(b0);
});

test('multi-select: the band selects, Esc and a click on empty space clear', async ({ page }) => {
  const a = await place(0.5, 0.2);
  const b = await place(1, 0.5);
  await open(page);
  const [pa, pb] = [await centre(ballOf(page, a)), await centre(ballOf(page, b))];
  await page.mouse.move(pa.x - 50, pa.y - 50);
  await page.mouse.down();
  await page.mouse.move(pb.x + 50, pb.y + 50, { steps: 5 });
  await expect(page.getByTestId('sound-band')).toBeVisible();
  await page.mouse.up();
  await expect(page.locator('.ball.sel')).toHaveCount(2);
  await page.keyboard.press('Escape');
  await expect(page.locator('.ball.sel')).toHaveCount(0);
  await ballOf(page, a).click();
  await expect(page.locator('.ball.sel')).toHaveCount(1);
  await page.mouse.click(pb.x + 100, pb.y + 120);
  await expect(page.locator('.ball.sel')).toHaveCount(0);
});

test('⌘D: a copy lands right after the sound, is selected, and goes on with ⌘D', async ({ page }) => {
  const a = await place(5, 0.3);
  await open(page);
  await ballOf(page, a).click();
  const base = await sound(a);
  await page.keyboard.press('ControlOrMeta+d');
  await expect.poll(async () => (await list()).length).toBe(2);
  const copy = (await list()).find((s) => s.id !== a)!;
  expect(copy.start_s).toBeCloseTo(base.start_s + base.len_s, 2);
  expect(copy.depth).toBe(base.depth);
  await expect(ballOf(page, copy.id)).toHaveClass(/\bsel\b/);
  await expect(ballOf(page, a)).not.toHaveClass(/\bsel\b/);

  await page.keyboard.press('ControlOrMeta+d');
  await expect.poll(async () => (await list()).length).toBe(3);
  const last = (await list()).find((s) => s.id !== a && s.id !== copy.id)!;
  expect(last.start_s).toBeCloseTo(copy.start_s + copy.len_s, 2);
});

test('⌘D: several sounds keep their spacing, in one undo step', async ({ page }) => {
  const a = await place(0.5, 0.3);
  const b = await place(3.5, 0.3);
  await open(page);
  await ballOf(page, a).click();
  await ballOf(page, b).click({ modifiers: ['Shift'] });
  const [sa, sb] = [await sound(a), await sound(b)];
  const after = await oneStep(page, async () => {
    await page.keyboard.press('ControlOrMeta+d');
  });
  const made = after.filter((s) => s.id !== a && s.id !== b).sort((x, y) => x.start_s - y.start_s);
  expect(made).toHaveLength(2);
  expect(made[1].start_s - made[0].start_s).toBeCloseTo(sb.start_s - sa.start_s, 2);
  expect(made[0].start_s).toBeCloseTo(sb.start_s + sb.len_s, 2);
});

test('⌘D: on the Edit page too, and a dialogue clip stays as it is', async ({ page }) => {
  const a = await place(5, 0.3);
  await open(page);
  await page.keyboard.press('3');
  await expect(page.getByTestId('dialogue-clip').first()).toBeVisible();

  await page.getByTestId('dialogue-clip').first().click();
  await page.keyboard.press('ControlOrMeta+d');
  await page.waitForTimeout(200);
  expect(await list()).toHaveLength(1);

  const clips = page.locator('.clip').filter({ hasText: 'Foil_crackle' });
  await clips.first().click();
  await page.keyboard.press('ControlOrMeta+d');
  await expect.poll(async () => (await list()).length).toBe(2);
  const base = await sound(a);
  const copy = (await list()).find((s) => s.id !== a)!;
  expect(copy.start_s).toBeCloseTo(base.start_s + base.len_s, 2);
  // the copy is the one selected: it's the later clip
  await expect(clips).toHaveCount(2);
  const boxes = await Promise.all([0, 1].map(async (i) => ({ x: (await clips.nth(i).boundingBox())!.x, sel: /\bsel\b/.test((await clips.nth(i).getAttribute('class')) ?? '') })));
  expect(boxes.sort((p, q) => p.x - q.x).map((b) => b.sel)).toEqual([false, true]);
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await list()).length).toBe(1);
});

test('multi-select: delete, loudness and keys act on all in one step', async ({ page }) => {
  const a = await place(0.5, 0.2);
  const b = await place(2, 0.4);
  await open(page);
  await ballOf(page, a).click();
  await ballOf(page, b).click({ modifiers: ['Shift'] });
  const after = await oneStep(page, async () => {
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowRight');
  });
  expect(after.find((s) => s.id === a)!.depth).toBeCloseTo(0.35);
  expect(after.find((s) => s.id === b)!.depth).toBeCloseTo(0.55);
  await oneStep(page, () => page.keyboard.press('Backspace'));
});

test('keys: arrows mean depth, pan and loudness only while sounds are selected', async ({ page }) => {
  const a = await place(1, 0.5);
  await open(page);
  await ballOf(page, a).click();
  await page.keyboard.press('Alt+ArrowRight');
  await expect.poll(async () => (await sound(a)).pan).toBeCloseTo(0.05);
  await page.keyboard.press('Shift+ArrowUp');
  await expect.poll(async () => (await sound(a)).gain_db).toBe(1);
  await page.keyboard.press('.');
  await expect.poll(async () => (await sound(a)).start_s).toBeCloseTo(1.1);
  await page.keyboard.press('Escape');
  await expect(page.locator('.ball.sel')).toHaveCount(0);
  const depth = (await sound(a)).depth;
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(500);
  expect((await sound(a)).depth).toBe(depth);
});

test('keys: every ball can be reached and announces itself', async ({ page }) => {
  await place(1, 0.5);
  await open(page);
  const ball = page.getByTestId('sound-ball').first();
  await ball.focus();
  await expect(ball).toHaveAttribute('aria-label', /Foil_crackle, Layer test sounds, starts .*, 2\.5 seconds, loudness 0 dB, Pan center, depth Room 50 percent/);
  await page.keyboard.press('Shift+Enter');
  await expect(ball).toHaveAttribute('aria-pressed', 'true');
});

test('merge: overlapping sounds at one depth become a group after settling and split on hover', async ({ page }) => {
  await place(1, 0.4);
  await place(1.05, 0.42);
  const far = await place(1, 0.8);
  await open(page);
  const group = page.getByTestId('sound-group');
  await expect(group).toHaveCount(1);
  await expect(group.locator('.badge')).toHaveText('2');
  await expect(ballOf(page, far)).toBeVisible();
  const c = await centre(group);
  await page.mouse.move(c.x, c.y);
  await expect(page.locator('.ball:not(.tucked)')).toHaveCount(3);
  await page.mouse.move(c.x, 100);
  await expect(page.locator('.ball.tucked')).toHaveCount(2);
});

test('rules: a fresh move merges only after the settle, never during the drag', async ({ page }) => {
  const a = await place(1, 0.4);
  const b = await place(4, 0.4);
  await open(page);
  const pa = await centre(ballOf(page, a));
  const pb = await centre(ballOf(page, b));
  await page.mouse.move(pb.x, pb.y);
  await page.mouse.down();
  await page.mouse.move(pa.x, pa.y + 14, { steps: 8 });
  await page.waitForTimeout(700);
  await expect(page.getByTestId('sound-group')).toHaveCount(0);
  await page.mouse.up();
  await page.mouse.move(pa.x + 300, 120);
  await expect(page.getByTestId('sound-group')).toHaveCount(1, { timeout: 3000 });
});

test('buckets: the eye fades a bucket on this page only', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  const rev = (await rpc<{ rev: number }>('project_status')).rev;
  const eye = foil(page).getByTestId('bucket-eye');
  await eye.click();
  await expect(ballOf(page, a)).toHaveClass(/faded/);
  await expect(page.locator('.ball.sel')).toHaveCount(0);
  expect((await rpc<{ rev: number }>('project_status')).rev).toBe(rev);
  await expect(ballOf(page, a)).toHaveCSS('pointer-events', 'none');
  await eye.click();
  await expect(ballOf(page, a)).not.toHaveClass(/faded/);
});

test('the Sounds button and S hide the layer and bring it back', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  const row = page.getByTestId('arrange-row').first();
  const left = (await row.boundingBox())!.x;
  await page.keyboard.press('s');
  await expect(ballOf(page, a)).toHaveClass(/out/);
  await expect.poll(async () => (await row.boundingBox())!.x).toBeGreaterThan(left + 100);
  await page.getByTestId('sounds-toggle').click();
  await expect(ballOf(page, a)).not.toHaveClass(/out/);
  await expect.poll(async () => (await row.boundingBox())!.x).toBe(left);
});

test('one document: a change on Arrange shows on Edit, and the reverse', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  await page.keyboard.press('ArrowRight');
  await expect.poll(async () => (await sound(a)).depth).toBeCloseTo(0.35);
  const ev = await rpc<{ placements: { id: string; depth: number }[] }>('edit_view', { scene: '1' });
  expect(ev.placements.find((p) => p.id === a)!.depth).toBeCloseTo(0.35);
  await rpc('command', { cmd: 'update_placement', placement_id: a, patch: { gain_db: -9 } });
  await expect(ballOf(page, a)).toHaveAttribute('aria-label', /loudness −9 dB/, { timeout: 4000 });
});

test('scroll scrubs, and the playhead stays on the centre line', async ({ page }) => {
  await place(1, 0.3);
  await open(page);
  const now = page.getByTestId('time-now');
  const t0 = await now.textContent();
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  await page.mouse.move(field.x + 300, 300);
  await page.mouse.wheel(0, 200);
  await expect(now).not.toHaveText(t0 ?? '');
});

test('space plays from where the scrub stopped, even in a pause', async ({ page }) => {
  await open(page);
  const lines = await rpc<{ line_id: string }[]>('list_lines', { scene: '1' });
  const v = await rpc<{ items: { line_id: string; end_s: number; start_s: number }[] }>('scene_view', { scene: '1' });
  const a = v.items.find((i) => i.line_id === lines[0].line_id)!;
  const b = v.items.find((i) => i.line_id === lines[1].line_id)!;
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  await page.mouse.move(field.x + 300, 300);
  // scroll into the pause between the first two lines
  await page.mouse.wheel(0, (a.end_s + (b.start_s - a.end_s) / 2) / 0.008);
  await page.keyboard.press(' ');
  await expect.poll(async () => (await rpc<{ position_s: number }>('playhead')).position_s).toBeGreaterThanOrEqual(a.end_s);
  await page.keyboard.press(' ');
});

test('a ball pulses while its sound plays', async ({ page }) => {
  const a = await place(0.2, 0.3);
  await open(page);
  await page.keyboard.press(' ');
  await expect(ballOf(page, a)).toHaveClass(/pulse/);
  await page.keyboard.press(' ');
});

test('reordering lines takes the anchored sounds along', async ({ page }) => {
  const a = await place(0.2, 0.3);
  const lines = await rpc<{ line_id: string }[]>('list_lines', { scene: '1' });
  await rpc('move_sound', { placement_id: a, anchor_line_id: lines[0].line_id });
  await open(page);
  const start = (await sound(a)).start_s;
  const sc = await rpc<{ id: string }[]>('list_scenes');
  await rpc('command', { cmd: 'move_item', scene_id: sc[0].id, item_id: lines[0].line_id, to_index: 2 });
  await expect.poll(async () => (await sound(a)).start_s).toBeGreaterThan(start);
  await expect.poll(async () => +((await ballOf(page, a).getAttribute('data-start')) ?? 0)).toBeGreaterThan(start);
  await rpc('undo');
});

test('a new sound moves no other length line', async ({ page }) => {
  await place(1, 0.2);
  await place(1.2, 0.5);
  await open(page);
  await page.waitForTimeout(700);
  const lines = page.getByTestId('sound-line');
  const before = await Promise.all((await lines.all()).map((l) => l.boundingBox()));
  await rpc('place_from_bucket', { scene: '1', asset_id: asset, bucket_id: bucket, at_s: 0.9, depth: 0.8 });
  await expect(lines).toHaveCount(3);
  await page.waitForTimeout(800);
  const now = await Promise.all((await lines.all()).map((l) => l.boundingBox()));
  for (const b of before) expect(now).toContainEqual(b);
});

test('buckets: ⌥ on an eye shows only that bucket, again shows all', async ({ page }) => {
  const b2 = await rpc<{ bucket_id: string }>('create_bucket', { name: 'Layer test others', keywords: ['tray'], scenes: [(await rpc<{ id: string }[]>('list_scenes'))[0].id] });
  await open(page);
  const mine = foil(page).getByTestId('bucket-eye');
  const other = page.getByRole('region', { name: 'Layer test others' }).getByTestId('bucket-eye');
  await mine.click({ modifiers: ['Alt'] });
  await expect(mine).toHaveAttribute('aria-pressed', 'true');
  await expect(other).toHaveAttribute('aria-pressed', 'false');
  await mine.click({ modifiers: ['Alt'] });
  await expect(other).toHaveAttribute('aria-pressed', 'true');
  await rpc('delete_bucket', { bucket_id: b2.bucket_id, confirm: true });
});

test('reduced motion: no swirl, no wobble, no overshoot', async ({ page }) => {
  await place(1, 0.4);
  await place(1.05, 0.42);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await open(page);
  const group = page.getByTestId('sound-group');
  await expect(group).toHaveCount(1);
  await expect(group).toHaveCSS('animation-name', 'none');
  await expect(group.locator('.swirl')).toHaveCSS('animation-name', 'none');
});

test('the header stays on one row beside the sounds', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await open(page);
  const nav = (await page.getByRole('navigation', { name: 'Pages' }).boundingBox())!;
  const button = (await page.getByTestId('sounds-toggle').boundingBox())!;
  expect(Math.abs(nav.y + nav.height / 2 - (button.y + button.height / 2))).toBeLessThan(4);
});

/** Fails the test on any error the page throws: a broken render can leave the rest looking fine. */
function watchErrors(page: Page) {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(m.text());
  });
  return errors;
}

test('colours repeat: seven and more buckets with sounds still draw a line each', async ({ page }) => {
  const errors = watchErrors(page);
  const dir = mkdtempSync(join(tmpdir(), 'tones-'));
  const sc = await rpc<{ id: string }[]>('list_scenes');
  const made: string[] = [];
  const ids: string[] = [];
  for (let i = 0; i < 8; i++) {
    const file = join(dir, `Tone_${i}.wav`);
    wav(file, 0.5);
    const { assets } = await rpc<{ assets: { id: string }[] }>('import_sounds', { paths: [file] });
    const { bucket_id } = await rpc<{ bucket_id: string }>('create_bucket', { name: `Layer test tone ${i}`, keywords: ['tone'], scenes: [sc[0].id] });
    made.push(bucket_id);
    await rpc('bucket_keep', { bucket_id, asset_id: assets[0].id });
    ids.push((await rpc<{ id: string }>('place_from_bucket', { scene: '1', asset_id: assets[0].id, bucket_id, at_s: i * 0.6, depth: 0.1 * i })).id);
  }
  try {
    await open(page);
    await expect(page.getByTestId('sound-ball')).toHaveCount(8);
    // a path per bucket, even where two buckets share a colour
    await expect(page.locator('path.thin')).toHaveCount(8);
    // and they follow the document: not stuck on their first render
    const before = await page.locator('path.thin').evaluateAll((els) => els.map((e) => e.getAttribute('d')));
    await rpc('move_sound', { placement_id: ids[7], at_s: 4.9 });
    await expect.poll(() => page.locator('path.thin').evaluateAll((els) => els.map((e) => e.getAttribute('d')))).not.toEqual(before);
    expect(errors).toEqual([]);
  } finally {
    for (const id of made) await rpc('delete_bucket', { bucket_id: id, confirm: true });
  }
});

test('colours repeat: two characters seven colours apart still show Arrange', async ({ page }) => {
  const errors = watchErrors(page);
  // the demo has two characters: give them colours that wrap onto the same one
  await page.route(`${bridge}/rpc`, async (route) => {
    const method = route.request().postDataJSON()?.method;
    if (method !== 'scene_view') return route.continue();
    const res = await route.fetch();
    const body = await res.json();
    (body.result.characters as { color_index: number }[]).forEach((c, i) => (c.color_index = i * 7));
    await route.fulfill({ response: res, json: body });
  });
  await open(page);
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
  await expect(page.getByTestId('sounds-field')).toBeVisible();
  // one spine path per character, in the same colour
  const fills = await page.locator('svg.art > path[fill]').evaluateAll((els) => els.map((e) => e.getAttribute('fill')));
  expect(fills).toHaveLength(2);
  expect(fills[0]).toBe(fills[1]);
  expect(errors).toEqual([]);
});

/** The ball and the document must agree: the ball shows what is stored, not a leftover preview. */
async function agrees(page: Page, id: string, depth: number, x: number) {
  await expect.poll(async () => (await sound(id)).depth).toBeCloseTo(depth, 3);
  await expect(ballOf(page, id)).toHaveAttribute('data-depth', depth.toFixed(3));
  await expect.poll(async () => (await ballOf(page, id).boundingBox())!.x).toBeCloseTo(x, 0);
}

test('previews: a click right after a nudge leaves no stale value behind', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  await page.waitForTimeout(800);
  const x = (await ballOf(page, a).boundingBox())!.x;
  for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowRight');
  // inside the burst window: the press itself sends the nudge
  const c = await centre(ballOf(page, a));
  await page.mouse.move(c.x, c.y);
  await page.mouse.down();
  await page.waitForTimeout(100);
  await page.mouse.up();
  await expect.poll(async () => (await sound(a)).depth).toBeCloseTo(0.45, 3);
  await page.keyboard.press('ControlOrMeta+z');
  await agrees(page, a, 0.3, x);
  // the next step starts from what is stored
  await page.keyboard.press('ArrowRight');
  await expect.poll(async () => (await sound(a)).depth).toBeCloseTo(0.35, 3);
  await expect(ballOf(page, a)).toHaveAttribute('data-depth', '0.350');
});

test('previews: the same after ⇧ scroll', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  const c = await centre(ballOf(page, a));
  await page.mouse.move(c.x, c.y);
  await page.keyboard.down('Shift');
  for (let i = 0; i < 4; i++) await page.mouse.wheel(0, 40);
  await page.keyboard.up('Shift');
  await page.mouse.down();
  await page.waitForTimeout(100);
  await page.mouse.up();
  await expect.poll(async () => (await sound(a)).gain_db).toBeLessThan(-5);
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => (await sound(a)).gain_db).toBe(0);
  await expect(ballOf(page, a)).toHaveAttribute('aria-label', /loudness 0 dB/);
});

test('previews: Esc during a drag right after a nudge lands on the nudge', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowRight');
  const c = await centre(ballOf(page, a));
  await page.mouse.move(c.x, c.y);
  await page.mouse.down();
  await page.mouse.move(c.x + 90, c.y, { steps: 4 });
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect.poll(async () => (await sound(a)).depth).toBeCloseTo(0.45, 3);
  await expect(ballOf(page, a)).toHaveAttribute('data-depth', '0.450');
  await page.keyboard.press('ControlOrMeta+z');
  await expect(ballOf(page, a)).toHaveAttribute('data-depth', '0.300');
});

test('previews: a drag that ends where it began right after a nudge', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).click();
  for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowRight');
  const c = await centre(ballOf(page, a));
  await page.mouse.move(c.x, c.y);
  await page.mouse.down();
  await page.mouse.move(c.x + 30, c.y, { steps: 3 });
  await page.mouse.move(c.x, c.y, { steps: 3 });
  await page.mouse.up();
  await expect.poll(async () => (await sound(a)).depth).toBeCloseTo(0.45, 3);
  await page.keyboard.press('ControlOrMeta+z');
  await expect(ballOf(page, a)).toHaveAttribute('data-depth', '0.300');
});

// ---- the opened sound ----

type Full = Sound & { in_s: number; fade_in_s: number };
const full = async (id: string) => (await list()).find((s) => s.id === id) as Full;
const view = (page: Page) => page.getByTestId('opened-sound');
const part = (page: Page, p: string) => page.getByTestId(`opened-${p}`);

/** Opens by double click; the view glides the sound's line to the middle, so it waits for that. */
async function openSound(page: Page, id: string) {
  await ballOf(page, id).dblclick();
  await expect(view(page)).toBeVisible();
  await page.waitForTimeout(450);
}

test('opened: a double click opens it beside the spine, joined to its line by a wedge', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  const v = view(page);
  await expect(v).toContainText('Foil_crackle');
  await expect(page.getByTestId('opened-reading')).toContainText('Layer test sounds');
  await expect(page.getByTestId('opened-reading')).toContainText('Close');
  await expect(page.getByTestId('opened-reading')).toContainText('Pan center');
  await expect(page.getByTestId('opened-wedge')).toHaveAttribute('d', /M\d/);
  await expect(page.getByTestId('opened-used')).toHaveAttribute('d', /Z$/);
  await expect(page.getByTestId('opened-playhead')).toHaveCount(1);
  const card = (await v.boundingBox())!;
  const spine = (await page.getByTestId('arrange-row').first().boundingBox())!;
  expect(card.x).toBeGreaterThan(spine.x + spine.width);
  await page.screenshot({ path: 'test-results/opened-sound.png' });
});

test('opened: the playhead runs through the view, and the view stays open', async ({ page }) => {
  const a = await place(0.5, 0.2);
  await open(page);
  await openSound(page, a);
  const head = page.getByTestId('opened-playhead');
  const before = await head.getAttribute('d');
  // in the view Space is the sound alone: ⇧ Space is the timeline
  await page.keyboard.press('Shift+Space');
  await expect.poll(() => head.getAttribute('d')).not.toBe(before);
  await expect(view(page)).toBeVisible();
  await page.getByRole('button', { name: 'Play or stop (Space)' }).click();
});

test('opened: a double click on the length line opens it too', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await page.getByTestId('sound-line').first().dblclick();
  await expect(view(page)).toBeVisible();
  await expect(ballOf(page, a)).toHaveAttribute('aria-pressed', 'true');
});

test('opened: Enter opens the sound in focus, Esc closes and gives the focus back', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await ballOf(page, a).focus();
  await page.keyboard.press('Enter');
  await expect(view(page)).toBeVisible();
  await expect(view(page)).toBeFocused();
  await expect(page.getByTestId('sounds-said')).toContainText('Opened');
  await page.keyboard.press('Escape');
  await expect(view(page)).toBeHidden();
  await expect(ballOf(page, a)).toBeFocused();
  // the sound stays selected; a second Esc clears it
  await expect(ballOf(page, a)).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Escape');
  await expect(ballOf(page, a)).toHaveAttribute('aria-pressed', 'false');
});

test('opened: a click outside closes, the transport does not', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  await page.getByRole('button', { name: 'Favorite take (F)' }).click();
  await expect(view(page)).toBeVisible();
  await page.getByTestId('arrange-row').first().click();
  await expect(view(page)).toBeHidden();
});

test('opened: a sound in a hidden bucket closes, and so does one that goes', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  await foil(page).getByRole('button', { name: 'Fade Layer test sounds' }).click();
  await expect(view(page)).toBeHidden();
  await foil(page).getByRole('button', { name: 'Show Layer test sounds' }).click();
  await openSound(page, a);
  await rpc('remove_sound', { placement_id: a });
  await page.keyboard.press('s');
  await expect(view(page)).toBeHidden();
});

test('opened: the bottom edge trims, snapping to the dialogue, in one step', async ({ page }) => {
  const a = await place(1, 0.2);
  await rpc('command', { cmd: 'set_sound_fades', placement_id: a, auto: true });
  await open(page);
  await openSound(page, a);
  const before = await full(a);
  const after = await oneStep(page, async () => {
    await drag(page, await centre(part(page, 'out')), 0, -60);
  });
  const s = after.find((x) => x.id === a)!;
  expect(s.len_s).toBeLessThan(before.len_s);
  expect(s.start_s).toBe(before.start_s);
  expect(s.fade_out_s).toBeCloseTo(Math.min(0.3, s.len_s * 0.3), 3);
});

/** Pixels from a part of the view to a time, read off the view's own edges. */
async function pxTo(page: Page, a: string, from: number, to: number) {
  const s = await full(a);
  const top = await centre(part(page, 'in'));
  const bottom = await centre(part(page, 'out'));
  return ((to - from) / s.len_s) * (bottom.y - top.y);
}

const spokenLines = async () => (await rpc<{ items: { kind: string; line_id: string; start_s: number; end_s: number }[] }>('scene_view', { scene: '1' })).items.filter((i) => i.kind === 'line' && i.end_s > i.start_s);

/** The other specs change the demo's lines: a sound placed to run over the end of a line. */
async function overALine() {
  const line = (await spokenLines()).find((l) => l.end_s >= 1)!;
  return { line, id: await place(line.end_s - 1, 0.2) };
}

test('opened: an edge near a line snaps to it, within about 8 px', async ({ page }) => {
  const { line, id: a } = await overALine();
  await open(page);
  await openSound(page, a);
  const s = await full(a);
  const end = line.end_s;
  const dy = await pxTo(page, a, s.start_s + s.len_s, end);
  const c = await centre(part(page, 'out'));
  await page.mouse.move(c.x, c.y);
  await page.mouse.down();
  await page.mouse.move(c.x, c.y + dy / 2, { steps: 3 });
  await page.mouse.move(c.x, c.y + dy + 4, { steps: 3 });
  await expect(page.getByTestId('opened-tip')).toContainText(/Ends/);
  await page.mouse.up();
  await expect.poll(async () => (await full(a)).len_s).toBeCloseTo(end - s.start_s, 3);
});

test('opened: past the snap distance the edge goes free', async ({ page }) => {
  const a = await place(0.5, 0.2);
  await open(page);
  await openSound(page, a);
  const s = await full(a);
  const ts = (await spokenLines()).flatMap((l) => [l.start_s, l.end_s]);
  const px = await pxTo(page, a, 0, 1);
  // a spot well clear of every line edge
  let want = s.start_s + s.len_s - 0.2;
  while (ts.some((t) => Math.abs(t - want) * px < 20) && want > s.start_s + 0.4) want -= 0.02;
  await drag(page, await centre(part(page, 'out')), 0, (want - s.start_s - s.len_s) * px);
  await expect.poll(async () => (await full(a)).len_s).toBeCloseTo(want - s.start_s, 1);
  const len = (await full(a)).len_s;
  expect(ts.some((t) => Math.abs(s.start_s + len - t) < 0.002)).toBe(false);
});

test('opened: a start trimmed onto a line hangs on it, as one step', async ({ page }) => {
  const { line, id: a } = await overALine();
  await open(page);
  await openSound(page, a);
  const s = await full(a);
  const dy = await pxTo(page, a, s.start_s, line.end_s);
  const after = await oneStep(page, async () => {
    await drag(page, await centre(part(page, 'in')), 0, dy + 3);
  });
  const t = after.find((x) => x.id === a)!;
  expect(t.start_s).toBeCloseTo(line.end_s, 3);
  expect(t.anchor?.line_id).toBe(line.line_id);
});

test('opened: over the waveform, scroll scrubs and ⇧ scroll sets the loudness', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  const c = await centre(part(page, 'move'));
  await page.mouse.move(c.x, c.y);
  const head = page.getByTestId('opened-playhead');
  const before = await head.getAttribute('d');
  await page.mouse.wheel(0, 40);
  await expect.poll(() => head.getAttribute('d')).not.toBe(before);
  await page.keyboard.down('Shift');
  await page.mouse.wheel(0, -40);
  await page.keyboard.up('Shift');
  await expect.poll(async () => (await sound(a)).gain_db).toBeGreaterThan(0);
});

test('opened: the top edge trims and the audio stays where it was', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  const before = await full(a);
  await oneStep(page, async () => {
    await drag(page, await centre(part(page, 'in')), 0, 50);
  });
  const s = await full(a);
  const moved = s.start_s - before.start_s;
  expect(moved).toBeGreaterThan(0);
  expect(s.in_s).toBeCloseTo(before.in_s + moved, 2);
  expect(s.len_s).toBeCloseTo(before.len_s - moved, 2);
});

test('opened: the corner dots set the fades; only the fade out turns auto off', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  await oneStep(page, async () => {
    await drag(page, await centre(part(page, 'fadeIn')), 0, 40);
  });
  expect((await full(a)).fade_in_s).toBeGreaterThan(0.1);
  expect((await full(a)).auto_fade).toBe(true);
  await oneStep(page, async () => {
    await drag(page, await centre(part(page, 'fadeOut')), 0, -40);
  });
  expect((await full(a)).fade_out_s).toBeGreaterThan(0.3);
  expect((await full(a)).auto_fade).toBe(false);
});

test('opened: a press on a dot that goes nowhere changes nothing', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  const steps = await undoSteps();
  await part(page, 'fadeOut').click();
  await page.waitForTimeout(300);
  expect(await undoSteps()).toBe(steps);
  expect((await full(a)).auto_fade).toBe(true);
});

test('opened: dragging the middle moves the sound, ⌥ slips the audio inside', async ({ page }) => {
  const a = await place(1, 0.2);
  await rpc('command', { cmd: 'trim_placement', placement_id: a, edge: 'start', delta_s: 0.5 });
  await rpc('command', { cmd: 'trim_placement', placement_id: a, edge: 'end', delta_s: -0.5 });
  await open(page);
  await openSound(page, a);
  const base = await full(a);
  await oneStep(page, async () => {
    await page.keyboard.down('Alt');
    await drag(page, await centre(part(page, 'move')), 0, 30);
    await page.keyboard.up('Alt');
  });
  const slipped = await full(a);
  expect(slipped.in_s).toBeLessThan(base.in_s);
  expect(slipped.start_s).toBe(base.start_s);
  expect(slipped.len_s).toBe(base.len_s);
  await oneStep(page, async () => {
    await drag(page, await centre(part(page, 'move')), 0, 50);
  });
  const moved = await full(a);
  expect(moved.start_s).toBeGreaterThan(base.start_s);
  expect(moved.in_s).toBe(slipped.in_s);
});

test('opened: Esc during a gesture cancels it and keeps the view', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  const before = await full(a);
  const c = await centre(part(page, 'out'));
  await page.mouse.move(c.x, c.y);
  await page.mouse.down();
  await page.mouse.move(c.x, c.y - 40, { steps: 4 });
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect(view(page)).toBeVisible();
  expect((await full(a)).len_s).toBe(before.len_s);
  await expect(page.getByTestId('opened-readout')).toContainText(`${before.len_s.toFixed(1)} of`);
});

test('opened: the quick buttons, one step each', async ({ page }) => {
  const { id: a } = await overALine();
  await open(page);
  await openSound(page, a);
  const at = (await full(a)).start_s;
  const spoken = await spokenLines();
  const end = Math.min(...spoken.map((l) => l.end_s).filter((t) => t > at + 0.05));
  const next = Math.min(...spoken.map((l) => l.start_s).filter((t) => t > at + 0.05));
  const button = (name: string) => view(page).getByRole('button', { name, exact: true });

  const steps = await undoSteps();
  await button('End with line').click();
  await expect.poll(undoSteps).toBe(steps + 1);
  await expect.poll(async () => (await full(a)).len_s).toBeCloseTo(Math.min(2.5, end - at), 2);
  await expect(button('End with line')).toHaveAttribute('aria-pressed', 'true');

  await button('Until next line').click();
  await expect.poll(undoSteps).toBe(steps + 2);
  await expect.poll(async () => (await full(a)).len_s).toBeCloseTo(Math.min(2.5, next - at), 2);

  await button('Full').click();
  await expect.poll(async () => (await full(a)).len_s).toBeCloseTo(2.5, 2);
  await expect(button('Full')).toHaveAttribute('aria-pressed', 'true');
});

test('opened: the Auto fade button hands the fade back to the length', async ({ page }) => {
  const a = await place(1, 0.2);
  await rpc('command', { cmd: 'set_sound_fades', placement_id: a, fade_out_s: 1 });
  await open(page);
  await openSound(page, a);
  const auto = view(page).getByRole('button', { name: 'Auto fade' });
  await expect(auto).toHaveAttribute('aria-pressed', 'false');
  await oneStep(page, () => auto.click());
  await expect(auto).toHaveAttribute('aria-pressed', 'true');
  expect((await full(a)).fade_out_s).toBeCloseTo(0.3, 3);
});

test('opened: every part answers to the keys, a burst is one step', async ({ page, browserName }) => {
  test.fail(browserName === 'webkit', 'BUG: in WebKit, Tab from the opened sound skips its close button. macOS WebKit tabs to buttons only with keyboard navigation on in System Settings.');
  const a = await place(1, 0.2);
  await open(page);
  await ballOf(page, a).focus();
  await page.keyboard.press('Enter');
  await expect(view(page)).toBeFocused();
  const base = await full(a);

  await page.keyboard.press('Tab');
  await expect(view(page).getByRole('button', { name: /close/ })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByTestId('opened-scrub')).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(part(page, 'in')).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(part(page, 'move')).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(part(page, 'out')).toBeFocused();

  await oneStep(page, async () => {
    for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowUp');
  });
  expect((await full(a)).len_s).toBeCloseTo(base.len_s - 0.3, 3);

  await page.keyboard.press('Tab');
  await expect(part(page, 'fadeIn')).toBeFocused();
  await oneStep(page, async () => {
    await page.keyboard.press('ArrowDown');
  });
  expect((await full(a)).fade_in_s).toBeCloseTo(base.fade_in_s + 0.1, 3);

  await page.keyboard.press('Tab');
  await expect(part(page, 'fadeOut')).toBeFocused();
  await oneStep(page, async () => {
    await page.keyboard.press('ArrowUp');
  });
  expect((await full(a)).auto_fade).toBe(false);
  await expect(page.getByTestId('sounds-said')).toContainText('Fade out');

  // loudness, depth and pan keep their keys in here
  await oneStep(page, async () => {
    await page.keyboard.press('Shift+ArrowUp');
  });
  await expect(page.getByTestId('opened-reading')).toContainText('+1 dB');
});

test('opened: the scrubber plays the sound alone, I and O trim to it, Enter closes', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).dblclick();
  await expect(view(page)).toBeVisible();
  const rail = page.getByTestId('opened-scrub');
  const head = async () => +(await rail.getAttribute('aria-valuenow'))!;
  const box = (await rail.boundingBox())!;
  const before = await sound(a);
  expect(await head()).toBe(0);

  // a click on the rail puts the scrubber there, and nothing else changes
  const at = async (share: number) => page.mouse.click(box.x + box.width / 2, box.y + box.height * share);
  await at(0.3);
  const inPoint = await head();
  expect(inPoint).toBeGreaterThan(0.2);
  expect(await sound(a)).toMatchObject({ start_s: before.start_s, len_s: before.len_s });

  // Space is the sound alone: the preview player runs, the timeline's playhead stays
  const pos = await page.getByTestId('time-now').textContent();
  await page.keyboard.press('Space');
  await expect(page.getByTestId('preview-player')).toBeVisible();
  await expect.poll(head).toBeGreaterThan(inPoint);
  await page.keyboard.press('Space');
  await expect(page.getByTestId('time-now')).toHaveText(pos ?? '');

  // I and O: one undo step each
  await at(0.3);
  const steps = await undoSteps();
  await page.keyboard.press('i');
  await expect.poll(undoSteps).toBe(steps + 1);
  await expect.poll(async () => (await sound(a)).start_s).toBeCloseTo(before.start_s + inPoint, 1);
  await at(0.6);
  const outPoint = await head();
  await page.keyboard.press('o');
  await expect.poll(undoSteps).toBe(steps + 2);
  await expect.poll(async () => (await sound(a)).len_s).toBeCloseTo(outPoint - inPoint, 1);

  await page.keyboard.press('Enter');
  await expect(view(page)).toBeHidden();
});

test('opened: a drag on the rail scrubs and stops with the mouse', async ({ page }) => {
  const a = await place(1, 0.3);
  await open(page);
  await ballOf(page, a).dblclick();
  const rail = page.getByTestId('opened-scrub');
  const box = (await rail.boundingBox())!;
  const x = box.x + box.width / 2;
  await page.mouse.move(x, box.y + box.height * 0.2);
  await page.mouse.down();
  for (let i = 3; i <= 6; i++) {
    await page.mouse.move(x, box.y + (box.height * i) / 10);
    await page.waitForTimeout(110);
  }
  await expect(page.getByTestId('preview-player')).toBeVisible();
  await page.mouse.up();
  await expect(page.getByRole('button', { name: /^Play preview of/ })).toBeVisible();
  // a click on the sound itself moves the scrubber and doesn't move the sound
  const start = (await sound(a)).start_s;
  const was = await rail.getAttribute('aria-valuenow');
  await page.getByTestId('opened-move').click({ position: { x: 20, y: 8 } });
  await expect(rail).not.toHaveAttribute('aria-valuenow', was ?? '');
  expect((await sound(a)).start_s).toBe(start);
});

test('opened: Tab stays in the view and comes round', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await ballOf(page, a).focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Tab');
  for (let i = 0; i < 12; i++) await page.keyboard.press('Tab');
  await expect.poll(() => page.evaluate(() => !!document.activeElement?.closest('[data-testid="opened-sound"]'))).toBe(true);
});

test('opened: what it changes reaches the Edit page, and the reverse', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await openSound(page, a);
  await drag(page, await centre(part(page, 'fadeOut')), 0, -30);
  await expect.poll(async () => (await full(a)).auto_fade).toBe(false);
  const ev = await rpc<{ placements: { id: string; fade_out_s: number; auto_fade: boolean }[] }>('edit_view', { scene: '1' });
  const onEdit = ev.placements.find((p) => p.id === a)!;
  expect(onEdit.fade_out_s).toBe((await full(a)).fade_out_s);
  expect(onEdit.auto_fade).toBe(false);
  await rpc('command', { cmd: 'update_placement', placement_id: a, patch: { fade_in_s: 0.4 } });
  await expect(page.getByTestId('opened-readout')).toContainText('fade in 0.40', { timeout: 4000 });
});

test('keys: Tab walks the balls in time, one Tab stop for the layer', async ({ page }) => {
  const late = await place(4, 0.2);
  const early = await place(0.5, 0.6);
  await open(page);
  await expect(ballOf(page, early)).toHaveAttribute('tabindex', '0');
  await expect(ballOf(page, late)).toHaveAttribute('tabindex', '-1');
  await ballOf(page, early).focus();
  await page.keyboard.press('Tab');
  await expect(ballOf(page, late)).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(ballOf(page, early)).toBeFocused();
});

test('keys: ⇧Enter builds a selection, ⌘A takes all, and it is said aloud', async ({ page }) => {
  const a = await place(0.5, 0.2);
  const b = await place(3, 0.6);
  await open(page);
  await ballOf(page, a).focus();
  await page.keyboard.press('Shift+Enter');
  await page.keyboard.press('Tab');
  await page.keyboard.press('Shift+Enter');
  await expect(ballOf(page, a)).toHaveAttribute('aria-pressed', 'true');
  await expect(ballOf(page, b)).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByTestId('sounds-said')).toContainText('2 sounds selected');
  await oneStep(page, async () => {
    await page.keyboard.press('ArrowRight');
  });
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('sounds-said')).toContainText('Selection cleared');
  await page.keyboard.press('ControlOrMeta+a');
  await expect(ballOf(page, a)).toHaveAttribute('aria-pressed', 'true');
  await expect(ballOf(page, b)).toHaveAttribute('aria-pressed', 'true');
});

test('keys: an eye reached with Tab works with Enter; after a click Space still plays', async ({ page }) => {
  await place(1, 0.2);
  await open(page);
  const eye = foil(page).getByRole('button', { name: 'Fade Layer test sounds' });
  await eye.focus();
  await page.keyboard.press('Enter');
  await expect(foil(page).getByRole('button', { name: 'Show Layer test sounds' })).toBeVisible();
  await expect(page.getByTestId('sounds-said')).toContainText('Layer test sounds faded');
  await foil(page).getByRole('button', { name: 'Show Layer test sounds' }).click();
  await page.locator('body').click({ position: { x: 5, y: 400 } });
  const eye2 = foil(page).getByRole('button', { name: 'Fade Layer test sounds' });
  await eye2.click();
  await page.keyboard.press('Space');
  const play = page.getByRole('button', { name: 'Play or stop (Space)' });
  await expect(play).toHaveAttribute('data-playing', 'true');
  await expect(foil(page).getByRole('button', { name: 'Show Layer test sounds' })).toBeVisible();
  await page.keyboard.press('Space');
  await expect(play).toHaveAttribute('data-playing', 'false');
});

test('reduced motion: the opened view and the glide do not animate', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  const a = await place(1, 0.2);
  await open(page);
  await ballOf(page, a).dblclick();
  await expect(view(page)).toBeVisible();
  expect(await view(page).evaluate((el) => getComputedStyle(el).animationName)).toBe('none');
  const glide = await page.locator('.world').evaluate((el) => getComputedStyle(el).transitionDuration);
  expect(glide).toBe('0s');
});

test('one document: pan and depth show in the Edit inspector, and the reverse', async ({ page }) => {
  const a = await place(1, 0.4);
  await rpc('command', { cmd: 'set_sound_pan', placement_id: a, pan: -0.5 });
  await open(page);
  await page.keyboard.press('3');
  await page.getByRole('button', { name: 'Foil_crackle' }).first().click();
  await page.keyboard.press('i');
  const panField = page.getByRole('textbox', { name: /^Pan/ });
  const depthField = page.getByRole('textbox', { name: /^Depth/ });
  await expect(panField).toHaveValue('−50');
  await expect(depthField).toHaveValue('40');
  await depthField.fill('70');
  await depthField.blur();
  await expect.poll(async () => (await sound(a)).depth).toBeCloseTo(0.7, 3);
  await panField.fill('25');
  await panField.blur();
  await expect.poll(async () => (await sound(a)).pan).toBeCloseTo(0.25, 3);
  await page.keyboard.press('2');
  await expect(ballOf(page, a)).toHaveAttribute('data-depth', '0.700');
  await expect(ballOf(page, a)).toHaveAttribute('aria-label', /Pan R 25/);
});
