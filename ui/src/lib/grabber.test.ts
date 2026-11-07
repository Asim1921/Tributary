import { describe, expect, it } from 'vitest';

import { bytesToKilobytes, fileNameOf, fromLines, kilobytesToBytes, toLines } from './grabber';

describe('fromLines', () => {
  it('drops blanks and spacing', () => {
    expect(fromLines(' a\n\n  b  \n\n')).toEqual(['a', 'b']);
    expect(fromLines('')).toEqual([]);
  });

  it('reverses toLines', () => {
    expect(fromLines(toLines(['a', 'b']))).toEqual(['a', 'b']);
  });
});

describe('fileNameOf', () => {
  it('gives the last part of an address, decoded', () => {
    expect(fileNameOf('https://x.test/a/my%20file.pdf?v=1')).toBe('my file.pdf');
  });

  it('falls back to the address', () => {
    expect(fileNameOf('https://x.test/')).toBe('https://x.test/');
    expect(fileNameOf('not an address')).toBe('not an address');
  });
});

describe('sizes', () => {
  it('reads kilobytes', () => {
    expect(kilobytesToBytes('10')).toBe(10240);
    expect(kilobytesToBytes(' 1.5 ')).toBe(1536);
  });

  it('treats blank or nonsense as no limit', () => {
    expect(kilobytesToBytes('')).toBe(null);
    expect(kilobytesToBytes('   ')).toBe(null);
    expect(kilobytesToBytes('lots')).toBe(null);
    expect(kilobytesToBytes('-5')).toBe(null);
  });

  it('shows what was saved', () => {
    expect(bytesToKilobytes(10240)).toBe('10');
    expect(bytesToKilobytes(null)).toBe('');
  });
});
