import { expect, test, type Page } from '@playwright/test';

// The mapping editor (D104) on the demo session: mark a take by hand, add a line, delete one.
// Each test undoes what it did: the session is shared.

const shots = process.env.CUTAWAY_SHOTS;

async function open(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await expect(page.getByTestId('take-pos')).toHaveText('2 / 2');
}

async function openEditor(page: Page) {
  await page.keyboard.press('m');
  await expect(page.getByTestId('mapping-editor')).toBeVisible();
  await expect(page.getByTestId('mapping-word').first()).toBeVisible();
  await expect(page.getByTestId('mapping-marks')).toContainText('–');
  await expect(page.locator('[data-testid="mapping-lane"] .wave')).toBeVisible();
}

test('M opens on the take, with its marks, words and the whole recording', async ({ page }) => {
  await open(page);
  await openEditor(page);
  await expect(page.getByTestId('mapping-own-take').first()).toBeVisible();
  await expect(page.getByTestId('mapping-overview')).toBeVisible();
  await expect(page.getByTestId('mark-in')).toBeVisible();
  await expect(page.getByTestId('mark-out')).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/mapping.png` });
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('mapping-editor')).toBeHidden();
});

test('only Options has it', async ({ page }) => {
  await open(page);
  await page.keyboard.press('2');
  await page.keyboard.press('m');
  await expect(page.getByTestId('mapping-editor')).toBeHidden();
});

test('marks drag, and a new take is created, picked and undone', async ({ page }) => {
  await open(page);
  await openEditor(page);
  const marks = page.getByTestId('mapping-marks');
  const before = await marks.textContent();

  const lane = (await page.getByTestId('mapping-lane').boundingBox())!;
  const handle = (await page.getByTestId('mark-out').boundingBox())!;
  await page.mouse.move(handle.x + handle.width / 2, lane.y + 80);
  await page.mouse.down();
  await page.mouse.move(handle.x + handle.width / 2 + 60, lane.y + 80, { steps: 4 });
  await page.mouse.up();
  await expect(marks).not.toHaveText(before!);

  await page.getByTestId('mapping-create').click();
  await expect(page.getByTestId('mapping-editor')).toBeHidden();
  await expect(page.getByTestId('take-pos')).toHaveText('3 / 3');
  await page.keyboard.press('Meta+z');
  await expect(page.getByTestId('take-pos')).toHaveText('2 / 2');
});

test('a search jumps to a passage and marks it', async ({ page }) => {
  await open(page);
  await openEditor(page);
  const said = await page.getByTestId('mapping-word').nth(1).textContent();
  await page.getByTestId('mapping-search').fill(said!.replace(/[.,?!]/g, ''));
  const hit = page.getByTestId('mapping-hit').first();
  await expect(hit).toBeVisible();
  const before = await page.getByTestId('mapping-marks').textContent();
  await hit.click();
  await expect(page.getByTestId('mapping-marks')).not.toHaveText(before!);
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('mapping-editor')).toBeHidden();
});

test('Enter in the search takes the first hit, then the keys are the editor\'s again', async ({ page }) => {
  await open(page);
  await openEditor(page);
  const search = page.getByTestId('mapping-search');
  await search.fill('coffee');
  await expect(page.getByTestId('mapping-hit').first()).toBeVisible();
  const before = await page.getByTestId('mapping-marks').textContent();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('mapping-marks')).not.toHaveText(before!);
  await expect(search).not.toBeFocused();
  await page.keyboard.press('i');
  await expect(search).toHaveValue('coffee');
  await page.keyboard.press('Escape');
});

test('Space plays the marks without a click first', async ({ page }) => {
  await open(page);
  await openEditor(page);
  await page.keyboard.press('Space');
  await expect(page.getByTestId('mapping-search')).toHaveValue('');
  await expect(page.getByTestId('mapping-play').locator('rect')).toBeVisible();
  await page.keyboard.press('Space');
  await page.keyboard.press('Escape');
});

test('a drag on the waveform marks a stretch, I and O mark at the locator', async ({ page }) => {
  await open(page);
  await openEditor(page);
  const marks = page.getByTestId('mapping-marks');
  const lane = (await page.getByTestId('mapping-lane').boundingBox())!;
  await page.mouse.move(lane.x + 100, lane.y + 80);
  await page.mouse.down();
  await page.mouse.move(lane.x + 300, lane.y + 80, { steps: 4 });
  await page.mouse.up();
  const dragged = await marks.textContent();

  await page.mouse.click(lane.x + 200, lane.y + 80);
  await page.keyboard.press('o');
  await expect(marks).not.toHaveText(dragged!);
  await page.keyboard.press('Escape');
});

test('the wheel moves along the recording, ⌥ wheel zooms', async ({ page }) => {
  await open(page);
  await openEditor(page);
  const lane = (await page.getByTestId('mapping-lane').boundingBox())!;
  const port = page.locator('[data-testid="mapping-overview"] .port');
  await page.mouse.move(lane.x + 300, lane.y + 80);

  const start = (await port.boundingBox())!;
  await page.mouse.wheel(300, 0);
  await expect.poll(async () => (await port.boundingBox())!.x).toBeGreaterThan(start.x);
  expect((await port.boundingBox())!.width).toBeCloseTo(start.width, 0);

  await page.keyboard.down('Alt');
  await page.mouse.wheel(0, -200);
  await page.keyboard.up('Alt');
  await expect.poll(async () => (await port.boundingBox())!.width).toBeLessThan(start.width);
  await page.keyboard.press('Escape');
});

test('the plus shows on hover between two lines', async ({ page }) => {
  await open(page);
  const plus = page.getByTestId('add-line').nth(2);
  await expect(plus).toHaveCSS('opacity', '0');
  await plus.hover();
  await expect(plus).toHaveCSS('opacity', '1');
  if (shots) await page.screenshot({ path: `${shots}/mapping-plus.png` });
});

test('the plus adds a line between two lines, from a stretch', async ({ page }) => {
  await open(page);
  const count = await page.locator('.boxs').count();
  await page.getByTestId('add-line').nth(2).click();
  await expect(page.getByTestId('mapping-editor')).toBeVisible();
  await expect(page.getByTestId('mapping-search')).toBeFocused();
  await expect(page.getByTestId('mapping-word').first()).toBeVisible();
  await expect(page.getByTestId('mapping-create')).toBeDisabled();

  const lane = (await page.getByTestId('mapping-lane').boundingBox())!;
  await page.mouse.move(lane.x + 300, lane.y + 80);
  await page.mouse.down();
  await page.mouse.move(lane.x + 700, lane.y + 80, { steps: 4 });
  await page.mouse.up();
  await page.getByTestId('line-text').fill('Oh wow.');
  if (shots) await page.screenshot({ path: `${shots}/mapping-add.png` });
  await page.getByTestId('mapping-create').click();
  await expect(page.getByTestId('mapping-editor')).toBeHidden();
  await expect(page.getByTestId('current-text')).toContainText('wow');
  await expect(page.locator('.added')).toBeVisible();

  await page.keyboard.press('Meta+z');
  await expect(page.locator('.boxs')).toHaveCount(count);
});

test('the X asks first, deletes, and ⌘Z brings the line back', async ({ page }) => {
  await open(page);
  const lines = page.locator('.boxs');
  const count = await lines.count();
  const row = lines.first();
  const text = await row.locator('.txt').textContent();
  await row.hover();
  await row.getByTestId('delete-line').click();

  // a no first
  await expect(page.getByRole('dialog')).toContainText(text!);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toBeHidden();
  await expect(lines).toHaveCount(count);

  await row.hover();
  await row.getByTestId('delete-line').click();
  if (shots) await page.screenshot({ path: `${shots}/mapping-delete.png` });
  await page.keyboard.press('Enter');
  await expect(lines).toHaveCount(count - 1);

  await page.keyboard.press('Meta+z');
  await expect(lines).toHaveCount(count);
});
