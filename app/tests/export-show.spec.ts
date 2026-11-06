import { expect, test } from '@playwright/test';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// A show export from the show home: the Range lists episodes and their scenes, nothing can go
// out before the pipeline folder is confirmed, and it lands there. Closes the demo episode and
// opens it again at the end, like the studio test.

const bridge = 'http://127.0.0.1:7358';
const shots = process.env.CUTAWAY_SHOTS;

async function rpc<T = Record<string, unknown>>(method: string, params: object = {}): Promise<T> {
  const r = await fetch(`${bridge}/rpc`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ method, params }) });
  const b = (await r.json()) as { ok: boolean; result: T; error?: { message: string } };
  if (!b.ok) throw new Error(`${method}: ${b.error?.message}`);
  return b.result;
}

test('a show export asks for its folder once, and lands in it', async ({ page }) => {
  const demo = (await rpc<{ path: string }>('project_status')).path;
  const root = join(mkdtempSync(join(tmpdir(), 'cutaway-show-export-')), 'tm-2610');
  mkdirSync(join(root, 'Episodes/Episode 2/Sound'), { recursive: true });
  writeFileSync(join(root, 'Episodes/Episode 2/episode.jetplane.json'), '{"schemaVersion":1,"kind":"episode","name":"Turkey Time","episodeNumber":2}');
  writeFileSync(join(root, 'library.jetplane.json'), '{"kind":"library"}');
  await rpc('close_project');
  await rpc('create_show', { root });
  await rpc('migrate_episode', { source: demo, episode: 'Episode 2' });

  await page.goto('/');
  await page.getByTestId('show-export').click();
  await expect(page.getByRole('region', { name: 'Export' })).toBeVisible();

  // ticking an episode takes all its scenes; a scene can go
  await page.getByTestId('show-episode').check();
  await page.getByRole('button', { name: 'Remove scene 2' }).click();
  await expect(page.getByText('1 scene of 1 episode')).toBeVisible();

  // nothing leaves for a guessed folder
  const go = page.getByTestId('export-go');
  await expect(page.getByTestId('pipeline-confirm')).toBeVisible();
  await expect(page.getByTestId('pipeline-path')).toHaveValue(join(root, 'Sound'));
  await expect(go).toBeDisabled();
  expect(existsSync(join(root, 'Sound/Exports'))).toBe(false);
  if (shots) await page.screenshot({ path: `${shots}/show-export.png`, fullPage: true });

  await page.getByTestId('pipeline-use').click();
  await expect(page.getByTestId('pipeline-confirm')).toBeHidden();
  await expect(go).toBeEnabled();
  await go.click();
  await expect(page.getByTestId('export-done')).toContainText('Saved 1 scene to the pipeline folder', { timeout: 30_000 });

  const scene = join(root, 'Sound/Exports/Ep02/Sc01_Lab');
  const [version] = readdirSync(scene);
  expect(version).toMatch(/^v001_\d{4}-\d{2}-\d{2}$/);
  expect(readdirSync(join(scene, version))).toContain('E02_S01_v001_Mix.wav');

  await rpc('close_show');
  await rpc('open_project', { path: demo });
});
