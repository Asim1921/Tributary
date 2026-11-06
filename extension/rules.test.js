import { describe, expect, it } from 'vitest';

import {
  CLICK_WINDOW_MS,
  baseName,
  cookieHeader,
  downloadUrl,
  extensionOf,
  isExcluded,
  shouldCapture,
  usableLinks,
  wildcardMatch,
} from './rules.js';

const NOW = 1_000_000;

const settings = {
  enabled: true,
  fileTypes: ['zip', 'exe', 'mp4'],
  exclusions: ['*.intranet.test'],
  forceKey: 'ctrl',
  skipKey: 'alt',
};

const item = {
  url: 'https://files.test/report.zip',
  filename: 'C:\\Users\\me\\Downloads\\report.zip',
  incognito: false,
};

const decide = (changes = {}, click = null, chosen = settings) =>
  shouldCapture({ ...item, ...changes }, chosen, click, NOW);

describe('names', () => {
  it('finds the base name with either slash', () => {
    expect(baseName('C:\\a\\b.zip')).toBe('b.zip');
    expect(baseName('/a/b.zip')).toBe('b.zip');
    expect(baseName('b.zip')).toBe('b.zip');
    expect(baseName(undefined)).toBe('');
  });

  it('finds the extension, ignoring case', () => {
    expect(extensionOf('a.ZIP')).toBe('zip');
    expect(extensionOf('a.tar.gz')).toBe('gz');
    expect(extensionOf('noextension')).toBe('');
    expect(extensionOf('.hidden')).toBe('');
  });
});

describe('wildcardMatch', () => {
  it('matches like a person would expect', () => {
    expect(wildcardMatch('*.test', 'a.test')).toBe(true);
    expect(wildcardMatch('*.test', 'atest')).toBe(false);
    expect(wildcardMatch('A?C', 'abc')).toBe(true);
    expect(wildcardMatch('exact', 'exactly')).toBe(false);
  });

  it('treats regular expression characters as plain text', () => {
    expect(wildcardMatch('a+b', 'aab')).toBe(false);
    expect(wildcardMatch('a.b', 'axb')).toBe(false);
    expect(wildcardMatch('(x', '(x')).toBe(true);
  });
});

describe('isExcluded', () => {
  it('matches sites and whole addresses', () => {
    expect(isExcluded('https://a.intranet.test/x', ['*.intranet.test'])).toBe(true);
    expect(isExcluded('https://x.test/private/a.zip', ['*/private/*'])).toBe(true);
    expect(isExcluded('https://x.test/a.zip', ['*.intranet.test'])).toBe(false);
  });

  it('ignores blanks and bad addresses', () => {
    expect(isExcluded('https://x.test/a.zip', ['', '  '])).toBe(false);
    expect(isExcluded('not a url', ['*'])).toBe(false);
    expect(isExcluded('https://x.test/a.zip', undefined)).toBe(false);
  });
});

describe('shouldCapture', () => {
  it('takes a listed file type', () => {
    expect(decide()).toBe(true);
  });

  it('leaves other file types alone', () => {
    expect(decide({ filename: 'page.html', url: 'https://files.test/page.html' })).toBe(false);
  });

  it('falls back to the address when the browser has no name yet', () => {
    expect(decide({ filename: '' })).toBe(true);
    expect(decide({ filename: '', url: 'https://files.test/a.txt' })).toBe(false);
  });

  it('does nothing when turned off or when the app cannot be reached', () => {
    expect(decide({}, null, { ...settings, enabled: false })).toBe(false);
    expect(decide({}, null, null)).toBe(false);
  });

  it('never touches private windows, even with the force key', () => {
    expect(decide({ incognito: true })).toBe(false);
    expect(decide({ incognito: true }, { ctrl: true, at: NOW })).toBe(false);
  });

  it('leaves downloads started by other extensions alone', () => {
    expect(decide({ byExtensionId: 'abc' })).toBe(false);
  });

  it('cannot take addresses that only exist inside the browser', () => {
    expect(decide({ url: 'blob:https://files.test/123' })).toBe(false);
    expect(decide({ url: 'data:text/plain,hello' })).toBe(false);
    expect(decide({ url: 'file:///C:/a.zip' })).toBe(false);
  });

  it('takes FTP downloads', () => {
    expect(decide({ url: 'ftp://files.test/a.zip' })).toBe(true);
  });

  it('prefers the final address after redirects', () => {
    const redirected = { url: 'https://a.test/go', finalUrl: 'https://b.test/a.zip' };
    expect(downloadUrl(redirected)).toBe('https://b.test/a.zip');
    expect(decide({ ...redirected, filename: '' })).toBe(true);
  });

  it('respects the exclusion list', () => {
    expect(decide({ url: 'https://x.intranet.test/a.zip' })).toBe(false);
  });

  it('skips a download when the skip key was held', () => {
    expect(decide({}, { alt: true, at: NOW })).toBe(false);
  });

  it('forces a download when the force key was held', () => {
    const page = { filename: 'page.html', url: 'https://files.test/page.html' };
    expect(decide(page, { ctrl: true, at: NOW })).toBe(true);
  });

  it('lets the force key override the exclusion list', () => {
    expect(decide({ url: 'https://x.intranet.test/a.zip' }, { ctrl: true, at: NOW })).toBe(true);
  });

  it('lets the skip key win if both are held', () => {
    expect(decide({}, { ctrl: true, alt: true, at: NOW })).toBe(false);
  });

  it('ignores keys from a click long ago', () => {
    const stale = { alt: true, at: NOW - CLICK_WINDOW_MS - 1 };
    expect(decide({}, stale)).toBe(true);
  });

  it('allows the keys to be turned off', () => {
    const noKeys = { ...settings, forceKey: 'none', skipKey: 'none' };
    expect(decide({}, { alt: true, at: NOW }, noKeys)).toBe(true);
  });
});

describe('cookieHeader', () => {
  it('joins cookies the way a browser sends them', () => {
    const cookies = [
      { name: 'a', value: '1' },
      { name: 'b', value: '2' },
    ];
    expect(cookieHeader(cookies)).toBe('a=1; b=2');
  });

  it('copes with no cookies', () => {
    expect(cookieHeader([])).toBe('');
    expect(cookieHeader(undefined)).toBe('');
  });
});

describe('usableLinks', () => {
  it('keeps web links once each, in order', () => {
    const links = ['https://a.test/1', 'javascript:void(0)', 'https://a.test/2', 'https://a.test/1'];
    expect(usableLinks(links)).toEqual(['https://a.test/1', 'https://a.test/2']);
  });

  it('drops anything else', () => {
    expect(usableLinks(['mailto:a@b.test', 'blob:https://a.test/1', 'not a url'])).toEqual([]);
  });
});
