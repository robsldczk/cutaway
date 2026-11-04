import { expect, test, type Locator, type Page } from '@playwright/test';

// The rooms on Import › Sounds: one per scene, one undo step per slider move.

async function openRooms(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await page.getByRole('tab', { name: 'Sounds' }).click();
  await page.getByTestId('scene-rooms').locator('summary').click();
  await expect(page.getByTestId('scene-room').first()).toBeVisible();
}

/** Moves a slider as a release does: the value, then one change event. */
async function setSlider(s: Locator, value: number) {
  await s.evaluate((el: HTMLInputElement, v) => {
    el.value = String(v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}

test('import › sounds: a room per scene, set and undone', async ({ page }) => {
  await openRooms(page);
  const rows = page.getByTestId('scene-room');
  const size = page.getByRole('slider', { name: 'Size of scene 1' });
  const reading = rows.first().getByTestId('room-reading');
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

test('import › sounds: a room stays with its scene', async ({ page }) => {
  await openRooms(page);
  const rows = page.getByTestId('scene-room');
  test.skip((await rows.count()) < 2, 'the demo session has one scene');
  const wet1 = page.getByRole('slider', { name: 'Wet of scene 1' });
  const wet2 = page.getByRole('slider', { name: 'Wet of scene 2' });
  const other = await wet2.inputValue();
  await setSlider(wet1, 0.2);
  await expect(wet1).toHaveValue('0.2');
  await expect(wet2).toHaveValue(other);
  await wet1.focus();
  await page.keyboard.press('ControlOrMeta+z');
});

test('import › sounds: arrow keys on a room slider stay on the slider', async ({ page }) => {
  await openRooms(page);
  const wet = page.getByRole('slider', { name: 'Wet of scene 1' });
  const start = +(await wet.inputValue());
  await wet.focus();
  await page.keyboard.press('ArrowRight');
  await expect(wet).toHaveValue(String(+(start + 0.01).toFixed(2)));
  await expect(page.getByTestId('scene-rooms')).toBeVisible();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(wet).toHaveValue(String(start));
});
