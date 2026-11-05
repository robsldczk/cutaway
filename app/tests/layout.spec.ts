import { expect, test, type Page } from '@playwright/test';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Small windows: every toolbar stays one row, inside the window, on nothing else, and what the
// page needs is still in reach. Sizes from the app's minimum up.

const SIZES = [
  [900, 600],
  [1024, 700],
  [1100, 700],
  [1180, 720],
  [1280, 760],
  [1440, 900],
] as const;
const PAGES = [
  ['1', 'options'],
  ['2', 'arrange'],
  ['3', 'edit'],
  ['4', 'export'],
] as const;
const shots = process.env.CUTAWAY_SHOTS;
const bridge = 'http://127.0.0.1:7358';

async function rpc<T = Record<string, unknown>>(method: string, params: object = {}): Promise<T> {
  const r = await fetch(`${bridge}/rpc`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  const b = (await r.json()) as { ok: boolean; result: T; error?: { message: string } };
  if (!b.ok) throw new Error(`${method}: ${b.error?.message}`);
  return b.result;
}

/** A second of a quiet tone, 16-bit mono. */
function wav(path: string) {
  const sr = 48000;
  const b = Buffer.alloc(44 + sr * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + sr * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(sr, 24);
  b.writeUInt32LE(sr * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(sr * 2, 40);
  for (let i = 0; i < sr; i++) b.writeInt16LE(Math.round(4000 * Math.sin((i * 440 * 2 * Math.PI) / sr)), 44 + i * 2);
  writeFileSync(path, b);
}

async function open(page: Page, w: number, h: number) {
  await page.setViewportSize({ width: w, height: h });
  await page.goto('/');
  await expect(page.getByTestId('header')).toBeVisible();
}

/** Boxes of the shown children of a bar. */
const boxes = (page: Page, testid: string) =>
  page.getByTestId(testid).evaluate((bar) =>
    [...bar.children]
      .filter((c) => getComputedStyle(c).display !== 'none' && getComputedStyle(c).position !== 'fixed')
      .flatMap((c) => (getComputedStyle(c).display === 'contents' ? [...c.children] : [c]))
      .map((c) => {
        const r = c.getBoundingClientRect();
        return { name: c.className, left: r.left, right: r.right, top: r.top, bottom: r.bottom };
      })
      .filter((r) => r.right > r.left),
  );

function apart(list: { name: string; left: number; right: number }[]) {
  const sorted = [...list].sort((a, b) => a.left - b.left);
  for (let i = 1; i < sorted.length; i++) expect(sorted[i].left, `${sorted[i - 1].name} runs into ${sorted[i].name}`).toBeGreaterThanOrEqual(sorted[i - 1].right - 1);
}

for (const [w, h] of SIZES) {
  test(`${w}×${h}: every page keeps its bars in one row, inside the window`, async ({ page }) => {
    await open(page, w, h);
    for (const [key, name] of PAGES) {
      await page.keyboard.press(key);
      await page.waitForTimeout(250);
      const bar = await page.getByTestId('header').boundingBox();
      expect(bar!.height, `${name}: one row`).toBeLessThanOrEqual(64);
      const kids = await boxes(page, 'header');
      for (const k of kids) expect(k.right, `${name}: ${k.name} inside the window`).toBeLessThanOrEqual(w);
      apart(kids);
      // the title's own parts: a name shortens, it doesn't run over the next one
      apart(await page.locator('.title').evaluate((t) => [...t.children].map((c) => ({ name: c.className, ...JSON.parse(JSON.stringify(c.getBoundingClientRect())) })).filter((r) => r.width > 0)));
      if (name === 'edit') {
        const c = await page.getByTestId('control-bar').boundingBox();
        expect(c!.y, 'the Edit bar clears the header').toBeGreaterThanOrEqual(bar!.y + bar!.height);
        const tools = await boxes(page, 'control-bar');
        for (const k of tools) expect(k.right, `${k.name} inside the bar`).toBeLessThanOrEqual(c!.x + c!.width);
        apart(tools);
      }
      const over = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth);
      expect(over, `${name}: no sideways scroll`).toBeLessThanOrEqual(0);
      if (shots) await page.screenshot({ path: `${shots}/${w}-${name}.png` });
    }
  });
}

test('the scene rail is there at the smallest window, between the bars, with no scrollbar', async ({ page }) => {
  await open(page, 900, 600);
  const rail = page.getByRole('navigation', { name: 'Scenes' });
  await expect(rail).toBeVisible();
  const r = (await rail.boundingBox())!;
  const header = (await page.getByTestId('header').boundingBox())!;
  expect(r.y).toBeGreaterThanOrEqual(header.y + header.height);
  const list = rail.locator('.list');
  expect(await list.evaluate((el) => getComputedStyle(el).scrollbarWidth)).toBe('none');
  await rail.getByRole('button', { name: /^Scene 2/ }).click();
  await expect(page.getByTestId('current-box')).toBeVisible();
});

test('the ⋯ menu holds what the header gave up', async ({ page }) => {
  await open(page, 900, 600);
  await expect(page.getByRole('button', { name: 'History', exact: true })).toBeHidden();
  await page.getByTestId('header-more').click();
  const menu = page.getByRole('menu', { name: 'More' });
  await menu.getByRole('menuitem', { name: 'Settings' }).click();
  await expect(menu).toBeHidden();
  await expect(page.getByRole('dialog').or(page.locator('.sheet')).first()).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByTestId('header-more').click();
  await expect(menu.getByRole('menuitem', { name: 'History' })).toBeVisible();
  await page.keyboard.press('Escape');
});

test('the Edit bar keeps its text buttons in reach', async ({ page }) => {
  await open(page, 900, 600);
  await page.keyboard.press('3');
  await expect(page.getByTestId('control-bar')).toBeVisible();
  await page.getByTestId('bar-more').click();
  const menu = page.getByRole('menu', { name: 'More controls' });
  await expect(menu.getByRole('button', { name: /Ripple/ })).toBeVisible();
  await page.mouse.click(450, 500);
  await expect(menu).toBeHidden();
});

test('a narrow window: the bucket opens over Arrange, steps aside for a drag, and the sound lands', async ({ page }) => {
  const file = join(mkdtempSync(join(tmpdir(), 'layout-')), 'Narrow door.wav');
  wav(file);
  const { assets } = await rpc<{ assets: { id: string }[] }>('import_sounds', { paths: [file] });
  const [scene] = await rpc<{ id: string }[]>('list_scenes');
  const { bucket_id } = await rpc<{ bucket_id: string }>('create_bucket', { name: 'Narrow test', keywords: ['door'], scenes: [scene.id] });
  await rpc('bucket_keep', { bucket_id, asset_id: assets[0].id });
  const sounds = async () => (await rpc<unknown[]>('list_sounds', { scene: '1' })).length;
  const before = await sounds();

  await open(page, 1000, 700);
  await page.keyboard.press('2');
  await page.getByTestId('bucket-tab').click();
  const bucket = page.getByRole('complementary', { name: /Bucket for scene/ });
  await expect(bucket).toBeVisible();
  const row = page.locator('.srow.kept').filter({ hasText: 'Narrow door' }).first();
  await row.scrollIntoViewIfNeeded();
  const b = (await row.boundingBox())!;
  const field = (await page.getByTestId('sounds-field').boundingBox())!;
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x - 20, b.y + 10, { steps: 4 });
  await expect(bucket).toHaveCSS('pointer-events', 'none');
  await page.mouse.move(field.x + 60, field.y + field.height / 2, { steps: 6 });
  await page.mouse.up();
  await expect.poll(sounds).toBe(before + 1);
  await expect(bucket).not.toHaveCSS('pointer-events', 'none');

  await page.keyboard.press('b');
  await expect(bucket).toBeHidden();
  await rpc('undo');
  await rpc('delete_bucket', { bucket_id, confirm: true });
});
