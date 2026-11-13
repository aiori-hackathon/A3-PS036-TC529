import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  UnknownFactError,
  allFacts,
  formatFactValue,
  formatPercent,
  formatRatio,
  getFact,
} from '../src/lib/facts';
import { collectSources, findTypedNumbers } from '../src/lib/guard';
import { centreOf, rippleGeometry } from '../src/lib/ripple';
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

describe('ripple geometry', () => {
  it('anchors the ring at the pointer', () => {
    const geometry = rippleGeometry(400, 240, 120, 60);
    expect(geometry.left).toBe(120);
    expect(geometry.top).toBe(60);
  });

  it('reaches the farthest corner once the ring has grown by its scale', () => {
    const geometry = rippleGeometry(320, 480, 0, 0);
    const finalRadius = (geometry.diameter / 2) * 1.8;
    expect(finalRadius).toBeCloseTo(Math.hypot(320, 480), 6);
  });

  it('uses the same diameter for points mirrored across the centre', () => {
    const near = rippleGeometry(500, 300, 40, 260);
    const far = rippleGeometry(500, 300, 460, 40);
    expect(near.diameter).toBeCloseTo(far.diameter, 6);
  });

  it('returns the exact centre of a surface', () => {
    expect(centreOf(640, 360)).toEqual({ x: 320, y: 180 });
  });
});

const generatedDir = join(siteRoot, 'src/data/generated');

interface LadderStep {
  level: number;
  index: number;
  side: string;
}

interface LadderLeaf {
  index: number;
  condensed_bytes: number;
  auth_path: LadderStep[];
}

interface LadderTree {
  leaf_count: number;
  levels: number;
  ladder_bytes: number;
  signed_ladder_bytes: number;
  plain_signature_bytes: number;
  condensed_total: number;
  condensed_min: number;
  condensed_max: number;
  nodes: { level: number; index: number }[];
  leaves: LadderLeaf[];
}

function loadGenerated(name: string): Record<string, unknown> {
  return JSON.parse(readFileSync(join(generatedDir, `${name}.json`), 'utf8')) as Record<
    string,
    unknown
  >;
}

describe('facts', () => {
  it('resolves every fact id the copy deck names', () => {
    const copy = readFileSync(join(siteRoot, '../docs/site/copy.md'), 'utf8');
    const ids = [...new Set([...copy.matchAll(/F-[a-z0-9-]+/g)].map((match) => match[0]))];
    expect(ids.length).toBeGreaterThanOrEqual(16);
    for (const id of ids) {
      expect(() => getFact(id), id).not.toThrow();
    }
  });

  it('throws at build time for an unknown fact id', () => {
    expect(() => getFact('F-not-a-fact')).toThrow(UnknownFactError);
    expect(() => getFact('F-not-a-fact')).toThrow(/unknown fact id/);
  });

  it('formats every registered fact to its generated display value', () => {
    expect(allFacts()).toHaveLength(16);
    for (const fact of allFacts()) {
      expect(formatFactValue(fact), fact.id).toBe(fact.display);
    }
  });
});

describe('fact formatter', () => {
  it('matches the terminal dashboard conventions', () => {
    expect(formatRatio(9434.44263867255)).toBe('9,434');
    expect(formatRatio(77.33947336502698)).toBe('77.34');
    expect(formatRatio(1.6281767047921734)).toBe('1.628');
    expect(formatPercent(96.50795315682281)).toBe('96.508');
    expect(formatPercent(97.30747196988297)).toBe('97.307');
  });

  it('marks an extrapolated value with the approximation sign', () => {
    expect(
      formatFactValue({ kind: 'ratio', raw: 992.4, denominator: null, class: 'extrapolated' }),
    ).toBe('≈992.4');
    expect(
      formatFactValue({ kind: 'ratio', raw: 992.4, denominator: null, class: 'derived' }),
    ).toBe('992.4');
  });
});

describe('numeric-literal guard', () => {
  it('accepts the committed site source', () => {
    const sources = collectSources(join(siteRoot, 'src'));
    expect(sources.length).toBeGreaterThan(0);
    expect(findTypedNumbers(sources)).toEqual([]);
  });

  it('rejects a typed wire size and a typed percentage', () => {
    const injected = 'Wire is 2,743,352 B, a 96.508 % reduction.';
    const violations = findTypedNumbers([{ path: 'injected.astro', text: injected }]);
    expect(violations.map((violation) => violation.match)).toEqual(['2,743,352 B', '96.508 %']);
  });

  it('allows years, versions and CSS values', () => {
    const allowed = [
      'Copyright 2026.',
      'Astro 7.4.4 is pinned.',
      'padding: 1rem; border-radius: 2px;',
      '<div style="width: 100%">x</div>',
      'transition-duration: 250ms;',
    ].join('\n');
    expect(findTypedNumbers([{ path: 'allowed.astro', text: allowed }])).toEqual([]);
  });

  it('fails the command when a typed number is injected', () => {
    const directory = mkdtempSync(join(tmpdir(), 'climb-guard-'));
    try {
      writeFileSync(join(directory, 'injected.astro'), 'The ladder is 2,743,352 B.\n');
      expect(() =>
        execFileSync(process.execPath, [join(siteRoot, 'src/lib/guard.ts'), directory], {
          stdio: 'pipe',
        }),
      ).toThrow();
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});

describe('ladder data', () => {
  const ladder = loadGenerated('ladder') as { trees: LadderTree[] };

  for (const leafCount of [8, 16]) {
    it(`carries a complete ${leafCount}-leaf tree and every authentication path`, () => {
      const tree = ladder.trees.find((candidate) => candidate.leaf_count === leafCount);
      expect(tree).toBeDefined();
      if (tree === undefined) {
        return;
      }
      const levels = Math.log2(leafCount);
      expect(tree.levels).toBe(levels);
      expect(tree.nodes).toHaveLength(2 * leafCount - 1);
      expect(tree.leaves).toHaveLength(leafCount);
      const nodes = new Set(tree.nodes.map((node) => `${node.level}:${node.index}`));
      for (const leaf of tree.leaves) {
        expect(leaf.auth_path).toHaveLength(levels);
        let node = leaf.index;
        for (const step of leaf.auth_path) {
          expect(nodes.has(`${step.level}:${step.index}`)).toBe(true);
          expect(step.index).toBe(node ^ 1);
          expect(step.side).toBe(node % 2 === 0 ? 'right' : 'left');
          node >>= 1;
        }
      }
      const lengths = tree.leaves.map((leaf) => leaf.condensed_bytes);
      expect(Math.min(...lengths)).toBe(tree.condensed_min);
      expect(Math.max(...lengths)).toBe(tree.condensed_max);
      expect(lengths.reduce((sum, value) => sum + value, 0)).toBe(tree.condensed_total);
      expect(tree.signed_ladder_bytes).toBe(tree.ladder_bytes + 4 + tree.plain_signature_bytes);
    });
  }
});

describe('generated site data', () => {
  it('ships six sealed documents', () => {
    for (const name of ['facts', 'claims', 'provenance', 'series', 'ladder', 'commits']) {
      const document = loadGenerated(name) as Record<string, unknown> & { content_sha256: string };
      expect(document.content_sha256, name).toMatch(/^[0-9a-f]{64}$/);
      expect(document.schema_version, name).toBe(1);
    }
  });

  it('records the commit, rustc, submodule pins and a host fingerprint', () => {
    const provenance = loadGenerated('provenance') as {
      commit: string;
      rustc: string;
      date: string;
      submodules: { path: string; sha: string }[];
      host_fingerprint_sha256: string;
    };
    expect(provenance.commit).toMatch(/^[0-9a-f]{40}$/);
    expect(provenance.rustc).toContain('rustc');
    expect(provenance.date).toMatch(/^\d{4}-\d{2}-\d{2}T/);
    expect(provenance.submodules.length).toBeGreaterThan(0);
    expect(provenance.host_fingerprint_sha256).toMatch(/^[0-9a-f]{64}$/);
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
