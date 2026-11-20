import { describe, expect, it } from 'vitest';

import { looksLikeChecksum } from './checksum';

const SHA256 = 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad';

describe('looksLikeChecksum', () => {
  it('accepts a checksum however it is pasted, and nothing at all', () => {
    for (const text of [
      '',
      '   ',
      SHA256,
      ` ${SHA256.toUpperCase()}\n`,
      `sha256:${SHA256}`,
      `SHA-256: ${SHA256}`,
      `SHA256 ${SHA256.match(/.{8}/g)?.join(' ')}`,
      'a'.repeat(32),
      `md5 ${'a'.repeat(32)}`,
      'a'.repeat(40),
      'a'.repeat(128),
    ]) {
      expect(looksLikeChecksum(text), text).toBe(true);
    }
  });

  it('refuses what is not one', () => {
    for (const text of [
      'abc123',
      'a'.repeat(63),
      'g'.repeat(64),
      `md5:${SHA256}`,
      'https://example.com/file.iso',
    ]) {
      expect(looksLikeChecksum(text), text).toBe(false);
    }
  });
});
