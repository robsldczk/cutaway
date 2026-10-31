import { expect, test } from '@playwright/test';

// Round 2 screens on the demo session (no sound service connected).

test('import › files: cast cards, a type fix through "Needs you", redo match', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await expect(page.getByTestId('cast-card')).toHaveCount(2);
  await expect(page.getByRole('region', { name: 'Needs you' })).toBeHidden();

  // Sara's recording made a director room file by mistake: Sara needs a recording
  await page.getByRole('button', { name: 'Change type of Sara.wav' }).click();
  const needs = page.getByRole('region', { name: 'Needs you' });
  await expect(needs).toContainText('No recording found');
  await expect(page.getByTestId('cast-card')).toHaveCount(1);
  // undo puts it back
  await page.keyboard.press('ControlOrMeta+z');
  await expect(page.getByTestId('cast-card')).toHaveCount(2);

  await page.getByRole('button', { name: 'Redo match' }).click();
  await expect(page.getByText('Matched just now')).toBeVisible();
  await expect(page.getByText('Ensemble · shared with 1 other').first()).toBeVisible();
});

test('import › sounds: spot script, a new bucket and its panel; ⇧B goes to the editor', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await page.getByRole('tab', { name: 'Sounds' }).click();
  await page.getByRole('button', { name: 'Spot script' }).click();
  await expect(page.getByTestId('bucket-card').first()).toBeVisible();
  const spotted = await page.getByTestId('bucket-card').count();

  await page.getByRole('button', { name: 'New bucket' }).click();
  await page.getByRole('textbox', { name: 'Bucket name' }).fill('Fridge hum');
  const kw = page.getByRole('textbox', { name: 'Add keyword' });
  await kw.fill('fridge');
  await kw.press('Enter');
  await kw.fill('hum');
  await kw.press('Enter');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.getByTestId('bucket-card')).toHaveCount(spotted + 1);
  const panel = page.getByRole('complementary', { name: 'Bucket Fridge hum' });
  await expect(panel).toBeVisible();
  await expect(panel).toContainText('Connect Epidemic Sound');
  await expect(panel.getByRole('button', { name: 'Remove hum' })).toBeVisible();

  await page.keyboard.press('Shift+B');
  await expect(page.getByRole('region', { name: 'Control bar' })).toBeVisible();
});

test('edit: control bar toggles the bucket, inspector, mixer, cycle; lanes mute and solo', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
  await page.keyboard.press('3');
  await expect(page.getByRole('region', { name: 'Control bar' })).toBeVisible();
  await expect(page.locator('[data-lane="PAUL"]')).toBeVisible();
  await expect(page.locator('[data-lane="sfx1"]')).toBeVisible();

  // B hides the docked bucket (a tab remains), B brings it back
  await expect(page.getByRole('complementary', { name: /Bucket for scene/ })).toBeVisible();
  await page.keyboard.press('b');
  await expect(page.getByRole('button', { name: 'Show bucket (B)' })).toBeVisible();
  await page.keyboard.press('b');
  await expect(page.getByRole('complementary', { name: /Bucket for scene/ })).toBeVisible();

  // I inspector, X mixer, C cycle
  await page.keyboard.press('i');
  await expect(page.getByRole('complementary', { name: 'Inspector' })).toBeVisible();
  await page.keyboard.press('x');
  await expect(page.getByRole('region', { name: 'Mixer' })).toBeVisible();
  await page.keyboard.press('c');
  await expect(page.getByRole('button', { name: 'Cycle (C)' })).toHaveAttribute('aria-pressed', 'true');

  // a dialogue clip selects into the inspector; mute the lane, then undo
  await page.locator('[data-lane="PAUL"] [role="button"]').first().click();
  await expect(page.getByRole('complementary', { name: 'Inspector' })).toContainText('Paul');
  const mute = page.getByRole('region', { name: 'Timeline' }).getByRole('button', { name: 'Mute Paul' });
  await mute.click();
  await expect(mute).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('ControlOrMeta+z');
  await expect(mute).toHaveAttribute('aria-pressed', 'false');
});
