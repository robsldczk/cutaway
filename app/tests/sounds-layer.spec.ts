import { expect, test, type Locator, type Page } from '@playwright/test';
import { mkdtempSync, writeFileSync } from 'node:fs';
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

type Sound = { id: string; start_s: number; len_s: number; depth: number; pan: number; gain_db: number; anchor: { line_id: string; word: number | null; edge: string } | null; fade_out_s: number; auto_fade: boolean };
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

test('depth: sideways is depth, and small vertical motion leaves the timing alone', async ({ page }) => {
  const a = await place(1, 0.1);
  await open(page);
  const start = (await sound(a)).start_s;
  const after = await oneStep(page, () => centre(ballOf(page, a)).then((c) => drag(page, c, 120, 8)));
  const s = after.find((x) => x.id === a)!;
  expect(s.depth).toBeGreaterThan(0.2);
  expect(s.start_s).toBe(start);
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

test('move: dropped where nothing snaps, a sound is free', async ({ page }) => {
  const a = await place(0.5, 0.3);
  await rpc('move_sound', { placement_id: a, anchor_line_id: (await rpc<{ line_id: string }[]>('list_lines', { scene: '1' }))[0].line_id, offset_s: 0.4 });
  await open(page);
  const after = await oneStep(page, () => centre(ballOf(page, a)).then((c) => drag(page, c, 0, 44)));
  expect(after.find((x) => x.id === a)!.anchor).toBe(null);
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
  await page.keyboard.press('Space');
  await expect.poll(() => head.getAttribute('d')).not.toBe(before);
  await expect(view(page)).toBeVisible();
  await page.keyboard.press('Space');
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

test('opened: every part answers to the keys, a burst is one step', async ({ page }) => {
  const a = await place(1, 0.2);
  await open(page);
  await ballOf(page, a).focus();
  await page.keyboard.press('Enter');
  await expect(view(page)).toBeFocused();
  const base = await full(a);

  await page.keyboard.press('Tab');
  await expect(view(page).getByRole('button', { name: /close/ })).toBeFocused();
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

// ---- the room in the transport ----

const chip = (page: Page) => page.getByTestId('room-chip');
const roomOf = async () => (await rpc<{ room: { size: number; wet: number; damping: number } }[]>('scene_rooms'))[0].room;

async function setSize(page: Page, v: number) {
  await page.getByRole('slider', { name: "Size of this scene's room" }).evaluate((el: HTMLInputElement, x) => {
    el.value = String(x);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }, v);
}

test('room: the transport reads the scene’s room, and only beside the sounds', async ({ page }) => {
  const r = await roomOf();
  await open(page);
  await expect(chip(page)).toHaveText(`Room ${Math.round(r.size * 100)} · wet ${Math.round(r.wet * 100)}`);
  await page.keyboard.press('s');
  await expect(chip(page)).toBeHidden();
  await page.keyboard.press('s');
  await expect(chip(page)).toBeVisible();
  await page.keyboard.press('1');
  await expect(chip(page)).toBeHidden();
});

test('room: the quick size is one undo step and keeps the rest of the room', async ({ page }) => {
  const r = await roomOf();
  await open(page);
  await chip(page).click();
  const steps = await undoSteps();
  await setSize(page, 0.81);
  await expect.poll(undoSteps).toBe(steps + 1);
  await expect.poll(roomOf).toEqual({ ...r, size: 0.81 });
  await expect(chip(page)).toContainText('Room 81');
  await page.getByRole('slider', { name: "Size of this scene's room" }).blur();
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(roomOf).toEqual(r);
  await expect(chip(page)).toContainText(`Room ${Math.round(r.size * 100)}`);
});

test('room: the shortcut opens the settings at this scene, and Back comes back', async ({ page }) => {
  await open(page);
  await chip(page).click();
  await page.getByTestId('room-settings').click();
  await expect(page.getByTestId('scene-room').first()).toBeVisible();
  await expect(page.getByRole('slider', { name: 'Size of scene 1' })).toBeFocused();
  await page.getByRole('button', { name: /Back to Arrange/ }).click();
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
});

test('room: one source of truth, a change in the settings shows in the read-out', async ({ page }) => {
  const r = await roomOf();
  const sc = await rpc<{ id: string }[]>('list_scenes');
  await open(page);
  await rpc('command', { cmd: 'set_scene_room', scene_id: sc[0].id, room: { ...r, wet: 0.2 } });
  await page.keyboard.press('ArrowDown');
  await expect(chip(page)).toContainText('wet 20');
  await rpc('undo');
});

test('room: Esc closes the pop-up and gives the focus back', async ({ page }) => {
  await open(page);
  await chip(page).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('room-pop')).toBeVisible();
  await page.getByRole('slider', { name: "Size of this scene's room" }).focus();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('room-pop')).toBeHidden();
  await expect(chip(page)).toBeFocused();
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
