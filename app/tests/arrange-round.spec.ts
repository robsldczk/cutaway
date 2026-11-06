import { expect, test, type Page } from '@playwright/test';

// Side by side, glue, pans and scene marks on Arrange.

async function openArrange(page: Page) {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.keyboard.press('2');
  await expect(page.getByTestId('arrange-row').first()).toBeVisible();
}

test('scene mark: right click on the rail colours the scene, undo takes it back', async ({ page }) => {
  await openArrange(page);
  const scene = page.getByTestId('rail-scene').first();
  const plain = await scene.evaluate((el) => getComputedStyle(el).backgroundColor);

  await scene.click({ button: 'right' });
  await page.getByTestId('status-review').click();
  await expect(page.getByTestId('status-menu')).toBeHidden();
  await expect(scene).toHaveAttribute('data-status', 'review');
  await expect(scene).toHaveAttribute('aria-label', /Review/);
  expect(await scene.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toBe(plain);

  // the menu says what's set, and the keyboard reaches it
  await scene.click({ button: 'right' });
  await expect(page.getByTestId('status-review')).toHaveAttribute('aria-checked', 'true');
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('status-menu')).toBeHidden();
  await expect(scene).toHaveAttribute('data-status', 'review');

  await page.keyboard.press('Meta+z');
  await expect(scene).not.toHaveAttribute('data-status', 'review');
});
