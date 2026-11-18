import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { expect, test } from '@playwright/test';

import { expectNoAxeViolations, settle } from './a11y';

// axe is injected as a test script; the page's own CSP still ships to users.
test.use({ bypassCSP: true });

const siteRoot = fileURLToPath(new URL('..', import.meta.url));

interface Evidence {
  readonly report: {
    readonly available: boolean;
    readonly check: {
      readonly agree: number;
      readonly drift: number;
      readonly missing: number;
    } | null;
  };
  readonly charts: readonly { readonly name: string }[];
  readonly sums: { readonly rows: readonly unknown[]; readonly available: boolean };
}

const evidence = JSON.parse(
  readFileSync(join(siteRoot, 'src/data/generated/evidence.json'), 'utf8'),
) as Evidence;

test('the report route embeds the report in a sandboxed frame', async ({ page }) => {
  await page.goto('/report');
  await settle(page);

  await expect(page.getByRole('heading', { level: 1, name: 'The full report' })).toBeVisible();

  const frame = page.locator('iframe[src="/evidence/report.html"]');
  await expect(frame).toHaveCount(1);
  await expect(frame).toHaveAttribute('sandbox', '');
  await expect(frame).toHaveAttribute('title', 'The rendered CLIMB report');

  const response = await page.request.get('/evidence/report.html');
  expect(response.status()).toBe(200);
  expect((await response.body()).length).toBeGreaterThan(0);

  await expect(page.locator('[data-report-empty]')).toHaveCount(0);
});

test('every published chart is captioned and loads from the evidence directory', async ({
  page,
}) => {
  await page.goto('/report');
  await settle(page);

  const charts = page.locator('[data-report-chart]');
  await expect(charts).toHaveCount(evidence.charts.length);
  for (const chart of evidence.charts) {
    const node = page.locator(`[data-report-chart="${chart.name}"]`);
    await expect(node).toHaveCount(1);
    await expect(node.locator('figcaption')).not.toBeEmpty();
    const src = await node.locator('img').getAttribute('src');
    expect(src).toBe(`/evidence/${chart.name}`);
    const response = await page.request.get(src ?? '');
    expect(response.status(), chart.name).toBe(200);
  }
});

test('the download links target the published evidence files', async ({ page }) => {
  await page.goto('/report');
  await settle(page);

  const links = page.locator('a[download]');
  await expect(links).toHaveCount(5);
  const hrefs = await links.evaluateAll((nodes) =>
    nodes.map((node) => node.getAttribute('href') ?? ''),
  );
  for (const href of hrefs) {
    expect(href.startsWith('/evidence/')).toBe(true);
    const response = await page.request.get(href);
    expect(response.status(), href).toBe(200);
  }

  await expect(page.getByRole('link', { name: 'Download the results artifact' })).toBeVisible();
  await expect(page.getByRole('link', { name: 'Open the HTML report' })).toBeVisible();
});

test('the page states the recorded check instead of a typed claim', async ({ page }) => {
  await page.goto('/report');
  await settle(page);

  const check = evidence.report.check;
  expect(check).not.toBeNull();
  if (check === null) return;
  const banner = page.locator('[data-report-check]');
  await expect(banner).toContainText(`${check.agree} agree`);
  await expect(banner).toContainText(`${check.drift} drift`);
  await expect(banner).toContainText(`${check.missing} missing`);
});

test('axe reports no violations on the report route', async ({ page }) => {
  await page.goto('/report');
  await expect(page.locator('h1')).toBeVisible();
  await expectNoAxeViolations(page);
});
