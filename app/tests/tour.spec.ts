import { expect, test, type Page } from '@playwright/test';

// The tours on the demo session. One session serves every spec, so this file's whole point is
// also its duty: what a tour tries goes back, and that's checked against the core, not the screen.
test.use({ storageState: { cookies: [], origins: [] } });

interface Item {
  kind: string;
  line_id: string;
  take_id: string;
  start_s: number;
  end_s: number;
  takes: { id: string; favorite: boolean }[];
}

const rpc = async <T = Record<string, never>>(page: Page, method: string, params: object = {}): Promise<T> => {
  const r = await page.request.post('http://127.0.0.1:7358/rpc', { data: { method, params } });
  const b = await r.json();
  if (!b.ok) throw new Error(b.error.message);
  return b.result;
};

/** What the cut is: order, picks, favorites and timing. */
async function cut(page: Page) {
  const { items } = await rpc<{ items: Item[] }>(page, 'scene_view', { scene: '1' });
  return JSON.stringify(items.map((i) => [i.line_id, i.take_id, i.start_s, i.end_s, i.takes.map((t) => t.favorite)]));
}

const seen = (page: Page, chapters: string[]) =>
  page.addInitScript((c) => {
    if (!localStorage.getItem('cutaway.tour')) localStorage.setItem('cutaway.tour', JSON.stringify(c));
  }, chapters);

async function open(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('header')).toBeVisible();
  await expect(page.getByTestId('current-box')).toBeVisible();
}

const card = (page: Page) => page.getByTestId('tour-card');
const next = (page: Page) => page.getByTestId('tour-next').click();
const at = (page: Page, pos: string) => expect(page.getByTestId('tour-pos')).toHaveText(pos);

/** Where the lit thing is, once the spotlight has slid over to it. */
async function lit(page: Page) {
  await page.waitForTimeout(350);
  return (await page.locator('[data-tour-lit]').boundingBox())!;
}

/** Ends the tour and waits for it to be over. */
async function skip(page: Page) {
  await page.getByTestId('tour-skip').click();
  await expect(page.getByTestId('tour')).toBeHidden();
}

test.afterEach(async ({ page }) => {
  const s = await rpc<{ review: { active: boolean } }>(page, 'project_status');
  if (s.review.active) await rpc(page, 'leave_review');
});

test('turns on the first time, and stays away once skipped', async ({ page }) => {
  await open(page);
  await expect(card(page)).toContainText('Your session, cut to the script');
  await at(page, '1 / 8');
  await skip(page);
  await page.reload();
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.waitForTimeout(500);
  await expect(page.getByTestId('tour')).toBeHidden();
});

test('walks the stops with the keys and the buttons, and lights what it talks about', async ({ page }) => {
  await open(page);
  await expect(card(page)).toBeVisible();
  await expect(page.getByTestId('tour-hole')).toHaveCount(0);
  await page.keyboard.press('Enter');
  await at(page, '2 / 8');
  await expect(page.getByTestId('tour-hole')).toBeVisible();
  await expect(page.locator('[data-tour="box"]')).toHaveAttribute('data-tour-lit', '');
  await next(page);
  await at(page, '3 / 8');
  await page.getByTestId('tour-back').click();
  await at(page, '2 / 8');
  for (let i = 2; i < 8; i++) await next(page);
  await at(page, '8 / 8');
  await expect(page.getByTestId('tour-next')).toHaveText('Done');
  await next(page);
  await expect(page.getByTestId('tour')).toBeHidden();
  expect(await page.evaluate(() => localStorage.getItem('cutaway.tour'))).toContain('options');
});

test('what was tried on the takes goes back, and the line and playhead with it', async ({ page }) => {
  await open(page);
  const before = await cut(page);
  await next(page);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await next(page);
  await expect(card(page)).toContainText('Its takes');
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => cut(page)).not.toBe(before);
  await at(page, '3 / 8');
  await page.keyboard.press('f');
  await page.getByTestId('take-pill').first().click();
  await skip(page);
  await expect.poll(() => cut(page)).toBe(before);
  await expect(page.getByTestId('take-pill').first()).toBeVisible();
});

test('nothing else gets through: not the pages, not undo, not a right click', async ({ page }) => {
  await open(page);
  const before = await cut(page);
  await next(page);
  await next(page);
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => cut(page)).not.toBe(before);
  const tried = await cut(page);
  await page.keyboard.press('2');
  await page.keyboard.press('Meta+z');
  await page.keyboard.press('Control+z');
  await page.keyboard.press('e');
  await page.getByTestId('take-pill').first().click({ button: 'right' });
  await page.waitForTimeout(300);
  await expect(card(page)).toContainText('Its takes');
  await expect(page.getByRole('menu')).toHaveCount(0);
  await expect(page.locator('[data-tour="box"]')).toBeVisible();
  expect(await cut(page)).toBe(tried);
  await skip(page);
  await expect.poll(() => cut(page)).toBe(before);
});

test('a quit in the middle is put right when the episode opens again', async ({ page }) => {
  await open(page);
  const before = await cut(page);
  await next(page);
  await next(page);
  await page.keyboard.press('ArrowRight');
  await expect.poll(() => cut(page)).not.toBe(before);
  await expect.poll(() => page.evaluate(() => localStorage.getItem('cutaway.tourOwed'))).toContain('steps');
  await page.reload();
  await expect(page.getByTestId('current-box')).toBeVisible();
  await expect.poll(() => cut(page)).toBe(before);
  expect(await page.evaluate(() => localStorage.getItem('cutaway.tourOwed'))).toBeNull();
  await skip(page);
});

test('? brings the page’s chapter back, after the keys', async ({ page }) => {
  await seen(page, ['options', 'arrange', 'edit', 'export', 'review']);
  await open(page);
  await expect(page.getByTestId('tour')).toBeHidden();
  await page.keyboard.press('?');
  await page.getByTestId('take-tour').click();
  await expect(card(page)).toContainText('Your session');
  await skip(page);
  await page.keyboard.press('2');
  await page.getByRole('button', { name: 'Keys and tour' }).first().click();
  await expect(page.getByTestId('take-tour')).toContainText('Arrange');
  await page.getByTestId('take-tour').click();
  await expect(card(page)).toContainText('Reorder');
  await skip(page);
});

test('Arrange: a drag on the grip, a pause and the sounds all go back', async ({ page }) => {
  await seen(page, ['options']);
  await open(page);
  const before = await cut(page);
  await page.keyboard.press('2');
  await expect(card(page)).toContainText('Reorder');
  const grip = await lit(page);
  await page.mouse.move(grip.x + grip.width / 2, grip.y + grip.height / 2);
  await page.mouse.down();
  await page.mouse.move(grip.x + grip.width / 2, grip.y + 120, { steps: 6 });
  await page.mouse.move(grip.x + grip.width / 2, grip.y + 240, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => cut(page)).not.toBe(before);
  await next(page);
  await next(page);
  await expect(card(page)).toContainText('Pauses');
  const moved = await cut(page);
  await page.keyboard.press('.');
  await expect.poll(() => cut(page)).not.toBe(moved);
  await next(page);
  const on = await page.getByTestId('sounds-toggle').getAttribute('aria-pressed');
  await page.keyboard.press('s');
  await expect(page.getByTestId('sounds-toggle')).not.toHaveAttribute('aria-pressed', on!);
  await next(page);
  await expect(page.getByTestId('tour')).toBeHidden();
  await expect.poll(() => cut(page)).toBe(before);
  await expect(page.getByTestId('sounds-toggle')).toHaveAttribute('aria-pressed', on!);
});

test('Edit: a dragged clip and the switches go back', async ({ page }) => {
  await seen(page, ['options', 'arrange']);
  await open(page);
  const before = await cut(page);
  await page.keyboard.press('3');
  await expect(card(page)).toContainText('The full timeline');
  await next(page);
  await next(page);
  await expect(card(page)).toContainText('Clips');
  const clip = await lit(page);
  await page.mouse.move(clip.x + clip.width / 2, clip.y + clip.height / 2);
  await page.mouse.down();
  await page.mouse.move(clip.x + clip.width / 2 + 90, clip.y + clip.height / 2, { steps: 8 });
  await page.mouse.up();
  await expect.poll(() => cut(page)).not.toBe(before);
  await next(page);
  const bucket = page.locator('[data-tour="bucket"]');
  const open0 = await bucket.getAttribute('aria-pressed');
  await page.keyboard.press('b');
  await expect(bucket).not.toHaveAttribute('aria-pressed', open0!);
  await next(page);
  const ripple = page.locator('[data-tour="ripple"]').first();
  await ripple.click();
  await expect(ripple).toHaveAttribute('aria-pressed', 'false');
  await skip(page);
  await expect.poll(() => cut(page)).toBe(before);
  await expect(ripple).toHaveAttribute('aria-pressed', 'true');
  await expect(bucket).toHaveAttribute('aria-pressed', open0!);
});

test('Export has its chapter, and nothing there can be pressed', async ({ page }) => {
  await seen(page, ['options', 'arrange', 'edit']);
  await open(page);
  await page.keyboard.press('4');
  await expect(card(page)).toContainText('What goes out');
  await next(page);
  await expect(card(page)).toContainText('Or get notes first');
  await next(page);
  await expect(page.getByTestId('tour')).toBeHidden();
});

test('review: it waits for who’s reviewing, and a note tried isn’t kept', async ({ page }) => {
  await seen(page, ['options', 'arrange', 'edit', 'export']);
  await open(page);
  const notes = (await rpc<{ review: { notes: number } }>(page, 'project_status')).review.notes;
  await page.locator('.ep').first().click();
  await page.getByTestId('enter-review').click();
  const sheet = page.getByTestId('who-sheet');
  await expect(sheet.or(card(page))).toBeVisible();
  if (await sheet.isVisible()) {
    await expect(page.getByTestId('tour')).toBeHidden();
    await page.getByTestId('who-name').fill('Anna Weber');
    await page.getByTestId('who-role-director').click();
    await page.getByTestId('who-start').click();
  }
  await expect(card(page)).toContainText('You’re reviewing the cut');
  for (let i = 0; i < 4; i++) await next(page);
  await expect(card(page)).toContainText('Write on a line');
  await page.keyboard.press('n');
  await expect(page.getByTestId('note-field')).toBeFocused();
  // what's being tried is lit, and the card keeps off it
  await expect(page.getByTestId('note-draft')).toHaveAttribute('data-tour-lit', '');
  await page.waitForTimeout(350);
  const [draft, over] = [(await page.getByTestId('note-draft').boundingBox())!, (await card(page).boundingBox())!];
  expect(over.x >= draft.x + draft.width || over.x + over.width <= draft.x || over.y >= draft.y + draft.height || over.y + over.height <= draft.y).toBe(true);
  await page.keyboard.type('Only trying');
  await page.keyboard.press('Enter');
  await expect(page.locator('.toast')).toContainText('aren’t kept');
  await expect(page.getByTestId('note-draft')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('note-draft')).toBeHidden();
  await expect(card(page)).toContainText('Write on a line');
  await skip(page);
  expect((await rpc<{ review: { notes: number } }>(page, 'project_status')).review.notes).toBe(notes);
  await page.getByTestId('review-help').click();
  await expect(page.getByTestId('take-tour')).toContainText('Review');
  await page.keyboard.press('?');
  await expect(page.getByTestId('take-tour')).toBeHidden();
});

test('on a narrow window the last stop points at the menu the ? went into', async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await open(page);
  for (let i = 0; i < 7; i++) await next(page);
  await expect(card(page)).toContainText('Back any time');
  await expect(page.getByTestId('header-more')).toHaveAttribute('data-tour-lit', '');
  await skip(page);
});
