import { expect, test, type Page } from '@playwright/test';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Coming back to where it was left. A reload is a relaunch for the page; closing the episode on
// the bridge as well makes it a launch with nothing open, which is what the app really starts with.

const bridge = 'http://127.0.0.1:7358/rpc';

async function rpc<T = unknown>(page: Page, method: string, params: object = {}): Promise<T> {
  const r = await page.request.post(bridge, { data: { method, params } });
  const body = await r.json();
  if (!body.ok) throw new Error(body.error.message);
  return body.result;
}

const scene = (page: Page, n: number) => page.getByRole('button', { name: new RegExp(`^Scene ${n}\\b.*, current`) });
const stored = (page: Page, key: string) => page.evaluate((k) => localStorage.getItem(k) ?? '', key);

/** Scene 2, on its second line, on the Arrange page. */
async function leave(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByTestId('rail-scene').nth(1).click();
  await expect(scene(page, 2)).toBeVisible();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('2');
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
  const time = await page.getByTestId('time-now').textContent();
  // written while working, not on the way out: a quit doesn't wait for the page
  await expect.poll(() => stored(page, 'cutaway.spots')).toContain('"page":"arrange"');
  return time as string;
}

async function expectLeft(page: Page, time: string) {
  await expect(page.getByRole('navigation', { name: 'Pages' }).getByRole('button', { name: /Arrange/ })).toHaveClass(/active/);
  await expect(scene(page, 2)).toBeVisible();
  await expect(page.getByTestId('time-now')).toHaveText(time);
}

test('a reload lands on the page, scene and playhead it was left on', async ({ page }) => {
  const time = await leave(page);
  await page.reload();
  await expectLeft(page, time);
});

test('a launch with nothing open reopens the episode, at its spot', async ({ page }) => {
  const time = await leave(page);
  await rpc(page, 'close_project');
  await page.reload();
  await expectLeft(page, time);
  expect((await rpc<{ path: string }>(page, 'project_status')).path).toBeTruthy();
});

test('an episode closed on purpose stays closed, and keeps its spot for later', async ({ page }) => {
  const time = await leave(page);
  const { path } = await rpc<{ path: string }>(page, 'project_status');
  await page.getByRole('button', { name: /^Demo/ }).click();
  await page.getByRole('menuitem', { name: 'Back to start screen' }).click();
  await expect(page.getByTestId('rail-scene')).toHaveCount(0);
  expect(await stored(page, 'cutaway.last')).toBe('');

  await page.reload();
  await expect(page.getByRole('button', { name: 'Open', exact: false }).first()).toBeVisible();
  await expect(page.getByTestId('rail-scene')).toHaveCount(0);

  await rpc(page, 'open_project', { path });
  await page.reload();
  await expectLeft(page, time);
});

test('the Edit page comes back at its zoom, with the playhead in sight', async ({ page }) => {
  await leave(page);
  const { path } = await rpc<{ path: string }>(page, 'project_status');
  const [[, spot]] = JSON.parse(await stored(page, 'cutaway.spots'));
  await page.evaluate(([p, s]) => localStorage.setItem('cutaway.spots', JSON.stringify([[p, s]])), [path, { ...spot, page: 'edit', at: 6, zoom: 400 }]);
  await page.reload();
  const timeline = page.locator('.scroll').first();
  await expect(timeline).toBeVisible();
  await expect.poll(() => timeline.evaluate((e) => e.scrollLeft)).toBeGreaterThan(1000);
  const left = await timeline.evaluate((e) => e.scrollLeft);
  const width = await timeline.evaluate((e) => e.clientWidth);
  expect(6 * 400).toBeGreaterThan(left);
  expect(6 * 400).toBeLessThan(left + width);
  await expect.poll(() => stored(page, 'cutaway.spots')).toContain('"zoom":400');
});

test('what has gone since is left out, and says so', async ({ page }) => {
  await leave(page);
  const { path } = await rpc<{ path: string }>(page, 'project_status');

  // the scene and line of the spot no longer exist: the page stays, the scene falls back
  await page.evaluate((p) => {
    localStorage.setItem('cutaway.spots', JSON.stringify([[p, { page: 'arrange', tab: 'files', scene: 'gone', line: 'gone', at: 999, zoom: 0 }]]));
  }, path);
  await page.reload();
  await expect(scene(page, 1)).toBeVisible();
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();

  // a file that's gone: the start screen, and a word why
  await rpc(page, 'close_project');
  await page.evaluate(() => localStorage.setItem('cutaway.last', JSON.stringify({ name: 'Lost episode', path: '/nowhere/lost.cutaway' })));
  await page.reload();
  await expect(page.getByRole('status')).toContainText("Lost episode couldn't be reopened");
  await expect(page.getByTestId('rail-scene')).toHaveCount(0);
  await rpc(page, 'open_project', { path });
});

test('a show comes back too: its episode with the show around it, or its home', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  const demo = (await rpc<{ path: string }>(page, 'project_status')).path;
  const root = join(mkdtempSync(join(tmpdir(), 'cutaway-resume-')), 'tm-2610');
  mkdirSync(join(root, 'Episodes/Episode 2/Sound'), { recursive: true });
  writeFileSync(join(root, 'Episodes/Episode 2/episode.jetplane.json'), '{"schemaVersion":1,"kind":"episode","name":"Turkey Time","episodeNumber":2}');
  writeFileSync(join(root, 'library.jetplane.json'), '{"kind":"library"}');
  await rpc(page, 'close_project');
  await rpc(page, 'create_show', { root });
  await rpc(page, 'migrate_episode', { source: demo, episode: 'Episode 2' });
  await rpc(page, 'open_episode', { episode: 'Episode 2' });

  await page.reload();
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByTestId('rail-scene').nth(1).click();
  await expect(scene(page, 2)).toBeVisible();
  await page.keyboard.press('2');
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
  await expect.poll(() => stored(page, 'cutaway.spots')).toContain('"page":"arrange"');
  const time = (await page.getByTestId('time-now').textContent()) as string;

  // a launch: nothing is open, not even the show
  await rpc(page, 'close_show');
  await page.reload();
  await expectLeft(page, time);
  expect((await rpc<{ show: { root: string } | null }>(page, 'project_status')).show?.root).toBe(root);

  // left on the show's home, it comes back there
  await page.getByRole('button', { name: /Turkey Time|Episode 2/ }).first().click();
  await page.getByRole('menuitem', { name: 'Show home' }).click();
  await expect(page.getByTestId('show-export')).toBeVisible();
  await rpc(page, 'close_show');
  await page.reload();
  await expect(page.getByTestId('show-export')).toBeVisible();

  await rpc(page, 'close_show');
  await rpc(page, 'open_project', { path: demo });
});
