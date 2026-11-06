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
