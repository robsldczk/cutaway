import { expect, test } from '@playwright/test';

// Playback leveling (D69): on by default, switched in Settings, and the switch sticks.

test('playback leveling is on by default and can be switched off', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByRole('button', { name: 'Settings' }).click();
  const level = page.getByTestId('playback-level');
  await expect(level).toBeChecked();
  await level.uncheck();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByTestId('playback-level')).not.toBeChecked();
  await page.getByTestId('playback-level').check();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(page.getByTestId('playback-level')).toBeChecked();
});
