import { LOGIN_NEEDED, listDownloads, listQueues, onProgress, onQueuesChanged, onState } from './api';
import type { Download, DownloadRow, DownloadState, ProgressEvent, Queue } from './types';

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
  queues = $state<Queue[]>([]);
  /** Ids that finished since the app opened, in order, for the complete dialog. */
  justCompleted = $state<number[]>([]);
  /** Ids that just failed because the server wants a login. */
  needLogin = $state<number[]>([]);

  /** Downloads that just ended, waiting for the list to show why. */
  private ended = new Map<number, DownloadState>();

  async start(): Promise<void> {
    await onProgress((event) => this.applyProgress(event));
    await onState((event) => {
      if (event.state === 'completed' || event.state === 'failed') {
        this.ended.set(event.id, event.state);
      }
      void this.refresh();
    });
    await onQueuesChanged(() => void this.refresh());
    await this.refresh();
  }

  async refresh(): Promise<void> {
    const [downloads, queues] = await Promise.all([listDownloads(), listQueues()]);
    this.queues = queues;
    const previous = new Map(this.rows.map((row) => [row.id, row]));

    for (const download of downloads) {
      if (this.ended.get(download.id) !== download.state) continue;

      this.ended.delete(download.id);
      if (download.state === 'completed') {
        this.justCompleted.push(download.id);
      } else if (download.error === LOGIN_NEEDED) {
        this.needLogin.push(download.id);
      }
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
