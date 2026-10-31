import { expect, test } from '@playwright/test';

test('open the project, flip takes, play and stop, delete a word and undo', async ({ page }) => {
  await page.goto('/');
  const box = page.getByTestId('current-box');
  await expect(box).toBeVisible();
  await expect(page.getByTestId('current-text')).toContainText('machine');

  // takes with the arrow keys (the director's pick, T2, is selected by default)
  const pos = page.getByTestId('take-pos');
  await expect(pos).toHaveText('2 / 2');
  await page.keyboard.press('ArrowLeft');
  await expect(pos).toHaveText('1 / 2');
  await page.keyboard.press('ArrowRight');
  await expect(pos).toHaveText('2 / 2');

  // play / stop with Space
  const play = page.getByRole('button', { name: 'Play or stop (Space)' });
  await page.keyboard.press('Space');
  await expect(play).toHaveAttribute('data-playing', 'true');
  await expect(page.getByTestId('time-now')).not.toHaveText('00:00.0');
  await page.keyboard.press('Space');
  await expect(play).toHaveAttribute('data-playing', 'false');

  // box editor: delete a word, then undo
  await page.keyboard.press('e');
  await expect(page.getByTestId('box-editor')).toBeVisible();
  await page.getByTestId('edit-mode').click();
  const first = page.getByTestId('comp-word').first();
  await expect(first).toHaveAttribute('data-deleted', 'false');
  await first.click();
  await page.keyboard.press('Backspace');
  await expect(page.getByTestId('comp-word').first()).toHaveAttribute('data-deleted', 'true');
  await page.keyboard.press('ControlOrMeta+z');
  await expect(page.getByTestId('comp-word').first()).toHaveAttribute('data-deleted', 'false');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('box-editor')).toBeHidden();
});

test('arrange shows rows and pauses; the edit page shows lanes', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.keyboard.press('2');
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
  await expect(page.getByTestId('gap-row').first()).toContainText('s');
  await page.keyboard.press('3');
  await expect(page.getByTestId('edit-lane').first()).toBeVisible();
});
