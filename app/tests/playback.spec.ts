import { expect, test } from '@playwright/test';

// Playback leveling (D69): on by default, switched in Settings, and the switch sticks.

test('playback leveling is on by default and can be switched off', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByRole('button', { name: 'Settings' }).click();
  const level = page.getByTestId('playback-level');
  const on = level.getByRole('radio', { name: 'On' });
  const off = level.getByRole('radio', { name: 'Off' });
  await expect(on).toBeChecked();
  await off.click();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(off).toBeChecked();
  await on.click();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Settings' }).click();
  await expect(on).toBeChecked();
});
