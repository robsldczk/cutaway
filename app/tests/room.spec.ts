import { expect, test } from '@playwright/test';

// The room strip next to the current line: the talk between takes, read as sentences,
// sliding when the take changes; the raw transcript only with the developer switch.

test('room strip: shared gap between takes, slides with the take, raw only in dev', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  const pos = page.getByTestId('take-pos');
  await expect(pos).toHaveText('2 / 2');

  const strip = page.getByRole('complementary', { name: 'Room talk around this take' });
  const talk = strip.getByTestId('room-talk');
  await expect(talk.filter({ hasText: 'For the next: quieter.' })).toHaveCount(1);
  await expect(strip.locator('details')).toHaveCount(0);

  const rail = strip.locator('.rail');
  const at2 = await rail.evaluate((el) => getComputedStyle(el).transform);
  await strip.getByRole('button', { name: 'Take 1' }).click();
  await expect(pos).toHaveText('1 / 2');
  await expect.poll(() => rail.evaluate((el) => getComputedStyle(el).transform)).not.toBe(at2);
  await page.waitForTimeout(800);
  await page.screenshot({ path: 'test-results/room-strip-take1.png' });

  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByTestId('dev-switch').check();
  await page.keyboard.press('Escape');
  await expect(strip.locator('details').first()).toBeVisible();
});
