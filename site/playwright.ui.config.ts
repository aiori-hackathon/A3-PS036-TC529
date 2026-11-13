import { defineConfig } from '@playwright/test';

/**
 * The primitive gallery is a development-only route, so this run needs the dev
 * server rather than the production preview. It is a separate config so the
 * production smoke suite keeps testing the built output.
 */
export default defineConfig({
  testDir: './ui-tests',
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:4322',
    trace: 'retain-on-failure',
    reducedMotion: 'no-preference',
  },
  webServer: {
    // The dev CLI detaches into a background daemon when it detects an
    // unattended shell, which would make this wrapper exit before the server
    // is ready. Pin it to the foreground for the duration of the run.
    command: 'ASTRO_DEV_BACKGROUND=0 npm run dev -- --port 4322',
    url: 'http://localhost:4322/_dev/ui',
    reuseExistingServer: true,
    timeout: 180_000,
  },
});
