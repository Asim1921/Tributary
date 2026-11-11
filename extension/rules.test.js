import { describe, expect, it } from 'vitest';

import {
  CLICK_WINDOW_MS,
  MIN_MEDIA_BYTES,
  addMedia,
  baseName,
  cookieHeader,
  downloadUrl,
  extensionOf,
  headerValue,
  isExcluded,
  isWorthPanel,
  keepDocument,
  MIN_PLAYER_HEIGHT,
  MIN_PLAYER_WIDTH,
  pageDownload,
  videoPageAddress,
  mediaKind,
  mediaLabel,
  responseLength,
  shouldCapture,
  usableLinks,
  wildcardMatch,
} from './rules.js';

describe('mediaKind', () => {
  const big = MIN_MEDIA_BYTES;

  it('recognises streams by address or content type', () => {
    const stream = (url, contentType) => mediaKind({ url, type: 'xmlhttprequest', contentType });

    expect(stream('https://x.test/a.m3u8?t=1', 'text/plain')).toBe('stream');
    expect(stream('https://x.test/a.mpd', null)).toBe('stream');
    expect(stream('https://x.test/play', 'application/vnd.apple.mpegurl')).toBe('stream');
    expect(stream('https://x.test/play', 'application/x-mpegURL')).toBe('stream');
    expect(stream('https://x.test/play', 'application/dash+xml')).toBe('stream');
  });

  it('counts what a media element loads as a file', () => {
    const file = (contentType) =>
      mediaKind({ url: 'https://x.test/a', type: 'media', contentType, length: big });

    expect(file('video/mp4')).toBe('file');
    expect(file('audio/mpeg')).toBe('file');
    expect(file('text/plain')).toBe(null);
  });

  it('leaves pieces fetched by a script alone, as protected players use', () => {
    const piece = { url: 'https://x.test/videoplayback?range=0-999', contentType: 'video/mp4' };

    expect(mediaKind({ ...piece, type: 'xmlhttprequest', length: big })).toBe(null);
    expect(mediaKind({ ...piece, type: 'other', length: big })).toBe(null);
  });

  it('skips small files, and allows a size nobody gave', () => {
    const file = (length) =>
      mediaKind({ url: 'https://x.test/a.mp3', type: 'media', contentType: 'audio/mpeg', length });

    expect(file(big - 1)).toBe(null);
    expect(file(big)).toBe('file');
    expect(file(null)).toBe('file');
  });

  it('ignores addresses that cannot be downloaded', () => {
    const local = { type: 'media', contentType: 'video/mp4' };

    expect(mediaKind({ ...local, url: 'blob:https://x.test/1' })).toBe(null);
    expect(mediaKind({ ...local, url: 'data:video/mp4;base64,AA' })).toBe(null);
    expect(mediaKind({ ...local, url: 'not an address' })).toBe(null);
  });
});

describe('responseLength', () => {
  const headers = (...pairs) => pairs.map(([name, value]) => ({ name, value }));

  it('prefers the whole size of a range response', () => {
    expect(responseLength(headers(['Content-Range', 'bytes 0-1/5000'], ['Content-Length', '2']))).toBe(
      5000,
    );
  });

  it('falls back to the length', () => {
    expect(responseLength(headers(['content-length', '42']))).toBe(42);
  });

  it('is null when nothing says', () => {
    expect(responseLength(headers(['Content-Range', 'bytes 0-1/*']))).toBe(null);
    expect(responseLength([])).toBe(null);
    expect(responseLength(undefined)).toBe(null);
  });

  it('finds headers whatever their case', () => {
    expect(headerValue(headers(['CONTENT-TYPE', 'a']), 'content-type')).toBe('a');
    expect(headerValue(headers(), 'content-type')).toBe(null);
  });
});

describe('addMedia', () => {
  const stream = (url) => ({ url, kind: 'stream' });
  const file = (url) => ({ url, kind: 'file' });

  it('puts streams first', () => {
    const list = addMedia([file('https://x.test/a.mp4')], stream('https://x.test/b/m.m3u8'));
    expect(list.map((item) => item.kind)).toEqual(['stream', 'file']);
  });

  it('does not list the same address twice', () => {
    const list = [file('https://x.test/a.mp4')];
    expect(addMedia(list, file('https://x.test/a.mp4'))).toBe(list);
  });

  it('treats the playlists beside a stream as part of it', () => {
    const list = [stream('https://x.test/v/master.m3u8')];
    expect(addMedia(list, stream('https://x.test/v/low.m3u8?t=1'))).toBe(list);
  });

  it('keeps streams from other folders', () => {
    const list = [stream('https://x.test/one/master.m3u8')];
    expect(addMedia(list, stream('https://x.test/two/master.m3u8'))).toHaveLength(2);
  });

  it('does not let a page fill the list without end', () => {
    let list = [];
    for (let index = 0; index < 500; index += 1) {
      list = addMedia(list, file(`https://x.test/${index}.mp4`));
    }
    expect(list).toHaveLength(20);
  });
});

describe('a page opening', () => {
  const find = (url, documentId) => ({ url, kind: 'file', documentId });

  it('clears what the page before it found', () => {
    const list = [find('https://x.test/old.mp4', 'old')];
    expect(keepDocument(list, 'new')).toEqual([]);
  });

  it('keeps what the new page found first', () => {
    const list = [find('https://x.test/old.mp4', 'old'), find('https://x.test/new.mp4', 'new')];
    expect(keepDocument(list, 'new').map((item) => item.url)).toEqual(['https://x.test/new.mp4']);
  });

  it('changes nothing when everything belongs to the page', () => {
    const list = [find('https://x.test/new.mp4', 'new')];
    expect(keepDocument(list, 'new')).toBe(list);
  });

  it('does not lose a video when a reload finds it before the old one is cleared', () => {
    const url = 'https://x.test/same.mp4';
    const list = addMedia([find(url, 'old')], find(url, 'new'));
    expect(keepDocument(list, 'new')).toEqual([find(url, 'new')]);
  });
});

describe('isWorthPanel', () => {
  it('wants a real player', () => {
    expect(isWorthPanel({ width: 640, height: 360 })).toBe(true);
    expect(isWorthPanel({ width: MIN_PLAYER_WIDTH, height: MIN_PLAYER_HEIGHT })).toBe(true);
  });

  it('leaves thumbnails and previews alone', () => {
    expect(isWorthPanel({ width: MIN_PLAYER_WIDTH - 1, height: 360 })).toBe(false);
    expect(isWorthPanel({ width: 640, height: MIN_PLAYER_HEIGHT - 1 })).toBe(false);
    expect(isWorthPanel({ width: 0, height: 0 })).toBe(false);
  });

  it('leaves a hidden player alone', () => {
    expect(isWorthPanel({ width: 640, height: 360, hidden: true })).toBe(false);
    expect(isWorthPanel(null)).toBe(false);
  });
});

describe('videoPageAddress', () => {
  const watch = 'https://www.youtube.com/watch?v=pBzadnf1Z4M&list=PLD8ifvNZtu-c&index=1';

  // Each of these is what a real YouTube page showed in Chrome.
  it('finds the video playing in a playlist, not the playlist', () => {
    expect(videoPageAddress({ page: watch, named: undefined, watching: 'pBzadnf1Z4M' })).toBe(
      'https://www.youtube.com/watch?v=pBzadnf1Z4M',
    );
  });

  it('finds the video in the miniplayer while the page shows the playlist or the home page', () => {
    for (const page of ['https://www.youtube.com/playlist?list=PLD8ifvNZtu-c', 'https://www.youtube.com/']) {
      expect(videoPageAddress({ page, watching: '2Kqrs38vHfQ' })).toBe(
        'https://www.youtube.com/watch?v=2Kqrs38vHfQ',
      );
    }
  });

  it('finds a preview playing on the home page from the name its player gives it', () => {
    expect(
      videoPageAddress({
        page: 'https://www.youtube.com/',
        named: 'https://www.youtube.com/watch?v=VO_oAPD0EdQ',
        link: 'https://www.youtube.com/watch?v=VO_oAPD0EdQ&list=RDVO_oAPD0EdQ&start_radio=1',
      }),
    ).toBe('https://www.youtube.com/watch?v=VO_oAPD0EdQ');
  });

  it('finds a preview from the link around it on any site', () => {
    expect(videoPageAddress({ page: 'https://videos.test/feed', link: 'https://videos.test/watch/7' })).toBe(
      'https://videos.test/watch/7',
    );
    expect(videoPageAddress({ page: 'https://www.videos.test/', link: 'https://m.videos.test/watch/7' })).toBe(
      'https://m.videos.test/watch/7',
    );
  });

  it('uses where the tab is now when the player says nothing', () => {
    expect(videoPageAddress({ page: 'https://videos.test/watch/9' })).toBe('https://videos.test/watch/9');
    expect(videoPageAddress({ page: watch, watching: '' })).toBe(watch);
  });

  it("works on YouTube's mobile and music sites too", () => {
    expect(videoPageAddress({ page: 'https://m.youtube.com/', watching: 'abcdefghijk' })).toBe(
      'https://m.youtube.com/watch?v=abcdefghijk',
    );
    expect(videoPageAddress({ page: 'https://music.youtube.com/', watching: 'abcdefghijk' })).toBe(
      'https://music.youtube.com/watch?v=abcdefghijk',
    );
  });

  it('never takes an address from the page that leads to another site', () => {
    const page = 'https://videos.test/feed';
    expect(videoPageAddress({ page, link: 'https://elsewhere.test/watch/1' })).toBe(page);
    expect(videoPageAddress({ page, named: 'http://192.168.1.1/admin' })).toBe(page);
    expect(videoPageAddress({ page, link: 'https://evilvideos.test/' })).toBe(page);
  });

  it('never takes anything but a web address', () => {
    const page = 'https://videos.test/feed';
    for (const bad of ['javascript:alert(1)', 'file:///C:/Windows/win.ini', 'data:text/html,x', 'not a url']) {
      expect(videoPageAddress({ page, named: bad, link: bad }), bad).toBe(page);
    }
    expect(videoPageAddress({ page: 'file:///C:/a.html' })).toBe(null);
    expect(videoPageAddress({})).toBe(null);
  });

  it('only builds a YouTube address from a real video id, and only on YouTube', () => {
    const home = 'https://www.youtube.com/';
    for (const bad of ['../../x', 'a b c d e f', 'x', 'abc&list=1', 'a'.repeat(40)]) {
      expect(videoPageAddress({ page: home, watching: bad }), bad).toBe(home);
    }
    expect(videoPageAddress({ page: 'https://videos.test/', watching: 'abcdefghijk' })).toBe('https://videos.test/');
  });
});

describe('pageDownload', () => {
  const page = { url: 'https://www.youtube.com/watch?v=abc', title: ' A film ', userAgent: 'Agent' };

  it('names the page it came from as the referrer', () => {
    expect(pageDownload({ ...page, page: 'https://www.youtube.com/' }).referrer).toBe('https://www.youtube.com/');
  });

  it('describes the page as a page, with its title', () => {
    expect(pageDownload(page)).toEqual({
      type: 'download',
      url: page.url,
      referrer: page.url,
      userAgent: 'Agent',
      fileName: 'A film',
      video: true,
      page: true,
    });
  });

  it('never carries a cookie, whatever else it is given', () => {
    const message = pageDownload({ ...page, cookies: 'session=secret' });
    expect(JSON.stringify(message)).not.toContain('secret');
    expect(Object.keys(message)).not.toContain('cookies');
  });

  it('leaves the name out when the page has no title', () => {
    expect(pageDownload({ ...page, title: '   ' }).fileName).toBeUndefined();
    expect(pageDownload({ ...page, title: undefined }).fileName).toBeUndefined();
  });

  it('only describes web pages', () => {
    for (const url of ['file:///C:/a.html', 'ftp://x.test/a', 'about:blank', 'chrome://extensions', 'javascript:alert(1)', 'not a url']) {
      expect(pageDownload({ ...page, url }), url).toBe(null);
    }
  });
});

describe('mediaLabel', () => {
  it('names a stream plainly', () => {
    expect(mediaLabel({ kind: 'stream', url: 'https://x.test/master.m3u8' })).toBe('Video stream');
  });

  it('names a file from its address', () => {
    expect(mediaLabel({ kind: 'file', url: 'https://x.test/a%20b.mp4?t=1' })).toBe('a b.mp4');
    expect(mediaLabel({ kind: 'file', url: 'https://x.test/100%.mp4' })).toBe('100%.mp4');
    expect(mediaLabel({ kind: 'file', url: 'https://x.test/' })).toBe('Video');
  });
});

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
