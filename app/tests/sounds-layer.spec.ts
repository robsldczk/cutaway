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
  bucket = have.buckets.find((b) => b.name === 'Food and foil')?.id ?? (await rpc<{ bucket_id: string }>('create_bucket', { name: 'Food and foil', keywords: ['foil'], scenes: [sc[0].id] })).bucket_id;
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
const foil = (page: Page) => page.getByRole('region', { name: 'Food and foil' });

async function bucketRow(page: Page) {
  const sec = foil(page);
  await expect(sec).toBeVisible();
  const row = sec.getByTestId('bucket-sound');
  if (!(await row.count())) await sec.getByRole('button', { name: /Food and foil/ }).click();
  await expect(row.first()).toBeVisible();
  return row.first();
}

const ballOf = (page: Page, id: string) => page.locator(`[data-sound="${id}"]`);

async function centre(l: Locator) {
  const b = (await l.boundingBox())!;
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
}

const undoSteps = async () => (await rpc<{ state: string }[]>('history', { limit: 200 })).filter((r) => r.state === 'done').length;
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
  await expect(ball).toHaveAttribute('aria-label', /Foil_crackle, Food and foil, starts .*, 2\.5 seconds, loudness 0 dB, Pan center, depth Room 50 percent/);
  await page.keyboard.press('Enter');
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
  const b2 = await rpc<{ bucket_id: string }>('create_bucket', { name: 'Props', keywords: ['tray'], scenes: [(await rpc<{ id: string }[]>('list_scenes'))[0].id] });
  await open(page);
  const mine = foil(page).getByTestId('bucket-eye');
  const other = page.getByRole('region', { name: 'Props' }).getByTestId('bucket-eye');
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
