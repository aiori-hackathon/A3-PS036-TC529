import { defineConfig } from 'vitest/config';

/**
 * Unit tests live in `tests/`. The browser tests in `e2e/` are driven by
 * Playwright, which owns its own runner, so Vitest must not collect them.
 */
export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    hookTimeout: 180_000,
    testTimeout: 180_000,
  },
});
