import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { afterEach, describe, expect, it } from 'vitest';

import {
  evidence,
  evidenceFiles,
  findReleaseBundle,
  formatBytes,
  releaseBundleNames,
  shortDigest,
} from '../src/lib/evidence';

const directories: string[] = [];

function temporaryDirectory(): string {
  const directory = mkdtempSync(join(tmpdir(), 'climb-evidence-'));
  directories.push(directory);
  return directory;
}

afterEach(() => {
  for (const directory of directories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

describe('published evidence files', () => {
  it('names every manifest payload plus the manifest and provenance', () => {
    const files = evidenceFiles();
    for (const row of evidence.sums.rows) {
      expect(files).toContain(row.path);
    }
    expect(files).toContain('SHA256SUMS');
    expect(files).toContain('PROVENANCE.txt');
    expect(files).toContain('report.html');
    expect(new Set(files).size).toBe(files.length);
  });

  it('describes the committed report and its check', () => {
    expect(evidence.report.available).toBe(true);
    expect(evidence.report.bytes).toBeGreaterThan(0);
    expect(evidence.report.sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(evidence.report.check?.registry_ok).toBe(true);
    expect(evidence.report.check?.problems).toEqual([]);
  });

  it('keeps the release bundle out of the manifest-derived list', () => {
    const files = evidenceFiles();
    expect(files.some((file) => file.endsWith('.tar.gz'))).toBe(false);
  });
});

describe('byte and digest formatting', () => {
  it('separates thousands and refuses negative input', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1234)).toBe('1,234 B');
    expect(formatBytes(374660)).toBe('374,660 B');
    expect(formatBytes(-1)).toBe('—');
    expect(formatBytes(Number.NaN)).toBe('—');
  });

  it('shows the first twelve hex characters', () => {
    expect(shortDigest('00bfb56f9aa9be9fb94ce25a634b89c2451c6dbc01a8d03473a0a660481d321e')).toBe(
      '00bfb56f9aa9',
    );
    expect(shortDigest('abc')).toBe('abc');
  });
});

describe('release bundle discovery', () => {
  it('accepts only the release naming pattern and never the TUI artifact', () => {
    const directory = temporaryDirectory();
    writeFileSync(join(directory, 'climb-47a4a14.tar.gz'), 'release');
    writeFileSync(join(directory, 'climb-abcdef0.tar.gz'), 'other');
    writeFileSync(join(directory, 'climb-tui-47a4a14-x86_64-unknown-linux-gnu.tar.gz'), 'tui');
    writeFileSync(join(directory, 'climb-nothex.tar.gz'), 'not hex');
    writeFileSync(join(directory, 'notes.txt'), 'notes');

    expect(releaseBundleNames(directory).sort()).toEqual([
      'climb-47a4a14.tar.gz',
      'climb-abcdef0.tar.gz',
    ]);
  });

  it('selects the newest bundle and hashes it', () => {
    const directory = temporaryDirectory();
    const older = join(directory, 'climb-aaaaaaa.tar.gz');
    const newer = join(directory, 'climb-bbbbbbb.tar.gz');
    writeFileSync(older, 'older bytes');
    writeFileSync(newer, 'newer bytes');
    utimesSync(older, new Date('2026-01-01T00:00:00Z'), new Date('2026-01-01T00:00:00Z'));
    utimesSync(newer, new Date('2026-02-01T00:00:00Z'), new Date('2026-02-01T00:00:00Z'));

    const bundle = findReleaseBundle(directory);
    expect(bundle).not.toBeNull();
    if (bundle === null) return;
    expect(bundle.name).toBe('climb-bbbbbbb.tar.gz');
    expect(bundle.bytes).toBe(Buffer.byteLength('newer bytes'));
    expect(bundle.sha256).toBe(
      createHash('sha256').update(Buffer.from('newer bytes')).digest('hex'),
    );
  });

  it('returns null when packaging has not run', () => {
    expect(findReleaseBundle(join(temporaryDirectory(), 'absent'))).toBeNull();
    expect(releaseBundleNames(join(temporaryDirectory(), 'absent'))).toEqual([]);
    expect(findReleaseBundle(temporaryDirectory())).toBeNull();
  });
});
