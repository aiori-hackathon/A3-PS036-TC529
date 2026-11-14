import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import {
  allCommandLines,
  commands,
  expectedOutputLines,
  reproductionModeLabel,
} from '../src/lib/reproduce';
import { claimCounts, claims } from '../src/lib/verify';

const siteRoot = fileURLToPath(new URL('..', import.meta.url));
const repositoryRoot = join(siteRoot, '..');

interface EvidenceShape {
  readonly report: {
    readonly check: {
      readonly agree: number;
      readonly drift: number;
      readonly missing: number;
      readonly registry_ok: boolean;
      readonly report_found: boolean;
      readonly problems: readonly string[];
    } | null;
  };
  readonly sums: { readonly rows: readonly unknown[] };
  readonly site_documents: readonly unknown[];
  readonly charts: readonly { readonly name: string }[];
  readonly reproduction_mode: string;
}

const evidence = JSON.parse(
  readFileSync(join(siteRoot, 'src/data/generated/evidence.json'), 'utf8'),
) as EvidenceShape;

const provenance = JSON.parse(
  readFileSync(join(siteRoot, 'src/data/generated/provenance.json'), 'utf8'),
) as { commit: string; content_identity: string };

describe('reproduction commands', () => {
  const readme = readFileSync(join(repositoryRoot, 'README.md'), 'utf8');
  const notes = readFileSync(join(repositoryRoot, 'docs/REPRODUCIBILITY.md'), 'utf8');

  it('quotes the README and the reproducibility notes verbatim', () => {
    for (const line of allCommandLines()) {
      const source = line.includes('sample-check') ? notes : readme;
      expect(source, line).toContain(line);
    }
  });

  it('keeps every quickstart route and both rebuild modes', () => {
    expect(commands.quickstart.map((group) => group.id)).toEqual(['clone', 'install', 'container']);
    expect(commands.regenerate.map((group) => group.id)).toEqual(['report-only', 'remeasure']);
    expect(commands.verify.lines).toEqual(['cargo xtask sample-check']);
  });

  it('offers exactly one command group per route and mode', () => {
    const ids = [
      commands.verify.id,
      ...commands.quickstart.map((group) => group.id),
      ...commands.regenerate.map((group) => group.id),
    ];
    expect(new Set(ids).size).toBe(ids.length);
    for (const group of [commands.verify, ...commands.quickstart, ...commands.regenerate]) {
      expect(group.lines.length).toBeGreaterThan(0);
      expect(group.note.trim()).not.toBe('');
    }
  });
});

describe('expected output', () => {
  const lines = expectedOutputLines({
    claims: claimCounts(claims),
    check: evidence.report.check,
    manifestRows: evidence.sums.rows.length,
    documents: evidence.site_documents.length,
    contentIdentity: provenance.content_identity,
    commitShort: provenance.commit.slice(0, 7),
  });

  it('prints the committed claim and manifest counts, not typed ones', () => {
    const counts = claimCounts(claims);
    expect(lines[0]).toBe(
      `site-data: wrote ${evidence.site_documents.length} document(s) to ./site/src/data/generated`,
    );
    expect(lines[1]).toBe(
      `site-check: ${evidence.site_documents.length} document(s) fresh; site/src has no typed benchmark number`,
    );
    expect(lines[2]).toBe(
      `sample-check: ${counts.total} claims, ${evidence.report.check?.agree} in report, ${evidence.report.check?.agree} agree, ${evidence.report.check?.drift} drift, ${evidence.report.check?.missing} missing`,
    );
    expect(lines[3]).toBe(`sample-check: ${evidence.sums.rows.length} manifest entries match`);
    expect(lines[4]).toContain(provenance.content_identity);
    expect(lines[5]).toContain(provenance.commit.slice(0, 7));
    expect(lines[6]).toBe('sample-check: committed sample pair is consistent');
  });

  it('states an absent report instead of claiming zero drift', () => {
    const absent = expectedOutputLines({
      claims: claimCounts(claims),
      check: null,
      manifestRows: 0,
      documents: 7,
      contentIdentity: provenance.content_identity,
      commitShort: provenance.commit.slice(0, 7),
    });
    expect(absent[2]).toContain('report not present');
    expect(absent[2]).toContain('check unavailable');
    expect(absent[2]).not.toContain('0 drift');
  });

  it('labels the recorded mode and never upgrades an unknown one', () => {
    expect(reproductionModeLabel('remeasure')).toBe('re-measured');
    expect(reproductionModeLabel('report-only')).toBe('report-only (measurements reused)');
    expect(reproductionModeLabel('unknown')).toBe('not recorded in this artifact');
    expect(['unknown', 'report-only', 'remeasure']).toContain(evidence.reproduction_mode);
  });
});

describe('report chart captions', () => {
  const copy = JSON.parse(readFileSync(join(siteRoot, 'src/data/report-charts.json'), 'utf8')) as {
    charts: { file: string; title: string; caption: string }[];
  };

  it('covers every generated chart exactly once', () => {
    const files = copy.charts.map((chart) => chart.file).sort();
    const published = evidence.charts.map((chart) => chart.name).sort();
    expect(files).toEqual(published);
    expect(new Set(files).size).toBe(files.length);
  });

  it('writes a title and a caption for each chart', () => {
    for (const chart of copy.charts) {
      expect(chart.title.trim().length, chart.file).toBeGreaterThan(0);
      expect(chart.caption.trim().length, chart.file).toBeGreaterThan(0);
      expect(chart.caption.length, chart.file).toBeLessThanOrEqual(240);
    }
  });
});
