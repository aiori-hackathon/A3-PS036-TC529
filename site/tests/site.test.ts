import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
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
import { gzipSync } from 'node:zlib';
import { beforeAll, describe, expect, it, vi } from 'vitest';

vi.mock('gsap/gsap-core', () => ({
  gsap: { registerPlugin: vi.fn(), set: vi.fn(), to: vi.fn() },
}));
vi.mock('gsap/CSSPlugin', () => ({ CSSPlugin: {} }));
vi.mock('gsap/ScrollTrigger', () => ({
  ScrollTrigger: { batch: vi.fn(), create: vi.fn(), refresh: vi.fn() },
}));

import {
  UnknownFactError,
  allFacts,
  formatFactValue,
  formatPercent,
  formatRatio,
  getFact,
} from '../src/lib/facts';
import {
  MOTION_BUDGET_BYTES,
  PRELOADER_HARD_LIMIT,
  PRELOADER_MAX,
  counterFrame,
  counterValue,
  progressScale,
} from '../src/lib/motion';
import { collectSources, findTypedNumbers } from '../src/lib/guard';
import {
  buildLimitItems,
  markdownSection,
  notMeasuredItems,
  renderInline,
  scopeSentence,
} from '../src/lib/limits';
import {
  byteRaceModel,
  condensedWidthPercent,
  dialogIdFor,
  formatByteCount,
  maxCondensedPaths,
} from '../src/lib/results';
import { centreOf, rippleGeometry } from '../src/lib/ripple';
import { contrastRatio } from '../src/lib/tokens';

const siteRoot = fileURLToPath(new URL('..', import.meta.url));
const dist = join(siteRoot, 'dist');

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

describe('tokens', () => {
  const climbCss = readFileSync(join(siteRoot, 'src/styles/climb.css'), 'utf8');
  const declared = new Map(
    [...climbCss.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)].map((match) => [
      match[1] ?? '',
      (match[2] ?? '').trim().toLowerCase(),
    ]),
  );

  // Brand v3, the values the prototype's `:root` declares. The skin carries
  // them in `climb.css`; this list is the test's copy of the design authority.
  const brand: Record<string, string> = {
    ink: '#07080a',
    ground: '#13161b',
    g1: '#1b1e25',
    g2: '#0f1217',
    well: '#0d0f13',
    cr: '#f2ede4',
    mut: '#a9a398',
    'ink-card': '#0b0b0d',
    'card-body': '#33333a',
    or: '#ff5400',
    or2: '#fe7f00',
    rose: '#ffa6a6',
    navy: '#06234b',
    mar: '#4d1111',
    ln: 'rgba(242,237,228,.13)',
    edge: 'rgba(242,237,228,.48)',
    'glass-scrim': 'rgba(7,8,10,.78)',
  };

  it('declares every palette token in the skin stylesheet with the same value', () => {
    for (const [name, value] of Object.entries(brand)) {
      expect(declared.get(name), `${name} is missing from climb.css`).toBe(value);
    }
  });

  it('meets the contrast target of every used foreground on the page ground', () => {
    const ground = brand.ground ?? '#13161b';
    for (const name of ['cr', 'mut', 'or', 'or2', 'rose'] as const) {
      expect(contrastRatio(brand[name] ?? ground, ground), name).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('declares swap and size-adjusted fallback metrics for both faces', () => {
    expect(climbCss.match(/font-display:swap/g)).toHaveLength(2);
    expect(climbCss).toContain('size-adjust:101.8%');
    expect(climbCss).toContain('size-adjust:100.3%');
    expect(climbCss).toContain('ascent-override:93%');
    expect(climbCss).toContain('ascent-override:100.5%');
  });

  it('ships the two variable latin faces under the 120 KB budget', () => {
    const fonts = readdirSync(join(siteRoot, 'public/fonts')).filter((name) =>
      name.endsWith('.woff2'),
    );
    expect(fonts.sort()).toEqual(['bricolage-grotesque-latin.woff2', 'geist-latin.woff2']);
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
  it('resolves every fact id the copy deck names, or finds it declared as a gap', () => {
    const copy = readFileSync(join(siteRoot, '../docs/site/copy.md'), 'utf8');
    const gaps = readFileSync(join(siteRoot, '../docs/site/fact-gaps.md'), 'utf8');
    const ids = [...new Set([...copy.matchAll(/F-[a-z0-9-]+/g)].map((match) => match[0]))];
    expect(ids.length).toBeGreaterThanOrEqual(16);
    for (const id of ids) {
      try {
        getFact(id);
      } catch {
        // The copy deck may name a number whose fact is a registered gap; the
        // gap register is the only place allowed to carry it until it lands.
        expect(gaps, id).toContain('`' + id + '`');
      }
    }
  });

  it('throws at build time for an unknown fact id', () => {
    expect(() => getFact('F-not-a-fact')).toThrow(UnknownFactError);
    expect(() => getFact('F-not-a-fact')).toThrow(/unknown fact id/);
  });

  it('formats every registered fact to its generated display value', () => {
    expect(allFacts()).toHaveLength(18);
    for (const fact of allFacts()) {
      expect(formatFactValue(fact), fact.id).toBe(fact.display);
    }
  });

  it('derives the hero byte facts from the recorded eight-leaf ladder tree', () => {
    const ladder = loadGenerated('ladder') as { trees: LadderTree[] };
    const tree = ladder.trees.find((candidate) => candidate.leaf_count === 8);
    expect(tree).toBeDefined();
    if (tree === undefined) {
      return;
    }
    const plain = getFact('F-plain-signature-bytes');
    const condensed = getFact('F-condensed-path-bytes');
    expect(plain.raw).toBe(tree.plain_signature_bytes);
    expect(plain.display).toBe(formatFactValue(plain));
    expect(condensed.raw).toBe(tree.leaves[0]?.condensed_bytes);
    expect(condensed.display).toBe(formatFactValue(condensed));
    expect(plain.unit).toBe('B');
    expect(condensed.unit).toBe('B');
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

describe('results interaction helpers', () => {
  // Test data: the two hero byte facts. The production values come from the
  // fact register, not from these literals.
  const fullBytes = 7856;
  const condensedBytes = 124;

  it('names a dialog from its fact id', () => {
    expect(dialogIdFor('F-wire-medium-c-vs-a')).toBe('evidence-f-wire-medium-c-vs-a');
    expect(dialogIdFor('F-udp-c-fit')).toBe('evidence-f-udp-c-fit');
  });

  it('never offers more condensed responses than the recorded bytes support', () => {
    expect(maxCondensedPaths(fullBytes, condensedBytes)).toBe(63);
    expect(maxCondensedPaths(condensedBytes, fullBytes)).toBe(1);
    expect(maxCondensedPaths(0, condensedBytes)).toBe(1);
    expect(maxCondensedPaths(Number.NaN, condensedBytes)).toBe(1);
  });

  it('clamps the condensed bar to its share of one full response', () => {
    expect(condensedWidthPercent(0, fullBytes)).toBe(0);
    expect(condensedWidthPercent(fullBytes, fullBytes)).toBe(100);
    expect(condensedWidthPercent(fullBytes * 2, fullBytes)).toBe(100);
    expect(condensedWidthPercent(condensedBytes, fullBytes)).toBeCloseTo(1.578, 3);
  });

  it('builds a byte-race frame from the two facts and clamps the slider', () => {
    const frame = byteRaceModel(fullBytes, condensedBytes, 63);
    expect(frame.maxPaths).toBe(63);
    expect(frame.paths).toBe(63);
    expect(frame.bytes).toBe(7812);
    expect(frame.widthPercent).toBeLessThan(100);
    expect(byteRaceModel(fullBytes, condensedBytes, 0).paths).toBe(1);
    expect(byteRaceModel(fullBytes, condensedBytes, 1000).paths).toBe(63);
    expect(formatByteCount(frame.bytes)).toBe('7,812');
  });
});

describe('honest limits sources', () => {
  const methodology = readFileSync(join(siteRoot, '..', 'docs/METHODOLOGY.md'), 'utf8');
  const copyDeck = readFileSync(join(siteRoot, '..', 'docs/site/copy.md'), 'utf8');

  it('reads the exact scope sentence from the copy deck footer', () => {
    expect(scopeSentence(copyDeck)).toBe(
      'This website presents recorded results and a client-side dashboard. The DNSSEC and PQC benchmark itself is not deployed on the Internet; the DNS testbed runs locally.',
    );
  });

  it('keeps every what-we-did-not-measure paragraph', () => {
    const items = notMeasuredItems(copyDeck);
    expect(items).toHaveLength(5);
    expect(items[0]).toMatch(/^No Internet latency\./);
    expect(items[4]).toMatch(/^No security property\./);
  });

  it('extracts a methodology section as prose and bullets', () => {
    const section = markdownSection(methodology, 'Environment pinning');
    expect(section).not.toBeNull();
    expect(section?.paragraphs[0]).toContain('Every measurement run records its environment');
    expect(section?.bullets).toHaveLength(4);
    expect(section?.bullets.join(' ')).toContain('rust-toolchain.toml');
    expect(markdownSection(methodology, 'Not a section')).toBeNull();
  });

  it('assembles the accordion in reading order', () => {
    const items = buildLimitItems(methodology, copyDeck);
    expect(items.map((item) => item.id)).toEqual([
      'scope',
      'environment',
      'sampling',
      'baselines',
      'not-measured',
    ]);
    expect(items[0]?.paragraphs[0]).toBe(scopeSentence(copyDeck));
    expect(items[4]?.bullets).toHaveLength(5);
  });

  it('escapes HTML before rendering the small inline subset', () => {
    expect(renderInline('`hash` <script> & **bold**')).toBe(
      '<code>hash</code> &lt;script&gt; &amp; <strong>bold</strong>',
    );
    expect(renderInline('[BASELINES.md](BASELINES.md)')).toBe('BASELINES.md');
  });
});

describe('team data', () => {
  const team = JSON.parse(readFileSync(join(siteRoot, 'src/data/team.json'), 'utf8')) as {
    members: { name: string; role: string }[];
  };
  const readme = readFileSync(join(siteRoot, '..', 'README.md'), 'utf8');

  it('matches the names and roles in the repository team table', () => {
    expect(team.members.map((member) => member.name)).toEqual([
      'Pratyay Mukherjee',
      'Mohit Kumar Yadav',
      'Dr. Maibam Devi',
    ]);
    for (const member of team.members) {
      expect(readme).toContain(`[${member.name}]`);
      expect(readme).toContain(`| ${member.role} |`);
    }
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
  it('ships seven sealed documents', () => {
    for (const name of [
      'facts',
      'claims',
      'provenance',
      'series',
      'ladder',
      'commits',
      'evidence',
    ]) {
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

interface EvidenceDocument {
  readonly published_base: string;
  readonly report: {
    readonly available: boolean;
    readonly path: string;
    readonly bytes: number | null;
    readonly sha256: string | null;
    readonly check: {
      readonly agree: number;
      readonly drift: number;
      readonly missing: number;
      readonly registry_ok: boolean;
      readonly report_found: boolean;
    } | null;
  };
  readonly charts: readonly {
    readonly name: string;
    readonly bytes: number;
    readonly sha256: string;
  }[];
  readonly sums: {
    readonly available: boolean;
    readonly file: string;
    readonly rows: readonly {
      readonly path: string;
      readonly bytes: number;
      readonly listed_sha256: string;
      readonly sha256: string;
      readonly matches: boolean;
    }[];
  };
  readonly reproduction_mode: string;
  readonly site_documents: readonly {
    readonly file: string;
    readonly bytes: number;
    readonly sha256: string;
  }[];
}

interface ClaimsDocument {
  readonly total: number;
  readonly passed: number;
  readonly claims: readonly {
    readonly id: string;
    readonly pointer: string;
    readonly class: string | null;
    readonly size: string | null;
  }[];
}

describe('evidence document', () => {
  const evidence = loadGenerated('evidence') as unknown as EvidenceDocument;
  const sampleDir = join(siteRoot, '../docs/sample-output');

  it('names the published directory and the report it embeds', () => {
    expect(evidence.published_base).toBe('/evidence');
    expect(evidence.report.available).toBe(true);
    expect(evidence.report.path).toBe('docs/sample-output/report.html');
    expect(evidence.report.bytes).toBe(statSync(join(sampleDir, 'report.html')).size);
    expect(evidence.report.sha256).toMatch(/^[0-9a-f]{64}$/);
  });

  it('records a report-versus-results check that agrees with the registry', () => {
    const claims = loadGenerated('claims') as unknown as ClaimsDocument;
    const check = evidence.report.check;
    expect(check).not.toBeNull();
    if (check === null) return;
    expect(check.report_found).toBe(true);
    expect(check.registry_ok).toBe(true);
    expect(check.agree).toBe(claims.total);
    expect(check.drift).toBe(0);
    expect(check.missing).toBe(0);
    expect(claims.passed).toBe(claims.total);
  });

  it('recomputes every manifest digest from the committed file', () => {
    expect(evidence.sums.available).toBe(true);
    expect(evidence.sums.file).toBe('SHA256SUMS');
    const lines = readFileSync(join(sampleDir, 'SHA256SUMS'), 'utf8')
      .split('\n')
      .filter((line) => line.trim() !== '');
    expect(evidence.sums.rows).toHaveLength(lines.length);
    for (const row of evidence.sums.rows) {
      const bytes = readFileSync(join(sampleDir, row.path));
      const digest = createHash('sha256').update(bytes).digest('hex');
      expect(row.bytes, row.path).toBe(bytes.length);
      expect(row.sha256, row.path).toBe(digest);
      expect(row.listed_sha256, row.path).toBe(digest);
      expect(row.matches, row.path).toBe(true);
    }
  });

  it('lists exactly the charts the report page captions', () => {
    const copy = JSON.parse(
      readFileSync(join(siteRoot, 'src/data/report-charts.json'), 'utf8'),
    ) as {
      charts: { file: string }[];
    };
    const captioned = copy.charts.map((chart) => chart.file).sort();
    const published = evidence.charts.map((chart) => chart.name).sort();
    expect(published).toEqual(captioned);
    for (const chart of evidence.charts) {
      const bytes = readFileSync(join(sampleDir, chart.name));
      expect(chart.bytes).toBe(bytes.length);
      expect(chart.sha256).toBe(createHash('sha256').update(bytes).digest('hex'));
    }
  });

  it('classifies every claim and names the size it belongs to', () => {
    const claims = loadGenerated('claims') as unknown as ClaimsDocument;
    expect(claims.claims).toHaveLength(claims.total);
    const sizes = new Set<string | null>();
    for (const claim of claims.claims) {
      expect(claim.class, claim.pointer).toMatch(
        /^(measured|derived|analytical|extrapolated|constant|generated)$/,
      );
      sizes.add(claim.size);
    }
    for (const size of sizes) {
      expect(['Small', 'OneK', 'Medium', 'Large', null]).toContain(size);
    }
  });

  it('names every generated document it describes', () => {
    const files = evidence.site_documents.map((document) => document.file).sort();
    expect(files).toEqual(
      [
        'claims.json',
        'commits.json',
        'facts.json',
        'ladder.json',
        'provenance.json',
        'series.json',
      ].sort(),
    );
    for (const document of evidence.site_documents) {
      if (document.file === 'evidence.json') continue;
      const bytes = readFileSync(join(generatedDir, document.file));
      expect(document.bytes, document.file).toBe(bytes.length);
      expect(document.sha256, document.file).toBe(createHash('sha256').update(bytes).digest('hex'));
    }
  });

  it('states the reproduction mode without inventing one', () => {
    expect(['unknown', 'report-only', 'remeasure']).toContain(evidence.reproduction_mode);
    const results = JSON.parse(readFileSync(join(sampleDir, 'climb-results.json'), 'utf8')) as {
      reproduction_mode?: string;
    };
    expect(evidence.reproduction_mode).toBe(results.reproduction_mode ?? 'unknown');
  });
});

describe('motion core', () => {
  const motionSource = readFileSync(join(siteRoot, 'src/lib/motion.ts'), 'utf8');

  it('eases a counter between its ends and clamps overshoot', () => {
    expect(counterValue(0, 100, 0)).toBe(0);
    expect(counterValue(0, 100, 1)).toBe(100);
    expect(counterValue(0, 100, 0.5)).toBeCloseTo(87.5, 6);
    expect(counterValue(10, 20, -3)).toBe(10);
    expect(counterValue(10, 20, 4)).toBe(20);
  });

  it('formats an intermediate frame with the final text conventions', () => {
    expect(counterFrame(2743352, '2,743,352')).toBe('2,743,352');
    expect(counterFrame(1234.5, '2,743,352')).toBe('1,235');
    expect(counterFrame(852, '852 of 852')).toBe('852 of 852');
    expect(counterFrame(950, '≈992.4')).toBe('≈950.0');
  });

  it('clamps a scroll progress value and rejects a non-finite one', () => {
    expect(progressScale(-1)).toBe(0);
    expect(progressScale(0.42)).toBe(0.42);
    expect(progressScale(2)).toBe(1);
    expect(progressScale(Number.NaN)).toBe(0);
  });

  it('caps the preloader below its CSS hard limit', () => {
    expect(PRELOADER_MAX).toBeLessThanOrEqual(PRELOADER_HARD_LIMIT);
    expect(PRELOADER_HARD_LIMIT).toBe(1500);
    const preloader = readFileSync(join(siteRoot, 'src/components/Preloader.astro'), 'utf8');
    expect(preloader).toContain('1500ms');
    expect(preloader).toMatch(/prefers-reduced-motion: reduce/);
  });

  it('keeps one frame loop and no scroll listener or layout read', () => {
    expect(motionSource.match(/requestAnimationFrame\(/g)).toHaveLength(3);
    expect(motionSource).toContain('visibilitychange');
    expect(motionSource).toContain('passive: true');
    expect(motionSource).not.toContain("addEventListener('scroll'");
    expect(motionSource).not.toContain('getBoundingClientRect');
  });

  it('loads the optional plugins outside the static chunk', () => {
    expect(motionSource).toContain("await import('lenis')");
    expect(motionSource).toContain("import('./hero')");
    expect(motionSource).toContain("import('gsap/CSSPlugin')");
    expect(motionSource).not.toMatch(/from ['"]lenis['"]/);
    expect(motionSource).not.toMatch(/from ['"]\.\/hero['"]/);
    expect(motionSource).not.toMatch(/from ['"]gsap\/CSSPlugin['"]/);
  });

  it('keeps the static motion core under its gzipped byte budget', async () => {
    const { build } = await import('vite');
    const built = (await build({
      root: siteRoot,
      logLevel: 'silent',
      build: {
        write: false,
        minify: 'oxc',
        reportCompressedSize: false,
        lib: {
          entry: join(siteRoot, 'src/lib/motion.ts'),
          formats: ['es'],
          fileName: 'motion',
        },
      },
    })) as unknown as
      | { output: { type: string; fileName: string; code?: string; isEntry?: boolean }[] }
      | { output: { type: string; fileName: string; code?: string; isEntry?: boolean }[] }[];

    const outputs = (Array.isArray(built) ? built : [built]).flatMap((result) => result.output);
    const chunks = outputs.filter(
      (
        output,
      ): output is {
        type: 'chunk';
        fileName: string;
        code: string;
        isEntry: boolean;
        imports: string[];
      } => output.type === 'chunk' && typeof output.code === 'string',
    );
    const entries = chunks.filter((chunk) => chunk.isEntry);
    expect(entries).toHaveLength(1);
    const entry = entries[0];
    if (entry === undefined) {
      throw new Error('the motion build produced no entry chunk');
    }

    // Follow the entry's static imports: a shared chunk is still downloaded
    // before the first frame and therefore counts against the budget.
    const byName = new Map(chunks.map((chunk) => [chunk.fileName, chunk]));
    const staticFiles = new Set<string>();
    const queue = [entry.fileName];
    while (queue.length > 0) {
      const name = queue.pop();
      if (name === undefined || staticFiles.has(name)) {
        continue;
      }
      const chunk = byName.get(name);
      if (chunk === undefined) {
        continue;
      }
      staticFiles.add(name);
      queue.push(...chunk.imports);
    }

    let staticBytes = 0;
    for (const chunk of chunks) {
      const gzip = gzipSync(Buffer.from(chunk.code)).length;
      const isStatic = staticFiles.has(chunk.fileName);
      staticBytes += isStatic ? gzip : 0;
      console.log(
        `${chunk.fileName}: ${chunk.code.length} B raw, ${gzip} B gzip, ${
          isStatic ? 'static' : 'lazy'
        }`,
      );
    }
    console.log(`static motion core: ${staticBytes} B gzip (budget ${MOTION_BUDGET_BYTES} B)`);
    expect(staticBytes).toBeLessThanOrEqual(MOTION_BUDGET_BYTES);
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

  it('emits the report, verify and reproduce routes with their evidence', () => {
    for (const route of ['report', 'verify', 'reproduce']) {
      expect(existsSync(join(dist, route, 'index.html')), route).toBe(true);
    }

    const report = readFileSync(join(dist, 'report/index.html'), 'utf8');
    expect(report).toContain('The full report');
    expect(report).toContain('src="/evidence/report.html"');
    expect(report).toContain('sandbox=""');
    expect([...report.matchAll(/data-report-chart="/g)]).toHaveLength(7);
    expect(report).toContain('href="/evidence/climb-results.json"');

    const claims = loadGenerated('claims') as unknown as ClaimsDocument;
    const verify = readFileSync(join(dist, 'verify/index.html'), 'utf8');
    expect(verify).toContain('Verify the numbers');
    expect(verify).toContain('data-claims-search');
    expect([...verify.matchAll(/data-claim-row\b/g)]).toHaveLength(claims.total);
    expect(verify).toContain('data-registry-status="clean"');
    expect(verify).toContain('cargo xtask sample-check');

    const reproduce = readFileSync(join(dist, 'reproduce/index.html'), 'utf8');
    expect(reproduce).toContain('Rebuild every number');
    expect(reproduce).toContain('data-expected-output');
    expect(reproduce).toContain('data-sha-checker');
    expect(reproduce).toContain('href="/evidence/SHA256SUMS"');
    expect(reproduce).toContain('href="/evidence/PROVENANCE.txt"');

    const evidence = loadGenerated('evidence') as unknown as EvidenceDocument;
    for (const row of evidence.sums.rows) {
      expect(existsSync(join(dist, 'evidence', row.path)), row.path).toBe(true);
    }
    expect(existsSync(join(dist, 'evidence', 'SHA256SUMS'))).toBe(true);
    expect(existsSync(join(dist, 'evidence', 'PROVENANCE.txt'))).toBe(true);
    expect(existsSync(join(dist, 'evidence', 'report.html'))).toBe(true);
  });

  it('links every internal destination that this build ships', () => {
    // Routes that a later phase builds. The list is explicit so a typo in a
    // built route cannot hide behind the same exemption.
    const pending = new Set(['/dashboard', '/calculator', '/method', '/commits']);
    const pages = walk(dist).filter((path) => path.endsWith('.html'));
    expect(pages.length).toBeGreaterThanOrEqual(5);

    const missing: string[] = [];
    for (const page of pages) {
      const html = readFileSync(page, 'utf8');
      for (const match of html.matchAll(/(?:href|src)="(\/[^"]*)"/g)) {
        const raw = match[1] ?? '';
        const pathname = decodeURI(raw.split('#')[0]?.split('?')[0] ?? '');
        if (pathname === '' || pending.has(pathname)) {
          continue;
        }
        const target = join(dist, pathname);
        const found =
          (existsSync(target) && statSync(target).isFile()) ||
          existsSync(join(target, 'index.html'));
        if (!found) {
          missing.push(`${relative(dist, page)} -> ${raw}`);
        }
      }
    }
    expect(missing).toEqual([]);
  });

  it('emits the home route with no executable inline script', () => {
    const html = readFileSync(join(dist, 'index.html'), 'utf8');
    expect(html).toContain('Post-quantum DNSSEC, measured.');
    const scripts = [...html.matchAll(/<script\b[^>]*>/gi)].map((match) => match[0]);
    const executable = scripts.filter((tag) => !/type="application\/json"/.test(tag));
    expect(executable).toHaveLength(1);
    expect(executable[0]).toContain('src=');
    expect(executable[0]).toContain('type="module"');
    // The one non-executable block is the explorer's data, not code.
    expect(scripts.filter((tag) => /type="application\/json"/.test(tag))).toHaveLength(1);
    expect(html).toContain('id="climb-data"');
    expect(html).not.toContain('<style');
  });

  it('emits the wordmark hero and both actions', () => {
    const html = readFileSync(join(dist, 'index.html'), 'utf8');
    expect(html).toContain('<h1>Post-quantum DNSSEC, measured.</h1>');
    expect(html).toContain('Explore the benchmark');
    expect(html).toContain('View on GitHub');
    expect(html).toContain('class="btn p"');
    expect(html).toContain('aria-label="CLIMB, Cryptographic Ladder Impact Measurement Benchmark"');
    expect(html).toContain('id="gc"');
    expect(html).toContain('id="ob"');
    expect(html).toContain('id="of"');
  });

  it('emits the eight sections, the four result facts, the ladder and the tab switch', () => {
    const html = readFileSync(join(dist, 'index.html'), 'utf8');
    for (const id of [
      'results',
      'capabilities',
      'ladder',
      'how',
      'architecture',
      'transparency',
      'team',
    ]) {
      expect(html).toContain(`id="${id}"`);
    }
    expect([...html.matchAll(/class="nm"/g)]).toHaveLength(4);
    for (const id of [
      'F-wire-medium-c-vs-a',
      'F-sign-medium-c-vs-a',
      'F-verify-medium-c-vs-a',
      'F-udp-c-fit',
    ]) {
      expect(html).toContain(`data-fact-id="${id}"`);
    }
    expect(html).toContain('role="tablist"');
    expect(html).toContain('id="gC"');
    expect(html).toContain('id="gt"');
    expect(html).toContain('id="dt"');
    expect(html).toContain('id="climb-data"');
    // The stale prototype values never reach the page.
    for (const stale of ['9,405', '215.3', '1.90', '771 of 771']) {
      expect(html, stale).not.toContain(stale);
    }
    expect(html).toContain(getFact('F-claims-count').display);
    expect(html).toContain('Pratyay Mukherjee');
    expect(html).toContain('Mohit Kumar Yadav');
    expect(html).toContain('Dr. Maibam Devi');
    expect(html).toContain(
      scopeSentence(readFileSync(join(siteRoot, '..', 'docs/site/copy.md'), 'utf8')),
    );
  });
});
