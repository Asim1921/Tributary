import { listDownloads, onProgress, onState } from './api';
import type { Download, DownloadRow, ProgressEvent } from './types';

function toRow(download: Download, previous?: DownloadRow): DownloadRow {
  return {
    ...download,
    // The saved figure lags a running download, so keep the live one.
    downloaded:
      previous && download.state === 'running'
        ? Math.max(previous.downloaded, download.downloaded)
        : download.downloaded,
    speed: download.state === 'running' ? (previous?.speed ?? 0) : 0,
    segments: previous?.segments ?? [],
  };
}

/** The list of downloads, kept current by events from the app. */
class DownloadStore {
  rows = $state<DownloadRow[]>([]);
  /** Ids that finished since the app opened, in order, for the complete dialog. */
  justCompleted = $state<number[]>([]);

  private previousStates = new Map<number, string>();

  async start(): Promise<void> {
    await onProgress((event) => this.applyProgress(event));
    await onState(() => void this.refresh());
    await this.refresh();
  }

  async refresh(): Promise<void> {
    const downloads = await listDownloads();
    const previous = new Map(this.rows.map((row) => [row.id, row]));

    for (const download of downloads) {
      const before = this.previousStates.get(download.id);
      if (before === 'running' && download.state === 'completed') {
        this.justCompleted.push(download.id);
      }
      this.previousStates.set(download.id, download.state);
    }

    this.rows = downloads.map((download) => toRow(download, previous.get(download.id)));
  }

  private applyProgress(event: ProgressEvent): void {
    const row = this.rows.find((item) => item.id === event.id);
    if (!row) return;

    row.downloaded = event.downloaded;
    row.speed = event.bytesPerSecond;
    row.segments = event.segments;
    if (event.total !== null) row.size = event.total;
  }

  byId(id: number): DownloadRow | undefined {
    return this.rows.find((row) => row.id === id);
  }

  get totalSpeed(): number {
    return this.rows.reduce((sum, row) => sum + (row.state === 'running' ? row.speed : 0), 0);
  }
}

export const downloads = new DownloadStore();
