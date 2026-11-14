import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { expect, test } from '@playwright/test';

import { expectNoAxeViolations, settle } from './a11y';

const siteRoot = fileURLToPath(new URL('..', import.meta.url));
const repositoryRoot = join(siteRoot, '..');

interface Claims {
  readonly total: number;
}

interface Evidence {
  readonly report: { readonly check: { readonly agree: number } | null };
  readonly sums: { readonly rows: readonly unknown[] };
  readonly reproduction_mode: string;
}

interface Provenance {
  readonly commit: string;
  readonly content_identity: string;
}

const claims = JSON.parse(
  readFileSync(join(siteRoot, 'src/data/generated/claims.json'), 'utf8'),
) as Claims;
const evidence = JSON.parse(
  readFileSync(join(siteRoot, 'src/data/generated/evidence.json'), 'utf8'),
) as Evidence;
const provenance = JSON.parse(
  readFileSync(join(siteRoot, 'src/data/generated/provenance.json'), 'utf8'),
) as Provenance;

test('the reproduce route shows the quickstart, rebuild modes and expected output', async ({
  page,
}) => {
  await page.goto('/reproduce');
  await settle(page);

  await expect(page.getByRole('heading', { level: 1, name: 'Rebuild every number' })).toBeVisible();
  await expect(page.locator('[data-quickstart]')).toHaveCount(3);
  await expect(page.locator('[data-rebuild]')).toHaveCount(2);

  const output = page.locator('[data-expected-output]');
  await expect(output).toContainText(`sample-check: ${claims.total} claims`);
  await expect(output).toContainText(`${evidence.sums.rows.length} manifest entries match`);
  await expect(output).toContainText(provenance.content_identity);
  await expect(output).toContainText(provenance.commit.slice(0, 7));

  await expect(page.locator('[data-reproduction-mode]')).toHaveText(
    evidence.reproduction_mode === 'unknown'
      ? 'not recorded in this artifact'
      : evidence.reproduction_mode,
  );
});

test('the release bundle download is published when the checkout has one', async ({ page }) => {
  await page.goto('/reproduce');
  await settle(page);

  const bundle = page.locator('[data-release-bundle]');
  const candidates = existsSync(join(repositoryRoot, 'dist'))
    ? readdirSync(join(repositoryRoot, 'dist')).filter((name) =>
        /^climb-[0-9a-f]{7,40}\.tar\.gz$/.test(name),
      )
    : [];

  if (candidates.length === 0) {
    await expect(bundle).toContainText('No release bundle has been published from this checkout.');
    return;
  }

  const link = bundle.locator(`a[href^="/evidence/climb-"]`);
  await expect(link).toHaveCount(1);
  const href = await link.getAttribute('href');
  expect(href).not.toBeNull();
  const response = await page.request.get(href ?? '');
  expect(response.status()).toBe(200);
  await expect(bundle).toContainText('SHA-256');
});

test('the in-browser checker matches a real file and names a mismatch', async ({ page }) => {
  await page.goto('/reproduce');
  await settle(page);

  const input = page.locator('[data-sha-input]');
  const verdict = page.locator('[data-sha-verdict]');

  await input.setInputFiles(join(repositoryRoot, 'docs/sample-output/climb-results.json'));
  await expect(verdict).toHaveAttribute('data-verdict', 'match');
  await expect(verdict).toContainText('byte-identical');
  await expect(page.locator('[data-sha-computed]')).toHaveText(/^[0-9a-f]{64}$/);
  await expect(page.locator('[data-sha-recorded]')).toHaveText(/^[0-9a-f]{64}$/);

  await input.setInputFiles({
    name: 'climb-results.json',
    mimeType: 'application/json',
    buffer: Buffer.from('tampered'),
  });
  await expect(verdict).toHaveAttribute('data-verdict', 'mismatch');
  await expect(verdict).toContainText('differs from the recorded copy');
});

test('the page names a file that is not in the manifest', async ({ page }) => {
  await page.goto('/reproduce');
  await settle(page);

  await page.locator('[data-sha-input]').setInputFiles({
    name: 'not-published.json',
    mimeType: 'application/json',
    buffer: Buffer.from('{}'),
  });
  const verdict = page.locator('[data-sha-verdict]');
  await expect(verdict).toHaveAttribute('data-verdict', 'unknown');
  await expect(verdict).toContainText('not listed in the recorded manifest');
  await expect(page.locator('[data-sha-recorded]')).toHaveText('—');
});

test('the copy button reports the clipboard result', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/reproduce');
  await settle(page);

  const button = page.getByRole('button', { name: 'Copy the command' }).first();
  await button.click();
  const status = page.locator('[data-copy-status]').first();
  await expect(status).toContainText('clipboard');
  const clipboard = await page.evaluate(() => navigator.clipboard.readText());
  expect(clipboard).toContain('cargo xtask reproduce');
});

test('axe reports no violations on the reproduce route', async ({ page }) => {
  await page.goto('/reproduce');
  await expect(page.locator('h1')).toBeVisible();
  await expectNoAxeViolations(page);
});
