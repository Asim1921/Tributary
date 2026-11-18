import { describe, expect, it } from 'vitest';

import {
  CLICK_WINDOW_MS,
  MAX_PAGE_MEDIA,
  MIN_IMAGE_SIDE,
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
  largestCandidate,
  looksLikeVideoPage,
  MIN_PLAYER_LONG_SIDE,
  MIN_PLAYER_SHORT_SIDE,
  mediaFileKind,
  pageDownload,
  pageMediaBatch,
  statedSize,
  streamHeaders,
  videoPageAddress,
  mediaKind,
  mediaLabel,
  responseLength,
  shouldCapture,
  usableLinks,
  wildcardMatch,
  worthSaving,
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
    expect(isWorthPanel({ width: MIN_PLAYER_LONG_SIDE, height: MIN_PLAYER_SHORT_SIDE })).toBe(true);
  });

  it('leaves thumbnails and previews alone', () => {
    expect(isWorthPanel({ width: MIN_PLAYER_LONG_SIDE - 1, height: MIN_PLAYER_SHORT_SIDE })).toBe(false);
    expect(isWorthPanel({ width: 640, height: MIN_PLAYER_SHORT_SIDE - 1 })).toBe(false);
    expect(isWorthPanel({ width: MIN_PLAYER_SHORT_SIDE - 1, height: 640 })).toBe(false);
    expect(isWorthPanel({ width: 0, height: 0 })).toBe(false);
  });

  it('wants an upright player too, as phone videos play', () => {
    // How Dailymotion showed an upright video.
    expect(isWorthPanel({ width: 239, height: 425 })).toBe(true);
    expect(isWorthPanel({ width: MIN_PLAYER_SHORT_SIDE, height: MIN_PLAYER_LONG_SIDE })).toBe(true);
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

describe('pageDownload inside an embedded player', () => {
  const player = 'https://player.test/embed/1';

  it('names the page the player is embedded in, and the page holding it', () => {
    const found = pageDownload({ url: player, page: player, parent: 'https://host.test/', host: 'https://host.test/film/2' });
    expect(found.referrer).toBe('https://host.test/');
    expect(found.host).toBe('https://host.test/film/2');
  });

  it('names the page itself on a page of its own', () => {
    const found = pageDownload({ url: 'https://videos.test/watch/1', page: 'https://videos.test/watch/1' });
    expect(found.referrer).toBe('https://videos.test/watch/1');
    expect(found.host).toBeUndefined();
  });

  it('never names anything but a web page', () => {
    for (const bad of ['javascript:alert(1)', 'file:///C:/a.html', 'data:text/html,x', 'not a url']) {
      const found = pageDownload({ url: player, page: player, parent: bad, host: bad });
      expect(found.referrer, bad).toBe(player);
      expect(found.host, bad).toBeUndefined();
    }
  });

  it('has no host page to fall back on when the host is the player itself', () => {
    expect(pageDownload({ url: player, page: player, host: player }).host).toBeUndefined();
  });
});

describe('streamHeaders', () => {
  const tab = 'https://host.test/film/2';

  it('asks for a stream as the embedded player did', () => {
    expect(streamHeaders({ initiator: 'https://player.test', request: 'xmlhttprequest' }, tab)).toEqual({
      referrer: 'https://player.test/',
      origin: 'https://player.test',
    });
  });

  it('sends no Origin for a video the player loaded directly, as the browser does', () => {
    expect(streamHeaders({ initiator: 'https://player.test', request: 'media' }, tab)).toEqual({
      referrer: 'https://player.test/',
      origin: undefined,
    });
  });

  it('keeps the page address for a stream the page asked for itself', () => {
    expect(streamHeaders({ initiator: 'https://host.test', request: 'xmlhttprequest' }, tab)).toEqual({ referrer: tab });
  });

  it('keeps the page address when the browser named no usable origin', () => {
    for (const initiator of [undefined, 'null', 'chrome-extension://abc', 'not a url']) {
      expect(streamHeaders({ initiator, request: 'media' }, tab), String(initiator)).toEqual({ referrer: tab });
    }
    expect(streamHeaders(undefined, tab)).toEqual({ referrer: tab });
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

describe('largestCandidate', () => {
  const page = 'https://www.instagram.com/someone/';

  it('takes the widest picture a srcset offers', () => {
    // As Instagram's grid lists them.
    const srcset =
      'https://cdn.test/a_s150.jpg?stp=x 150w, https://cdn.test/a_s640.jpg?stp=x 640w, https://cdn.test/a_s1080.jpg?stp=x 1080w, https://cdn.test/a_s320.jpg 320w';
    expect(largestCandidate(srcset, 'https://cdn.test/a_s320.jpg', page)).toBe('https://cdn.test/a_s1080.jpg?stp=x');
  });

  it('takes the densest when sizes are given as densities', () => {
    expect(largestCandidate('/a.jpg 1x, /a@2x.jpg 2x, /a@1_5x.jpg 1.5x', '/a.jpg', 'https://x.test/p/')).toBe('https://x.test/a@2x.jpg');
  });

  it('keeps a comma that is part of an address', () => {
    expect(largestCandidate('https://img.test/w_400,h_300/a.jpg 400w, https://img.test/w_800,h_600/a.jpg 800w', null, page)).toBe(
      'https://img.test/w_800,h_600/a.jpg',
    );
  });

  it('falls back on the picture the browser chose', () => {
    expect(largestCandidate('', 'https://cdn.test/only.jpg', page)).toBe('https://cdn.test/only.jpg');
    expect(largestCandidate(undefined, '/only.jpg', 'https://x.test/a/')).toBe('https://x.test/only.jpg');
  });

  it('never gives anything but a web address', () => {
    expect(largestCandidate('data:image/png;base64,AAAA 100w', 'blob:https://x.test/1', page)).toBe(null);
  });
});

describe('worthSaving', () => {
  it('keeps pictures big enough to be content', () => {
    expect(worthSaving({ url: 'https://cdn.test/a.jpg', naturalWidth: 1080, naturalHeight: 1350 })).toBe(true);
    expect(worthSaving({ url: 'https://cdn.test/a.jpg', width: 300, height: 300 })).toBe(true);
  });

  it('leaves icons, avatars and tracking pixels', () => {
    expect(worthSaving({ url: 'https://cdn.test/icon.png', naturalWidth: 24, naturalHeight: 24 })).toBe(false);
    expect(worthSaving({ url: 'https://cdn.test/avatar.jpg', naturalWidth: 150, naturalHeight: 150 })).toBe(false);
    expect(worthSaving({ url: 'https://t.test/p.gif', naturalWidth: 1, naturalHeight: 1, width: 1, height: 1 })).toBe(false);
    expect(worthSaving({ url: 'https://cdn.test/a.jpg', naturalWidth: MIN_IMAGE_SIDE - 1, naturalHeight: 2000 })).toBe(false);
  });

  it('goes by the size shown when the picture has none of its own yet', () => {
    expect(worthSaving({ url: 'https://cdn.test/a.jpg', width: 240, height: 240, naturalWidth: 0, naturalHeight: 0 })).toBe(true);
  });

  it('leaves drawings and anything that is not a web file', () => {
    expect(worthSaving({ url: 'https://cdn.test/logo.svg', naturalWidth: 800, naturalHeight: 800 })).toBe(false);
    expect(worthSaving({ url: 'data:image/png;base64,AAAA', naturalWidth: 800, naturalHeight: 800 })).toBe(false);
  });
});

describe('looksLikeVideoPage', () => {
  it("knows a single video's own page", () => {
    for (const url of [
      'https://www.instagram.com/reel/C1a2B3c4D5e/',
      'https://www.instagram.com/someone/reels/C1a2B3c4D5e/',
      'https://www.facebook.com/someone/videos/1234567890/',
      'https://www.facebook.com/watch/?v=1234567890',
      'https://www.facebook.com/reel/1234567890',
      'https://www.youtube.com/watch?v=abc',
      'https://www.youtube.com/shorts/abc',
      'https://www.tiktok.com/@someone/video/1234567890',
    ]) {
      expect(looksLikeVideoPage(url), url).toBe(true);
    }
  });

  it('leaves pages that are not one video', () => {
    for (const url of [
      'https://www.instagram.com/p/C1a2B3c4D5e/',
      'https://www.instagram.com/someone/reels/',
      'https://www.facebook.com/someone/videos/',
      'https://www.youtube.com/watcher',
      'https://x.test/',
      'javascript:alert(1)',
    ]) {
      expect(looksLikeVideoPage(url), url).toBe(false);
    }
  });
});

describe('mediaFileKind', () => {
  it('tells pictures and video files by their names', () => {
    expect(mediaFileKind('https://x.test/a/photo.JPG?x=1')).toBe('image');
    expect(mediaFileKind('https://x.test/clip.webm')).toBe('video');
    expect(mediaFileKind('https://x.test/page.html')).toBe(null);
    expect(mediaFileKind('ftp://x.test/a.jpg')).toBe(null);
  });
});

describe('statedSize', () => {
  it('reads the size an address gives its picture', () => {
    expect(statedSize('https://cdn.test/p.jpg?stp=dst-jpg_e35_s640x640_tt6')).toBe(640 * 640);
    expect(statedSize('https://cdn.test/p.jpg?cstp=mx2047x1487&ctp=s960x960')).toBe(2047 * 1487);
    expect(statedSize('https://images.test/photo-17934?w=2000&q=60')).toBe(2000 * 2000);
    expect(statedSize('https://cdn.test/photo.jpg')).toBe(0);
  });
});

describe('pageMediaBatch', () => {
  const page = 'https://www.instagram.com/someone/';
  const picture = (name, size = 640) => ({
    srcset: `https://cdn.test/${name}_s320.jpg 320w, https://cdn.test/${name}_s1080.jpg 1080w`,
    src: `https://cdn.test/${name}_s320.jpg`,
    width: 300,
    height: 300,
    naturalWidth: size,
    naturalHeight: size,
  });

  it('offers each picture at its largest, each once, in the order found', () => {
    const batch = pageMediaBatch({ images: [picture('a'), picture('b'), picture('a')] }, page);
    expect(batch).toEqual([
      { url: 'https://cdn.test/a_s1080.jpg', kind: 'image' },
      { url: 'https://cdn.test/b_s1080.jpg', kind: 'image' },
    ]);
  });

  it('offers a photo shown at two sizes once, at the bigger, where it was first found', () => {
    const shown = (query, size) => ({
      src: `https://scontent.fbcdn.test/v/t39/809146614_4484013928506139_3181913787670602881_n.png?${query}`,
      width: size,
      height: size,
      naturalWidth: size,
      naturalHeight: size,
    });
    const batch = pageMediaBatch(
      { images: [shown('stp=s320x320&oh=1', 320), picture('b'), shown('stp=s960x960&oh=2', 960)] },
      page,
    );
    expect(batch).toEqual([
      { url: 'https://scontent.fbcdn.test/v/t39/809146614_4484013928506139_3181913787670602881_n.png?stp=s960x960&oh=2', kind: 'image' },
      { url: 'https://cdn.test/b_s1080.jpg', kind: 'image' },
    ]);
  });

  it("takes the bigger of a photo's sizes by its address when it hasn't loaded yet", () => {
    const base = 'https://scontent.fbcdn.test/v/t39/809146614_4484013928506139_3181913787670602881_n.png';
    const batch = pageMediaBatch(
      {
        images: [
          { src: `${base}?stp=dst-jpg_s320x320&oh=1`, width: 168, height: 168, naturalWidth: 320, naturalHeight: 320 },
          { src: `${base}?stp=dst-jpg&cstp=mx2047x1487&ctp=s960x960&oh=2`, width: 500, height: 360, naturalWidth: 0, naturalHeight: 0 },
        ],
      },
      page,
    );
    expect(batch).toEqual([{ url: `${base}?stp=dst-jpg&cstp=mx2047x1487&ctp=s960x960&oh=2`, kind: 'image' }]);
  });

  it('keeps pictures apart whose short names say nothing of what they are', () => {
    const named = (query) => ({ src: `https://site.test/image.jpg?${query}`, naturalWidth: 800, naturalHeight: 600 });
    expect(pageMediaBatch({ images: [named('id=1'), named('id=2')] }, page)).toHaveLength(2);
  });

  it('leaves the icons and avatars a page is full of', () => {
    const icon = { src: 'https://cdn.test/icon.png', naturalWidth: 24, naturalHeight: 24 };
    expect(pageMediaBatch({ images: [picture('avatar', 150), icon] }, page)).toEqual([]);
  });

  it('offers a video file as it is, and a video fed by script by its own page', () => {
    const batch = pageMediaBatch(
      {
        videos: [
          { src: 'https://cdn.test/clip.mp4' },
          { src: 'blob:https://www.instagram.com/1', link: 'https://www.instagram.com/reel/AbC/' },
          { src: 'blob:https://www.instagram.com/2' },
        ],
      },
      page,
    );
    expect(batch).toEqual([
      { url: 'https://cdn.test/clip.mp4', kind: 'video' },
      { url: 'https://www.instagram.com/reel/AbC/', kind: 'page' },
      { url: page, kind: 'page' },
    ]);
  });

  it('offers a video fed by script by the address it was seen on, as a feed rewrites it', () => {
    const batch = pageMediaBatch(
      {
        videos: [
          { src: 'blob:https://www.instagram.com/1', page: 'https://www.instagram.com/reel/One/' },
          { src: 'blob:https://www.instagram.com/2', page: 'https://www.instagram.com/reel/Two/' },
          { src: 'blob:https://www.instagram.com/3', page: 'https://elsewhere.test/reel/Three/' },
        ],
      },
      'https://www.instagram.com/reel/Two/',
    );
    expect(batch).toEqual([
      { url: 'https://www.instagram.com/reel/One/', kind: 'page' },
      { url: 'https://www.instagram.com/reel/Two/', kind: 'page' },
    ]);
  });

  it('offers a video whose source is a playlist as the page it plays on', () => {
    const batch = pageMediaBatch(
      {
        videos: [
          { src: 'https://cdn.test/live/master.m3u8?token=1', page },
          { src: 'https://cdn.test/dash/manifest.MPD' },
        ],
      },
      page,
    );
    expect(batch).toEqual([{ url: page, kind: 'page' }]);
  });

  it("offers a grid's video posts by their own pages, and only on this site", () => {
    const batch = pageMediaBatch(
      {
        links: [
          { href: 'https://www.instagram.com/reel/AbC/', holdsPicture: true },
          { href: 'https://www.instagram.com/p/XyZ/', holdsPicture: true },
          { href: 'https://www.instagram.com/reel/Text/', holdsPicture: false },
          { href: 'https://elsewhere.test/reel/AbC/', holdsPicture: true },
          { href: 'https://cdn.test/full.jpg', holdsPicture: false },
        ],
      },
      page,
    );
    expect(batch).toEqual([
      { url: 'https://www.instagram.com/reel/AbC/', kind: 'page' },
      { url: 'https://cdn.test/full.jpg', kind: 'image' },
    ]);
  });

  it('stops at a limit', () => {
    const images = Array.from({ length: MAX_PAGE_MEDIA + 50 }, (_, index) => picture(`n${index}`));
    expect(pageMediaBatch({ images }, page)).toHaveLength(MAX_PAGE_MEDIA);
  });

  it('offers nothing for a page that is not on the web', () => {
    expect(pageMediaBatch({ images: [picture('a')] }, 'file:///C:/a.html')).toEqual([]);
  });
});
