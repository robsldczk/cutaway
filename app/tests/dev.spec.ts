import { expect, test } from '@playwright/test';

// Dev panel on the demo session: the developer switch, the transcript under the hood,
// a mark linked to a script line, "Make take" and Apply, and the Learn tab.

test('dev panel: transcript, link a sentence to a line, make a take, learn tab', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Dev', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByTestId('dev-switch').getByRole('radio', { name: 'On' }).click();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Dev', exact: true }).click();
  await expect(page.getByTestId('dev-page')).toBeVisible();

  const table = page.getByRole('table', { name: 'Transcript, rescue and script' });
  await expect(table.getByRole('row').nth(1)).toBeVisible();
  // the director mic is there, as notes only
  await page.getByRole('group', { name: 'Filter' }).getByRole('button', { name: 'Director' }).click();
  await expect(table.getByRole('row').nth(1)).toContainText('DIR');
  await expect(table.getByRole('checkbox')).toHaveCount(0);

  // Paul's unmatched question about water, linked by hand to one of his lines
  const picker = page.getByRole('combobox', { name: 'Actor session' });
  const paul = (await picker.locator('option').allTextContents()).find((t) => t.startsWith('PAUL'));
  await picker.selectOption({ label: paul! });
  await page.getByRole('group', { name: 'Filter' }).getByRole('button', { name: 'All' }).click();
  await page.getByRole('searchbox', { name: 'Search' }).fill('water');
  const row = table.getByRole('row').filter({ hasText: 'water' }).first();
  await row.getByRole('checkbox').check();
  const bar = page.getByTestId('link-bar');
  await expect(bar).toContainText('water');
  const line = bar.getByLabel('Script line');
  await line.selectOption({ index: 1 });
  await bar.getByLabel('Note for Claude').fill('test: linked by hand');
  await bar.getByRole('button', { name: 'Make take' }).click();
  const mark = page.getByTestId('mark').first();
  await expect(mark).toContainText('proposed');
  await mark.getByRole('button', { name: 'Apply' }).click();
  await expect(mark).toContainText('applied');
  await expect(row).toContainText('linked by hand');

  await page.getByRole('tab', { name: 'Learn' }).click();
  await expect(page.getByTestId('dev-learn')).toContainText('Review a scene');
  await expect(page.getByTestId('dev-learn')).toContainText('water');

  // leave the shared demo session as it was: undo the take, the mark and the switch
  await page.keyboard.press('ControlOrMeta+z');
  await page.keyboard.press('ControlOrMeta+z');
  await page.keyboard.press('ControlOrMeta+z');
  await page.evaluate(() => localStorage.removeItem('cutaway.dev'));
});
