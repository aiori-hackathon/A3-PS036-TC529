// @ts-check
import { defineConfig } from 'astro/config';
import tailwindcss from '@tailwindcss/vite';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { TUI_DATA_FILES, tuiManifestFrom } from './src/lib/tui.ts';

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
      injectRoute({
        pattern: '/_dev/ui',
        entrypoint: './src/dev/ui.astro',
      });
    },
  },
};

const repoRoot = fileURLToPath(new URL('..', import.meta.url));
/** The browser module's build output, written by `trunk build`. */
const moduleDir = join(repoRoot, 'crates', 'climb-web', 'dist');
/** The committed recordings the dashboard reads at run time. */
const dataDir = join(repoRoot, 'docs', 'sample-output');

const JSON_TYPE = 'application/json; charset=utf-8';

/** @type {Record<string, string>} */
const CONTENT_TYPES = {
  '.js': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': JSON_TYPE,
};

/**
 * The browser dashboard is a build artifact, not a source file: it lives in
 * `crates/climb-web/dist` and is never copied into `public/`. This integration
 * serves it (and the recordings) from that directory while the dev server
 * runs, and emits both beside the built pages. When the module has not been
 * built the hooks stay quiet — the page then shows its own build state rather
 * than a stack trace, and a checkout without a wasm toolchain still builds.
 *
 * @type {import('astro').AstroIntegration}
 */
const dashboard = {
  name: 'climb-dashboard',
  hooks: {
    'astro:server:setup': ({ server }) => {
      server.middlewares.use((request, response, next) => {
        const path = new URL(request.url ?? '/', 'http://localhost').pathname;

        if (path === '/tui/manifest.json') {
          const manifest = tuiManifestFrom(existsSync(moduleDir) ? readdirSync(moduleDir) : []);
          if (manifest === null) {
            response.statusCode = 404;
            response.setHeader('content-type', JSON_TYPE);
            response.end('{"built":false}');
            return;
          }
          response.setHeader('content-type', JSON_TYPE);
          response.setHeader('cache-control', 'no-store');
          response.end(JSON.stringify(manifest));
          return;
        }

        const name = path.startsWith('/tui/')
          ? basename(path)
          : path.startsWith('/data/')
            ? basename(path)
            : null;
        if (name === null) {
          next();
          return;
        }

        const source = path.startsWith('/tui/')
          ? join(moduleDir, name)
          : TUI_DATA_FILES.some((file) => file === name)
            ? join(dataDir, name)
            : null;
        if (source === null || !existsSync(source)) {
          next();
          return;
        }

        response.setHeader('content-type', CONTENT_TYPES[extname(source)] ?? 'application/octet-stream');
        response.setHeader('cache-control', 'no-store');
        response.end(readFileSync(source));
      });
    },
    'astro:build:done': ({ dir, logger }) => {
      const out = fileURLToPath(dir);
      const manifest = tuiManifestFrom(existsSync(moduleDir) ? readdirSync(moduleDir) : []);

      if (manifest === null) {
        logger.warn(
          'dashboard window: crates/climb-web/dist has no build; the /tui window will show its build state',
        );
      } else {
        mkdirSync(join(out, 'tui'), { recursive: true });
        for (const asset of [basename(manifest.entry), basename(manifest.wasm)]) {
          copyFileSync(join(moduleDir, asset), join(out, 'tui', asset));
        }
        writeFileSync(join(out, 'tui', 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
        logger.info(`dashboard window: published ${basename(manifest.entry)}`);
      }

      mkdirSync(join(out, 'data'), { recursive: true });
      for (const file of TUI_DATA_FILES) {
        copyFileSync(join(dataDir, file), join(out, 'data', file));
      }
      logger.info(`dashboard data: published ${TUI_DATA_FILES.length} documents`);
    },
  },
};

export default defineConfig({
  site: 'http://localhost:4321',
  integrations: [devRoutes, dashboard],
  devToolbar: { enabled: false },
  vite: {
    plugins: [tailwindcss()],
  },
});
