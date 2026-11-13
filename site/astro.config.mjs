// @ts-check
import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';

/**
 * The token gallery is a development-only route. Astro ignores a page whose
 * path starts with an underscore, so the entrypoint lives outside `pages/` and
 * is injected only when the dev server starts. A production build therefore
 * has no route to emit.
 */
const devRoutes = {
  name: 'climb-dev-routes',
  hooks: {
    /** @param {{ command: string, injectRoute: (route: { pattern: string, entrypoint: string }) => void }} options */
    'astro:config:setup': ({ command, injectRoute }) => {
      if (command !== 'dev') return;
      injectRoute({
        pattern: '/_dev/tokens',
        entrypoint: './src/dev/tokens.astro',
      });
    },
  },
};

export default defineConfig({
  site: 'http://localhost:4321',
  integrations: [devRoutes],
  devToolbar: { enabled: false },
  vite: {
    plugins: [tailwindcss()],
  },
});
