import { expect, test, type Locator, type Page } from '@playwright/test';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Import › Rooms: a room at a time, one undo step per slider move.

async function openRooms(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await page.getByRole('tab', { name: 'Rooms' }).click();
  await expect(page.getByTestId('scene-room').first()).toBeVisible();
}

const rpc = async <T,>(method: string, params: object = {}): Promise<T> => {
  const r = await fetch('http://127.0.0.1:7358/rpc', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  return ((await r.json()) as { result: T }).result;
};

/** Eight seconds of a low hum at 8 kHz; `n` tells files apart, as identical ones are one sound. */
function hum(path: string, n: number) {
  const rate = 8000;
  const b = Buffer.alloc(44 + rate * 2 * 8);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + rate * 16, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(rate, 24);
  b.writeUInt32LE(rate * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(rate * 16, 40);
  for (let i = 0; i < rate * 8; i++) b.writeInt16LE(n * 40, 44 + i * 2);
  writeFileSync(path, b);
}

/** Moves a slider as a release does: the value, then one change event. */
async function setSlider(s: Locator, value: number) {
  await s.evaluate((el: HTMLInputElement, v) => {
    el.value = String(v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}

test('rooms: a room per scene, set and undone', async ({ page }) => {
  await openRooms(page);
  const rows = page.getByTestId('scene-room');
  const size = page.getByRole('slider', { name: 'Size of scene 1' });
  const reading = rows.first().getByTestId('room-reading');
  await rows.first().click();
  const start = await size.inputValue();
  const decay = await reading.textContent();

  await setSlider(size, 0.9);
  await expect(reading).not.toHaveText(decay ?? '');
  await expect(size).toHaveValue('0.9');
  await page.screenshot({ path: 'test-results/scene-rooms.png' });

  // the slider keeps the focus after a drag, and undo must still work there
  await size.focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(size).toHaveValue(start);
  await expect(reading).toHaveText(decay ?? '');

  await page.keyboard.press('ControlOrMeta+Shift+z');
  await expect(size).toHaveValue('0.9');
  await page.keyboard.press('ControlOrMeta+z');
  await expect(size).toHaveValue(start);
});

test('rooms: a room stays with its scene', async ({ page }) => {
  await openRooms(page);
  const rows = page.getByTestId('scene-room');
  test.skip((await rows.count()) < 2, 'the demo session has one scene');
  await rows.nth(0).click();
  const one = page.getByRole('slider', { name: 'Damping of scene 1' });
  await setSlider(one, 0.2);
  await expect(one).toHaveValue('0.2');
  await rows.nth(1).click();
  await expect(page.getByRole('slider', { name: 'Damping of scene 2' })).toHaveValue('0.5');
  await rows.nth(0).click();
  await expect(one).toHaveValue('0.2');
  await one.focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(one).toHaveValue('0.5');
});

test('rooms: arrow keys on a room slider stay on the slider', async ({ page }) => {
  await openRooms(page);
  const wet = page.getByRole('slider', { name: 'Wet of scene 1' });
  const start = +(await wet.inputValue());
  await wet.focus();
  await page.keyboard.press('ArrowRight');
  await expect(wet).toHaveValue(String(+(start + 0.01).toFixed(2)));
  await expect(page.getByTestId('rooms')).toBeVisible();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(wet).toHaveValue(String(start));
});

test('rooms: one ambience per room, chosen here and never among the sound effects', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'tones-'));
  const a = join(dir, 'Canteen_hum.wav');
  const b = join(dir, 'Canteen_fridge.wav');
  hum(a, 1);
  hum(b, 2);
  const { bucket_id } = await rpc<{ bucket_id: string }>('create_bucket', { name: 'Test canteen ambience', keywords: ['hum'], scenes: ['1'] });
  const { assets } = await rpc<{ assets: { id: string }[] }>('import_sounds', { paths: [a, b], bucket_id });
  const rooms = () => rpc<{ tone: { asset_id: string; gain_db: number } | null }[]>('scene_rooms');

  await openRooms(page);
  const choices = page.getByTestId('ambience-choice');
  await expect(choices.filter({ hasText: 'Canteen_hum' })).toHaveCount(1);
  await page.getByRole('button', { name: 'Use Canteen_hum' }).click();
  await expect.poll(async () => (await rooms())[0].tone?.asset_id).toBe(assets[0].id);
  const now = page.getByTestId('room-ambience');
  await expect(now).toContainText('Canteen_hum');

  // a second one takes its place: a room has one
  await page.getByRole('button', { name: 'Use Canteen_fridge' }).click();
  await expect(now).toContainText('Canteen_fridge');
  await expect(now).toHaveCount(1);
  expect((await rooms())[1].tone).toBeNull();

  // it plays on the preview player, and the level is one move
  await now.getByRole('button', { name: /^Play / }).click();
  await expect(page.getByTestId('preview-player')).toContainText('Canteen_fridge');
  await page.getByTestId('preview-player').getByRole('button').first().click();
  const level = page.getByRole('slider', { name: /Ambience level/ });
  await setSlider(level, -20);
  await expect.poll(async () => (await rooms())[0].tone?.gain_db).toBe(-20);

  // the find box narrows the choices
  await page.getByRole('searchbox', { name: 'Find an ambience' }).fill('zzz');
  await expect(page.getByText('Nothing by that name.')).toBeVisible();

  await now.getByRole('button', { name: 'Remove' }).click();
  await expect.poll(async () => (await rooms())[0].tone).toBeNull();

  // not a bucket: Import › Sounds doesn't list it
  await page.getByRole('tab', { name: 'Sounds' }).click();
  await expect(page.getByRole('button', { name: 'New bucket' })).toBeVisible();
  await expect(page.getByRole('article', { name: 'Bucket Test canteen ambience' })).toHaveCount(0);
  await rpc('delete_bucket', { bucket_id, confirm: true });
});

test('rooms: the voices sit beside the room, one undo step each', async ({ page }) => {
  await openRooms(page);
  const voices = page.getByTestId('voice-wet');
  await expect(voices.first()).toBeVisible();
  const first = voices.first().getByRole('slider');
  await expect(first).toHaveValue('0.25');
  await setSlider(first, 0.5);
  await expect(first).toHaveValue('0.5');
  await first.focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(first).toHaveValue('0.25');
});

test('rooms: an ambience starts at −28 dB, from the drop zone, which reaches the room open here', async ({ page }) => {
  const dir = mkdtempSync(join(tmpdir(), 'dropped-'));
  const file = join(dir, 'Dropped_bed.wav');
  hum(file, 3);
  await openRooms(page);
  const zone = page.getByTestId('ambience-drop');
  await expect(zone).toBeVisible();
  const call = (c: string, ...args: unknown[]) =>
    page.evaluate(([n, a]) => (window as unknown as Record<string, Record<string, (...x: unknown[]) => unknown>>).cutawayFileDrag[n as string](...(a as unknown[])), [c, args] as const);
  await call('enter', [file]);
  await expect(zone).toHaveClass(/live/);
  const b = (await zone.boundingBox())!;
  await call('over', b.x + 20, b.y + 10);
  await expect(zone).toHaveClass(/over/);
  await call('drop', [file], b.x + 20, b.y + 10);
  await expect(page.getByTestId('room-ambience')).toContainText('Dropped_bed');
  const rooms = await rpc<{ tone: { gain_db: number } | null }[]>('scene_rooms');
  expect(rooms[0].tone?.gain_db).toBe(-28);
  await page.getByTestId('room-ambience').getByRole('button', { name: 'Remove' }).click();
});

