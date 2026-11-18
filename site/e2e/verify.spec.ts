import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { expect, test } from '@playwright/test';

import { expectNoAxeViolations, settle } from './a11y';

// axe is injected as a test script; the page's own CSP still ships to users.
test.use({ bypassCSP: true });

const siteRoot = fileURLToPath(new URL('..', import.meta.url));

interface Claims {
  readonly total: number;
  readonly passed: number;
  readonly claims: readonly {
    readonly pointer: string;
    readonly class: string | null;
    readonly size: string | null;
  }[];
}

const claims = JSON.parse(
  readFileSync(join(siteRoot, 'src/data/generated/claims.json'), 'utf8'),
) as Claims;

test('the verify route renders the whole registry and the check banner', async ({ page }) => {
  await page.goto('/verify');
  await settle(page);

  await expect(page.getByRole('heading', { level: 1, name: 'Verify the numbers' })).toBeVisible();
  await expect(page.locator('[data-claim-row]')).toHaveCount(claims.total);
  await expect(page.locator('[data-claims-count]')).toHaveText(
    `Showing ${claims.total} of ${claims.total} claims.`,
  );

  const banner = page.locator('[data-registry-status]');
  await expect(banner).toHaveAttribute('data-registry-status', 'clean');
  await expect(banner).toHaveAttribute('data-report-drift', '0');
  await expect(banner).toContainText('Every registered claim resolved to its recorded value.');
  await expect(banner.locator('[data-first-failure]')).toHaveCount(0);
  await expect(banner).toContainText('agree');
});

test('the registry filters by search, result, class and size', async ({ page }) => {
  await page.goto('/verify');
  await settle(page);

  const count = page.locator('[data-claims-count]');
  const rows = page.locator('[data-claim-row]:visible');

  await page.locator('[data-claims-search]').fill('udp_fit');
  await expect(count).not.toHaveText(`Showing ${claims.total} of ${claims.total} claims.`);
  const searched = await rows.count();
  expect(searched).toBeGreaterThan(0);
  expect(searched).toBeLessThan(claims.total);

  await page.locator('[data-claims-status]').selectOption('fail');
  await expect(count).toHaveText('Showing 0 of 852 claims.');
  await expect(page.locator('[data-claims-empty]')).toBeVisible();

  await page.getByRole('button', { name: 'Reset filters' }).click();
  await expect(count).toHaveText(`Showing ${claims.total} of ${claims.total} claims.`);
  await expect(page.locator('[data-claims-search]')).toHaveValue('');

  await page.locator('[data-claims-class]').selectOption('measured');
  const measured = claims.claims.filter((claim) => claim.class === 'measured').length;
  await expect(rows).toHaveCount(measured);
  const classes = await rows.evaluateAll((nodes) =>
    nodes.map((node) => node.getAttribute('data-class')),
  );
  expect(new Set(classes)).toEqual(new Set(['measured']));

  await page.getByRole('button', { name: 'Reset filters' }).click();
  await page.locator('[data-claims-size]').selectOption('Large');
  const large = claims.claims.filter((claim) => claim.size === 'Large').length;
  await expect(rows).toHaveCount(large);
});

test('every row links its JSON pointer into the published artifact', async ({ page }) => {
  await page.goto('/verify');
  await settle(page);

  const first = claims.claims[0];
  expect(first).toBeDefined();
  if (first === undefined) return;
  const link = page.locator('[data-claim-row]').first().locator('a.claim-pointer');
  await expect(link).toHaveAttribute(
    'href',
    `/evidence/climb-results.json#${encodeURI(first.pointer)}`,
  );
  await expect(link).toHaveText(first.pointer);

  const response = await page.request.get('/evidence/climb-results.json');
  expect(response.status()).toBe(200);
});

test('the verify command is shown with a copy button', async ({ page }) => {
  await page.goto('/verify');
  await settle(page);

  await expect(page.locator('[data-copy-command]').first()).toContainText(
    'cargo xtask sample-check',
  );
  await expect(page.getByRole('button', { name: 'Copy the verify command' })).toBeVisible();
});

test('axe reports no violations on the verify route', async ({ page }) => {
  await page.goto('/verify');
  await expect(page.locator('h1')).toBeVisible();
  await expectNoAxeViolations(page);
});
