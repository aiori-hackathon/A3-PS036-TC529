import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { expect, test, type Page } from '@playwright/test';
import axe from 'axe-core';

const siteRoot = fileURLToPath(new URL('..', import.meta.url));
const generatedDir = join(siteRoot, 'src/data/generated');

interface Fact {
  readonly id: string;
  readonly display: string;
  readonly unit: string;
  readonly class: string;
  readonly raw: number | string | null;
}

interface LadderTree {
  readonly leaf_count: number;
  readonly plain_signature_bytes: number;
  readonly leaves: { readonly condensed_bytes: number }[];
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

interface HeroWindow {
  __climbLcp?: { startTime: number; tag: string | null; headline: boolean }[];
  __climbCls?: number;
}

function loadJson<T>(name: string): T {
  return JSON.parse(readFileSync(join(generatedDir, `${name}.json`), 'utf8')) as T;
}

function fact(id: string): Fact {
  const register = loadJson<{ facts: Fact[] }>('facts');
  const found = register.facts.find((candidate) => candidate.id === id);
  if (found === undefined) {
    throw new Error(`fact ${id} is not registered`);
  }
  return found;
}

async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
}

test.describe('hero screenshots', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('the mobile hero matches its baseline', async ({ page }) => {
    await page.goto('/');
    await settle(page);
    await expect(page).toHaveScreenshot('hero-390x844.png', {
      fullPage: true,
      animations: 'disabled',
    });
  });
});

test.describe('hero screenshots at desktop width', () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test('the desktop hero matches its baseline', async ({ page }) => {
    await page.goto('/');
    await settle(page);
    await expect(page).toHaveScreenshot('hero-1440x900.png', {
      fullPage: true,
      animations: 'disabled',
    });
  });
});

test('axe reports no violations on the home route', async ({ page }) => {
  await page.goto('/');
  await page.waitForSelector('h1');
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

test('the hero renders every figure and its artwork without JavaScript', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  try {
    await page.goto('/');

    await expect(
      page.getByRole('heading', { level: 1, name: 'Post-quantum DNSSEC, without the weight.' }),
    ).toBeVisible();
    await expect(page.getByRole('link', { name: 'Open the dashboard' })).toBeVisible();
    await expect(page.getByRole('link', { name: 'See the evidence' })).toBeVisible();
    await expect(page.locator('[data-hero-art]')).toBeVisible();

    for (const id of ['F-wire-medium-c-vs-a', 'F-sign-medium-c-vs-a', 'F-verify-medium-c-vs-a']) {
      await expect(page.locator(`[data-fact-id="${id}"]`).first()).toBeVisible();
    }

    await expect(page.locator('[data-byte-label="plain"]')).toBeVisible();
    await expect(page.locator('[data-byte-label="condensed"]')).toBeVisible();
    await expect(page.locator('[data-marquee]')).toBeVisible();
  } finally {
    await context.close();
  }
});

test('the artwork byte labels equal their recorded facts and ladder row', async ({ page }) => {
  await page.goto('/');

  const ladder = loadJson<{ trees: LadderTree[] }>('ladder');
  const tree = ladder.trees.find((candidate) => candidate.leaf_count === 8);
  expect(tree).toBeDefined();
  if (tree === undefined) {
    return;
  }

  const expected = [
    {
      label: 'plain',
      id: 'F-plain-signature-bytes',
      raw: tree.plain_signature_bytes,
    },
    {
      label: 'condensed',
      id: 'F-condensed-path-bytes',
      raw: tree.leaves[0]?.condensed_bytes,
    },
  ];

  for (const item of expected) {
    const registered = fact(item.id);
    expect(registered.raw, item.id).toBe(item.raw);

    const label = page.locator(`[data-byte-label="${item.label}"]`);
    await expect(label.locator(`[data-fact-id="${item.id}"]`)).toHaveCount(1);
    await expect(label.locator('.fact-value')).toHaveText(registered.display);
    await expect(label.locator('.fact-unit')).toHaveText(registered.unit);
    await expect(label.locator('.chip')).toContainText(
      registered.class.charAt(0).toUpperCase() + registered.class.slice(1),
    );
  }
});

test.describe('hero draw-in with motion allowed', () => {
  test.use({ reducedMotion: 'no-preference' });

  test('draws the artwork once and settles on the complete picture', async ({ page }) => {
    await page.goto('/');

    const shard = page.locator('.hero-art__shard').first();
    const animation = await shard.evaluate((node) => {
      const computed = getComputedStyle(node);
      return {
        name: computed.animationName,
        duration: computed.animationDuration,
        iterations: computed.animationIterationCount,
      };
    });
    expect(animation.name).toBe('hero-art-draw');
    expect(animation.duration).toBe('0.52s');
    expect(animation.iterations).toBe('1');

    await page.waitForTimeout(1200);
    const settled = await page.locator('[data-hero-art]').evaluate((node) => {
      const parts = [...node.querySelectorAll('.hero-art__shard, .hero-art__bar-fill')];
      return parts.map((part) => {
        const computed = getComputedStyle(part);
        return { opacity: Number(computed.opacity), transform: computed.transform };
      });
    });
    expect(settled.length).toBeGreaterThan(0);
    for (const part of settled) {
      expect(part.opacity).toBe(1);
      expect(part.transform === 'none' || part.transform.startsWith('matrix(1, 0, 0, 1')).toBe(
        true,
      );
    }
  });
});

test.describe('hero draw-in with reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });

  test('skips the animation and shows the complete artwork', async ({ page }) => {
    await page.goto('/');

    for (const selector of ['.hero-art__shard', '.hero-art__bar-fill', '.hero-art__foot-line']) {
      const name = await page
        .locator(selector)
        .first()
        .evaluate((node) => getComputedStyle(node).animationName);
      expect(name, selector).toBe('none');
    }

    const opacity = await page
      .locator('.hero-art__bar-fill')
      .evaluate((node) => Number(getComputedStyle(node).opacity));
    expect(opacity).toBe(1);
  });
});

test('the hero text is the largest paint and arrives within the 4G budget', async ({
  page,
  context,
}) => {
  await page.addInitScript(() => {
    const state = window as unknown as HeroWindow;
    state.__climbLcp = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        const paint = entry as PerformanceEntry & { element?: Element | null };
        const headline = document.querySelector('[data-hero-headline]');
        state.__climbLcp?.push({
          startTime: paint.startTime,
          tag: paint.element?.tagName ?? null,
          headline: paint.element === headline,
        });
      }
    }).observe({ type: 'largest-contentful-paint', buffered: true });
  });

  const client = await context.newCDPSession(page);
  await client.send('Network.enable');
  await client.send('Network.emulateNetworkConditions', {
    offline: false,
    latency: 170,
    downloadThroughput: (9 * 1024 * 1024) / 8,
    uploadThroughput: (1.5 * 1024 * 1024) / 8,
    connectionType: 'cellular4g',
  });
  await client.send('Emulation.setCPUThrottlingRate', { rate: 4 });

  await page.goto('/', { waitUntil: 'load' });
  await page.waitForTimeout(1000);

  const entries = await page.evaluate(() => (window as unknown as HeroWindow).__climbLcp ?? []);
  expect(entries.length).toBeGreaterThan(0);
  const last = entries[entries.length - 1];
  console.log(`hero LCP: ${last?.startTime ?? 'missing'} ms, element ${last?.tag ?? 'unknown'}`);
  expect(last?.headline).toBe(true);
  expect(last?.tag).toBe('H1');
  expect(last?.startTime ?? Number.POSITIVE_INFINITY).toBeLessThan(2500);
});

test('the hero records no layout shift', async ({ page }) => {
  await page.addInitScript(() => {
    const state = window as unknown as HeroWindow;
    state.__climbCls = 0;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries() as (PerformanceEntry & {
        value?: number;
        hadRecentInput?: boolean;
      })[]) {
        if (!entry.hadRecentInput) {
          state.__climbCls = (state.__climbCls ?? 0) + (entry.value ?? 0);
        }
      }
    }).observe({ type: 'layout-shift', buffered: true });
  });

  await page.goto('/', { waitUntil: 'load' });
  await page.waitForTimeout(1200);

  const cls = await page.evaluate(() => (window as unknown as HeroWindow).__climbCls ?? 0);
  console.log(`hero CLS: ${cls}`);
  expect(cls).toBeLessThan(0.01);
});
