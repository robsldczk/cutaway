import { expect, test, type Page } from '@playwright/test';

// The Export page on the demo session. The session is shared: what a test locks, it unlocks.
// Exported versions stay on disk, so tests read version numbers instead of assuming them.

const shots = process.env.CUTAWAY_SHOTS;
const bridge = 'http://127.0.0.1:7358/rpc';

async function rpc<T = unknown>(page: Page, method: string, params: object = {}): Promise<T> {
  const r = await page.request.post(bridge, { data: { method, params } });
  const body = await r.json();
  if (!body.ok) throw new Error(body.error.message);
  return body.result;
}

async function unlockAll(page: Page) {
  const { scenes } = await rpc<{ scenes: { number: number; lock: unknown }[] }>(page, 'export_status');
  for (const s of scenes) if (s.lock) await rpc(page, 'unlock_scene', { scene: String(s.number), confirm: true });
}

const box = (page: Page, number: number) => page.locator(`[data-testid="scene-box"][data-number="${number}"]`);

async function open(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await unlockAll(page);
  await page.keyboard.press('4');
  await expect(page.getByRole('region', { name: 'Export' })).toBeVisible();
  await expect(page.getByTestId('export-size')).toContainText('files');
}

test('Export is the fourth page, with the defaults of the sheet', async ({ page }) => {
  await open(page);
  const nav = page.getByRole('navigation', { name: 'Pages' });
  await expect(nav.getByRole('button')).toHaveText([/Import/, /Options/, /Arrange/, /Edit/, /Export/]);
  await expect(page.getByTestId('export-as')).toHaveValue('stems');
  for (const t of ['dialogue', 'room_voices', 'effects', 'room_effects', 'ambience', 'music', 'mix']) await expect(page.getByTestId(`track-${t}`)).toBeChecked();
  await expect(page.getByTestId('track-split_dialogue')).not.toBeChecked();
  await expect(page.getByTestId('format')).toHaveValue('wav/48000/24');
  await expect(page.getByTestId('loudness')).toHaveValue('-16');
  await expect(page.getByTestId('also-fcpxml')).toBeChecked();
  await expect(page.getByTestId('also-srt')).toBeChecked();
  await expect(page.getByTestId('also-cutlist')).not.toBeChecked();
  await expect(page.getByTestId('lock-after')).toBeChecked();
  await expect(page.getByTestId('save-to')).toHaveValue('episode');
  await expect(page.getByTestId('range-caption')).toContainText('1 of 2 scenes');
  await expect(page.getByTestId('saved-caption')).toContainText('Versions count up on their own');
  if (shots) await page.screenshot({ path: `${shots}/export.png`, fullPage: true });
});

test('the sheet lies wide: three columns side by side, and Export stays in view', async ({ page }) => {
  await open(page);
  const columns = page.getByRole('region', { name: 'Export' }).locator('.colx');
  const go = page.getByTestId('export-go');
  for (const [width, height] of [[1440, 900], [1280, 720], [900, 640]]) {
    await page.setViewportSize({ width, height });
    const boxes = await columns.evaluateAll((els) => els.map((e) => e.getBoundingClientRect().toJSON() as DOMRect));
    expect(boxes).toHaveLength(3);
    expect(boxes[1].x).toBeGreaterThanOrEqual(boxes[0].right);
    expect(boxes[2].x).toBeGreaterThanOrEqual(boxes[1].right);
    expect(boxes[1].y).toBe(boxes[0].y);
    expect(boxes[2].y).toBe(boxes[0].y);
    await expect(go).toBeInViewport({ ratio: 1 });
  }

  // no column is left empty when the rows go
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.getByTestId('export-as').selectOption('fcpxml');
  await expect(columns.nth(2)).toContainText('Also export');
  await page.getByTestId('export-as').selectOption('stems');

  // shorter than the sheet, the columns scroll inside it and the footer stays
  await page.setViewportSize({ width: 900, height: 420 });
  await expect(go).toBeInViewport({ ratio: 1 });
  const form = page.locator('.form');
  expect(await form.evaluate((e) => e.scrollHeight > e.clientHeight)).toBe(true);
  await page.getByTestId('save-to').scrollIntoViewIfNeeded();
  await expect(page.getByTestId('save-to')).toBeInViewport();
  await expect(go).toBeInViewport({ ratio: 1 });
});

test('what it exports as decides the rows', async ({ page }) => {
  await open(page);
  const tracks = page.getByTestId('track-dialogue');
  const format = page.getByTestId('format');
  await page.getByTestId('export-as').selectOption('mix');
  await expect(tracks).toBeHidden();
  await expect(format).toBeVisible();
  await expect(format.locator('option', { hasText: 'MP3' })).toHaveCount(2);
  await page.getByTestId('export-as').selectOption('fcpxml');
  await expect(tracks).toBeHidden();
  await expect(format).toBeHidden();
  await page.getByTestId('export-as').selectOption('stems');
  await expect(tracks).toBeVisible();
  await expect(format.locator('option', { hasText: 'MP3' })).toHaveCount(0);
});

test('an MP3 mix falls back to WAV when the mode goes back to stems', async ({ page }) => {
  await open(page);
  await page.getByTestId('export-as').selectOption('mix');
  await page.getByTestId('format').selectOption('mp3/48000/24');
  await page.getByTestId('export-as').selectOption('stems');
  await expect(page.getByTestId('format')).toHaveValue('wav/48000/24');
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('the footer follows the choices', async ({ page }) => {
  await open(page);
  const size = page.getByTestId('export-size');
  const before = await size.textContent();
  await page.getByTestId('track-music').uncheck();
  await expect(size).not.toHaveText(before as string);
  await page.getByTestId('select-all').click();
  await expect(page.getByTestId('range-caption')).toContainText('All 2 scenes');
  await expect(page.getByTestId('saved-caption')).toContainText('The episode becomes');
  await box(page, 2).click();
  await expect(page.getByTestId('range-caption')).toContainText('1 of 2 scenes');
  await expect(page.getByTestId('saved-caption')).not.toContainText('The episode becomes');
  await page.getByTestId('track-music').check();
});

test('the boxes select all, none, the other way round, and in runs', async ({ page }) => {
  await open(page);
  const caption = page.getByTestId('range-caption');
  const go = page.getByTestId('export-go');
  await expect(box(page, 1)).toBeChecked();
  await expect(box(page, 2)).not.toBeChecked();

  await page.getByTestId('select-invert').click();
  await expect(box(page, 1)).not.toBeChecked();
  await expect(box(page, 2)).toBeChecked();

  await page.getByTestId('select-none').click();
  await expect(caption).toContainText('0 of 2 scenes');
  await expect(go).toBeDisabled();

  await page.getByTestId('select-all').click();
  await expect(caption).toContainText('All 2 scenes');
  await expect(page.getByTestId('select-all')).toBeDisabled();

  // ⇧ takes the run from the last box touched, and turns it the way the box goes
  await page.getByTestId('select-none').click();
  await box(page, 1).click();
  await box(page, 2).click({ modifiers: ['Shift'] });
  await expect(caption).toContainText('All 2 scenes');
  await box(page, 2).click();
  await box(page, 1).click({ modifiers: ['Shift'] });
  await expect(caption).toContainText('0 of 2 scenes');

  // ⌘ is a click: it never lets go of the other boxes
  await box(page, 1).click();
  await box(page, 2).click({ modifiers: ['Meta'] });
  await expect(caption).toContainText('All 2 scenes');

  // a press sweeps across the boxes
  await page.getByTestId('select-none').click();
  const first = await box(page, 1).boundingBox();
  const second = await box(page, 2).boundingBox();
  if (!first || !second) throw new Error('the boxes are not on the page');
  await page.mouse.move(first.x + 30, first.y + 10);
  await page.mouse.down();
  await page.mouse.move(second.x + 30, second.y + 10, { steps: 4 });
  await page.mouse.up();
  await expect(caption).toContainText('All 2 scenes');
  // and the press ends: moving on ticks nothing
  await page.getByTestId('select-none').click();
  await page.mouse.move(first.x + 30, first.y + 10);
  await page.mouse.move(second.x + 30, second.y + 10);
  await expect(caption).toContainText('0 of 2 scenes');

  // the keyboard ticks too, with nothing played
  await box(page, 1).focus();
  await page.keyboard.press('Space');
  await expect(box(page, 1)).toBeChecked();
});

test('scenes come by their mark, and the marks count them', async ({ page }) => {
  await rpc(page, 'set_scene_status', { scene: '2', status: 'finished' });
  await rpc(page, 'set_scene_status', { scene: '1', status: 'review' });
  try {
    await open(page);
    const finished = page.getByTestId('select-finished');
    await expect(finished).toContainText('1');
    await expect(page.getByTestId('select-in_progress')).toBeDisabled();

    // the current scene was ticked: a mark takes just its own
    await finished.click();
    await expect(box(page, 1)).not.toBeChecked();
    await expect(box(page, 2)).toBeChecked();
    await expect(finished).toHaveAttribute('aria-pressed', 'true');

    // ⌘ adds, and takes off again
    await page.getByTestId('select-review').click({ modifiers: ['Meta'] });
    await expect(page.getByTestId('range-caption')).toContainText('All 2 scenes');
    await page.getByTestId('select-review').click({ modifiers: ['Meta'] });
    await expect(box(page, 1)).not.toBeChecked();
    await expect(box(page, 2)).toBeChecked();
  } finally {
    await rpc(page, 'set_scene_status', { scene: '1' });
    await rpc(page, 'set_scene_status', { scene: '2' });
  }
});

test('the episode goes out as one file, and folders need no quoting', async ({ page }) => {
  await open(page);
  await page.getByTestId('select-all').click();
  await page.getByTestId('export-as').selectOption('one_file');
  await expect(page.getByTestId('track-dialogue')).toBeHidden();
  await expect(page.getByTestId('saved-caption')).toContainText(/Saved as E\d+ v\d+/);
  await page.getByTestId('lock-after').uncheck();

  // a mix can be an MP3, stems can't
  const mp3 = await page.getByTestId('format').locator('option', { hasText: 'MP3' }).count();
  expect(mp3).toBeGreaterThan(0);

  await page.getByTestId('export-go').click();
  await expect(page.getByTestId('export-done')).toContainText('Saved one file', { timeout: 30_000 });
  const status = await rpc<{ scenes: { lock: unknown }[]; episode_versions: { folder: string; files: number }[] }>(page, 'export_status');
  expect(status.scenes.every((s) => !s.lock)).toBe(true);
  const newest = status.episode_versions[status.episode_versions.length - 1];
  expect(newest.folder.split('/Exports/')[1]).toMatch(/^Episode\/v\d{3}_\d{4}-\d{2}-\d{2}$/);

  // it's in the history, and the same scenes say so
  await expect(page.getByTestId('unchanged-caption')).toContainText('unchanged since');
  await page.getByTestId('export-as').selectOption('stems');
  await page.getByTestId('lock-after').check();
});

test('an export counts up, says so, and locks the scene everywhere', async ({ page }) => {
  await open(page);
  await page.getByTestId('export-go').click();
  await expect(page.getByTestId('export-done')).toContainText('Saved Sc. 1 v', { timeout: 30_000 });
  await expect(page.getByTestId('unchanged-caption')).toContainText('unchanged since');
  await expect(page.locator('summary')).toContainText('version');

  await page.keyboard.press('1');
  const banner = page.getByTestId('lock-banner');
  await expect(banner).toContainText("is locked. Exported as v");
  await expect(banner).toContainText("Timing can't change.");
  await expect(page.getByRole('button', { name: /Scene 1.*locked/ })).toBeVisible();
  if (shots) await page.screenshot({ path: `${shots}/locked.png` });

  // a blocked edit says so where it happened, and nothing moved
  const before = await page.getByTestId('take-pos').textContent();
  await page.keyboard.press('ArrowLeft');
  await expect(page.getByTestId('blocked')).toContainText('is locked');
  await expect(page.getByTestId('take-pos')).toHaveText(before as string);

  // the Edit page hides the trim handles of a locked scene
  await page.keyboard.press('3');
  await expect(page.getByTestId('lock-banner')).toBeVisible();
  await expect(page.locator('.eh')).toHaveCount(0);

  // unlocking asks first, says what it means, and keeps the lock on the way out
  await banner.getByRole('button', { name: 'Unlock' }).click();
  await expect(page.getByTestId('unlock-question')).toContainText("The exported files won't match anymore. The next export becomes v");
  await page.getByRole('button', { name: 'Keep locked' }).click();
  await expect(banner).toBeVisible();
  await banner.getByRole('button', { name: 'Unlock' }).click();
  await page.getByTestId('unlock-confirm').click();
  await expect(banner).toBeHidden();
  await expect(page.locator('.eh').first()).toBeAttached();
});

test('⌘E goes to the page, and on it exports', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await unlockAll(page);
  await page.keyboard.press('Meta+e');
  await expect(page.getByTestId('export-size')).toContainText('files');
  await page.getByTestId('lock-after').uncheck();
  await page.keyboard.press('Meta+e');
  await expect(page.getByTestId('export-done')).toContainText('Saved Sc. 1 v', { timeout: 30_000 });
  await page.getByTestId('lock-after').check();
});

test('locking by hand works from the scene list', async ({ page }) => {
  await open(page);
  await page.keyboard.press('1');
  await page.getByRole('button', { name: 'Expand scene list' }).click();
  await page.getByRole('button', { name: 'Lock scene 2' }).click();
  await expect(page.getByRole('button', { name: 'Unlock scene 2' })).toBeVisible();
  await page.getByRole('button', { name: 'Unlock scene 2' }).click();
  await page.getByTestId('unlock-confirm').click();
  await expect(page.getByRole('button', { name: 'Lock scene 2' })).toBeVisible();
});

test('a preset keeps the choices', async ({ page }) => {
  await open(page);
  await page.getByTestId('export-as').selectOption('mix');
  await page.getByTestId('save-preset').click();
  await page.getByTestId('preset-name').fill('Rough mix');
  await page.getByTestId('preset-name').press('Enter');
  await page.getByTestId('export-as').selectOption('stems');
  await page.getByLabel('Preset', { exact: true }).selectOption('Rough mix');
  await expect(page.getByTestId('export-as')).toHaveValue('mix');
  await rpc(page, 'delete_export_preset', { name: 'Rough mix' });
  await page.getByTestId('export-as').selectOption('stems');
});

test('at phone width the sheet stacks and nothing scrolls sideways', async ({ page }) => {
  await open(page);
  await page.setViewportSize({ width: 390, height: 800 });
  const over = await page.locator('.page').evaluate((el) => el.scrollWidth - el.clientWidth);
  expect(over).toBeLessThanOrEqual(0);
  await page.setViewportSize({ width: 1440, height: 900 });
});
