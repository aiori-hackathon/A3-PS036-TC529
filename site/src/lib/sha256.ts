/**
 * In-browser SHA-256 for the `/reproduce` checker.
 *
 * The digest is computed by the Web Crypto API on the file the visitor chose;
 * the file never leaves the page. The parser reads the same `SHA256SUMS`
 * manifest the repository commits, so the comparison is against the recorded
 * digest rather than one the page supplies.
 */

export const SHA256_PATTERN = /^[0-9a-f]{64}$/;

/** A digest as lowercase hex. */
export function digestHex(buffer: ArrayBuffer): string {
  return [...new Uint8Array(buffer)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

/** Compute the SHA-256 of some bytes with the platform's Web Crypto API. */
export async function sha256Hex(data: ArrayBuffer | Uint8Array): Promise<string> {
  const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
  const subtle = globalThis.crypto?.subtle;
  if (subtle === undefined) {
    throw new Error('Web Crypto is not available in this browser');
  }
  const digest = await subtle.digest('SHA-256', new Uint8Array(bytes));
  return digestHex(digest);
}

/** Parse a `<sha256>  <file>` manifest into a file-to-digest map. */
export function parseChecksumManifest(text: string): Map<string, string> {
  const rows = new Map<string, string>();
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed === '') {
      continue;
    }
    const [digest = '', ...rest] = trimmed.split(/\s+/);
    const file = rest.join(' ');
    if (SHA256_PATTERN.test(digest.toLowerCase()) && file !== '') {
      rows.set(file, digest.toLowerCase());
    }
  }
  return rows;
}

/** The recorded digest for a file name, or `null`. */
export function lookupChecksum(
  manifest: ReadonlyMap<string, string>,
  fileName: string,
): string | null {
  return manifest.get(fileName) ?? null;
}

/** Strip any directory component from a chosen or dropped file name. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

export type DigestVerdict = 'match' | 'mismatch';

/** Compare a recorded digest with a computed one, case-insensitively. */
export function compareDigest(recorded: string, computed: string): DigestVerdict {
  return recorded.toLowerCase() === computed.toLowerCase() ? 'match' : 'mismatch';
}
