export type DownloadState = 'running' | 'paused' | 'completed' | 'failed';

export interface Segment {
  start: number;
  end: number;
  written: number;
}

/** A download as the app reports it. */
export interface Download {
  id: number;
  url: string;
  fileName: string | null;
  destDir: string;
  finalPath: string | null;
  size: number | null;
  downloaded: number;
  state: DownloadState;
  error: string | null;
  description: string | null;
  resumable: boolean;
  addedAt: number;
  completedAt: number | null;
}

/** A download plus the live figures that only exist while it runs. */
export interface DownloadRow extends Download {
  speed: number;
  segments: Segment[];
}

export interface ProgressEvent {
  id: number;
  downloaded: number;
  total: number | null;
  bytesPerSecond: number;
  segments: Segment[];
}

export interface StateEvent {
  id: number;
  state: DownloadState;
}

export interface FileInfo {
  fileName: string;
  size: number | null;
  resumable: boolean;
}

export interface AddRequest {
  url: string;
  destDir?: string;
  fileName?: string;
  description?: string;
  paused?: boolean;
}

export interface EditRequest {
  url?: string;
  destDir?: string;
  description?: string;
}

export type Theme = 'system' | 'light' | 'dark';
export type ToolbarSize = 'large' | 'small';

export interface ColumnSetting {
  id: string;
  visible: boolean;
  width: number;
}

export interface Settings {
  downloadDir: string;
  tempDir: string | null;
  connections: number;
  theme: Theme;
  showStartDialog: boolean;
  showCompleteDialog: boolean;
  closeToTray: boolean;
  startWithWindows: boolean;
  columns: ColumnSetting[];
  toolbar: string[];
  toolbarSize: ToolbarSize;
  urlHistory: string[];
}
