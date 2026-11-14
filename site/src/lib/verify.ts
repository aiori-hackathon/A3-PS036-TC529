/**
 * Claims-registry filtering for `/verify`.
 *
 * The page is server-rendered in full so the table is complete without
 * JavaScript. This module only decides which rows stay visible once the
 * controls are used, and it is shared by the page script and the unit tests.
 */

import claimsFile from '../data/generated/claims.json';

export type ClaimStatus = 'pass' | 'fail';

export interface Claim {
  readonly id: string;
  readonly pointer: string;
  readonly value: unknown;
  readonly formatted: string;
  readonly verified: boolean;
  readonly class: string | null;
  readonly size: string | null;
}

export interface ClaimsDocument {
  readonly schema_version: number;
  readonly source: string;
  readonly total: number;
  readonly passed: number;
  readonly claims: readonly Claim[];
}

/** The committed claims registry, typed for the page and its tests. */
export const claimsDocument = claimsFile as unknown as ClaimsDocument;

/** Every registered claim, in registry order. */
export const claims = claimsDocument.claims;

export type StatusFilter = 'all' | ClaimStatus;

export interface ClaimFilters {
  readonly query: string;
  readonly status: StatusFilter;
  readonly className: string;
  readonly size: string;
}

export const DEFAULT_FILTERS: ClaimFilters = {
  query: '',
  status: 'all',
  className: 'all',
  size: 'all',
};

/** Evidence classes in the order the site publishes them. */
export const CLAIM_CLASSES = [
  'measured',
  'derived',
  'analytical',
  'extrapolated',
  'constant',
  'generated',
] as const;

/** Dataset sizes in reading order. */
export const CLAIM_SIZES = ['Small', 'OneK', 'Medium', 'Large'] as const;

const CLASS_LABELS: Record<string, string> = {
  measured: 'Measured',
  derived: 'Derived',
  analytical: 'Analytical',
  extrapolated: 'Extrapolated',
  constant: 'Constant',
  generated: 'Generated',
};

const SIZE_LABELS: Record<string, string> = {
  Small: 'Small',
  OneK: 'OneK',
  Medium: 'Medium',
  Large: 'Large',
};

/** The written label for an evidence class. */
export function classLabel(value: string | null): string {
  if (value === null || value === '') {
    return 'Unclassified';
  }
  return CLASS_LABELS[value] ?? value;
}

/** The written label for a dataset size. */
export function sizeLabel(value: string | null): string {
  if (value === null || value === '') {
    return '—';
  }
  return SIZE_LABELS[value] ?? value;
}

/** Pass or fail for one registered claim. */
export function claimStatus(claim: Pick<Claim, 'verified'>): ClaimStatus {
  return claim.verified ? 'pass' : 'fail';
}

/** The text a search query is matched against. */
export function claimSearchText(claim: Claim): string {
  return [
    claim.id,
    claim.pointer,
    claim.formatted,
    typeof claim.value === 'string' ? claim.value : JSON.stringify(claim.value),
    claim.class ?? '',
    claim.size ?? '',
  ]
    .join(' ')
    .toLowerCase();
}

/** Whether one claim survives every active filter. */
export function matchesFilters(claim: Claim, filters: ClaimFilters): boolean {
  return rowMatchesFilters(
    {
      search: claimSearchText(claim),
      status: claimStatus(claim),
      className: claim.class ?? 'unknown',
      size: claim.size ?? 'none',
    },
    filters,
  );
}

/** The subset of a claim a rendered table row carries for filtering. */
export interface ClaimRowData {
  /** Lower-case text the search box is matched against. */
  readonly search: string;
  readonly status: ClaimStatus;
  /** The recorded evidence class, or `unknown`. */
  readonly className: string;
  /** The recorded dataset size, or `none`. */
  readonly size: string;
}

/**
 * Whether a rendered row survives every active filter. The page script calls
 * this over the dataset attributes of the server-rendered rows, so the rule is
 * the same one the unit tests exercise.
 */
export function rowMatchesFilters(row: ClaimRowData, filters: ClaimFilters): boolean {
  const query = filters.query.trim().toLowerCase();
  if (query !== '' && !row.search.includes(query)) {
    return false;
  }
  if (filters.status !== 'all' && row.status !== filters.status) {
    return false;
  }
  if (filters.className !== 'all' && row.className !== filters.className) {
    return false;
  }
  if (filters.size !== 'all' && row.size !== filters.size) {
    return false;
  }
  return true;
}

/** The claims that survive the filters, in registry order. */
export function filterClaims(claims: readonly Claim[], filters: ClaimFilters): Claim[] {
  return claims.filter((claim) => matchesFilters(claim, filters));
}

export interface ClaimCounts {
  readonly total: number;
  readonly pass: number;
  readonly fail: number;
}

/** Totals for the registry banner. */
export function claimCounts(claims: readonly Claim[]): ClaimCounts {
  let pass = 0;
  for (const claim of claims) {
    if (claim.verified) {
      pass += 1;
    }
  }
  return { total: claims.length, pass, fail: claims.length - pass };
}

/** The evidence classes present in a registry, in publication order. */
export function presentClasses(claims: readonly Claim[]): string[] {
  const present = new Set(claims.map((claim) => claim.class ?? 'unknown'));
  const ordered: string[] = [...CLAIM_CLASSES];
  if (present.has('unknown')) {
    ordered.push('unknown');
  }
  return ordered.filter((value) => present.has(value));
}

/** The dataset sizes present in a registry, in reading order. */
export function presentSizes(claims: readonly Claim[]): string[] {
  const present = new Set(claims.map((claim) => claim.size).filter((value) => value !== null));
  return CLAIM_SIZES.filter((value) => present.has(value));
}

/** Whether the registry is fully resolved. */
export function registryStatus(claims: readonly Claim[]): 'clean' | 'failing' | 'empty' {
  if (claims.length === 0) {
    return 'empty';
  }
  return claims.every((claim) => claim.verified) ? 'clean' : 'failing';
}

/** Link a row's pointer at the published results artifact. */
export function claimPointerHref(pointer: string): string {
  return `/evidence/climb-results.json#${encodeURI(pointer)}`;
}
