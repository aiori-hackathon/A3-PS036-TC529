import { expect, type Page } from '@playwright/test';
import axe from 'axe-core';

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

/** Run axe over the current document and fail with one line per violation. */
export async function expectNoAxeViolations(page: Page): Promise<void> {
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
}

/** Wait for the webfonts so a layout-driven check is not measured mid-swap. */
export async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
}
