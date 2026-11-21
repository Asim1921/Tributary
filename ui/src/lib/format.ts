import { languageCode, mark, sayError, t } from './locale.svelte';
import type { Download, DownloadRow, DownloadState } from './types';

const UNITS = [mark('{size} B'), mark('{size} KB'), mark('{size} MB'), mark('{size} GB'), mark('{size} TB')];

/** Summarises a quality: its likely size and its length. */
export function describeVideoChoice(choice: {
  estimatedBytes: number | null;
  seconds: number;
}): string {
  const whole = Math.round(choice.seconds);
  const length = `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
  return choice.estimatedBytes === null
    ? length
    : t('about {size}, {length}', { size: formatBytes(choice.estimatedBytes), length });
}

/** Whether an address is a stream, so it needs the video dialog. */
export function isStreamAddress(url: string): boolean {
  const path = url.split(/[?#]/, 1)[0]?.toLowerCase() ?? '';
  return path.endsWith('.m3u8') || path.endsWith('.mpd');
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return t('Unknown');

  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }

  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return t(UNITS[unit]!, { size: value.toFixed(digits) });
}

export function formatSpeed(bytesPerSecond: number): string {
  return t('{size}/s', { size: formatBytes(bytesPerSecond) });
}

export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.round(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;

  if (hours > 0) return t('{hours}h {minutes}m', { hours: String(hours), minutes: String(minutes) });
  if (minutes > 0) return t('{minutes}m {seconds}s', { minutes: String(minutes), seconds: String(rest) });
  return t('{seconds}s', { seconds: String(rest) });
}

/** Seconds until a running download finishes, or null if it can't be estimated. */
export function secondsLeft(row: DownloadRow): number | null {
  if (row.state !== 'running' || row.size === null || row.speed <= 0) return null;
  return Math.max(0, row.size - row.downloaded) / row.speed;
}

export function formatTimeLeft(row: DownloadRow): string {
  const seconds = secondsLeft(row);
  return seconds === null ? '' : formatDuration(seconds);
}

/** Progress from 0 to 100, or null if the size is unknown. */
export function percent(download: Pick<Download, 'size' | 'downloaded' | 'state'>): number | null {
  if (download.state === 'completed') return 100;
  if (!download.size) return null;
  return Math.min(100, (download.downloaded / download.size) * 100);
}

const STATE_LABELS: Record<DownloadState, string> = {
  running: mark('Downloading'),
  paused: mark('Stopped'),
  completed: mark('Complete'),
  failed: mark('Failed'),
};

export function statusText(download: Download): string {
  if (download.awaitingLink && download.state !== 'running') return t('Waiting for a fresh link…');
  if (download.state === 'failed' && download.linkExpired) return t('Link expired');
  // A video saved as two parts is done once its sound is joined in.
  if (download.state === 'completed' && download.addingSound) return t('Adding sound…');
  if (download.state === 'completed' && download.finishing) return sayError(download.finishing);
  if (download.state === 'completed' && download.checking) return t('Checking…');
  if (download.state === 'completed' && download.checksumMatches === false) {
    return t("Checksum doesn't match");
  }
  if (download.state === 'completed' && download.checksumMatches === true) return t('Complete, verified');
  const value = percent(download);
  const label = t(STATE_LABELS[download.state]);
  if (download.state === 'completed' || value === null) return label;
  return t('{state} {percent}%', { state: label, percent: value.toFixed(1) });
}

/** How a download's checksum check went, for its properties. */
export function checksumText(download: Download): string {
  if (!download.checksum) return t('None given');
  const kind = download.checksum.algorithm;
  if (download.checking) return t('{kind}: checking…', { kind });
  if (download.checksumMatches === true) return t('{kind}: matches', { kind });
  if (download.checksumMatches === false) return t("{kind}: doesn't match", { kind });
  return t('{kind}: checked when the download finishes', { kind });
}

/** The best available name, for a download whose server hasn't been asked yet too. */
export function displayName(download: Pick<Download, 'fileName' | 'url'>): string {
  if (download.fileName) return download.fileName;

  try {
    const lastPart = new URL(download.url).pathname.split('/').filter(Boolean).pop();
    return lastPart ? decodeURIComponent(lastPart) : download.url;
  } catch {
    return download.url;
  }
}

export function formatDate(secondsSince1970: number | null): string {
  if (!secondsSince1970) return '';
  return new Date(secondsSince1970 * 1000).toLocaleString(languageCode(), {
    dateStyle: 'medium',
    timeStyle: 'short',
    numberingSystem: 'latn',
  });
}

/** The link in something dragged from a browser, if there is one. */
export function droppedLink(data: Pick<DataTransfer, 'getData'> | null): string | null {
  const text = data?.getData('text/uri-list') || data?.getData('text/plain') || '';
  const line = text
    .split(/\r?\n/)
    .map((item) => item.trim())
    .find((item) => item && !item.startsWith('#'));
  return line && isDownloadableUrl(line) ? line : null;
}

/** Whether an address is a torrent: a magnet link, or a `.torrent` file on the web. */
export function isTorrentAddress(text: string): boolean {
  const trimmed = text.trim();
  if (/^magnet:/i.test(trimmed)) return /xt=urn:btih:/i.test(trimmed);
  try {
    const url = new URL(trimmed);
    return (url.protocol === 'http:' || url.protocol === 'https:') && url.pathname.toLowerCase().endsWith('.torrent');
  } catch {
    return false;
  }
}

export function isDownloadableUrl(text: string): boolean {
  if (isTorrentAddress(text)) return true;
  try {
    const { protocol } = new URL(text.trim());
    return protocol === 'http:' || protocol === 'https:' || protocol === 'ftp:';
  } catch {
    return false;
  }
}
