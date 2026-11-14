import { describe, expect, it } from 'vitest';

import { provenance, recordedDate, recordedOffset, shortSha } from '../src/lib/provenance';

describe('provenance', () => {
  it('records a full commit and a content identity', () => {
    expect(provenance.commit).toMatch(/^[0-9a-f]{40}$/);
    expect(provenance.content_identity).toMatch(/^sha256:[0-9a-f]{64}$/);
    expect(provenance.pinned_inputs.length).toBe(provenance.submodules.length);
  });

  it('shortens a sha without changing it', () => {
    expect(shortSha(provenance.commit)).toBe(provenance.commit.slice(0, 12));
    expect(shortSha('abc')).toBe('abc');
  });

  it('formats the recorded timestamp without a locale or time-zone shift', () => {
    expect(recordedDate('2026-11-13T01:31:52+05:30')).toBe('2026-11-13 at 01:31');
    expect(recordedDate('not a timestamp')).toBe('not a timestamp');
    expect(recordedOffset('2026-11-13T01:31:52+05:30')).toBe('UTC+05:30');
    expect(recordedOffset('2026-11-13T01:31:52Z')).toBe('UTC');
    expect(recordedOffset('2026-11-13T01:31:52')).toBe('');
    expect(recordedOffset(provenance.date)).toBe('UTC+05:30');
  });
});
