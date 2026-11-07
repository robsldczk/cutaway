import { expect, test, type Page } from '@playwright/test';

// Review mode on the demo session. One session serves every test, so each leaves review the way
// it came in, and what's written stays: notes are looked for by what they say.

/** What these tests read of the core's answers. */
interface Line {
  kind: string;
  line_id: string;
  take_id: string;
  takes: { id: string }[];
}
interface Answer {
  id: string;
  rev: number;
  path: string;
  file: string;
  notes: number;
  items: Line[];
  review: { active: boolean };
  scenes: { number: number; lock: unknown; status: unknown }[];
}

const rpc = async <T = Answer>(page: Page, method: string, params: object = {}): Promise<T> => {
  const r = await page.request.post('http://127.0.0.1:7358/rpc', { data: { method, params } });
  const b = await r.json();
  if (!b.ok) throw new Error(b.error.message);
  return b.result;
};

// The session is shared with every other spec: what's locked, swapped or marked here goes back.
test.afterEach(async ({ page }) => {
  if ((await rpc(page, 'project_status')).review.active) await rpc(page, 'leave_review');
  const { scenes } = await rpc(page, 'export_status');
  for (const s of scenes) {
    if (s.lock) await rpc(page, 'unlock_scene', { scene: String(s.number), confirm: true });
    if (s.status) await rpc(page, 'set_scene_status', { scene: String(s.number) });
  }
});

async function open(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('header')).toBeVisible();
  // the header comes first: a page key does nothing until the scene is there
  await expect(page.getByTestId('current-box')).toBeVisible();
}

async function sayWho(page: Page, name = 'Anna Weber') {
  const sheet = page.getByTestId('who-sheet');
  if (await sheet.isVisible().catch(() => false)) {
    await page.getByTestId('who-name').fill(name);
    await page.getByTestId('who-role-director').click();
    await page.getByTestId('who-start').click();
    await expect(sheet).toBeHidden();
  }
}

/** A click on the spine at a moment of the scene. */
async function clickSpine(page: Page, fraction: number) {
  const box = (await page.getByTestId('review-spine').boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height * fraction);
}

async function leave(page: Page) {
  await page.getByTestId('review-done').click();
  await expect(page.getByTestId('review')).toBeHidden();
}

test('Export opens the scenes for review, asks who you are once, and takes a note', async ({ page }) => {
  await open(page);
  await page.keyboard.press('4');
  await expect(page.getByTestId('open-for-review')).toBeEnabled();
  await page.getByTestId('open-for-review').click();

  // who's picking the scenes: asked once, and the round carries it
  const sheet = page.getByTestId('who-sheet');
  await expect(sheet).toBeVisible();
  await expect(page.getByTestId('who-start')).toBeDisabled();
  await page.getByTestId('who-name').fill('Anna Weber');
  await page.getByTestId('who-role-director').click();
  await page.getByTestId('who-start').click();
  await expect(sheet).toBeHidden();
  await expect(page.getByTestId('review')).toBeVisible();
  await expect(page.getByTestId('reviewer-chip')).toContainText('Anna Weber');

  // the chip switches, and the sheet says what's in the round
  await page.getByTestId('reviewer-chip').click();
  await expect(sheet).toContainText('Scenes in this review');
  await expect(sheet).toContainText('picked by Anna Weber');
  await page.keyboard.press('Escape');
  await expect(sheet).toBeHidden();
  await expect(page.getByTestId('reviewer-chip')).toContainText('Director');
  await expect(page.getByTestId('round-picker')).toContainText('Round');

  // a click on the wave is a marker, with the words it's on
  await clickSpine(page, 0.3);
  const draft = page.getByTestId('note-draft');
  await expect(draft).toBeVisible();
  await page.getByTestId('note-field').fill('Can we make this bigger?');
  await page.getByTestId('draft-tag-performance').click();
  await page.getByTestId('draft-tag-timing').click();
  await page.keyboard.press('Enter');
  await expect(draft).toBeHidden();
  const note = page.getByTestId('note').filter({ hasText: 'Can we make this bigger?' });
  await expect(note).toBeVisible();
  await expect(note.getByTestId('note-tag')).toHaveText('Timing');
  await expect(note).toContainText('Anna Weber');
  await expect(page.getByTestId('note-pin').first()).toBeAttached();
  await expect(page.getByTestId('note-dot').first()).toBeAttached();
  await expect(note.getByTestId('note-time')).not.toContainText(' to ');

  // it's on the rail and in the scene's count
  await expect(page.getByTestId('notes-head')).toContainText('open');

  await page.screenshot({ path: 'test-results/review-note.png' });
  await leave(page);
});

test('a drag along the wave is a range, with a bracket beside the spine', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  await sayWho(page);
  const box = (await page.getByTestId('review-spine').boundingBox())!;
  const x = box.x + box.width / 2;
  await page.mouse.move(x, box.y + box.height * 0.05);
  await page.mouse.down();
  await page.mouse.move(x, box.y + box.height * 0.3, { steps: 6 });
  await page.mouse.up();
  await expect(page.getByTestId('note-draft')).toContainText(' to ');
  await page.getByTestId('note-field').fill('Range note');
  await page.getByTestId('note-save').click();
  await expect(page.getByTestId('note').filter({ hasText: 'Range note' })).toBeVisible();
  await expect(page.getByTestId('note-bracket').first()).toBeAttached();
  await leave(page);
});

test('review mode has no way to change the cut, and the bus says why', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  await sayWho(page);

  // nothing on the screen edits: no favorites, no box editor, no scene marks
  await expect(page.getByLabel('Favorite take (F)')).toHaveCount(0);
  await expect(page.getByLabel('Open box editor (E)')).toHaveCount(0);
  await page.getByTestId('rail-scene').first().click({ button: 'right' });
  await expect(page.getByTestId('status-menu')).toHaveCount(0);

  // keys that edit elsewhere do nothing here
  const rev = (await rpc(page, 'project_status')).rev;
  for (const k of ['f', 'p', ',', 'i', 'o']) await page.keyboard.press(k);
  await page.waitForTimeout(150);
  expect((await rpc(page, 'project_status')).rev).toBe(rev);

  // and the core refuses, whoever asks
  const v = await rpc(page, 'scene_view', { scene: '1' });
  const line = v.items.find((i) => i.kind === 'line')!.line_id;
  await expect(rpc(page, 'set_gap', { line_id: line, gap_s: 2 })).rejects.toThrow(/only takes notes/);
  await expect(rpc(page, 'command', { cmd: 'set_gap', line_id: line, gap_s: 2 })).rejects.toThrow(/would change the cut/);
  await leave(page);
  // back on the editing pages it's editing again
  await expect(page.getByTestId('header')).toBeVisible();
  expect((await rpc(page, 'project_status')).review.active).toBe(false);
});

test('resolving on an editing page shows as done in review, and reopening shows as open again', async ({ page }) => {
  await open(page);
  const n = await rpc(page, 'add_note', { scene: '1', at_s: 1.1, text: 'Resolve me', tag: 'sound', author: { name: 'Jonas Brandt', role: 'producer' } });
  await open(page);

  // the line you're on shows it too, not only the lines around it
  await expect(page.getByTestId('current-box').getByTestId('note-badge')).toBeVisible();

  // on the editing pages it's a marker and a line in the Notes tab
  await page.keyboard.press('2');
  await expect(page.getByTestId('note-badge').first()).toBeVisible();
  await page.getByTestId('note-badge').first().click();
  await expect(page.getByTestId('notes-panel')).toBeVisible();
  const card = page.getByTestId('notes-panel').getByTestId('note').filter({ hasText: 'Resolve me' });
  await expect(card).toBeVisible();
  await expect(card.getByTestId('note-bucket')).toBeVisible();

  // a reply and a resolve from the editing side: who's asked the first time
  await card.getByTestId('note-resolve').click();
  await sayWho(page, 'Rob');
  await page.getByTestId('notes-all').click();
  await expect(card).toHaveAttribute('data-status', 'resolved');
  await expect(card.getByTestId('note-done')).toContainText('Done by');

  const r = await rpc(page, 'list_notes', { scene: '1', status: 'resolved' });
  expect(JSON.stringify(r)).toContain('Resolve me');

  // the reviewer sees it done; reopening there is open here
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  const there = page.getByTestId('note').filter({ hasText: 'Resolve me' });
  await expect(there).toHaveAttribute('data-status', 'resolved');
  await there.getByTestId('note-resolve').click();
  await expect(there).toHaveAttribute('data-status', 'reopened');
  await expect(there.getByTestId('note-reopened')).toBeVisible();
  await leave(page);
  await expect(page.getByTestId('note-badge').first()).toBeVisible();
  void n;
});

test('search finds scenes, lines and notes and goes to them', async ({ page }) => {
  await open(page);
  await rpc(page, 'add_note', { scene: '2', text: 'Sandwich crinkle please', tag: 'sound', author: { name: 'Jonas Brandt', role: 'producer' } });
  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  await sayWho(page);

  await page.keyboard.press('Meta+k');
  await expect(page.getByTestId('review-search')).toBeFocused();
  await page.keyboard.type('sandwich');
  const results = page.getByTestId('search-results');
  await expect(results.getByTestId('search-line')).toBeVisible();
  await expect(results.getByTestId('search-note')).toContainText('Sandwich crinkle');
  await expect(results.getByText('Lines')).toBeVisible();
  await expect(results.getByText('Notes')).toBeVisible();
  await page.screenshot({ path: 'test-results/review-search.png' });
  await results.getByTestId('search-note').click();
  // it's on another scene: the jump goes there
  await expect(page.getByTestId('note').filter({ hasText: 'Sandwich crinkle' })).toBeVisible();
  await leave(page);
});

test('filters narrow the notes, and an undo takes the last one back', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  await sayWho(page);

  await page.keyboard.press('n');
  await page.getByTestId('note-field').fill('Undo me');
  await page.keyboard.press('Enter');
  const mine = page.getByTestId('note').filter({ hasText: 'Undo me' });
  await expect(mine).toBeVisible();

  await page.getByTestId('filter-kind').selectOption('music');
  await expect(mine).toBeHidden();
  await page.getByTestId('filter-kind').selectOption('');
  await page.getByTestId('filter-mine').click();
  await expect(mine).toBeVisible();
  await page.getByTestId('filter-all').click();

  await page.keyboard.press('Meta+z');
  await expect(mine).toBeHidden();
  await page.keyboard.press('Meta+Shift+z');
  await expect(mine).toBeVisible();
  await leave(page);
});

test('every arrival shows in the history, and a second add changes nothing', async ({ page }) => {
  await open(page);
  // what Add review would be given: a file saved from this episode
  const status = await rpc(page, 'project_status');
  const folder = status.path.replace(/[^/]+$/, '');
  await rpc(page, 'set_reviewer', { name: 'Albert', role: 'producer', remember: false });
  await rpc(page, 'add_note', { scene: '1', text: 'From Albert' });
  const sent = await rpc(page, 'send_review', { path: `${folder}albert-send.cutawayreview` });
  expect(sent.notes).toBeGreaterThan(0);

  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('open-review-history').click();
  await expect(page.getByTestId('history-round').first()).toBeVisible();
  await page.getByTestId('add-review-path').fill(sent.file);
  await page.getByTestId('add-review').click();
  await expect(page.getByTestId('add-review-done')).toContainText('Albert');
  await expect(page.getByTestId('add-review-done')).toContainText('0 new notes');
  await page.screenshot({ path: 'test-results/review-history.png' });
  await page.keyboard.press('Escape');
});

test('the Export page says when a locked scene has open notes', async ({ page }) => {
  await open(page);
  await rpc(page, 'add_note', { scene: '2', text: 'Locked note', author: { name: 'Jonas Brandt', role: 'producer' } });
  await rpc(page, 'lock_scene', { scene: '2' });
  await open(page);
  await page.keyboard.press('4');
  await expect(page.getByTestId('export-warning').first()).toContainText('is locked with');
  await rpc(page, 'unlock_scene', { scene: '2', confirm: true });
});

test('a note says when its element changed, and when it has left the cut', async ({ page }) => {
  await open(page);
  const v = await rpc(page, 'scene_view', { scene: '1' });
  const line = v.items.find((i) => i.kind === 'line' && i.takes.length > 1)!;
  const other = line.takes.find((t) => t.id !== line.take_id)!;
  const changed = await rpc(page, 'add_note', { line_id: line.line_id, word: 0, text: 'Try the other take', tag: 'performance', author: { name: 'Jonas Brandt', role: 'producer' } });
  await rpc(page, 'select_take', { line_id: line.line_id, take_id: other.id });
  const last = v.items.filter((i) => i.kind === 'line').pop()!;
  const gone = await rpc(page, 'add_note', { line_id: last.line_id, text: 'Lose this line', author: { name: 'Jonas Brandt', role: 'producer' } });
  await rpc(page, 'delete_line', { line_id: last.line_id, confirm: true });
  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  const c = page.locator(`[data-note="${changed.id}"]`);
  await expect(c.getByTestId('note-changed')).toHaveText('Changed since note');
  const d = page.locator(`[data-note="${gone.id}"]`);
  await expect(d.getByTestId('note-detached')).toBeVisible();
  await expect(d).toContainText('Lunch.');
  await leave(page);
  // the line comes back and the take goes back, as the other specs expect them
  await rpc(page, 'undo');
  await rpc(page, 'select_take', { line_id: line.line_id, take_id: line.take_id });
});

test('earlier rounds stay in the picker, and Send review saves one file', async ({ page }) => {
  await open(page);
  await rpc(page, 'start_review_round', { scenes: ['1', '2'] });
  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  await expect(page.getByTestId('round-picker')).toContainText('Round 2');
  await page.getByTestId('round-picker').click();
  await expect(page.getByTestId('round-item')).toHaveCount(2);
  await expect(page.getByTestId('round-item').first()).toContainText('current');
  await page.getByTestId('round-item').nth(1).click();
  await expect(page.getByTestId('round-picker')).toContainText('earlier');
  await expect(page.getByTestId('rail-scene').first()).toBeVisible();

  const status = await rpc(page, 'project_status');
  const out = status.path.replace(/[^/]+$/, 'sent-from-review.cutawayreview');
  page.once('dialog', (d) => d.accept(out));
  await page.getByTestId('send-review').click();
  await expect(page.getByRole('status').filter({ hasText: 'sent-from-review' })).toBeVisible();
  await leave(page);
});

test('exporting can open the same scenes for review in one go', async ({ page }) => {
  await open(page);
  await page.keyboard.press('4');
  await page.getByTestId('also-review').check();
  await expect(page.getByTestId('export-go')).toBeEnabled();
  await page.getByTestId('export-go').click();
  await expect(page.getByTestId('review')).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId('round-picker')).toContainText('cut v0');
  await leave(page);
});

const anna = { name: 'Anna Weber', role: 'director' };

/** Into review as Anna, on a fresh load. */
async function intoReview(page: Page) {
  await rpc(page, 'set_reviewer', { ...anna, remember: true });
  await open(page);
  await page.getByRole('button', { name: /Demo/ }).first().click();
  await page.getByTestId('enter-review').click();
  await expect(page.getByTestId('review')).toBeVisible();
  await sayWho(page);
}

test('a note can be removed, and ⌘Z brings it back', async ({ page }) => {
  await open(page);
  const n = await rpc(page, 'add_note', { scene: '1', at_s: 1.1, text: 'Take me out', author: anna });
  await intoReview(page);
  const card = page.locator(`[data-note="${n.id}"]`);
  await expect(card).toBeVisible();
  await card.getByTestId('note-remove').click();
  await expect(card).toHaveCount(0);
  await expect(page.getByRole('status').filter({ hasText: 'Note removed' })).toBeVisible();
  await page.keyboard.press('Meta+z');
  await expect(card).toBeVisible();
  await card.getByTestId('note-remove').click();
  await expect(card).toHaveCount(0);
  await leave(page);
});

test('someone else’s note can be answered, not moved or removed', async ({ page }) => {
  await open(page);
  const n = await rpc(page, 'add_note', { scene: '1', at_s: 1.1, text: 'Not yours', author: { name: 'Jonas Brandt', role: 'producer' } });
  await intoReview(page);
  const card = page.locator(`[data-note="${n.id}"]`);
  await expect(card).toBeVisible();
  await expect(card.getByTestId('note-remove')).toHaveCount(0);
  await expect(card.getByTestId('note-reply-open')).toBeVisible();
  await expect(rpc(page, 'remove_note', { note: n.id, confirm: true })).rejects.toThrow(/only they or the editor/);
  await rpc(page, 'remove_note', { note: n.id, confirm: true, author: { name: 'Rob', role: 'editor' } });
  await leave(page);
});

test('a note drags by its dot to what is said there, in one undo step', async ({ page }) => {
  await open(page);
  const v = await rpc(page, 'scene_view', { scene: '1' });
  const lines = v.items.filter((i) => i.kind === 'line');
  const n = await rpc(page, 'add_note', { line_id: lines[0].line_id, word: 0, text: 'Move me', author: anna });
  await intoReview(page);
  const dot = page.locator(`[data-testid="note-dot"][data-spot="${n.id}"]`);
  const at = (await dot.boundingBox())!;
  const to = (await page.locator(`[data-line="${lines[1].line_id}"]`).boundingBox())!;
  await page.mouse.move(at.x + 6, at.y + 6);
  await page.mouse.down();
  await page.mouse.move(at.x + 6, to.y + to.height / 2, { steps: 8 });
  await expect(page.locator(`[data-note="${n.id}"]`).getByTestId('note-time')).not.toHaveText(/ to /);
  await page.mouse.up();
  const quote = page.locator(`[data-note="${n.id}"]`).locator('.quote');
  await expect(quote).toContainText('Sara');
  const moved = await rpc<{ threads: { notes: { id: string; line_id: string }[] }[] }>(page, 'list_notes', { scene: '1' });
  expect(moved.threads.flatMap((t) => t.notes).find((x) => x.id === n.id)!.line_id).toBe(lines[1].line_id);
  await page.keyboard.press('Meta+z');
  await expect(quote).toContainText('Paul');
  await leave(page);
});

test('a note drags by its card, and Esc puts it back', async ({ page, browserName }) => {
  test.fail(browserName === 'webkit', 'BUG: in WebKit, a note dragged by its card lands on another line. Not yet checked in the Tauri app: may be Playwright mouse input, not the app.');
  await open(page);
  const v = await rpc(page, 'scene_view', { scene: '1' });
  const lines = v.items.filter((i) => i.kind === 'line');
  const n = await rpc(page, 'add_note', { line_id: lines[0].line_id, text: 'Whole line', author: anna });
  await intoReview(page);
  const top = page.locator(`[data-note="${n.id}"]`).getByTestId('note-top');
  const b = (await top.boundingBox())!;
  const rev = (await rpc(page, 'project_status')).rev;
  await page.mouse.move(b.x + 60, b.y + 8);
  await page.mouse.down();
  await page.mouse.move(b.x + 60, b.y + 60, { steps: 5 });
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await page.waitForTimeout(150);
  expect((await rpc(page, 'project_status')).rev).toBe(rev);

  // a whole line lands on the line there, and stays a whole line
  const to = (await page.locator(`[data-line="${lines[2].line_id}"]`).boundingBox())!;
  const c = (await top.boundingBox())!;
  const spot = (await page.locator(`[data-testid="note-bracket"][data-spot="${n.id}"]`).boundingBox())!;
  await page.mouse.move(c.x + 60, c.y + 8);
  await page.mouse.down();
  await page.mouse.move(c.x + 60, c.y + 8 + (to.y + to.height / 2 - (spot.y + spot.height / 2)), { steps: 8 });
  await page.mouse.up();
  await expect.poll(async () => {
    const all = await rpc<{ threads: { notes: { id: string; on: string; line_id: string }[] }[] }>(page, 'list_notes', { scene: '1' });
    const x = all.threads.flatMap((t) => t.notes).find((y) => y.id === n.id)!;
    return `${x.on} ${x.line_id}`;
  }).toBe(`line ${lines[2].line_id}`);
  await page.keyboard.press('Meta+z');
  await leave(page);
});

test('the wheel glides through the scene, a little at a time, and back', async ({ page }) => {
  await intoReview(page);
  const time = async () => {
    const [m, s] = (await page.getByTestId('time-now').textContent())!.split(':');
    return +m * 60 + +s;
  };
  const col = page.locator('.col');
  const y = () => col.evaluate((el) => new DOMMatrix(getComputedStyle(el).transform).m42);
  await page.mouse.move(700, 450);
  const t0 = await time();
  const y0 = await y();

  // a small scroll is a small move, not a jump to the next line
  await page.mouse.wheel(0, 12);
  await expect.poll(y).toBeCloseTo(y0 - 12, 0);
  expect(await time()).toBeLessThan(t0 + 1);
  expect(await time()).toBeGreaterThan(t0);

  // a notch doesn't land at once: it's on its way in between
  const seen: number[] = [];
  await page.mouse.wheel(0, 100);
  for (let i = 0; i < 6; i++) {
    seen.push(await y());
    await page.waitForTimeout(16);
  }
  await expect.poll(y).toBeCloseTo(y0 - 112, 0);
  expect(new Set(seen.map(Math.round)).size).toBeGreaterThan(2);

  // slowly back up over the gaps between cards, where no time passes: it doesn't catch
  for (let i = 0; i < 40; i++) await page.mouse.wheel(0, -3);
  await expect.poll(y).toBeCloseTo(y0 + 8, 0);

  // ⇧ goes faster, and the ends hold
  await page.keyboard.down('Shift');
  await page.mouse.wheel(0, 40);
  await page.keyboard.up('Shift');
  await expect.poll(y).toBeLessThan(y0 - 100);
  for (let i = 0; i < 6; i++) await page.mouse.wheel(0, 2000);
  await expect.poll(time).toBeGreaterThan(3);
  const total = await page.locator('.times span').last().textContent();
  await expect(page.getByTestId('time-now')).toHaveText(total!);
  await leave(page);
});

for (const [w, h] of [[900, 600], [1024, 700], [1280, 760]]) {
  test(`${w}×${h}: the review header stays in one row and the notes clear the rail`, async ({ page }) => {
    await page.setViewportSize({ width: w, height: h });
    await open(page);
    await page.getByRole('button', { name: /Demo/ }).first().click();
    await page.getByTestId('enter-review').click();
    await expect(page.getByTestId('review')).toBeVisible();
    await sayWho(page);
    await page.screenshot({ path: `test-results/review-${w}.png` });
    const done = (await page.getByTestId('review-done').boundingBox())!;
    expect(done.x + done.width).toBeLessThanOrEqual(w);
    expect(done.y).toBeLessThan(30);
    const rail = (await page.getByRole('navigation', { name: 'Scenes' }).boundingBox())!;
    expect(rail.x + rail.width).toBeLessThanOrEqual(w);
    for (const n of await page.getByTestId('note-thread').all()) {
      const b = (await n.boundingBox())!;
      expect(b.x + b.width).toBeLessThanOrEqual(rail.x);
    }
    await leave(page);
  });
}
