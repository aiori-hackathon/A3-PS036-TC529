/**
 * Typed access to the generated evidence document.
 *
 * `evidence.json` records what the site can honestly say about the committed
 * sample: whether a report exists, what the report-versus-results check found,
 * the checksum manifest rows with recomputed digests, the chart list and the
 * digests of the other generated documents. A build must reach these values
 * here; a page never types an artifact size or digest.
 *
 * The release bundle is different: it is written to the repository's
 * gitignored `dist/` directory by the packaging step and may simply not exist.
 * `findReleaseBundle` returns `null` in that case so the page can show an
 * honest empty state instead of a dead link.
 */

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

import evidenceFile from '../data/generated/evidence.json';

export interface EvidenceCheck {
  readonly agree: number;
  readonly drift: number;
  readonly missing: number;
  readonly registry_ok: boolean;
  readonly report_found: boolean;
  readonly problems: readonly string[];
}

export interface EvidenceReport {
  readonly available: boolean;
  readonly path: string;
  readonly bytes: number | null;
  readonly sha256: string | null;
  readonly check: EvidenceCheck | null;
}

export interface EvidenceChart {
  readonly name: string;
  readonly bytes: number;
  readonly sha256: string;
}

export interface EvidenceManifestRow {
  readonly path: string;
  readonly bytes: number;
  readonly listed_sha256: string;
  readonly sha256: string;
  readonly matches: boolean;
}

export interface EvidenceManifest {
  readonly available: boolean;
  readonly file: string;
  readonly rows: readonly EvidenceManifestRow[];
}

export interface EvidenceSiteDocument {
  readonly file: string;
  readonly bytes: number;
  readonly sha256: string;
}

export interface Evidence {
  readonly schema_version: number;
  readonly source: string;
  readonly published_base: string;
  readonly report: EvidenceReport;
  readonly charts: readonly EvidenceChart[];
  readonly sums: EvidenceManifest;
  readonly reproduction_mode: string;
  readonly site_documents: readonly EvidenceSiteDocument[];
}

export const evidence = evidenceFile as unknown as Evidence;

/** Files the Astro integration publishes under `/evidence/`. */
export function evidenceFiles(document: Evidence = evidence): string[] {
  const files = new Set<string>();
  for (const row of document.sums.rows) {
    files.add(row.path);
  }
  if (document.sums.available) {
    files.add(document.sums.file);
  }
  files.add('PROVENANCE.txt');
  return [...files];
}

/** A whole byte count with thousands separators. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return '—';
  }
  return `${new Intl.NumberFormat('en-US').format(Math.round(bytes))} B`;
}

/** The first twelve hex characters of a digest. */
export function shortDigest(digest: string): string {
  return digest.slice(0, 12);
}

export interface ReleaseBundle {
  readonly name: string;
  readonly path: string;
  readonly bytes: number;
  readonly sha256: string;
  readonly modified_ms: number;
}

/** A packaged release: `climb-<7–40 hex>.tar.gz`, never the TUI artifact. */
const RELEASE_NAME = /^climb-[0-9a-f]{7,40}\.tar\.gz$/;

/**
 * The repository root, found from the process working directory.
 *
 * Astro bundles page frontmatter, so `import.meta.url` inside a page is not a
 * stable path to the source tree. The build runs either from the repository
 * root or from `site/`; the committed sample artifact distinguishes the two
 * without relying on either.
 */
export function repositoryRoot(start: string = process.cwd()): string {
  for (const candidate of [start, resolve(start, '..')]) {
    if (existsSync(join(candidate, 'docs', 'sample-output', 'climb-results.json'))) {
      return candidate;
    }
  }
  return resolve(start, '..');
}

/**
 * The packaged release names in `directory`, newest first. The Astro
 * integration uses this to serve a bundle in dev without hashing it on every
 * request; the page uses `findReleaseBundle` once per build.
 */
export function releaseBundleNames(directory: string): string[] {
  let entries;
  try {
    entries = readdirSync(directory, { withFileTypes: true });
  } catch {
    return [];
  }
  return entries
    .filter((entry) => entry.isFile() && RELEASE_NAME.test(entry.name))
    .map((entry) => entry.name)
    .sort((left, right) => {
      const difference =
        statSync(join(directory, right)).mtimeMs - statSync(join(directory, left)).mtimeMs;
      return difference === 0 ? right.localeCompare(left) : difference;
    });
}

/**
 * The newest release bundle in `directory`, or `null` when packaging has not
 * run in this checkout. Selection is by modification time, with the name as a
 * deterministic tie-break.
 */
export function findReleaseBundle(directory: string): ReleaseBundle | null {
  const name = releaseBundleNames(directory)[0];
  if (name === undefined) {
    return null;
  }

  const path = join(directory, name);
  const bytes = readFileSync(path);
  return {
    name,
    path,
    bytes: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    modified_ms: statSync(path).mtimeMs,
  };
}
