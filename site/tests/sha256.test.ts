import { describe, expect, it } from 'vitest';

import {
  SHA256_PATTERN,
  baseName,
  compareDigest,
  digestHex,
  lookupChecksum,
  parseChecksumManifest,
  sha256Hex,
} from '../src/lib/sha256';

describe('SHA-256', () => {
  it('matches the published test vector for "abc"', async () => {
    const digest = await sha256Hex(new TextEncoder().encode('abc'));
    expect(digest).toBe('ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad');
    expect(digest).toMatch(SHA256_PATTERN);
  });

  it('formats every byte as two lower-case hex characters', () => {
    expect(digestHex(new Uint8Array([0, 15, 16, 255]).buffer)).toBe('000f10ff');
  });

  it('refuses to pretend when the platform has no Web Crypto', async () => {
    const original = globalThis.crypto;
    Object.defineProperty(globalThis, 'crypto', { value: undefined, configurable: true });
    try {
      await expect(sha256Hex(new Uint8Array([1, 2, 3]))).rejects.toThrow(/Web Crypto/);
    } finally {
      Object.defineProperty(globalThis, 'crypto', { value: original, configurable: true });
    }
  });
});

describe('checksum manifests', () => {
  const text = [
    '00bfb56f9aa9be9fb94ce25a634b89c2451c6dbc01a8d03473a0a660481d321e  report.html',
    '',
    '00BFB56F9AA9BE9FB94CE25A634B89C2451C6DBC01A8D03473A0A660481D321E  climb-results.json',
    'not-a-row',
    'short  file.txt',
  ].join('\n');

  it('reads a digest and a file name per well-formed line', () => {
    const manifest = parseChecksumManifest(text);
    expect([...manifest.keys()]).toEqual(['report.html', 'climb-results.json']);
    expect(manifest.get('climb-results.json')).toBe(
      '00bfb56f9aa9be9fb94ce25a634b89c2451c6dbc01a8d03473a0a660481d321e',
    );
  });

  it('looks a file up by name and returns null for an absent one', () => {
    const manifest = parseChecksumManifest(text);
    expect(lookupChecksum(manifest, 'report.html')).toBe(
      '00bfb56f9aa9be9fb94ce25a634b89c2451c6dbc01a8d03473a0a660481d321e',
    );
    expect(lookupChecksum(manifest, 'absent.html')).toBeNull();
  });

  it('compares digests without caring about hex case', () => {
    expect(compareDigest('ABCDEF', 'abcdef')).toBe('match');
    expect(compareDigest('abcdef', 'abcdee')).toBe('mismatch');
  });
});

describe('file names', () => {
  it('strips a directory component on either platform', () => {
    expect(baseName('report.html')).toBe('report.html');
    expect(baseName('/tmp/evidence/report.html')).toBe('report.html');
    expect(baseName('C:\\Users\\judge\\report.html')).toBe('report.html');
  });
});
