import { expect, test } from '@playwright/test';

test('the home route renders the headline and the primary action', async ({ page }) => {
  await page.goto('/');

  await expect(
    page.getByRole('heading', { level: 1, name: 'Post-quantum DNSSEC, without the weight.' }),
  ).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open the dashboard' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Skip to content' })).toHaveCount(1);
});

test('the development token route is not served by the production build', async ({ page }) => {
  const response = await page.goto('/_dev/tokens');
  expect(response?.status()).toBe(404);
});
