import { describe, expect, it } from 'vitest';

import { writeClipboard } from '../src/lib/copy';

describe('clipboard helper', () => {
  it('reports a successful write', async () => {
    const written: string[] = [];
    const clipboard = {
      writeText: async (text: string) => {
        written.push(text);
      },
    };
    await expect(writeClipboard('cargo xtask sample-check', clipboard)).resolves.toBe(true);
    expect(written).toEqual(['cargo xtask sample-check']);
  });

  it('reports a refused write instead of claiming success', async () => {
    const clipboard = {
      writeText: async () => {
        throw new Error('permission denied');
      },
    };
    await expect(writeClipboard('cargo xtask sample-check', clipboard)).resolves.toBe(false);
  });

  it('reports a missing clipboard as a refusal', async () => {
    await expect(writeClipboard('text', undefined)).resolves.toBe(false);
  });
});
