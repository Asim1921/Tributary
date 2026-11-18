import { describe, expect, it } from 'vitest';

import { folderForPage, kindCounts, kindLabel } from './batch';

describe('folderForPage', () => {
  it("puts a page's media in a folder named after it", () => {
    expect(folderForPage('C:\\Users\\me\\Downloads', 'NASA (@nasa) • Instagram photos and videos')).toBe(
      'C:\\Users\\me\\Downloads\\NASA (@nasa) • Instagram photos and videos',
    );
  });

  it('leaves out what Windows will not have in a name', () => {
    expect(folderForPage('D:\\Saves\\', 'a/b\\c:d*e?f"g<h>i|j')).toBe('D:\\Saves\\a b c d e f g h i j');
    expect(folderForPage('D:\\Saves', 'Ends with dots...  ')).toBe('D:\\Saves\\Ends with dots');
  });

  it('keeps the name a sensible length', () => {
    const name = folderForPage('D:\\Saves', 'x'.repeat(300)).split('\\').at(-1);
    expect(name).toHaveLength(80);
  });

  it('never cuts a picture such as an emoji in two', () => {
    const name = folderForPage('D:\\Saves', '😀'.repeat(100)).split('\\').at(-1)!;
    expect(Array.from(name)).toHaveLength(80);
    // No half of a pair left over.
    expect(name).not.toMatch(/[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/);
  });

  it('does not use the names Windows keeps for devices', () => {
    for (const title of ['CON', 'nul', 'Prn', 'AUX', 'COM1', 'com0', 'LPT9', 'COM\u00b9', 'CON.txt', 'con .x']) {
      expect(folderForPage('D:\\Saves', title)).toBe(`D:\\Saves\\${title} page`);
    }
    expect(folderForPage('D:\\Saves', 'Console')).toBe('D:\\Saves\\Console');
    expect(folderForPage('D:\\Saves', 'COM10')).toBe('D:\\Saves\\COM10');
  });

  it('keeps the whole path short, but never the name under 16 characters', () => {
    const base = 'D:\\' + 'f'.repeat(120);
    const name = folderForPage(base, 'x'.repeat(300)).split('\\').at(-1)!;
    expect(base.length + 1 + name.length).toBeLessThanOrEqual(180);
    expect(name.length).toBeGreaterThanOrEqual(16);
    const deep = folderForPage('D:\\' + 'f'.repeat(250), 'x'.repeat(300)).split('\\').at(-1)!;
    expect(deep).toHaveLength(16);
  });

  it('has a name to fall back on', () => {
    expect(folderForPage('D:\\Saves', '')).toBe('D:\\Saves\\Page media');
    expect(folderForPage('D:\\Saves', null)).toBe('D:\\Saves\\Page media');
    expect(folderForPage('D:\\Saves', ' ?? ')).toBe('D:\\Saves\\Page media');
  });
});

describe('kindCounts', () => {
  it('counts pictures, and videos whether files or pages', () => {
    expect(kindCounts([{ kind: 'image' }, { kind: 'image' }, { kind: 'video' }, { kind: 'page' }, { kind: 'file' }, {}])).toEqual({
      pictures: 2,
      videos: 2,
      other: 2,
    });
  });
});

describe('kindLabel', () => {
  it('names each kind', () => {
    expect([kindLabel('image'), kindLabel('video'), kindLabel('page'), kindLabel('file'), kindLabel(undefined)]).toEqual([
      'Picture',
      'Video',
      'Video page',
      'File',
      'File',
    ]);
  });
});
