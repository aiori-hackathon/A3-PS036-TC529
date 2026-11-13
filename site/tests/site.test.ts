import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

import { contrastRatio, palette, usedPairs } from '../src/lib/tokens';

const siteRoot = fileURLToPath(new URL('..', import.meta.url));
const dist = join(siteRoot, 'dist');
const tokensCss = readFileSync(join(siteRoot, 'src/styles/tokens.css'), 'utf8');

function walk(root: string): string[] {
  const entries: string[] = [];
  for (const name of readdirSync(root)) {
    const path = join(root, name);
    if (statSync(path).isDirectory()) {
      entries.push(...walk(path));
    } else {
      entries.push(path);
    }
  }
  return entries;
}

const declaredColours = new Map(
  [...tokensCss.matchAll(/--color-([a-z-]+):\s*(#[0-9a-fA-F]{6});/g)].map((match) => {
    const name = match[1];
    const value = match[2];
    if (name === undefined || value === undefined) {
      throw new Error(`could not parse a colour token from: ${match[0]}`);
    }
    return [name, value.toLowerCase()];
  }),
);

describe('tokens', () => {
  it('declares every palette token in the stylesheet with the same value', () => {
    for (const [name, value] of Object.entries(palette)) {
      expect(declaredColours.get(name), `${name} is missing from tokens.css`).toBe(
        value.toLowerCase(),
      );
    }
    expect(declaredColours.size).toBe(Object.keys(palette).length);
  });

  it('meets the contrast target of every used foreground and background pair', () => {
    for (const pair of usedPairs) {
      const ratio = contrastRatio(palette[pair.foreground], palette[pair.background]);
      expect(
        ratio,
        `${pair.foreground} on ${pair.background} (${pair.usage})`,
      ).toBeGreaterThanOrEqual(pair.minimum);
    }
  });

  it('keeps the decorative slate out of every text pair', () => {
    const ratio = contrastRatio(palette.slate, palette.ink);
    expect(ratio).toBeLessThan(3);
    expect(usedPairs.some((pair) => pair.foreground === 'slate')).toBe(false);
  });

  it('declares swap and size-adjusted fallback metrics for every face', () => {
    expect(tokensCss.match(/font-display:\s*swap/g)).toHaveLength(4);
    expect(tokensCss).toContain('size-adjust: 111.12%');
    expect(tokensCss).toContain('size-adjust: 100.32%');
    expect(tokensCss).toContain('ascent-override: 99%');
    expect(tokensCss).toContain('ascent-override: 100.5%');
  });

  it('ships every woff2 subset under the 120 KB budget', () => {
    const fonts = readdirSync(join(siteRoot, 'public/fonts')).filter((name) =>
      name.endsWith('.woff2'),
    );
    expect(fonts).toHaveLength(4);
    const total = fonts.reduce(
      (sum, name) => sum + statSync(join(siteRoot, 'public/fonts', name)).size,
      0,
    );
    expect(total).toBeLessThanOrEqual(120 * 1024);
  });
});

describe('production build', () => {
  beforeAll(() => {
    execFileSync('npm', ['run', 'build'], { cwd: siteRoot, stdio: 'pipe' });
  }, 180_000);

  it('contains no development-only route', () => {
    expect(existsSync(join(dist, '_dev'))).toBe(false);
    const devPaths = walk(dist)
      .map((path) => relative(dist, path))
      .filter((path) => path.includes('_dev'));
    expect(devPaths).toEqual([]);
  });

  it('emits the home route with no inline script', () => {
    const html = readFileSync(join(dist, 'index.html'), 'utf8');
    expect(html).toContain('Post-quantum DNSSEC, without the weight.');
    expect(html).not.toMatch(/<script\b/i);
  });
});
