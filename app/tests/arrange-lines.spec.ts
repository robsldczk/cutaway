import { expect, test, type Page } from '@playwright/test';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Timing a sound against the words, and lines further back in the room, on Arrange.

async function rpc<T = unknown>(method: string, params: object = {}): Promise<T> {
  const r = await fetch('http://127.0.0.1:7358/rpc', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  const j = (await r.json()) as { ok: boolean; result: T; error?: { message: string } };
  if (!j.ok) throw new Error(j.error?.message ?? method);
  return j.result;
}

type Line = { line_id: string; start_s: number; end_s: number; depth: number; gain_db: number; fade_in_s: number };
type Sound = { id: string; start_s: number };

const lines = async () => (await rpc<{ items: (Line & { kind: string })[] }>('scene_view', { scene: '1' })).items.filter((i) => i.kind === 'line');
const sounds = () => rpc<Sound[]>('list_sounds', { scene: '1' });

/** A mono 16-bit WAV of noise. */
function wav(path: string, seconds: number) {
  const n = Math.round(48000 * seconds);
  const b = Buffer.alloc(44 + n * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + n * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(48000, 24);
  b.writeUInt32LE(96000, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.round((Math.random() * 2 - 1) * 6000), 44 + i * 2);
  writeFileSync(path, b);
}

let asset = '';
let bucket = '';

test.beforeAll(async () => {
  const file = join(mkdtempSync(join(tmpdir(), 'lines-')), 'Linetest_slam.wav');
  wav(file, 1);
  asset = (await rpc<{ assets: { id: string }[] }>('import_sounds', { paths: [file] })).assets[0].id;
  const have = await rpc<{ buckets: { id: string; name: string }[] }>('buckets', {});
  const sc = await rpc<{ id: string }[]>('list_scenes');
  bucket = have.buckets.find((b) => b.name === 'Line test sounds')?.id ?? (await rpc<{ bucket_id: string }>('create_bucket', { name: 'Line test sounds', keywords: ['linetest'], scenes: [sc[0].id] })).bucket_id;
  await rpc('bucket_keep', { bucket_id: bucket, asset_id: asset });
});

// the demo is shared with every other spec
test.afterAll(async () => {
  for (const s of await sounds()) await rpc('remove_sound', { placement_id: s.id });
  await rpc('delete_bucket', { bucket_id: bucket, confirm: true });
});

test.beforeEach(async () => {
  for (const s of await sounds()) await rpc('remove_sound', { placement_id: s.id });
  const all = await lines();
  await rpc('set_line_depth', { line_ids: all.map((l) => l.line_id), depth: 0 });
});

async function open(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.keyboard.press('2');
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
}

const rows = (page: Page) => page.getByTestId('arrange-row');
const chips = (page: Page) => page.getByTestId('depth-chip');
const menu = (page: Page) => page.getByTestId('line-menu');

async function setSlider(page: Page, value: number) {
  await page.getByTestId('line-depth').evaluate((el: HTMLInputElement, v) => {
    el.value = String(v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}

test.describe('the moved sound on the blocks', () => {
  async function ballAt(page: Page, at_s: number) {
    const { id } = await rpc<{ id: string }>('place_from_bucket', { scene: '1', asset_id: asset, bucket_id: bucket, at_s, depth: 0.4 });
    await open(page);
    const ball = page.locator(`[data-sound="${id}"]`);
    await expect(ball).toBeVisible();
    // the column has found its place
    await page.waitForTimeout(700);
    const b = (await ball.boundingBox())!;
    return { id, x: b.x + b.width / 2, y: b.y + b.height / 2 };
  }

  /** Drags the grabbed ball until it sits at scene time `t`, by its own time on the map. */
  async function dragUntil(page: Page, from: { x: number; y: number }, want: (cue: boolean, gap: boolean) => boolean) {
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    // ⌥ held: free of the snap, so the sound goes exactly where the mouse does
    await page.keyboard.down('Alt');
    for (let dy = 0; dy < 500; dy += 6) {
      await page.mouse.move(from.x, from.y + dy);
      const [cue, gap] = await Promise.all([page.getByTestId('sound-cue').count(), page.getByTestId('sound-cue-gap').count()]);
      if (dy > 12 && want(cue > 0, gap > 0)) return from.y + dy;
    }
    throw new Error('never got there');
  }

  test('a marker of its own follows the sound over the blocks, and the locator stays', async ({ page }) => {
    const all = await lines();
    const from = await ballAt(page, all[0].start_s + 0.2);
    await expect(page.getByTestId('sound-cue')).toHaveCount(0);
    const flag = page.locator('.snip .flag');
    const locator = await flag.evaluate((el) => (el as HTMLElement).style.left);
    const y = await dragUntil(page, from, (cue) => cue);
    const cue = page.getByTestId('sound-cue');
    await expect(cue).toHaveCount(1);
    const left = () => cue.evaluate((el) => parseFloat((el as HTMLElement).style.left));
    const first = await left();
    // a little further down the same block, later on its strip
    await page.mouse.move(from.x, y + 3);
    await expect.poll(left).toBeGreaterThan(first);
    expect(await flag.evaluate((el) => (el as HTMLElement).style.left)).toBe(locator);
    await page.mouse.up();
    await page.keyboard.up('Alt');
    await expect(cue).toHaveCount(0);
  });

  test('in a pause the marker crosses the column', async ({ page }) => {
    const all = await lines();
    const from = await ballAt(page, all[0].start_s + 0.2);
    await dragUntil(page, from, (_, gap) => gap);
    await expect(page.getByTestId('sound-cue-gap')).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('sound-cue-gap')).toHaveCount(0);
    await page.mouse.up();
    await page.keyboard.up('Alt');
  });

  test('the keys show it too, for as long as they run', async ({ page }) => {
    const all = await lines();
    const from = await ballAt(page, all[0].start_s + 0.5);
    await page.mouse.click(from.x, from.y);
    await page.keyboard.press('.');
    await expect(page.getByTestId('sound-cue')).toHaveCount(1);
    await expect(page.getByTestId('sound-cue')).toHaveCount(0);
  });

  test('a trim shows where the sound ends', async ({ page }) => {
    const all = await lines();
    const from = await ballAt(page, all[0].start_s);
    await page.mouse.click(from.x, from.y);
    const tail = page.getByTestId('line-tail');
    await expect(tail).toBeVisible();
    const b = (await tail.boundingBox())!;
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
    await page.mouse.down();
    await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2 - 8);
    await expect(page.getByTestId('sound-tip')).toBeVisible();
    const shown = (await page.getByTestId('sound-cue').count()) + (await page.getByTestId('sound-cue-gap').count());
    await page.keyboard.press('Escape');
    await page.mouse.up();
    expect(shown).toBe(1);
  });
});

test.describe('a line further back', () => {
  test('right click: a stop moves the line back, the chip says so, one undo brings it forward', async ({ page }) => {
    await open(page);
    await expect(chips(page)).toHaveCount(0);
    await rows(page).nth(1).click({ button: 'right' });
    await expect(menu(page)).toBeVisible();
    await page.getByTestId('depth-stop').filter({ hasText: /^Back$/ }).click();
    await expect(chips(page)).toHaveText(['Back 50 %']);
    await expect(page.getByTestId('line-depth-value')).toHaveText('Back 50 %');
    expect((await lines())[1].depth).toBe(0.5);
    await page.keyboard.press('Escape');
    await expect(menu(page)).toHaveCount(0);
    await page.keyboard.press('Meta+z');
    await expect(chips(page)).toHaveCount(0);
  });

  test('the slider sets it on release, and a click away closes the menu', async ({ page }) => {
    await open(page);
    await rows(page).first().click({ button: 'right' });
    await setSlider(page, 35);
    await expect(chips(page)).toHaveText(['Back 35 %']);
    expect((await lines())[0].depth).toBe(0.35);
    await page.mouse.click(5, 450);
    await expect(menu(page)).toHaveCount(0);
  });

  test('a right click does not move the column or the current line', async ({ page }) => {
    await open(page);
    const place = () => page.evaluate(() => (document.querySelector('.arr') as HTMLElement).style.transform);
    await page.waitForTimeout(400);
    const was = await place();
    await rows(page).nth(1).click({ button: 'right' });
    await page.waitForTimeout(400);
    expect(await place()).toBe(was);
  });

  test('⌥→ moves the current line, a burst is one undo step, and the take stays', async ({ page }) => {
    await open(page);
    const before = (await rpc<{ items: { take_index: number }[] }>('scene_view', { scene: '1' })).items;
    for (let i = 0; i < 4; i++) await page.keyboard.press('Alt+ArrowRight');
    await expect(chips(page)).toHaveText(['Back 20 %']);
    await expect.poll(async () => (await lines())[0].depth).toBe(0.2);
    expect((await rpc<{ items: { take_index: number }[] }>('scene_view', { scene: '1' })).items[0].take_index).toBe(before[0].take_index);
    await page.keyboard.press('Meta+z');
    await expect(chips(page)).toHaveCount(0);
  });

  test('⌘Z right after the keys takes back the burst, not the step before it', async ({ page }) => {
    await open(page);
    await page.keyboard.press('Alt+ArrowRight');
    await page.keyboard.press('Meta+z');
    await expect.poll(async () => (await lines())[0].depth).toBe(0);
  });

  test('the Edit inspector shows and sets it', async ({ page }) => {
    const id = (await lines())[0].line_id;
    await rpc('set_line_depth', { line_ids: [id], depth: 0.3 });
    await open(page);
    await page.keyboard.press('3');
    await page.keyboard.press('i');
    await page.getByTestId('dialogue-clip').first().click();
    const input = page.getByTestId('line-depth-edit');
    await expect(input).toHaveValue('30');
    await input.fill('60');
    await input.press('Enter');
    await expect.poll(async () => (await lines())[0].depth).toBe(0.6);
  });
});

test.describe('several lines', () => {
  test('⌘ click and ⇧ click select, a plain click and Esc let go', async ({ page }) => {
    await open(page);
    const picked = page.locator('.snip.picked');
    await rows(page).nth(2).click({ modifiers: ['Meta'] });
    await expect(picked).toHaveCount(2);
    await rows(page).nth(2).click({ modifiers: ['Meta'] });
    await expect(picked).toHaveCount(1);
    await rows(page).nth(2).click({ modifiers: ['Shift'] });
    await expect(picked).toHaveCount(3);
    await page.keyboard.press('Escape');
    await expect(picked).toHaveCount(0);
    await rows(page).nth(1).click({ modifiers: ['Meta'] });
    await rows(page).nth(2).click();
    await expect(picked).toHaveCount(0);
  });

  test('the menu acts on all of them as one undo step', async ({ page }) => {
    await open(page);
    await rows(page).nth(2).click({ modifiers: ['Shift'] });
    await rows(page).nth(1).click({ button: 'right' });
    await expect(menu(page)).toContainText('3 lines');
    await setSlider(page, 40);
    await expect(chips(page)).toHaveText(['Back 40 %', 'Back 40 %', 'Back 40 %']);
    await page.keyboard.press('Escape');
    await page.keyboard.press('Meta+z');
    await expect(chips(page)).toHaveCount(0);
  });

  test('a right click outside the selection lets go of it', async ({ page }) => {
    await open(page);
    await rows(page).nth(1).click({ modifiers: ['Meta'] });
    await rows(page).nth(2).click({ button: 'right' });
    await expect(menu(page)).not.toContainText('lines');
    await page.getByTestId('depth-stop').filter({ hasText: 'Far' }).click();
    await expect(chips(page)).toHaveCount(1);
    expect((await lines()).map((l) => l.depth).slice(0, 3)).toEqual([0, 0, 0.8]);
  });

  test('⌥← and ⌥→ move a group as one, and it stops at the end', async ({ page }) => {
    const all = await lines();
    await rpc('set_line_depth', { line_ids: [all[1].line_id], depth: 0.95 });
    await open(page);
    await rows(page).nth(1).click({ modifiers: ['Meta'] });
    await page.keyboard.press('Alt+ArrowRight');
    await page.keyboard.press('Alt+ArrowRight');
    await expect.poll(async () => (await lines()).slice(0, 2).map((l) => l.depth)).toEqual([0.05, 1]);
  });
});

test.describe('attributes', () => {
  test('copy and paste from the menu: depth, gain and fades, as one undo step', async ({ page }) => {
    const all = await lines();
    await rpc('command', { cmd: 'batch', commands: [{ cmd: 'set_clip_depth', line_id: all[0].line_id, depth: 0.4 }, { cmd: 'set_clip_gain', line_id: all[0].line_id, gain_db: -3 }, { cmd: 'set_clip_fades', line_id: all[0].line_id, fade_in_s: 0.1, fade_out_s: 0 }] });
    await open(page);
    await rows(page).first().click({ button: 'right' });
    await expect(page.getByTestId('paste-attrs')).toBeDisabled();
    await page.getByTestId('copy-attrs').click();
    await expect(menu(page)).toHaveCount(0);
    await rows(page).nth(2).click({ button: 'right' });
    await expect(page.getByTestId('paste-attrs')).toContainText('Back 40 % · −3.0 dB · fades');
    await page.getByTestId('paste-attrs').click();
    await expect.poll(async () => (await lines())[2].depth).toBe(0.4);
    const l = (await lines())[2];
    expect([l.gain_db, l.fade_in_s]).toEqual([-3, 0.1]);
    await page.keyboard.press('Meta+z');
    await expect.poll(async () => (await lines())[2].gain_db).toBe(0);
    await rpc('command', { cmd: 'batch', commands: [{ cmd: 'set_clip_gain', line_id: all[0].line_id, gain_db: 0 }, { cmd: 'set_clip_fades', line_id: all[0].line_id, fade_in_s: 0, fade_out_s: 0 }] });
  });

  test('⌥⌘C and ⌥⌘V work on the selection', async ({ page }) => {
    const all = await lines();
    await rpc('set_line_depth', { line_ids: [all[0].line_id], depth: 0.6 });
    await open(page);
    await page.keyboard.press('Meta+Alt+KeyC');
    await rows(page).nth(1).click({ modifiers: ['Meta'] });
    await rows(page).nth(2).click({ modifiers: ['Meta'] });
    await page.keyboard.press('Meta+Alt+KeyV');
    await expect.poll(async () => (await lines()).slice(0, 3).map((l) => l.depth)).toEqual([0.6, 0.6, 0.6]);
  });

  test('pasting with nothing copied says so', async ({ page }) => {
    await open(page);
    await page.keyboard.press('Meta+Alt+KeyV');
    await expect(page.getByText('Copy a line’s attributes first')).toBeVisible();
  });
});
