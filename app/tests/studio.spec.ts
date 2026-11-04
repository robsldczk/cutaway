import { expect, test } from '@playwright/test';
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { deflateSync } from 'node:zlib';

// Studio mode: a production folder becomes a show, its home shows the backdrop and the
// episode dropdown, an episode opens in its Sound folder and the library folders are
// buckets. Runs last (it closes the demo episode) and opens the demo again at the end.

const bridge = 'http://127.0.0.1:7358';

async function rpc<T = Record<string, unknown>>(method: string, params: object = {}): Promise<T> {
  const r = await fetch(`${bridge}/rpc`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  const b = (await r.json()) as { ok: boolean; result: T; error?: { message: string } };
  if (!b.ok) throw new Error(`${method}: ${b.error?.message}`);
  return b.result;
}

/** A quarter second of a quiet tone, 16-bit mono WAV. */
function wav(hz: number): Buffer {
  const sr = 48000;
  const n = sr / 4;
  const b = Buffer.alloc(44 + n * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + n * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(sr, 24);
  b.writeUInt32LE(sr * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.round(6000 * Math.sin((i * hz * 2 * Math.PI) / sr)), 44 + i * 2);
  return b;
}

/** A small two-colour PNG (the backdrop). */
function png(): Buffer {
  const w = 32;
  const h = 18;
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf: Buffer) => {
    let c = 0xffffffff;
    for (const x of buf) c = crcTable[(c ^ x) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const o = y * (w * 3 + 1) + 1 + x * 3;
      const [r, g, b] = x < 20 ? [200, 60, 30] : [30, 70, 190];
      raw[o] = r;
      raw[o + 1] = g;
      raw[o + 2] = b;
    }
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

test('a production folder as a show: backdrop, episode dropdown, folder buckets', async ({ page }) => {
  const demo = (await rpc<{ path: string }>('project_status')).path;
  const root = join(mkdtempSync(join(tmpdir(), 'cutaway-show-')), 'tm-2610');
  for (const d of ['Episodes/Episode 2/Sound', 'Episodes/Episode 0', 'Sound/sfx/doors', 'Sound/music']) mkdirSync(join(root, d), { recursive: true });
  writeFileSync(join(root, 'Episodes/Episode 2/episode.jetplane.json'), JSON.stringify({ kind: 'episode', name: 'Turkey Time', episodeNumber: 2 }));
  writeFileSync(join(root, 'Sound/sfx/doors/door slam.wav'), wav(300));
  writeFileSync(join(root, 'Sound/sfx/doors/door creak.wav'), wav(320));
  writeFileSync(join(root, 'Sound/sfx/ES_Doors, Screen, Bang - Epidemic Sound.wav'), wav(340));
  writeFileSync(join(root, 'Sound/sfx/ES_Doors, Wood, Knock - Epidemic Sound.wav'), wav(360));
  writeFileSync(join(root, 'Sound/sfx/ES_Cloth, Puffer Jacket - Epidemic Sound.wav'), wav(380));
  const image = join(root, '..', 'poster.png');
  writeFileSync(image, png());

  await rpc('create_show', { root });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'tm-2610' })).toBeVisible();
  const pick = page.getByLabel('Episode');
  await expect(pick.locator('option')).toHaveText(['Episode 0 (no project yet)', 'Episode 2 · Turkey Time (no project yet)']);

  // the backdrop, set once for the show
  await page.getByPlaceholder('Image path for the backdrop').fill(image);
  await page.getByRole('button', { name: 'Set backdrop…' }).click();
  await expect(page.locator('.bd')).toHaveAttribute('style', /data:image\/jpeg/);

  // pick Episode 2 and open it: its project is made in its Sound folder
  await pick.selectOption('Episode 2');
  await expect(page.getByText('Opening makes its project in Episodes/Episode 2/Sound')).toBeVisible();
  await page.getByRole('button', { name: 'Create and open' }).click();
  await expect(page.getByRole('button', { name: /tm-2610.*Episode 2/ })).toBeVisible();

  // the library folder is a bucket with its sounds
  await page.getByRole('tab', { name: 'Sounds' }).click();
  const doors = page.getByRole('article', { name: 'Bucket Doors' });
  await expect(doors).toBeVisible();
  await expect(doors).toContainText('Sound/sfx/doors');
  // its two files, and the two loose door sounds its name matches
  await expect(doors.locator('.tl.kept')).toHaveCount(4);

  // what no bucket wants waits in its own bucket
  await expect(page.getByRole('article', { name: 'Bucket Loose SFX' }).locator('.tl.kept')).toHaveCount(1);

  // Clean up proposes first: nothing moves until the list is confirmed, and an unticked
  // sound stays where it is
  await page.getByRole('button', { name: 'Clean up 3 loose' }).click();
  const sheet = page.getByRole('dialog', { name: 'Clean up loose sounds' });
  await expect(sheet.getByRole('region', { name: 'sfx/doors' })).toBeVisible();
  await expect(sheet).toContainText('1 sound stays loose');
  const loose = (n: string) => join(root, 'Sound/sfx', `ES_${n} - Epidemic Sound.wav`);
  expect(existsSync(loose('Doors, Screen, Bang'))).toBe(true);
  await sheet.getByRole('checkbox', { name: /Knock/ }).uncheck();
  await sheet.getByRole('button', { name: 'Move 1 file' }).click();
  await expect(sheet).toBeHidden();
  await expect(page.getByRole('button', { name: 'Clean up 2 loose' })).toBeVisible();
  expect(existsSync(loose('Doors, Screen, Bang'))).toBe(false);
  expect(existsSync(join(root, 'Sound/sfx/doors/ES_Doors, Screen, Bang - Epidemic Sound.wav'))).toBe(true);
  expect(existsSync(loose('Doors, Wood, Knock'))).toBe(true);

  // the header switches episodes and goes back to the show's home
  await page.getByRole('button', { name: /tm-2610.*Episode 2/ }).click();
  await page.getByRole('menuitem', { name: 'Show home' }).click();
  await expect(page.getByRole('heading', { name: 'tm-2610' })).toBeVisible();
  await expect(pick.locator('option').nth(1)).toHaveText('Episode 2 · Turkey Time');

  await rpc('close_show');
  await rpc('open_project', { path: demo });
});
