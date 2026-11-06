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
  queueId: number | null;
  category: string | null;
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
  isWebPage: boolean;
}

export interface AddRequest {
  url: string;
  destDir?: string;
  fileName?: string;
  description?: string;
  paused?: boolean;
  queueId?: number | null;
  category?: string | null;
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
  categories: Category[];
  rememberLastFolder: boolean;
  lastFolder: string | null;
  speedLimitEnabled: boolean;
  speedLimitKb: number;
  quotaEnabled: boolean;
  quotaMb: number;
  quotaHours: number;
}

export interface Category {
  name: string;
  folder: string | null;
  extensions: string[];
  sites: string[];
}

export interface CategoryChoice {
  name: string | null;
  folder: string;
}

export interface AfterAction {
  shutdown: boolean;
  force: boolean;
  program: string;
  wake: boolean;
}

export interface Queue {
  id: number;
  name: string;
  sync: boolean;
  concurrency: number;
  retries: number;
  startMinute: number | null;
  stopMinute: number | null;
  days: number;
  syncMinutes: number;
  after: AfterAction;
  active: boolean;
  files: number[];
}
