import { expect, test, type Locator, type Page } from '@playwright/test';
import axe from 'axe-core';

const GALLERY = '/_dev/ui';

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

async function tabTo(page: Page, target: Locator): Promise<void> {
  for (let presses = 0; presses < 80; presses += 1) {
    await page.keyboard.press('Tab');
    const reached = await target.evaluate((node) => node === document.activeElement);
    if (reached) {
      return;
    }
  }
  throw new Error('the target was not reachable by Tab');
}

test('axe reports no violations on the gallery', async ({ page }) => {
  await page.goto(GALLERY);
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

test('the card link is reachable by keyboard and keeps a rectangular ring', async ({ page }) => {
  await page.goto(GALLERY);
  const link = page.locator('[data-focus-check]');
  await tabTo(page, link);
  await expect(link).toBeFocused();

  const styles = await link.evaluate((node) => {
    const computed = getComputedStyle(node);
    return {
      outlineStyle: computed.outlineStyle,
      outlineWidth: computed.outlineWidth,
      outlineColor: computed.outlineColor,
      clipPath: computed.clipPath,
    };
  });
  expect(styles.outlineStyle).toBe('solid');
  expect(Number.parseFloat(styles.outlineWidth)).toBeGreaterThanOrEqual(2);
  expect(styles.outlineColor).toBe('rgb(7, 8, 10)');
  expect(styles.clipPath).toBe('none');

  const card = page.locator('[data-notch]').first();
  const cardStyles = await card.evaluate((node) => {
    const computed = getComputedStyle(node);
    return { clipPath: computed.clipPath, userSelect: computed.userSelect };
  });
  expect(cardStyles.clipPath).toBe('none');
  expect(cardStyles.userSelect).toBe('auto');
});

test('pointerdown starts a ring at the pointer and animationend removes it', async ({ page }) => {
  await page.goto(GALLERY);
  const surface = page.locator('[data-ripple-surface][data-tone="ink"]');
  await surface.scrollIntoViewIfNeeded();
  const box = await surface.boundingBox();
  if (box === null) {
    throw new Error('the ink ripple surface has no box');
  }
  await page.mouse.click(box.x + box.width * 0.3, box.y + box.height * 0.4);
  const ring = surface.locator('.ripple-ring');
  await expect(ring).toHaveCount(1);
  const ringBox = await ring.boundingBox();
  expect(ringBox).not.toBeNull();
  if (ringBox !== null) {
    expect(Math.abs(ringBox.x + ringBox.width / 2 - (box.x + box.width * 0.3))).toBeLessThan(2);
    expect(Math.abs(ringBox.y + ringBox.height / 2 - (box.y + box.height * 0.4))).toBeLessThan(2);
  }
  await ring.evaluate((node) => node.dispatchEvent(new Event('animationend')));
  await expect(ring).toHaveCount(0);
});

test('the fallback timer removes the ring when a fake clock advances', async ({ page }) => {
  await page.goto(GALLERY);
  await page.addStyleTag({ content: '.ripple-ring { animation: none !important; }' });
  await page.clock.install();
  const surface = page.locator('[data-ripple-surface][data-tone="ink"]');
  await surface.scrollIntoViewIfNeeded();
  const box = await surface.boundingBox();
  if (box === null) {
    throw new Error('the ink ripple surface has no box');
  }
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  const ring = surface.locator('.ripple-ring');
  await expect(ring).toHaveCount(1);
  await page.clock.fastForward(800);
  await expect(ring).toHaveCount(0);
});

test('Enter and Space on a control produce a centred ring', async ({ page }) => {
  await page.goto(GALLERY);
  const surface = page.locator('[data-ripple-surface][data-tone="paper"]');
  const trigger = surface.getByRole('button', { name: 'Fire a centred ripple' });
  await trigger.scrollIntoViewIfNeeded();
  const box = await surface.boundingBox();
  if (box === null) {
    throw new Error('the paper ripple surface has no box');
  }

  for (const key of ['Enter', ' ']) {
    await trigger.focus();
    await page.keyboard.press(key);
    const ring = surface.locator('.ripple-ring').last();
    await expect(ring).toHaveCount(1);
    const ringBox = await ring.boundingBox();
    expect(ringBox).not.toBeNull();
    if (ringBox !== null) {
      expect(Math.abs(ringBox.x + ringBox.width / 2 - (box.x + box.width / 2))).toBeLessThan(2);
      expect(Math.abs(ringBox.y + ringBox.height / 2 - (box.y + box.height / 2))).toBeLessThan(2);
    }
  }
});

test.describe('reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });

  test('replaces the ring with a short background flash', async ({ page }) => {
    await page.goto(GALLERY);
    await page.clock.install();
    const surface = page.locator('[data-ripple-surface][data-tone="paper"]');
    await surface.scrollIntoViewIfNeeded();
    const box = await surface.boundingBox();
    if (box === null) {
      throw new Error('the paper ripple surface has no box');
    }
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
    const flash = surface.locator('.ripple-flash');
    await expect(flash).toHaveCount(1);
    await expect(surface.locator('.ripple-ring')).toHaveCount(0);
    await page.clock.fastForward(150);
    await expect(flash).toHaveCount(0);
  });
});

for (const width of [320, 768, 1440]) {
  test(`no notch covers text at ${width} pixels`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(GALLERY);

    const issues = await page.evaluate(() => {
      const problems: string[] = [];
      for (const card of Array.from(document.querySelectorAll<HTMLElement>('[data-notch]'))) {
        const box = card.getBoundingClientRect();
        const computed = getComputedStyle(card);
        const notchWidth = Number.parseFloat(computed.getPropertyValue('--notch-w'));
        const notchHeight = Number.parseFloat(computed.getPropertyValue('--notch-h'));
        const corner = card.dataset.corner ?? 'tr';
        const onRight = corner.endsWith('r');
        const onTop = corner.startsWith('t');
        const notch = {
          left: onRight ? box.right - notchWidth : box.left,
          right: onRight ? box.right : box.left + notchWidth,
          top: onTop ? box.top : box.bottom - notchHeight,
          bottom: onTop ? box.top + notchHeight : box.bottom,
        };
        const content = card.querySelector('[data-notch-content]');
        if (content === null) {
          problems.push(`card has no content region at ${corner}`);
          continue;
        }
        const walker = document.createTreeWalker(content, NodeFilter.SHOW_TEXT);
        const range = document.createRange();
        let node = walker.nextNode();
        while (node !== null) {
          const text = node.textContent?.trim() ?? '';
          if (text !== '') {
            range.selectNodeContents(node);
            for (const rect of Array.from(range.getClientRects())) {
              const overlapX = Math.min(rect.right, notch.right) - Math.max(rect.left, notch.left);
              const overlapY = Math.min(rect.bottom, notch.bottom) - Math.max(rect.top, notch.top);
              if (overlapX > 1 && overlapY > 1) {
                problems.push(`${corner} notch covers "${text.slice(0, 40)}"`);
              }
              if (
                rect.left < box.left - 1 ||
                rect.right > box.right + 1 ||
                rect.top < box.top - 1 ||
                rect.bottom > box.bottom + 1
              ) {
                problems.push(`${corner} text escapes the card: "${text.slice(0, 40)}"`);
              }
            }
          }
          node = walker.nextNode();
        }
      }
      return problems;
    });

    expect(issues).toEqual([]);
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(overflow).toBeLessThanOrEqual(1);
  });
}

test('hovering a card changes paint but not its box', async ({ page }) => {
  await page.goto(GALLERY);
  const card = page.locator('[data-notch]').first();
  await card.scrollIntoViewIfNeeded();
  const before = await card.boundingBox();
  if (before === null) {
    throw new Error('the card has no box');
  }
  await card.hover();
  const after = await card.boundingBox();
  expect(after).not.toBeNull();
  if (after !== null) {
    expect(Math.abs(after.x - before.x)).toBeLessThan(0.5);
    expect(Math.abs(after.y - before.y)).toBeLessThan(0.5);
    expect(Math.abs(after.width - before.width)).toBeLessThan(0.5);
    expect(Math.abs(after.height - before.height)).toBeLessThan(0.5);
  }
});
