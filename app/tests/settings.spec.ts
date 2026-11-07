import { expect, test } from '@playwright/test';

// Settings in tabs (D163), the imprint and the licenses (D161), the version (D162).

test.beforeEach(async ({ page }) => {
  // a stand-in clipboard: WebKit in Playwright grants no clipboard permission
  await page.addInitScript(() => {
    const w = window as unknown as { copied: string[] };
    w.copied = [];
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: async (t: string) => void w.copied.push(t) }, configurable: true });
  });
  await page.goto('/');
  await expect(page.getByTestId('current-box')).toBeVisible();
});

test('settings open on General, switch tabs with clicks and arrows, and the version copies', async ({ page }) => {
  await page.keyboard.press('Meta+Comma');
  const sheet = page.getByRole('dialog', { name: 'Settings' });
  await expect(sheet).toBeVisible();
  await expect(page.getByTestId('settings-tab-general')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('dev-switch')).toBeVisible();

  await page.getByTestId('settings-tab-pacing').click();
  await expect(page.getByTestId('lead-in')).toBeVisible();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByTestId('settings-tab-connections')).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('textbox', { name: 'Epidemic Sound API key' })).toBeVisible();

  // the sheet stays one size, whatever the tab
  const h = (await sheet.boundingBox())!.height;
  await page.getByTestId('settings-tab-imprint').click();
  expect(Math.abs((await sheet.boundingBox())!.height - h)).toBeLessThan(2);

  const version = page.getByTestId('app-version');
  await expect(version).toHaveText(/^\d+\.\d+\.\d+ · [0-9a-f]{7}(-modified)?$|unknown/);
  await version.click();
  await expect(version).toHaveAttribute('title', 'Copied');
  const copied = await page.evaluate(() => (window as unknown as { copied: string[] }).copied);
  expect(copied[0]).toMatch(/^Cutaway \d+\.\d+\.\d+ \(([0-9a-f]{7}(-modified)?|unknown)\)$/);

  await page.keyboard.press('Escape');
  await expect(sheet).toBeHidden();
});

test('the imprint is English then the German original, and can be selected', async ({ page }) => {
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByTestId('settings-tab-imprint').click();
  const imprint = page.getByTestId('imprint');
  await expect(imprint).toContainText('Legal notice (according to § 5 DDG)');
  await expect(imprint).toContainText('Angaben gemäß § 5 DDG');
  await expect(imprint).toContainText('Robert Sladeczek');
  await expect(imprint).toContainText('02977 Hoyerswerda');
  const text = await imprint.innerText();
  expect(text.indexOf('English')).toBeLessThan(text.indexOf('Deutsch'));
  expect(await imprint.evaluate((e) => getComputedStyle(e).userSelect)).not.toBe('none');
  await page.keyboard.press('Escape');
});

test('the licenses list every package, search them, and show a license text', async ({ page }) => {
  await page.getByRole('button', { name: 'Settings' }).click();
  await page.getByTestId('settings-tab-licenses').click();
  const tab = page.getByTestId('licenses');
  await expect(tab).toContainText('Cutaway is proprietary software');
  await expect(tab).toContainText('© 2026 Robert Sladeczek');
  const rows = page.getByTestId('license-row');
  await expect(rows.first()).toBeVisible();
  expect(await rows.count()).toBeGreaterThan(400);

  await page.getByTestId('license-search').fill('parakeet');
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText('CC-BY-4.0');

  await page.getByTestId('license-search').fill('svelte mit');
  const svelte = rows.filter({ has: page.locator('.pname', { hasText: /^svelte$/ }) }).first();
  await svelte.click();
  await expect(svelte).toHaveAttribute('aria-expanded', 'true');
  await expect(tab.locator('pre').first()).toContainText('Permission is hereby granted');
  await page.keyboard.press('Escape');
});
