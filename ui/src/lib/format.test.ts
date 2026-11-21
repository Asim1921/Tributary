import { describe, expect, it } from 'vitest';

import { describeVideoChoice, isStreamAddress } from './format';

describe('describeVideoChoice', () => {
  it('gives the size and length', () => {
    expect(describeVideoChoice({ estimatedBytes: 1024 * 1024, seconds: 95 })).toBe(
      'about 1.0 MB, 1:35',
    );
  });

  it('gives only the length when the size is unknown', () => {
    expect(describeVideoChoice({ estimatedBytes: null, seconds: 4 })).toBe('0:04');
  });

  it('rounds up into the next minute', () => {
    expect(describeVideoChoice({ estimatedBytes: null, seconds: 59.6 })).toBe('1:00');
  });
});

describe('isStreamAddress', () => {
  it('recognises playlists and manifests', () => {
    expect(isStreamAddress('https://x.test/a.m3u8')).toBe(true);
    expect(isStreamAddress('https://x.test/A.MPD?token=1')).toBe(true);
    expect(isStreamAddress('https://x.test/a.m3u8#t=1')).toBe(true);
  });

  it('leaves other addresses alone', () => {
    expect(isStreamAddress('https://x.test/a.mp4')).toBe(false);
    expect(isStreamAddress('https://x.test/m3u8/a.zip')).toBe(false);
  });
});

import {
  displayName,
  droppedLink,
  formatBytes,
  formatDuration,
  formatTimeLeft,
  isDownloadableUrl,
  percent,
  secondsLeft,
  statusText,
  checksumText,
  isTorrentAddress,
} from './format';
import type { Download, DownloadRow } from './types';

const download: Download = {
  id: 1,
  url: 'https://example.test/files/report.pdf',
  fileName: null,
  destDir: 'C:\\Downloads',
  finalPath: null,
  size: 1000,
  downloaded: 250,
  state: 'running',
  error: null,
  description: null,
  resumable: true,
  addedAt: 0,
  completedAt: null,
  queueId: null,
  category: null,
};

const row: DownloadRow = { ...download, speed: 50, segments: [] };

describe('formatBytes', () => {
  it('picks a readable unit', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1023)).toBe('1023 B');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(5 * 1024 ** 2)).toBe('5.0 MB');
    expect(formatBytes(150 * 1024 ** 3)).toBe('150 GB');
  });

  it('says when the size is unknown', () => {
    expect(formatBytes(null)).toBe('Unknown');
  });

  it('stops at the largest unit', () => {
    expect(formatBytes(5000 * 1024 ** 4)).toBe('5000 TB');
  });
});

describe('formatDuration', () => {
  it('shows the two most useful units', () => {
    expect(formatDuration(5)).toBe('5s');
    expect(formatDuration(65)).toBe('1m 5s');
    expect(formatDuration(3700)).toBe('1h 1m');
  });

  it('never goes negative', () => {
    expect(formatDuration(-10)).toBe('0s');
  });
});

describe('time left', () => {
  it('divides what remains by the speed', () => {
    expect(secondsLeft(row)).toBe(15);
    expect(formatTimeLeft(row)).toBe('15s');
  });

  it('is unknown without a speed, a size or a running download', () => {
    expect(secondsLeft({ ...row, speed: 0 })).toBeNull();
    expect(secondsLeft({ ...row, size: null })).toBeNull();
    expect(secondsLeft({ ...row, state: 'paused' })).toBeNull();
    expect(formatTimeLeft({ ...row, speed: 0 })).toBe('');
  });
});

describe('percent', () => {
  it('reports progress', () => {
    expect(percent(download)).toBe(25);
  });

  it('is unknown without a size', () => {
    expect(percent({ ...download, size: null })).toBeNull();
    expect(percent({ ...download, size: 0 })).toBeNull();
  });

  it('is always 100 when complete, even if the size was never known', () => {
    expect(percent({ ...download, size: null, state: 'completed' })).toBe(100);
  });

  it('never passes 100', () => {
    expect(percent({ ...download, downloaded: 2000 })).toBe(100);
  });
});

describe('statusText', () => {
  it('includes progress while unfinished', () => {
    expect(statusText(download)).toBe('Downloading 25.0%');
    expect(statusText({ ...download, state: 'paused' })).toBe('Stopped 25.0%');
  });

  it('is plain when complete or when the size is unknown', () => {
    expect(statusText({ ...download, state: 'completed' })).toBe('Complete');
    expect(statusText({ ...download, state: 'completed', addingSound: true })).toBe('Adding sound…');
    expect(statusText({ ...download, state: 'completed', finishing: 'Converting to MP3…' })).toBe(
      'Converting to MP3…',
    );
    expect(statusText({ ...download, state: 'running', addingSound: true })).toBe('Downloading 25.0%');
    expect(statusText({ ...download, size: null })).toBe('Downloading');
  });

  it('says when a link has expired, and when a fresh one is awaited', () => {
    const failed = { ...download, state: 'failed' as const };
    expect(statusText({ ...failed, linkExpired: true })).toBe('Link expired');
    expect(statusText({ ...failed, linkExpired: true, awaitingLink: true })).toBe(
      'Waiting for a fresh link…',
    );
    expect(statusText({ ...download, awaitingLink: true })).toBe('Downloading 25.0%');
  });

  it('says how a checksum check went', () => {
    const done = { ...download, state: 'completed' as const };
    const sum = { algorithm: 'SHA-256', hex: 'ab' };
    expect(statusText({ ...done, checksum: sum, checking: true })).toBe('Checking…');
    expect(statusText({ ...done, checksum: sum, checksumMatches: true })).toBe('Complete, verified');
    expect(statusText({ ...done, checksum: sum, checksumMatches: false })).toBe("Checksum doesn't match");
    expect(statusText({ ...download, checksum: sum, checksumMatches: false })).toBe('Downloading 25.0%');

    expect(checksumText(done)).toBe('None given');
    expect(checksumText({ ...done, checksum: sum })).toBe('SHA-256: checked when the download finishes');
    expect(checksumText({ ...done, checksum: sum, checking: true })).toBe('SHA-256: checking…');
    expect(checksumText({ ...done, checksum: sum, checksumMatches: true })).toBe('SHA-256: matches');
    expect(checksumText({ ...done, checksum: sum, checksumMatches: false })).toBe("SHA-256: doesn't match");
  });
});

describe('displayName', () => {
  it('prefers the known file name', () => {
    expect(displayName({ fileName: 'a.zip', url: 'https://x.test/b.zip' })).toBe('a.zip');
  });

  it('falls back to the last part of the URL, decoded', () => {
    expect(displayName({ fileName: null, url: 'https://x.test/a/my%20file.zip?t=1' })).toBe(
      'my file.zip',
    );
  });

  it('falls back to the whole URL when there is no path', () => {
    expect(displayName({ fileName: null, url: 'https://x.test/' })).toBe('https://x.test/');
  });

  it('copes with text that is not a URL, or a bad escape', () => {
    expect(displayName({ fileName: null, url: 'not a url' })).toBe('not a url');
    expect(displayName({ fileName: null, url: 'https://x.test/100%' })).toBe(
      'https://x.test/100%',
    );
  });
});

describe('droppedLink', () => {
  const data = (values: Record<string, string>) => ({ getData: (type: string) => values[type] ?? '' });

  it('prefers the link list a browser provides', () => {
    expect(droppedLink(data({ 'text/uri-list': 'https://x.test/a.zip', 'text/plain': 'a link' }))).toBe(
      'https://x.test/a.zip',
    );
  });

  it('skips comment lines in a link list', () => {
    const list = '# from a page\r\nhttps://x.test/a.zip\r\n';
    expect(droppedLink(data({ 'text/uri-list': list }))).toBe('https://x.test/a.zip');
  });

  it('falls back to plain text', () => {
    expect(droppedLink(data({ 'text/plain': 'https://x.test/a.zip' }))).toBe('https://x.test/a.zip');
  });

  it('ignores anything that is not a downloadable link', () => {
    expect(droppedLink(data({ 'text/plain': 'just words' }))).toBeNull();
    expect(droppedLink(data({ 'text/uri-list': 'file:///C:/a.zip' }))).toBeNull();
    expect(droppedLink(data({}))).toBeNull();
    expect(droppedLink(null)).toBeNull();
  });
});

describe('isDownloadableUrl', () => {
  it('accepts web addresses', () => {
    expect(isDownloadableUrl('https://x.test/a.zip')).toBe(true);
    expect(isDownloadableUrl('  http://x.test/a.zip  ')).toBe(true);
    expect(isDownloadableUrl('ftp://x.test/a.zip')).toBe(true);
  });

  it('refuses everything else', () => {
    expect(isDownloadableUrl('')).toBe(false);
    expect(isDownloadableUrl('x.test/a.zip')).toBe(false);
    expect(isDownloadableUrl('file:///C:/Windows/win.ini')).toBe(false);
    expect(isDownloadableUrl('javascript:alert(1)')).toBe(false);
  });
});

describe('torrent addresses', () => {
  it('knows magnet links and .torrent files', () => {
    expect(isTorrentAddress('magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=a')).toBe(true);
    expect(isTorrentAddress('https://releases.test/ubuntu.iso.torrent?x=1')).toBe(true);
    expect(isTorrentAddress('magnet:?dn=no-hash')).toBe(false);
    expect(isTorrentAddress('https://x.test/a.iso')).toBe(false);
  });
});
