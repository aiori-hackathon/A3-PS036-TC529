import { defineConfig } from '@playwright/test';

/**
 * The offline proof run. The static server is started by the demo runner from
 * the built `site/dist`, so this config deliberately has no `webServer`: the
 * browser must reach only the loopback address the caller names. Every route
 * is loaded with all non-localhost requests blocked, which fails on a font,
 * script or stylesheet fetched from anywhere else.
 */
export default defineConfig({
  testDir: './e2e',
  testMatch: 'offline.spec.ts',
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: 'list',
  use: {
    baseURL: process.env.CLIMB_OFFLINE_BASE_URL ?? 'http://127.0.0.1:8080',
    trace: 'retain-on-failure',
  },
});
