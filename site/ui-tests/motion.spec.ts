import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';

const GALLERY = '/_dev/ui';

interface Fact {
  readonly id: string;
  readonly display: string;
}

const facts = (
  JSON.parse(
    readFileSync(new URL('../src/data/generated/facts.json', import.meta.url), 'utf8'),
  ) as { facts: Fact[] }
).facts;

function displayOf(id: string): string {
  const fact = facts.find((candidate) => candidate.id === id);
  if (fact === undefined) {
    throw new Error(`unknown fact id ${id}`);
  }
  return fact.display;
}

async function waitForMotion(page: Page): Promise<void> {
  await expect
    .poll(() => page.evaluate(() => document.documentElement.dataset.motionReady))
    .toBe('true');
}

async function scrollToBottom(page: Page): Promise<void> {
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
}

test('the gallery raises no console error or uncaught exception', async ({ page }) => {
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(message.text());
    }
  });
  page.on('pageerror', (error) => {
    errors.push(error.message);
  });

  await page.goto(GALLERY);
  await waitForMotion(page);
  await scrollToBottom(page);
  await page.waitForTimeout(250);

  expect(errors).toEqual([]);
});

test('the preloader releases the content within its hard limit', async ({ page }) => {
  await page.goto(GALLERY);
  await waitForMotion(page);

  const overlay = page.locator('[data-preloader]');
  await expect
    .poll(() => overlay.getAttribute('data-preloader-state'), { timeout: 3000 })
    .toMatch(/^(done|skipped)$/);

  const blocked = Number(await overlay.getAttribute('data-preloader-blocked-ms'));
  expect(blocked).toBeGreaterThanOrEqual(0);
  expect(blocked).toBeLessThanOrEqual(1500);
  await expect(overlay).toBeHidden();
});

test('a repeat visit in the same session skips the preloader', async ({ page }) => {
  await page.goto(GALLERY);
  await waitForMotion(page);
  const overlay = page.locator('[data-preloader]');
  await expect
    .poll(() => overlay.getAttribute('data-preloader-state'), { timeout: 3000 })
    .toMatch(/^(done|skipped)$/);

  await page.reload();
  await waitForMotion(page);
  await expect(overlay).toHaveAttribute('data-preloader-state', 'skipped');
  await expect(overlay).toHaveAttribute('data-preloader-blocked-ms', '0');
});

test('every counter lands exactly on its recorded fact', async ({ page }) => {
  await page.goto(GALLERY);
  await waitForMotion(page);

  const counters = page.locator('[data-counter]');
  const count = await counters.count();
  expect(count).toBeGreaterThan(0);

  for (let index = 0; index < count; index += 1) {
    const counter = counters.nth(index);
    await counter.scrollIntoViewIfNeeded();
    await expect
      .poll(() => counter.getAttribute('data-counter-state'), { timeout: 4000 })
      .toBe('final');

    const id = await counter.getAttribute('data-counter-id');
    if (id === null) {
      throw new Error('a counter is missing its fact id');
    }
    await expect(counter.locator('.fact-value')).toHaveText(displayOf(id));
  }
});

test('reveals appear once and stay shown when scrolled past again', async ({ page }) => {
  await page.goto(GALLERY);
  await waitForMotion(page);

  const reveals = page.locator('[data-reveal]');
  expect(await reveals.count()).toBeGreaterThan(0);

  await scrollToBottom(page);
  await expect
    .poll(() =>
      reveals.evaluateAll((nodes) => nodes.every((node) => node.dataset.revealState === 'shown')),
    )
    .toBe(true);

  await page.evaluate(() => window.scrollTo(0, 0));
  await page.waitForTimeout(250);
  const stillShown = await reveals.evaluateAll((nodes) =>
    nodes.every((node) => {
      const styles = getComputedStyle(node);
      return (
        node.dataset.revealState === 'shown' &&
        styles.opacity === '1' &&
        (styles.transform === 'none' || styles.transform === 'matrix(1, 0, 0, 1, 0, 0)')
      );
    }),
  );
  expect(stillShown).toBe(true);
});

test.describe('with a touch device', () => {
  test.use({ hasTouch: true });

  test('smooth scrolling stays off and the progress line reaches the end', async ({ page }) => {
    await page.goto(GALLERY);
    await waitForMotion(page);

    await expect(page.locator('html')).toHaveAttribute('data-smooth-scroll', 'off');

    await scrollToBottom(page);
    const bar = page.locator('[data-scroll-progress-bar]');
    await expect
      .poll(() => bar.evaluate((node) => new DOMMatrixReadOnly(getComputedStyle(node).transform).a))
      .toBeGreaterThan(0.99);
  });
});

test('the marquee pauses on hover and hides its duplicate from assistive technology', async ({
  page,
}) => {
  await page.goto(GALLERY);
  await waitForMotion(page);

  const marquee = page.locator('[data-marquee]');
  await marquee.scrollIntoViewIfNeeded();
  await expect(marquee.locator('[data-marquee-clone]')).toHaveAttribute('aria-hidden', 'true');

  const track = marquee.locator('[data-marquee-track]');
  await expect(track).toHaveCSS('animation-play-state', 'running');
  await marquee.hover();
  await expect(track).toHaveCSS('animation-play-state', 'paused');
});

test.describe('reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });

  test('shows every final state and never runs the preloader', async ({ page }) => {
    await page.goto(GALLERY);
    await waitForMotion(page);

    await expect(page.locator('html')).toHaveAttribute('data-motion', 'reduced');
    await expect(page.locator('html')).toHaveAttribute('data-smooth-scroll', 'off');
    await expect(page.locator('[data-preloader]')).toHaveAttribute(
      'data-preloader-state',
      'skipped',
    );

    const reveals = page.locator('[data-reveal]');
    const visible = await reveals.evaluateAll((nodes) =>
      nodes.every((node) => {
        const styles = getComputedStyle(node);
        return node.dataset.revealState === 'shown' && styles.opacity === '1';
      }),
    );
    expect(visible).toBe(true);

    const counters = page.locator('[data-counter]');
    expect(await counters.count()).toBeGreaterThan(0);
    const finals = await counters.evaluateAll((nodes) =>
      nodes.map((node) => node.dataset.counterState),
    );
    expect(finals.every((state) => state === 'final')).toBe(true);
  });
});

test.describe('without JavaScript', () => {
  test.use({ javaScriptEnabled: false });

  test('renders counters and reveals in their final state', async ({ page }) => {
    await page.goto(GALLERY);

    const counter = page.locator('[data-counter]').first();
    const id = await counter.getAttribute('data-counter-id');
    if (id === null) {
      throw new Error('a counter is missing its fact id');
    }
    await expect(counter.locator('.fact-value')).toHaveText(displayOf(id));

    const reveal = page.locator('[data-reveal]').first();
    await expect(reveal).toHaveCSS('opacity', '1');
    await expect(page.locator('[data-preloader]')).toBeHidden();
  });
});
