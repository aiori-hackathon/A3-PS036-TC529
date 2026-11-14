/**
 * Typed access to the generated provenance document.
 *
 * The run's commit, timestamp and content identity are recorded once in
 * `provenance.json`. Pages show them from here so the banner and the
 * reproduction page cannot disagree with the artifact they describe.
 */

import provenanceFile from '../data/generated/provenance.json';

export interface PinnedInput {
  readonly path: string;
  readonly sha: string;
}

export interface Provenance {
  readonly commit: string;
  readonly commit_short: string;
  readonly content_identity: string;
  readonly content_sha256: string;
  readonly date: string;
  readonly host_fingerprint_sha256: string;
  readonly pinned_inputs: readonly PinnedInput[];
  readonly rustc: string;
  readonly schema_version: number;
  readonly source: string;
  readonly submodules: readonly PinnedInput[];
}

export const provenance = provenanceFile as unknown as Provenance;

/** The first twelve hex characters of a commit or digest. */
export function shortSha(sha: string): string {
  return sha.slice(0, 12);
}

/**
 * The recorded timestamp in a stable, readable form.
 *
 * The artifact stores a full ISO timestamp with its own offset. Splitting the
 * string avoids a locale- or time-zone-dependent formatter, which would make
 * the rendered page differ between machines.
 */
export function recordedDate(date: string): string {
  const match = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})/.exec(date);
  return match === null ? date : `${match[1]} at ${match[2]}`;
}

/** The UTC offset the timestamp carries, when it states one. */
export function recordedOffset(date: string): string {
  const match = /([+-]\d{2}:\d{2}|Z)$/.exec(date);
  return match === null ? '' : match[1] === 'Z' ? 'UTC' : `UTC${match[1]}`;
}
