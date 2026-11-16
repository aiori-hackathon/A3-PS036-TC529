import { expect, test, type Page } from '@playwright/test';

/**
 * The demonstration has to work with no network. This run loads every route
 * from the loopback static server while aborting every request whose hostname
 * is not `127.0.0.1` or `localhost`. A font, script, stylesheet or image that
 * escapes would appear as a blocked or foreign request, and any console error
 * or failed request fails the test.
 */

const ROUTES = ['/', '/tui/', '/verify/', '/report/', '/reproduce/'] as const;
const LOCAL_HOSTS = new Set(['127.0.0.1', 'localhost']);

/**
 * The report route embeds the self-contained report in a deliberately
 * script-less sandbox (`sandbox=""`). Chromium emits one notice for the
 * report's JSON claims block even though that block cannot execute. The notice
 * is browser-enforced, names the report document and is the only console
 * message this run tolerates; every page-originated error still fails.
 */
const SANDBOX_NOTICE =
  /^Blocked script execution in '[^']*\/evidence\/report\.html' because the document's frame is sandboxed and the 'allow-scripts' permission is not set\.$/;

function isLocal(raw: string): boolean {
  const url = new URL(raw);
  return LOCAL_HOSTS.has(url.hostname);
}

async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await document.fonts.ready;
  });
}

test('every route loads with all non-localhost requests blocked', async ({ page }) => {
  const blocked: string[] = [];
  const foreign: string[] = [];
  const failed: string[] = [];
  const consoleErrors: string[] = [];
  const sandboxNotices: string[] = [];

  await page.route('**/*', async (route) => {
    const url = route.request().url();
    if (!isLocal(url)) {
      blocked.push(url);
      await route.abort('blockedbyclient');
      return;
    }
    await route.continue();
  });
  page.on('request', (request) => {
    const url = request.url();
    if (url.startsWith('http') && !isLocal(url)) {
      foreign.push(url);
    }
  });
  page.on('requestfailed', (request) => {
    const url = request.url();
    if (url.startsWith('http') && !isLocal(url)) {
      blocked.push(url);
      return;
    }
    failed.push(`${request.method()} ${url} (${request.failure()?.errorText ?? 'failed'})`);
  });
  page.on('console', (message) => {
    if (message.type() !== 'error') {
      return;
    }
    const text = message.text();
    if (SANDBOX_NOTICE.test(text)) {
      sandboxNotices.push(text);
      return;
    }
    consoleErrors.push(text);
  });
  page.on('pageerror', (error) => {
    consoleErrors.push(error.message);
  });

  for (const route of ROUTES) {
    const response = await page.goto(route, { waitUntil: 'load' });
    expect(response, route).not.toBeNull();
    expect(response?.status(), route).toBe(200);
    expect(response?.ok(), route).toBe(true);
    await settle(page);

    if (route === '/tui/') {
      const stage = page.locator('[data-tui-stage]');
      await expect(stage).toBeVisible();
      await expect
        .poll(async () => stage.getAttribute('data-tui-state'), { timeout: 30_000 })
        .not.toBe('loading');
      const manifest = await page.request.get('/tui/manifest.json');
      if (manifest.status() === 200) {
        await expect(stage).toHaveAttribute('data-tui-state', 'ready');
      }
    }

    const resources = await page.evaluate(() =>
      performance
        .getEntriesByType('resource')
        .map((entry) => entry.name)
        .filter((name) => name.startsWith('http')),
    );
    for (const resource of resources) {
      expect(isLocal(resource), `${route} loaded ${resource}`).toBe(true);
    }
  }

  expect(blocked, `blocked external requests: ${blocked.join(', ')}`).toEqual([]);
  expect(foreign, `foreign requests: ${foreign.join(', ')}`).toEqual([]);
  expect(failed, `failed requests: ${failed.join(' | ')}`).toEqual([]);
  expect(consoleErrors, `console errors: ${consoleErrors.join(' | ')}`).toEqual([]);
  // At most one notice per visit to the embedded report; anything more means
  // another sandboxed document appeared.
  expect(sandboxNotices.length).toBeLessThanOrEqual(1);
  if (sandboxNotices.length > 0) {
    console.log(`sandbox notices: ${sandboxNotices.length} (browser-enforced, report embed)`);
  }
});
