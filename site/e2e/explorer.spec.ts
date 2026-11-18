/**
 * The ladder explorer's state machine, the fact slots and the routes the new
 * content links to. The parity suite owns the animation inventory; this file
 * owns the interaction and content contract for the MS2 sections.
 */
import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { expectNoAxeViolations } from './a11y';

const facts = JSON.parse(
  readFileSync(fileURLToPath(new URL('../src/data/generated/facts.json', import.meta.url)), 'utf8'),
) as {
  facts: {
    id: string;
    display: string;
    kind: string;
  }[];
};

async function open(page: import('@playwright/test').Page): Promise<void> {
  await page.goto('/', { waitUntil: 'load' });
  await expect(page.locator('#cp .c')).toHaveCount(7);
}

test.describe('ladder explorer', () => {
  test('mouse: a card opens its branch, a leaf fills the detail pane', async ({ page }) => {
    await open(page);
    const caps = page.locator('#cp .c');
    await expect(caps.first()).toHaveAttribute('aria-pressed', 'true');

    await caps.nth(3).click();
    await expect(caps.nth(3)).toHaveAttribute('aria-pressed', 'true');
    await expect(caps.first()).toHaveAttribute('aria-pressed', 'false');
    await expect(page.locator('#lv .l')).toHaveCount(4);

    await page.locator('#lv .l').nth(2).click();
    await expect(page.locator('#lv .l').nth(2)).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('#dt h3')).toHaveText('Ladder cache');
    await expect(page.locator('#dt .cr')).toHaveText('Memory and cache use');
    await expect(page.locator('#dt .base')).toContainText('Verify this claim');
    await expect(page.locator('#dt .cmd code')).toHaveText('cargo xtask memory-report');
  });

  test('keyboard: arrows rove, Enter selects, Escape returns to the capability', async ({
    page,
  }) => {
    await open(page);
    const caps = page.locator('#cp .c');
    const leaves = page.locator('#lv .l');

    await caps.first().focus();
    await page.keyboard.press('ArrowDown');
    await expect(caps.nth(1)).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(caps.nth(1)).toHaveAttribute('aria-pressed', 'true');
    await expect(leaves).toHaveCount(2);
    await expect(leaves.first()).toHaveAttribute('aria-pressed', 'true');

    await page.keyboard.press('Tab');
    await expect(leaves.first()).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(leaves.nth(1)).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(leaves.nth(1)).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('#dt h3')).toHaveText('Trusted cached ladder');

    await page.keyboard.press('Escape');
    await expect(caps.nth(1)).toBeFocused();
  });

  test('ARIA: every control states what it controls and the pane is a live region', async ({
    page,
  }) => {
    await open(page);
    for (const button of await page.locator('#cp .c').all()) {
      expect(await button.getAttribute('aria-controls')).toContain('lv');
      expect(await button.getAttribute('aria-controls')).toContain('dt');
      expect(['true', 'false']).toContain(await button.getAttribute('aria-pressed'));
    }
    for (const button of await page.locator('#lv .l').all()) {
      expect(await button.getAttribute('aria-controls')).toBe('dt');
      expect(['true', 'false']).toContain(await button.getAttribute('aria-pressed'));
    }
    await expect(page.locator('#dt')).toHaveAttribute('aria-live', 'polite');
    await expect(page.locator('#dt')).toHaveAttribute('aria-atomic', 'true');
  });

  test('tabs: arrows, Home and End move selection and the panel follows', async ({ page }) => {
    await open(page);
    const tabs = page.locator('[role=tab]');
    await tabs.nth(2).focus();
    await page.keyboard.press('ArrowRight');
    await expect(tabs.nth(0)).toHaveAttribute('aria-selected', 'true');
    await expect(tabs.nth(0)).toBeFocused();
    await page.keyboard.press('End');
    await expect(tabs.nth(2)).toHaveAttribute('aria-selected', 'true');
    await page.keyboard.press('Home');
    await expect(tabs.nth(0)).toHaveAttribute('aria-selected', 'true');

    await tabs.nth(1).click();
    await expect(tabs.nth(1)).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('#ph')).toHaveText('B: full MTL signature per message');
    await expect(page.locator('#gt')).toContainText('mean');
    const visible = await page.evaluate(() =>
      ['gA', 'gB', 'gC'].filter(
        (id) => (document.getElementById(id) as HTMLElement).style.display !== 'none',
      ),
    );
    expect(visible).toEqual(['gB']);
  });

  test('every fact slot renders its generated display and the stale strings are absent', async ({
    page,
  }) => {
    await page.goto('/', { waitUntil: 'load' });
    // The ledger's count-up starts at zero and settles at the fact value.
    await page.locator('#results').scrollIntoViewIfNeeded();
    await page.waitForTimeout(1800);
    const slots = await page.locator('[data-fact-id]').evaluateAll((elements) =>
      elements.map((element) => ({
        id: element.getAttribute('data-fact-id') ?? '',
        text: element.textContent?.trim() ?? '',
      })),
    );
    expect(slots.length).toBeGreaterThan(10);
    for (const slot of slots) {
      const fact = facts.facts.find((entry) => entry.id === slot.id);
      expect(fact, slot.id).toBeDefined();
      const candidates =
        fact?.kind === 'count_of'
          ? [fact.display, fact.display.split(' ')[0] ?? '']
          : [fact?.display ?? ''];
      expect(candidates, slot.id).toContain(slot.text);
    }

    const html = await page.content();
    for (const stale of ['9,405', '215.3', '771']) {
      expect(html, stale).not.toContain(stale);
    }
  });

  test('every internal link on the home route answers', async ({ page }) => {
    await page.goto('/', { waitUntil: 'load' });
    const hrefs = await page
      .locator('a[href^="/"]')
      .evaluateAll((links) => [
        ...new Set(links.map((link) => (link as HTMLAnchorElement).getAttribute('href') ?? '')),
      ]);
    expect(hrefs).toEqual(expect.arrayContaining(['/tui', '/verify', '/report', '/reproduce']));
    for (const href of hrefs) {
      const path = href.split('#')[0] ?? '';
      if (path === '') continue;
      const response = await page.request.get(path);
      expect(response.status(), href).toBe(200);
    }
  });

  test('reduced motion still opens the explorer and shows its final state', async ({ browser }) => {
    const context = await browser.newContext({ reducedMotion: 'reduce' });
    const page = await context.newPage();
    await open(page);
    await page.locator('#cp .c').nth(3).click();
    await expect(page.locator('#lv .l')).toHaveCount(4);
    await page.locator('#lv .l').nth(2).click();
    await expect(page.locator('#dt h3')).toHaveText('Ladder cache');
    await expect(page.locator('#dt')).toBeVisible();
    await expect(page.locator('#sv .pd')).toHaveCount(0);
    await expect(page.locator('.arch circle')).toHaveCount(0);
    await context.close();
  });

  test('has no axe violations with the dashboard section present', async ({ browser }) => {
    test.setTimeout(60_000);
    // axe is injected inline, so the audit runs in a context that bypasses the
    // page's own script policy; the page under test is the production build.
    const context = await browser.newContext({ bypassCSP: true });
    const page = await context.newPage();
    await open(page);
    // Reveal the ladder before the audit: its pane is opacity 0 until the
    // section intersects, and an element the reader has not reached is not a
    // meaningful contrast sample.
    await page.locator('#ladder').scrollIntoViewIfNeeded();
    await page.waitForTimeout(1200);
    await page.locator('#dashboard').scrollIntoViewIfNeeded();
    await page.waitForTimeout(600);
    await expectNoAxeViolations(page);
    await context.close();
  });
});
