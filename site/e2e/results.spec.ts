import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { expect, test, type Page } from '@playwright/test';
import axe from 'axe-core';

import { notMeasuredItems, scopeSentence } from '../src/lib/limits';

const siteRoot = fileURLToPath(new URL('..', import.meta.url));
const generatedDir = join(siteRoot, 'src/data/generated');
const repositoryRoot = join(siteRoot, '..');

interface Fact {
  readonly id: string;
  readonly display: string;
  readonly unit: string;
  readonly class: string;
  readonly baseline: string;
  readonly pointer: string;
  readonly raw: number | string | null;
}

interface AxeViolation {
  readonly id: string;
  readonly impact: string | null;
  readonly nodes: { readonly target: unknown[] }[];
}

interface AxeWindow {
  readonly axe: {
    run: (context: Document) => Promise<{ violations: AxeViolation[] }>;
  };
}

function register(): Fact[] {
  const path = join(generatedDir, 'facts.json');
  return JSON.parse(readFileSync(path, 'utf8')).facts as Fact[];
}

function fact(id: string): Fact {
  const found = register().find((candidate) => candidate.id === id);
  if (found === undefined) {
    throw new Error(`fact ${id} is not registered`);
  }
  return found;
}

function copy(): string {
  return readFileSync(join(repositoryRoot, 'docs/site/copy.md'), 'utf8');
}

const byteFormat = new Intl.NumberFormat('en-US');

async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
}

test('every rendered figure carries its recorded display, unit and evidence', async ({ page }) => {
  await page.goto('/');
  await settle(page);

  const nodes = page.locator('[data-fact-id]');
  const count = await nodes.count();
  expect(count).toBeGreaterThan(0);

  const facts = new Map(register().map((entry) => [entry.id, entry]));
  for (let index = 0; index < count; index += 1) {
    const node = nodes.nth(index);
    const id = await node.getAttribute('data-fact-id');
    expect(id).not.toBeNull();
    if (id === null) {
      continue;
    }
    const recorded = facts.get(id);
    expect(recorded, id).toBeDefined();
    if (recorded === undefined) {
      continue;
    }

    await expect(node.locator('.fact-value'), id).toHaveText(recorded.display);
    if (recorded.unit !== '—' && recorded.unit !== '') {
      await expect(node.locator('.fact-unit'), id).toHaveText(recorded.unit);
    }
    if (recorded.baseline !== 'n/a') {
      await expect(node.locator('.fact-baseline'), id).toContainText(recorded.baseline);
    }
  }

  const stats = [
    'F-wire-medium-c-vs-a',
    'F-sign-medium-c-vs-a',
    'F-verify-medium-c-vs-a',
    'F-udp-c-fit',
  ];
  for (const id of stats) {
    const stat = page.locator(`[data-result-stat="${id}"]`);
    await expect(stat).toHaveCount(1);
    await expect(stat.locator('.fact-value')).toHaveText(fact(id).display);
  }
});

test('the evidence chip is keyboard operable and keeps focus inside the drawer', async ({
  page,
}) => {
  await page.goto('/');
  await settle(page);

  const id = 'F-wire-medium-c-vs-a';
  const chip = page.locator(`[data-evidence-open="${id}"]`);
  const dialog = page.locator(`[data-evidence-drawer="${id}"]`);
  const close = dialog.locator('[data-evidence-close]');
  const verify = dialog.getByRole('link', { name: 'Open Verify' });

  await expect(dialog).toBeHidden();
  await chip.focus();
  await page.keyboard.press('Enter');
  await expect(dialog).toBeVisible();
  await expect(close).toBeFocused();

  await page.keyboard.press('Shift+Tab');
  await expect(verify).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(close).toBeFocused();

  for (let step = 0; step < 5; step += 1) {
    await page.keyboard.press('Tab');
    const inside = await dialog.evaluate((node) => node.contains(document.activeElement));
    expect(inside, `tab step ${step}`).toBe(true);
  }

  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(chip).toBeFocused();
});

test('the drawer shows the fact value, class, baseline, pointer, source and identity', async ({
  page,
}) => {
  await page.goto('/');
  await settle(page);

  const id = 'F-udp-c-fit';
  const recorded = fact(id);
  await page.locator(`[data-evidence-open="${id}"]`).click();
  const dialog = page.locator(`[data-evidence-drawer="${id}"]`);
  await expect(dialog).toBeVisible();

  await expect(dialog.locator('.fact-value').first()).toHaveText(recorded.display);
  await expect(dialog.locator('dd').nth(0)).toContainText(recorded.display);
  await expect(dialog.locator('dd').nth(1)).toHaveText('Analytical');
  await expect(dialog.locator('dd').nth(2)).toHaveText(recorded.baseline);
  await expect(dialog.locator('dd').nth(3)).toHaveText(recorded.pointer);
  await expect(dialog.locator('dd').nth(4)).toHaveText('docs/sample-output/climb-results.json');
  await expect(dialog.locator('dd').nth(5)).not.toBeEmpty();
  await expect(dialog.getByRole('button', { name: 'Copy JSON pointer' })).toHaveAttribute(
    'data-copy-pointer',
    recorded.pointer,
  );
  await expect(dialog.getByRole('link', { name: 'Open Verify' })).toHaveAttribute(
    'href',
    '/verify',
  );
});

test('the byte race moves by keyboard and its numbers are the recorded byte arithmetic', async ({
  page,
}) => {
  await page.goto('/');
  await settle(page);

  const full = fact('F-plain-signature-bytes');
  const condensed = fact('F-condensed-path-bytes');
  if (typeof full.raw !== 'number' || typeof condensed.raw !== 'number') {
    throw new Error('the byte race needs two numeric byte facts');
  }
  const max = Math.max(1, Math.floor(full.raw / condensed.raw));

  const range = page.locator('[data-byte-race-range]');
  const bytes = page.locator('[data-byte-race-bytes]');
  const paths = page.locator('[data-byte-race-paths]').first();

  await expect(range).toHaveAttribute('max', String(max));
  await expect(bytes).toHaveText(condensed.display);
  await expect(paths).toHaveText('1');

  await range.focus();
  await page.keyboard.press('End');
  await expect(paths).toHaveText(String(max));
  await expect(bytes).toHaveText(byteFormat.format(max * condensed.raw));
  await expect(range).toHaveAttribute(
    'aria-valuetext',
    `${max} condensed responses, ${byteFormat.format(max * condensed.raw)} bytes`,
  );

  await page.keyboard.press('Home');
  await expect(paths).toHaveText('1');
  await expect(bytes).toHaveText(condensed.display);
});

test('the limits accordion carries the scope sentence and the not-measured list', async ({
  page,
}) => {
  await page.goto('/');
  await settle(page);

  const limits = page.locator('[data-limits]');
  const scope = limits.locator('[data-limit-id="scope"]');
  await expect(scope).toHaveJSProperty('open', true);
  await expect(scope).toContainText(scopeSentence(copy()));

  for (const title of ['Environment pinning', 'Sampling and extrapolation', 'Baselines']) {
    await expect(limits.getByText(title, { exact: true })).toBeVisible();
  }

  const notMeasured = limits.locator('[data-limit-id="not-measured"]');
  await notMeasured.locator('summary').click();
  await expect(notMeasured).toHaveJSProperty('open', true);
  for (const item of notMeasuredItems(copy())) {
    await expect(notMeasured).toContainText(item);
  }

  await scope.locator('summary').focus();
  await page.keyboard.press('Enter');
  await expect(scope).toHaveJSProperty('open', false);
  await page.keyboard.press('Space');
  await expect(scope).toHaveJSProperty('open', true);
});

test('the team is named from the repository with no portraits', async ({ page }) => {
  await page.goto('/');
  await settle(page);

  const team = page.locator('[data-team]');
  await expect(team.getByRole('heading', { name: 'Team CLIMB' })).toBeVisible();
  await expect(team).toContainText('AIORI-3 Hackathon');
  await expect(team).toContainText('PS-036');
  await expect(team).toContainText('A3-PS036-TC529');

  for (const [name, role] of [
    ['Pratyay Mukherjee', 'Team Lead and Developer'],
    ['Mohit Kumar Yadav', 'Lead Researcher, UI and Logo Designer'],
    ['Dr. Maibam Devi', 'Faculty Mentor'],
  ] as const) {
    await expect(team.getByText(name, { exact: true })).toBeVisible();
    await expect(team.getByText(role, { exact: true })).toBeVisible();
  }

  await expect(team.locator('img')).toHaveCount(0);
  await expect(team.locator('a').first()).toHaveAttribute('rel', 'noreferrer');
});

test('the footer band opens the dashboard above the link grid', async ({ page }) => {
  await page.goto('/');
  await settle(page);

  const band = page.locator('footer [data-footer-band]');
  await expect(band).toHaveAttribute('href', '/dashboard');
  await expect(band).toContainText('Open the dashboard');
  await expect(band.locator('svg')).toBeVisible();

  const background = await band.evaluate((node) => getComputedStyle(node).backgroundColor);
  expect(background).toBe('rgb(255, 84, 0)');
  const fontSize = await band
    .locator('.footer-band__label')
    .evaluate((node) => Number.parseFloat(getComputedStyle(node).fontSize));
  expect(fontSize).toBeGreaterThanOrEqual(44);

  const bandBox = await band.boundingBox();
  const gridBox = await page.locator('footer a[href="/calculator"]').boundingBox();
  expect(bandBox).not.toBeNull();
  expect(gridBox).not.toBeNull();
  if (bandBox !== null && gridBox !== null) {
    expect(bandBox.y + bandBox.height).toBeLessThanOrEqual(gridBox.y);
  }

  await expect(page.locator('[data-footer-claims] .fact-value')).toHaveText(
    fact('F-claims-count').display,
  );
});

test('axe reports no violations with an evidence drawer open', async ({ page }) => {
  await page.goto('/');
  await page.waitForSelector('h1');
  await page.locator('[data-evidence-open="F-sign-medium-c-vs-a"]').click();
  await expect(page.locator('[data-evidence-drawer="F-sign-medium-c-vs-a"]')).toBeVisible();

  await page.addScriptTag({ content: axe.source });
  const results = await page.evaluate(async () => {
    const engine = (window as unknown as AxeWindow).axe;
    return engine.run(document);
  });
  const summary = results.violations.map(
    (violation) =>
      `${violation.id} (${violation.impact ?? 'unknown'}): ${violation.nodes
        .map((node) => JSON.stringify(node.target))
        .join(' | ')}`,
  );
  expect(summary).toEqual([]);
});

test.describe('with reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });

  test('the results settle without animated transitions', async ({ page }) => {
    await page.goto('/');
    await settle(page);

    const transition = await page
      .locator('[data-byte-race-fill]')
      .evaluate((node) => Number.parseFloat(getComputedStyle(node).transitionDuration));
    expect(transition).toBeLessThanOrEqual(0.001);

    const scroll = await page.evaluate(
      () => getComputedStyle(document.documentElement).scrollBehavior,
    );
    expect(scroll).toBe('auto');

    await page.locator('[data-evidence-open="F-verify-medium-c-vs-a"]').click();
    const dialog = page.locator('[data-evidence-drawer="F-verify-medium-c-vs-a"]');
    await expect(dialog).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
  });
});
