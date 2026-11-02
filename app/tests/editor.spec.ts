import { expect, test, type Page } from '@playwright/test';

// The Edit page as a minimal NLE (M11) on the demo session: fixed time scale, trims,
// clip mute and ripple delete in every view, splits, lock to script, collapsed previews.

const shots = process.env.CUTAWAY_SHOTS;

async function openEdit(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.keyboard.press('3');
  await expect(page.getByTestId('dialogue-clip').first()).toBeVisible();
  // the docked bucket and its downloads are not part of these tests
  await page.keyboard.press('b');
}

const box = async (page: Page, sel: string, i: number) => (await page.locator(sel).nth(i).boundingBox())!;

test('the time scale stays fixed when a clip moves past the end', async ({ page }) => {
  await openEdit(page);
  const clips = page.getByTestId('dialogue-clip');
  const n = await clips.count();
  const first = await box(page, '[data-testid="dialogue-clip"]', 0);
  const last = await box(page, '[data-testid="dialogue-clip"]', n - 1);
  await page.mouse.move(last.x + last.width / 2, last.y + last.height / 2);
  await page.mouse.down();
  await page.mouse.move(last.x + last.width / 2 + 120, last.y + last.height / 2, { steps: 6 });
  await page.mouse.up();
  await expect.poll(async () => (await box(page, '[data-testid="dialogue-clip"]', n - 1)).x).toBeGreaterThan(last.x + 100);
  const after = await box(page, '[data-testid="dialogue-clip"]', 0);
  expect(Math.abs(after.width - first.width)).toBeLessThan(1);
  if (shots) await page.screenshot({ path: `${shots}/edit-moved.png` });
});

test('trim handles move an edge; undo brings it back', async ({ page }) => {
  await openEdit(page);
  const c = page.getByTestId('dialogue-clip').first();
  const b = (await c.boundingBox())!;
  await c.hover();
  const h = c.getByRole('slider', { name: 'Trim start' });
  const hb = (await h.boundingBox())!;
  await page.mouse.move(hb.x + 3, hb.y + hb.height / 2);
  await page.mouse.down();
  await page.mouse.move(hb.x + 33, hb.y + hb.height / 2, { steps: 5 });
  if (shots) await page.screenshot({ path: `${shots}/edit-trimming.png` });
  await page.mouse.up();
  await expect.poll(async () => (await c.boundingBox())!.width).toBeLessThan(b.width - 15);
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(async () => Math.abs((await c.boundingBox())!.width - b.width)).toBeLessThan(1);
});

test('⌃M mutes a clip in place and the line greys out on Arrange', async ({ page }) => {
  await openEdit(page);
  const c = page.getByTestId('dialogue-clip').first();
  const line = await c.getAttribute('data-line');
  await c.click();
  await page.keyboard.press('Control+m');
  await expect(page.locator(`[data-testid="dialogue-clip"][data-line="${line}"]`).first()).toHaveClass(/mclip/);
  await page.keyboard.press('2');
  await expect(page.getByTestId('arrange-row').first()).toHaveClass(/clip-muted/);
  await expect(page.getByTestId('clip-state').first()).toHaveText('Muted');
  if (shots) await page.screenshot({ path: `${shots}/arrange-muted.png` });
  await page.getByTestId('clip-state').first().click();
  await expect(page.getByTestId('arrange-row').first()).not.toHaveClass(/clip-muted/);
});

test('⌘T splits at the playhead; ⌫ ripple-deletes a part and the mark restores it', async ({ page }) => {
  await openEdit(page);
  const c = page.getByTestId('dialogue-clip').first();
  const line = await c.getAttribute('data-line');
  const b = (await c.boundingBox())!;
  const ruler = (await page.getByTestId('ruler').boundingBox())!;
  await page.mouse.click(b.x + b.width / 2, ruler.y + 22);
  await c.click({ position: { x: 10, y: b.height / 2 } });
  await page.keyboard.press('ControlOrMeta+t');
  const parts = page.locator(`[data-testid="dialogue-clip"][data-line="${line}"]`);
  await expect(parts).toHaveCount(2);
  if (shots) await page.screenshot({ path: `${shots}/edit-split.png` });
  const p1 = parts.nth(1);
  const w1 = (await p1.boundingBox())!.width;
  await p1.click({ position: { x: w1 / 2, y: 8 } });
  await page.keyboard.press('Backspace');
  await expect(parts).toHaveCount(1);
  await expect(page.getByTestId('cut-mark')).toHaveCount(1);
  await page.getByTestId('cut-mark').click();
  await expect(parts).toHaveCount(2);
});

test('unlocked clips move freely; the clip lock puts them back on the script', async ({ page }) => {
  await openEdit(page);
  await page.getByRole('button', { name: 'Lock', exact: true }).click();
  const c = page.getByTestId('dialogue-clip').first();
  const b = (await c.boundingBox())!;
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2 + 60, b.y + b.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect(c).toHaveClass(/free/);
  await c.hover();
  await c.getByRole('button', { name: 'Lock clip' }).click();
  await expect(c).not.toHaveClass(/free/);
  // locking again keeps it where it was put
  await expect.poll(async () => (await c.boundingBox())!.x).toBeGreaterThan(b.x + 40);
});

test('a collapsed group previews its clips; the script strip keeps its words', async ({ page }) => {
  await openEdit(page);
  await expect(page.getByTestId('strip-word').first()).toBeVisible();
  await page.getByRole('button', { name: 'Dialogue' }).click();
  await expect(page.getByTestId('group-preview')).toBeVisible();
  await expect(page.getByTestId('dialogue-clip')).toHaveCount(0);
  await expect(page.getByTestId('strip-word').first()).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/edit-collapsed.png` });
});

test('Arrange lengthens a clip into its handle and undo shortens it again', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.keyboard.press('2');
  const row = page.getByTestId('arrange-row').first();
  const dur = row.getByTestId('row-dur');
  const len = async () => parseFloat((await dur.textContent()) ?? '0');
  const before = await len();
  // the strip shows handle audio on both sides, so the end handle sits inside the strip
  const end = row.getByRole('slider', { name: 'Clip end' });
  await end.hover(); // waits for the column to stop moving
  const strip = (await row.locator('.strip').boundingBox())!;
  const eb = (await end.boundingBox())!;
  expect(eb.x + eb.width).toBeLessThan(strip.x + strip.width - 10);
  await page.mouse.down();
  await page.mouse.move(strip.x + strip.width - 2, eb.y + eb.height / 2, { steps: 6 });
  await page.mouse.up();
  await expect.poll(len).toBeGreaterThan(before + 0.3);
  if (shots) await page.screenshot({ path: `${shots}/arrange-lengthened.png` });
  await page.keyboard.press('ControlOrMeta+z');
  await expect.poll(len).toBeCloseTo(before, 1);
});
