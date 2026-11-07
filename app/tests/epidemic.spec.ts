import { expect, test } from '@playwright/test';

// Import › Sounds against the Epidemic Sound stand-in (tests/epidemic-standin.mjs): the free
// search, dragging results onto "New bucket" and onto a bucket, and bucket pages of 5.
test('import › sounds: search Epidemic, drag into buckets, pages of 5', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();

  // connect (the key goes to the in-memory store of the test server)
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByTestId('settings-tab-connections').click();
  await page.getByRole('textbox', { name: 'Epidemic Sound API key' }).fill('e2e-key-12345');
  await page.getByRole('button', { name: 'Connect' }).first().click();
  await expect(page.getByText('Connected').first()).toBeVisible();
  await page.keyboard.press('Escape');

  await page.getByRole('button', { name: 'Import', exact: true }).click();
  await page.getByRole('tab', { name: 'Sounds' }).click();
  const box = page.getByRole('textbox', { name: 'Search Epidemic Sound' });
  await box.fill('creak');
  await box.press('Enter');
  const results = page.getByRole('complementary', { name: 'Epidemic Sound search' }).getByTestId('search-result');
  await expect(results).toHaveCount(10);

  // onto "New bucket": a bucket named after the search, holding the sound
  const before = await page.getByTestId('bucket-card').count();
  await results.nth(0).dragTo(page.getByTestId('new-bucket-drop'));
  await expect(page.getByTestId('bucket-card')).toHaveCount(before + 1);
  const creak = page.getByRole('article', { name: 'Bucket creak' });
  await expect(creak).toBeVisible();

  // onto that bucket
  await results.nth(3).dragTo(creak);
  await expect(page.getByRole('status')).toContainText('Added to creak');
  await expect(results.nth(3)).toContainText('in a bucket');

  // its panel: the two dropped sounds; a search adds 5 and keeps them; Load more adds 5
  await page.getByRole('button', { name: 'Open creak' }).click();
  const panel = page.getByRole('complementary', { name: 'Bucket creak' });
  await expect(panel.getByTestId('preview-row')).toHaveCount(2);
  await panel.getByRole('button', { name: 'Search', exact: true }).click();
  await expect(panel.getByTestId('preview-row')).toHaveCount(7);
  await panel.getByRole('button', { name: 'Load more' }).click();
  await expect(panel.getByTestId('preview-row')).toHaveCount(12);

  // leave the test server as the other tests expect it: not connected
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByTestId('settings-tab-connections').click();
  await page.getByRole('button', { name: 'Disconnect' }).click();
  await expect(page.getByRole('textbox', { name: 'Epidemic Sound API key' })).toBeVisible();
  await page.keyboard.press('Escape');
});
