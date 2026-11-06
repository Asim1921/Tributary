import type { Download, DownloadRow, DownloadState } from './types';

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** Summarises a quality: its likely size and its length. */
export function describeVideoChoice(choice: {
  estimatedBytes: number | null;
  seconds: number;
}): string {
  const whole = Math.round(choice.seconds);
  const length = `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
  return choice.estimatedBytes === null
    ? length
    : `about ${formatBytes(choice.estimatedBytes)}, ${length}`;
}

/** Whether an address is a stream, so it needs the video dialog. */
export function isStreamAddress(url: string): boolean {
  const path = url.split(/[?#]/, 1)[0]?.toLowerCase() ?? '';
  return path.endsWith('.m3u8') || path.endsWith('.mpd');
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return 'Unknown';

  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }

  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.round(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;

  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${rest}s`;
  return `${rest}s`;
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
  running: 'Downloading',
  paused: 'Stopped',
  completed: 'Complete',
  failed: 'Failed',
};

export function statusText(download: Download): string {
  const value = percent(download);
  if (download.state === 'completed' || value === null) return STATE_LABELS[download.state];
  return `${STATE_LABELS[download.state]} ${value.toFixed(1)}%`;
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
  return new Date(secondsSince1970 * 1000).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
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

export function isDownloadableUrl(text: string): boolean {
  try {
    const { protocol } = new URL(text.trim());
    return protocol === 'http:' || protocol === 'https:' || protocol === 'ftp:';
  } catch {
    return false;
  }
}
