import { expect, test, type Page } from '@playwright/test';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, renameSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { deflateSync } from 'node:zlib';

// The welcome screen, against real projects in a temp folder. Runs after the specs that need the
// demo open, and opens it again at the end: every test here starts with nothing open.

const bridge = 'http://127.0.0.1:7358';

async function rpc<T = Record<string, unknown>>(method: string, params: object = {}): Promise<T> {
  const r = await fetch(`${bridge}/rpc`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  const b = (await r.json()) as { ok: boolean; result: T; error?: { message: string } };
  if (!b.ok) throw new Error(`${method}: ${b.error?.message}`);
  return b.result;
}

/** A small two-colour PNG. */
function png(): Buffer {
  const w = 32;
  const h = 18;
  const table = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf: Buffer) => {
    let c = 0xffffffff;
    for (const x of buf) c = table[(c ^ x) & 0xff] ^ (c >>> 8);
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
      [raw[o], raw[o + 1], raw[o + 2]] = x < 20 ? [200, 60, 30] : [30, 70, 190];
    }
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

interface Recent {
  name: string;
  path: string;
  show?: boolean;
}

let demo = '';
let base = '';
let root = '';
let burrito = '';
let pilot = '';
let solo = '';
let noted = '';
let image = '';

test.beforeAll(async () => {
  demo = (await rpc<{ path: string }>('project_status')).path;
  await rpc('close_show');
  base = mkdtempSync(join(tmpdir(), 'cutaway-welcome-'));
  root = join(base, 'TARMAC');
  await rpc('create_show', { parent: base, name: 'TARMAC', open: false });
  pilot = (await rpc<{ file: string }>('create_episode', { name: 'Pilot', show: root, open: false })).file;
  burrito = (await rpc<{ file: string }>('create_episode', { name: 'Time Burrito', show: root, open: false })).file;
  mkdirSync(join(root, 'Episodes/Episode 3'));
  solo = (await rpc<{ file: string }>('create_episode', { name: 'The Lighthouse Keeper', parent: base, open: false })).file;
  // an episode with scenes and an open note
  noted = (await rpc<{ file: string }>('create_demo', { parent: base, name: 'Noted' })).file;
  await rpc('set_reviewer', { name: 'Anna Weber', role: 'director', remember: false });
  await rpc('add_note', { scene: '1', text: 'louder' });
  await rpc('add_note', { scene: '1', text: 'softer' });
  await rpc('close_show');
  image = join(base, 'poster.png');
  writeFileSync(image, png());
});

test.afterEach(async () => {
  await rpc('close_show');
});

test.afterAll(async () => {
  await rpc('open_project', { path: demo });
});

/** The shelf, with these recent items. */
async function shelf(page: Page, recents?: Recent[]) {
  const list = recents ?? [
    { name: 'Time Burrito', path: burrito },
    { name: 'The Lighthouse Keeper', path: solo },
    { name: 'Demo', path: noted },
    { name: 'Pilot', path: pilot },
    { name: 'TARMAC', path: root, show: true },
  ];
  await page.addInitScript((r) => {
    if (!sessionStorage.getItem('seeded')) {
      sessionStorage.setItem('seeded', '1');
      localStorage.setItem('cutaway.recent', JSON.stringify(r));
    }
  }, list);
  await page.goto('/');
  await expect(page.getByTestId('welcome')).toBeVisible();
}

const row = (page: Page, name: string | RegExp) => page.getByRole('listbox', { name });
const tile = (page: Page, name: string | RegExp) => page.getByRole('option', { name });
const hero = (page: Page) => page.getByRole('region', { name: 'Selected' });
const title = (page: Page) => hero(page).getByRole('heading');
const editButton = (page: Page) => hero(page).getByRole('button', { name: /^Edit/ });
const reviewButton = (page: Page) => hero(page).getByRole('button', { name: /^Review/ });
const opened = async () => (await fetch(`${bridge}/rpc`, { method: 'POST', body: JSON.stringify({ method: 'project_status', params: {} }) }).then((r) => r.json())).ok as boolean;

test('the most recent item is selected, and a click only selects', async ({ page }) => {
  await shelf(page);
  await expect(title(page)).toHaveText('Time Burrito');
  await expect(hero(page)).toContainText('TARMAC');
  await tile(page, /^The Lighthouse Keeper/).click();
  await expect(title(page)).toHaveText('The Lighthouse Keeper');
  await expect(hero(page)).toContainText('Single episode');
  await page.waitForTimeout(200);
  await expect(page.getByTestId('welcome')).toBeVisible();
  expect(await opened()).toBe(false);
});

test('hovering highlights and leaves the hero alone', async ({ page }) => {
  await shelf(page);
  await tile(page, /^Pilot/).hover();
  await page.waitForTimeout(100);
  await expect(title(page)).toHaveText('Time Burrito');
});

test('Edit and ↩ open for editing, Review and ⌥↩ read-only, whichever is selected', async ({ page }) => {
  /** Back to the shelf as a closed episode leaves it: a launch would reopen one left open. */
  const leave = async () => {
    await rpc('close_show');
    await page.evaluate(() => localStorage.removeItem('cutaway.last'));
    await page.reload();
    await expect(page.getByTestId('welcome')).toBeVisible();
    await expect(title(page)).toHaveText('Demo');
  };
  const reviewing = page.getByTestId('who-sheet').or(page.getByTestId('review')).first();

  await shelf(page);
  await tile(page, /^Demo/).click();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('current-box')).toBeVisible();
  expect((await rpc<{ review: { active: boolean } }>('project_status')).review.active).toBe(false);

  await leave();
  await page.keyboard.press('Alt+Enter');
  await expect(reviewing).toBeVisible();
  expect((await rpc<{ review: { package: boolean } }>('project_status')).review.package).toBe(true);

  await leave();
  await reviewButton(page).click();
  await expect(reviewing).toBeVisible();

  await leave();
  await editButton(page).click();
  await expect(page.getByTestId('current-box')).toBeVisible();
});

test('the transition says what is opening, and how', async ({ page }) => {
  await shelf(page);
  await tile(page, /^Demo/).click();
  await page.route('**/rpc', async (route) => {
    if ((route.request().postData() ?? '').includes('open_review')) await new Promise((r) => setTimeout(r, 400));
    await route.continue();
  });
  await reviewButton(page).click();
  const o = page.getByTestId('opening');
  await expect(o).toContainText('Review · read-only');
  await expect(o).toContainText('Demo');
  await expect(o).toBeHidden();
});

test('the Review button counts the open notes, on the tile too', async ({ page }) => {
  await shelf(page);
  await expect(reviewButton(page)).not.toContainText('note');
  await tile(page, /^Demo/).click();
  await expect(reviewButton(page)).toContainText('2 notes');
  await expect(tile(page, /^Demo/)).toContainText('2');
});

test('a show takes over the first row, and Esc or the title brings it back', async ({ page }) => {
  await shelf(page);
  await tile(page, /^TARMAC/).click();
  await expect(title(page)).toHaveText('TARMAC');
  await expect(hero(page)).toContainText('3 episodes');
  await expect(page.getByTestId('row-title')).toHaveText('TARMAC');
  const first = row(page, 'TARMAC').getByRole('option');
  await expect(first).toHaveText([/New episode/, /Episode 3/, /Pilot/, /Time Burrito/]);

  // an episode without a project can be edited, not reviewed
  await tile(page, /^Episode 3/).click();
  await expect(hero(page)).toContainText('No project yet');
  await expect(reviewButton(page)).toBeDisabled();
  await expect(editButton(page)).toBeEnabled();

  await page.keyboard.press('Escape');
  await expect(page.getByTestId('row-title')).toHaveText('Recent episodes');
  await expect(title(page)).toHaveText('Time Burrito');

  await tile(page, /^TARMAC/).click();
  await page.getByRole('button', { name: 'Recent episodes' }).click();
  await expect(page.getByTestId('row-title')).toHaveText('Recent episodes');
});

test('a show opens at its home, for editing or for review', async ({ page }) => {
  await shelf(page);
  await tile(page, /^TARMAC/).click();
  await editButton(page).click();
  await expect(page.getByRole('button', { name: 'Close show' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'TARMAC' })).toBeVisible();
  await page.getByRole('button', { name: 'Close show' }).click();

  await expect(page.getByTestId('welcome')).toBeVisible();
  await tile(page, /^TARMAC/).click();
  await reviewButton(page).click();
  await expect(page.getByText('Show, for review')).toBeVisible();
});

test('the single episodes group lists them, and opens nothing itself', async ({ page }) => {
  await shelf(page);
  await tile(page, /^Single episodes/).click();
  await expect(page.getByTestId('row-title')).toHaveText('Single episodes');
  await expect(editButton(page)).toBeDisabled();
  await expect(reviewButton(page)).toBeDisabled();
  await page.keyboard.press('Enter');
  await page.waitForTimeout(150);
  expect(await opened()).toBe(false);
  const list = row(page, 'Single episodes').getByRole('option');
  await expect(list).toHaveText([/New episode/, /The Lighthouse Keeper/, /Demo/]);
  await tile(page, /^The Lighthouse Keeper/).click();
  await expect(editButton(page)).toBeEnabled();
});

test('the arrows walk the rows, and a New tile leaves the hero', async ({ page }) => {
  await shelf(page);
  await page.keyboard.press('ArrowRight');
  await expect(title(page)).toHaveText('The Lighthouse Keeper');
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');
  await expect(tile(page, 'New episode').first()).toHaveAttribute('aria-selected', 'true');
  await expect(title(page)).toHaveText('Time Burrito');
  await page.keyboard.press('ArrowDown');
  await expect(title(page)).toHaveText('TARMAC');
  await page.keyboard.press('ArrowUp');
  await expect(page.getByTestId('row-title')).toHaveText('TARMAC');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('row-title')).toHaveText('Recent episodes');
});

test('⌘N makes an episode in the selected show, selected and closed', async ({ page }) => {
  await shelf(page);
  await page.keyboard.press('ControlOrMeta+n');
  const sheet = page.getByRole('dialog', { name: 'New episode' });
  await expect(sheet.getByRole('radio', { name: 'TARMAC' })).toHaveAttribute('aria-checked', 'true');
  await expect(sheet.getByRole('button', { name: /Create/ })).toBeDisabled();
  await page.keyboard.type('Lunch Break');
  await page.keyboard.press('Enter');
  await expect(sheet).toBeHidden();
  await expect(title(page)).toHaveText('Lunch Break');
  await expect(hero(page)).toContainText('TARMAC');
  expect(await opened()).toBe(false);
  expect(existsSync(join(root, 'Episodes/Lunch Break/Sound/Lunch Break.cutaway'))).toBe(true);
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: /TARMAC.*Lunch Break/ })).toBeVisible();
});

test('No show makes a single episode in the default folder', async ({ page }) => {
  await shelf(page);
  await tile(page, /^The Lighthouse Keeper/).click();
  await page.getByRole('button', { name: /^New episode/ }).first().click();
  const sheet = page.getByRole('dialog', { name: 'New episode' });
  await expect(sheet.getByRole('radio', { name: 'No show' })).toHaveAttribute('aria-checked', 'true');
  await page.keyboard.type('One Off');
  const where = await sheet.getByTestId('sheet-where').textContent();
  expect(where).toMatch(/projects\/One Off$/);
  await sheet.getByRole('button', { name: /Create/ }).click();
  await expect(title(page)).toHaveText('One Off');
  await expect(hero(page)).toContainText('Single episode');
  expect(existsSync(join(where as string, 'One Off.cutaway'))).toBe(true);
  expect(await opened()).toBe(false);
});

test('⇧⌘N makes a show with a cover, selected and closed', async ({ page }) => {
  await shelf(page);
  page.on('dialog', (d) => void d.accept(image));
  await page.keyboard.press('ControlOrMeta+Shift+n');
  const sheet = page.getByRole('dialog', { name: 'New show' });
  await page.keyboard.type('Night Shift');
  await sheet.getByTestId('sheet-cover').click();
  await expect(sheet.getByTestId('sheet-cover')).toHaveAttribute('style', /data:image\/jpeg/);
  const where = (await sheet.getByTestId('sheet-where').textContent()) as string;
  await page.keyboard.press('Escape');
  await expect(sheet).toBeHidden();
  expect(existsSync(where)).toBe(false);

  await page.keyboard.press('ControlOrMeta+Shift+n');
  await page.keyboard.type('Night Shift');
  await sheet.getByTestId('sheet-cover').click();
  await sheet.getByRole('button', { name: /Create/ }).click();
  await expect(title(page)).toHaveText('Night Shift');
  await expect(hero(page)).toContainText('Show');
  expect(existsSync(join(where, 'Cover.jpg'))).toBe(true);
  expect(await opened()).toBe(false);
  await expect(tile(page, /^Night Shift/).locator('.cov')).toHaveAttribute('style', /data:image\/jpeg/);
});

test('⌘O puts what it opens on the shelf and selects it', async ({ page }) => {
  await shelf(page, [{ name: 'Pilot', path: pilot }]);
  page.on('dialog', (d) => void d.accept(solo));
  await page.keyboard.press('ControlOrMeta+o');
  await expect(title(page)).toHaveText('The Lighthouse Keeper');
  expect(await opened()).toBe(false);
});

test('without a cover there is a wash; dropping an image sets one on that item', async ({ page }) => {
  const own = (await rpc<{ file: string }>('create_episode', { name: 'Covered', parent: base, open: false })).file;
  await shelf(page, [{ name: 'Covered', path: own }, { name: 'Pilot', path: pilot }]);
  const t = tile(page, /^Covered/);
  await expect(t.locator('.cov')).toHaveAttribute('style', /gradient/);
  const box = (await t.boundingBox())!;
  const at = [box.x + 30, box.y + 30];
  await page.evaluate(([p, x, y]) => {
    const w = window as unknown as { cutawayFileDrag: { enter(p: string[]): void; over(x: number, y: number): void } };
    w.cutawayFileDrag.enter([p as string]);
    w.cutawayFileDrag.over(x as number, y as number);
  }, [image, ...at]);
  await expect(t).toHaveClass(/drop/);
  await page.evaluate(([p, x, y]) => (window as unknown as { cutawayFileDrag: { drop(p: string[], x: number, y: number): Promise<void> } }).cutawayFileDrag.drop([p as string], x as number, y as number), [image, ...at]);
  await expect(t.locator('.cov')).toHaveAttribute('style', /data:image\/jpeg/);
  expect(existsSync(join(dirname(own), 'Cover.jpg'))).toBe(true);
  expect(await opened()).toBe(false);
});

test('a show’s cover shows on its episodes until they have their own', async ({ page }) => {
  const r = join(base, 'Covered show');
  await rpc('create_show', { root: r, open: false });
  const f = (await rpc<{ file: string }>('create_episode', { name: 'One', show: r, open: false })).file;
  await rpc('set_cover', { target: r, image });
  await shelf(page, [{ name: 'One', path: f }]);
  await expect(tile(page, /^One/).locator('.cov')).toHaveAttribute('style', /data:image\/jpeg/);
  await expect(hero(page).locator('.cov').last()).toHaveAttribute('style', /data:image\/jpeg/);
});

test('a dropped script starts an episode, named after the file', async ({ page }) => {
  await shelf(page, [{ name: 'Pilot', path: pilot }]);
  const script = join(base, 'Cold_Open.fountain');
  writeFileSync(script, 'INT. HANGAR - NIGHT\n\nPAUL\nHello there.\n');
  await page.evaluate((p) => (window as unknown as { cutawayFileDrag: { drop(p: string[], x: number, y: number): Promise<void> } }).cutawayFileDrag.drop([p], 700, 300), script);
  const sheet = page.getByRole('dialog', { name: 'New episode' });
  await expect(sheet.getByRole('textbox')).toHaveValue('Cold Open');
  await expect(sheet).toContainText('Starts with 1 file');
  await sheet.getByRole('radio', { name: 'No show' }).click();
  await sheet.getByRole('button', { name: /Create/ }).click();
  await expect(title(page)).toHaveText('Cold Open');
  expect(await opened()).toBe(false);
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: /Cold Open/ }).first()).toBeVisible();
  expect((await rpc<{ scenes: number }>('project_status')).scenes).toBe(1);
});

test('a dropped review goes to its episode, and the count follows', async ({ page }) => {
  // a note left in a package reaches the shared folder as a review file; it's taken away and dropped
  const made = (await rpc<{ file: string }>('create_demo', { parent: base, name: 'Reviewed' })).file;
  await rpc('close_show');
  await rpc('open_review', { path: made });
  await rpc('set_reviewer', { name: 'Albert', role: 'producer', remember: false });
  await rpc('add_note', { scene: '1', text: 'more air' });
  await rpc('close_show');
  const rolling = join(dirname(made), 'Reviews/albert.cutawayreview');
  const sent = join(base, 'albert.cutawayreview');
  renameSync(rolling, sent);

  await shelf(page, [{ name: 'Reviewed', path: made }]);
  await expect(reviewButton(page)).not.toContainText('note');
  await page.evaluate((p) => (window as unknown as { cutawayFileDrag: { drop(p: string[], x: number, y: number): Promise<void> } }).cutawayFileDrag.drop([p], 700, 300), sent);
  await expect(page.getByRole('status')).toContainText('Albert: 1 new note');
  await expect(reviewButton(page)).toContainText('1 note');
  expect(await opened()).toBe(false);
  expect(existsSync(sent)).toBe(false);
});

test('a missing item is dimmed, offers Locate… and relinks', async ({ page }) => {
  const f = (await rpc<{ file: string }>('create_episode', { name: 'Moved', parent: base, open: false })).file;
  const moved = join(base, 'Moved elsewhere.cutaway');
  renameSync(f, moved);
  await shelf(page, [{ name: 'Moved', path: f }, { name: 'Pilot', path: pilot }]);
  await expect(tile(page, /^Moved/)).toHaveClass(/gone/);
  await expect(hero(page).getByRole('button', { name: /^Locate/ })).toBeVisible();
  await expect(editButton(page)).toHaveCount(0);
  await expect(reviewButton(page)).toHaveCount(0);

  page.on('dialog', (d) => void d.accept(moved));
  await hero(page).getByRole('button', { name: /^Locate/ }).click();
  await expect(editButton(page)).toBeVisible();
  await expect(tile(page, /^Moved/)).not.toHaveClass(/gone/);
  const list = await page.evaluate(() => JSON.parse(localStorage.getItem('cutaway.recent') ?? '[]'));
  expect(list[0].path).toBe(moved);
  expect(list).toHaveLength(2);
});

test('Remove from recents forgets it and deletes nothing', async ({ page }) => {
  await shelf(page);
  await tile(page, /^Demo/).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Remove from recents' }).click();
  await expect(tile(page, /^Demo/)).toHaveCount(0);
  expect(existsSync(noted)).toBe(true);
  // a show goes with its episodes, or they'd bring it back
  await tile(page, /^TARMAC/).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Remove from recents' }).click();
  await expect(tile(page, /^TARMAC/)).toHaveCount(0);
  await expect(tile(page, /^Time Burrito/)).toHaveCount(0);
  expect(existsSync(root)).toBe(true);
});

test('Set cover… from the right click', async ({ page }) => {
  const f = (await rpc<{ file: string }>('create_episode', { name: 'By menu', parent: base, open: false })).file;
  await shelf(page, [{ name: 'By menu', path: f }]);
  page.on('dialog', (d) => void d.accept(image));
  await tile(page, /^By menu/).click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Set cover…' }).click();
  await expect(tile(page, /^By menu/).locator('.cov')).toHaveAttribute('style', /data:image\/jpeg/);
});

test('an empty shelf offers the demo session and nothing to open', async ({ page }) => {
  await shelf(page, []);
  await expect(hero(page).getByRole('button', { name: 'Demo session' })).toBeVisible();
  await expect(editButton(page)).toHaveCount(0);
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog', { name: 'New episode' })).toBeVisible();
});

test('a file that went between the look and the click says so and stays here', async ({ page }) => {
  const f = (await rpc<{ file: string }>('create_episode', { name: 'Fleeting', parent: base, open: false })).file;
  await shelf(page, [{ name: 'Fleeting', path: f }]);
  renameSync(f, `${f}.gone`);
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('opening')).toBeHidden();
  await expect(tile(page, /^Fleeting/)).toHaveClass(/gone/);
  copyFileSync(`${f}.gone`, f);
});
