import { describe, expect, it } from 'vitest';

import {
  CLAIM_CLASSES,
  CLAIM_SIZES,
  claimCounts,
  claimPointerHref,
  claimSearchText,
  claimStatus,
  classLabel,
  claims,
  DEFAULT_FILTERS,
  filterClaims,
  matchesFilters,
  presentClasses,
  presentSizes,
  registryStatus,
  rowMatchesFilters,
  sizeLabel,
  type Claim,
  type ClaimFilters,
} from '../src/lib/verify';

function filters(overrides: Partial<ClaimFilters> = {}): ClaimFilters {
  return { ...DEFAULT_FILTERS, ...overrides };
}

describe('claims registry', () => {
  it('counts the committed registry as fully resolved', () => {
    const counts = claimCounts(claims);
    expect(counts.total).toBe(852);
    expect(counts.pass).toBe(counts.total);
    expect(counts.fail).toBe(0);
    expect(registryStatus(claims)).toBe('clean');
  });

  it('classifies every claim and only uses the published size names', () => {
    expect(presentClasses(claims)).toEqual([
      'measured',
      'derived',
      'analytical',
      'extrapolated',
      'generated',
    ]);
    expect(presentSizes(claims)).toEqual([...CLAIM_SIZES]);
    for (const claim of claims) {
      expect(claim.class, claim.pointer).not.toBeNull();
      expect(CLAIM_CLASSES as readonly string[]).toContain(claim.class);
      expect([...CLAIM_SIZES, null]).toContain(claim.size);
    }
  });

  it('reports an empty or failing registry without pretending it passed', () => {
    expect(registryStatus([])).toBe('empty');
    const first = claims[0];
    if (first === undefined) throw new Error('the registry is empty');
    const failed: Claim = { ...first, verified: false };
    expect(registryStatus([failed])).toBe('failing');
    expect(claimStatus(failed)).toBe('fail');
  });

  it('reports provenance-style labels for unknown values', () => {
    expect(classLabel('measured')).toBe('Measured');
    expect(classLabel('constant')).toBe('Constant');
    expect(classLabel('generated')).toBe('Generated');
    expect(classLabel(null)).toBe('Unclassified');
    expect(classLabel('mystery')).toBe('mystery');
    expect(sizeLabel('OneK')).toBe('OneK');
    expect(sizeLabel(null)).toBe('—');
    expect(sizeLabel('Mystery')).toBe('Mystery');
  });

  it('builds a results-artifact link from the JSON pointer', () => {
    expect(claimPointerHref('/derived/udp_fit/rows/0/messages')).toBe(
      '/evidence/climb-results.json#/derived/udp_fit/rows/0/messages',
    );
    expect(claimPointerHref('/comparisons/2/wire_reduction_c_vs_a_percent')).toContain(
      '/evidence/climb-results.json#/comparisons/',
    );
  });
});

describe('claim filtering', () => {
  it('matches the search box against id, pointer, value, class and size', () => {
    const claim = claims.find((row) => row.pointer.includes('/udp_fit/rows/0/'));
    expect(claim).toBeDefined();
    if (claim === undefined) return;
    const text = claimSearchText(claim);
    expect(text).toContain(claim.id.toLowerCase());
    expect(text).toContain(claim.pointer.toLowerCase());
    expect(text).toContain(claim.class ?? '');
    expect(text).toBe(text.toLowerCase());
  });

  it('filters by pointer substring and keeps only matching rows', () => {
    const selected = filterClaims(claims, filters({ query: '/udp_fit/' }));
    expect(selected.length).toBeGreaterThan(0);
    expect(selected.length).toBeLessThan(claims.length);
    expect(selected.every((claim) => claim.pointer.includes('/udp_fit/'))).toBe(true);
  });

  it('filters by result, class and size independently', () => {
    expect(filterClaims(claims, filters({ status: 'fail' }))).toEqual([]);
    expect(filterClaims(claims, filters({ status: 'pass' }))).toHaveLength(claims.length);

    const analytical = filterClaims(claims, filters({ className: 'analytical' }));
    expect(analytical.length).toBeGreaterThan(0);
    expect(analytical.every((claim) => claim.class === 'analytical')).toBe(true);

    const large = filterClaims(claims, filters({ size: 'Large' }));
    expect(large.length).toBeGreaterThan(0);
    expect(large.every((claim) => claim.size === 'Large')).toBe(true);
  });

  it('combines every filter and surfaces nothing when they disagree', () => {
    const combined = filterClaims(
      claims,
      filters({ query: 'udp', className: 'analytical', size: 'Small' }),
    );
    expect(combined.length).toBeGreaterThan(0);
    expect(
      combined.every(
        (claim) =>
          claim.class === 'analytical' &&
          claim.size === 'Small' &&
          claimSearchText(claim).includes('udp'),
      ),
    ).toBe(true);

    expect(
      filterClaims(claims, filters({ query: 'udp', className: 'measured', size: 'Large' })),
    ).toEqual([]);
  });

  it('treats an empty or padded query as no query', () => {
    expect(filterClaims(claims, filters({ query: '   ' }))).toHaveLength(claims.length);
  });

  it('applies the same rule to a row as to a claim', () => {
    const claim = claims[0];
    if (claim === undefined) throw new Error('the registry is empty');
    const row = {
      search: claimSearchText(claim),
      status: claimStatus(claim),
      className: claim.class ?? 'unknown',
      size: claim.size ?? 'none',
    };
    for (const active of [
      filters(),
      filters({ query: claim.id }),
      filters({ status: 'pass' }),
      filters({ status: 'fail' }),
      filters({ className: claim.class ?? 'all' }),
      filters({ size: claim.size ?? 'all' }),
      filters({ query: 'no-such-claim-anywhere' }),
    ]) {
      expect(rowMatchesFilters(row, active)).toBe(matchesFilters(claim, active));
    }
  });
});
