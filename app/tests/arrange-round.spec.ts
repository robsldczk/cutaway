import { expect, test, type Locator, type Page } from '@playwright/test';

// Side by side, glue, pans and scene marks on Arrange.

async function rpc<T = unknown>(method: string, params: object = {}): Promise<T> {
  const r = await fetch('http://127.0.0.1:7358/rpc', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  return ((await r.json()) as { result: T }).result;
}

/** Moves a slider as a release does: the value, then one change event. */
async function setSlider(s: Locator, value: number) {
  await s.evaluate((el: HTMLInputElement, v) => {
    el.value = String(v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}

async function openArrange(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.keyboard.press('2');
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
}

test('scene mark: right click on the rail colours the scene, undo takes it back', async ({ page }) => {
  await openArrange(page);
  const scene = page.getByTestId('rail-scene').first();
  const plain = await scene.evaluate((el) => getComputedStyle(el).backgroundColor);

  await scene.click({ button: 'right' });
  await page.getByTestId('status-review').click();
  await expect(page.getByTestId('status-menu')).toBeHidden();
  await expect(scene).toHaveAttribute('data-status', 'review');
  await expect(scene).toHaveAttribute('aria-label', /Review/);
  expect(await scene.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(plain);

  // the menu says what's set, and the keyboard reaches it
  await scene.click({ button: 'right' });
  await expect(page.getByTestId('status-review')).toHaveAttribute('aria-checked', 'true');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('status-menu')).toBeHidden();
  await expect(scene).toHaveAttribute('data-status', 'review');

  await page.keyboard.press('Meta+z');
  await expect(scene).not.toHaveAttribute('data-status', 'review');
});

test('pan: a scene takes the room’s value until it gets its own, and goes back to it', async ({ page }) => {
  await openArrange(page);
  await expect(page.getByTestId('room-chip')).toHaveCount(0);
  await page.getByTestId('pan-button').click();
  const pop = page.getByTestId('pan-pop');
  await expect(pop).toBeVisible();
  const row = page.getByTestId('pan-row').first();
  const slider = row.getByRole('slider');
  const value = row.getByTestId('pan-value');
  await expect(value).toHaveText('C');

  // the room's value, then this scene inherits it
  await page.getByTestId('pan-room').click();
  await setSlider(slider, -0.4);
  await expect(value).toHaveText('L 40');
  await page.getByTestId('pan-scene').click();
  await expect(value).toHaveText('L 40');
  await expect(row.getByTestId('pan-reset')).toHaveCount(0);

  // a change here stays here, and the room keeps its value
  await setSlider(slider, 0.3);
  await expect(value).toHaveText('R 30');
  await expect(row.getByTestId('pan-reset')).toBeVisible();
  const [p] = await rpc<{ pan: number; room: number; custom: boolean }[]>('scene_pans', { scene: '1' });
  expect(p).toMatchObject({ pan: 0.3, room: -0.4, custom: true });

  await row.getByTestId('pan-reset').click();
  await expect(value).toHaveText('L 40');

  // one step each, all the way back
  await page.getByTestId('pan-scene').click();
  for (let i = 0; i < 3; i++) await page.keyboard.press('Meta+z');
  await expect(value).toHaveText('C');

  await page.keyboard.press('Escape');
  await expect(pop).toBeHidden();
  await expect(page.getByTestId('pan-button')).toBeFocused();
});

test('pan: how far a panned voice goes back into the room is a setting', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  const status = () => rpc<{ pan_room: number }>('project_status');
  expect((await status()).pan_room).toBe(0.5);
  await page.getByRole('button', { name: 'Settings' }).click();
  const slider = page.getByTestId('pan-room-setting');
  await setSlider(slider, 20);
  await expect.poll(async () => (await status()).pan_room).toBe(0.2);
  await setSlider(slider, 50);
  await expect.poll(async () => (await status()).pan_room).toBe(0.5);
});

/** Drags a line by its grip onto the right half of another. */
async function dragBeside(page: Page, from: number, onto: number) {
  const rows = page.getByTestId('arrange-row');
  const grip = (await rows.nth(from).locator('.grip').boundingBox())!;
  const box = (await rows.nth(onto).boundingBox())!;
  await page.mouse.move(grip.x + 8, grip.y + 8);
  await page.mouse.down();
  const to = { x: box.x + box.width * 0.8, y: box.y + box.height / 2 };
  for (let i = 1; i <= 8; i++) await page.mouse.move(grip.x + ((to.x - grip.x) * i) / 8, grip.y + ((to.y - grip.y) * i) / 8);
}

test('side by side: drag a line onto another, offset it, and put it back', async ({ page }) => {
  await openArrange(page);
  const rows = page.getByTestId('arrange-row');
  const count = await rows.count();
  const steps = async () => (await rpc<{ state: string }[]>('history', { limit: 100000 })).filter((r) => r.state === 'done').length;
  const before = await steps();

  await dragBeside(page, 2, 1);
  await expect(page.locator('.snip.slotted')).toHaveCount(1);
  await page.mouse.up();

  const pair = page.getByTestId('arrange-pair');
  await expect(pair).toHaveCount(1);
  await expect(pair.getByTestId('arrange-row')).toHaveCount(2);
  await expect(rows).toHaveCount(count);
  await expect.poll(steps).toBe(before + 1);
  const cards = () => Promise.all([pair.getByTestId('arrange-row').first().boundingBox(), pair.getByTestId('arrange-row').last().boundingBox()]);
  // the column settles after the drop
  await expect.poll(async () => { const [x, y] = await cards(); return Math.abs(x!.y - y!.y); }).toBeLessThan(2);
  const [a, b] = (await cards()) as { x: number; y: number; width: number }[];
  expect(b.x).toBeGreaterThan(a.x + a.width - 1);
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/pair.png` : 'test-results/pair.png' });

  // a third can't join: the pair offers no place
  await dragBeside(page, 0, 1);
  await expect(page.locator('.snip.slotted')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.mouse.move(10, 10);
  await page.mouse.up();

  // the offset: the second card follows the mouse, and the view keeps it
  const pill = page.getByTestId('pair-offset');
  await expect(pill).toHaveText('together');
  const p = (await pill.boundingBox())!;
  await page.mouse.move(p.x + 4, p.y + 4);
  await page.mouse.down();
  for (let i = 1; i <= 4; i++) await page.mouse.move(p.x + 4, p.y + 4 + 6.5 * i);
  await page.mouse.up();
  await expect(pill).toHaveText('+1.00 s');
  await expect.poll(async () => { const [x, y] = await cards(); return y!.y - x!.y; }).toBeGreaterThan(20);
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/pair-offset.png` : 'test-results/pair-offset.png' });

  await page.getByTestId('pair-undo').click();
  await expect(pair).toHaveCount(0);
  await page.keyboard.press('Meta+z');
  await expect(page.getByTestId('pair-offset')).toHaveText('+1.00 s');
  await page.keyboard.press('Meta+z');
  await page.keyboard.press('Meta+z');
  await expect(page.getByTestId('arrange-pair')).toHaveCount(0);
});
