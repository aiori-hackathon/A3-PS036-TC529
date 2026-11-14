import { expect, test } from '@playwright/test';

import { settle } from './a11y';

test('the tui route loads the window or states why it is unavailable', async ({ page }) => {
  await page.goto('/tui');
  await settle(page);

  await expect(
    page.getByRole('heading', { level: 1, name: 'The dashboard, in this tab.' }),
  ).toBeVisible();

  const stage = page.locator('[data-tui-stage]');
  await expect(stage).toBeVisible();
  await expect
    .poll(async () => stage.getAttribute('data-tui-state'), { timeout: 25_000 })
    .not.toBe('loading');

  const state = await stage.getAttribute('data-tui-state');
  expect(['ready', 'unavailable', 'failed']).toContain(state);

  const manifest = await page.request.get('/tui/manifest.json');
  expect([200, 404]).toContain(manifest.status());
  if (manifest.status() === 200) {
    // This checkout built the WebAssembly module, so the window must run it
    // rather than fall back to the absent-build state.
    expect(state).toBe('ready');
    await expect(stage).toHaveAttribute('aria-busy', 'false');
  } else {
    expect(state).toBe('unavailable');
    await expect(page.locator('[data-tui-fault]')).toBeVisible();
    await expect(page.locator('[data-tui-fault-body]')).toContainText(
      'was built without the WebAssembly window',
    );
  }
});
