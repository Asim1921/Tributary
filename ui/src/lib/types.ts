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
  isStream: boolean;
}

export interface AddRequest {
  url: string;
  destDir?: string;
  fileName?: string;
  description?: string;
  paused?: boolean;
  queueId?: number | null;
  category?: string | null;
  username?: string;
  password?: string;
  rememberLogin?: boolean;
  captureToken?: number;
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
  userAgent: string;
  proxyChoice: ProxyChoice;
  proxyUrl: string;
  proxyUsername: string;
  proxyExceptions: string;
  ftpPassive: boolean;
  serverRules: ServerRule[];
  siteLogins: SiteLogin[];
  scanEnabled: boolean;
  scannerPath: string;
  scannerArguments: string;
  sounds: Sounds;
  captureEnabled: boolean;
  ffmpegPath: string;
  videoPanel: boolean;
  captureTypes: string[];
  captureExclusions: string[];
  forceKey: string;
  skipKey: string;
  watchClipboard: boolean;
  showDropTarget: boolean;
}

export interface CaptureItem {
  url: string;
  fileName: string | null;
  video: boolean;
}

export interface GrabberProject {
  id: number;
  name: string;
  startUrl: string;
  template: string;
  maxDepth: number;
  maxPages: number;
  scope: 'site' | 'folder';
  filesAnywhere: boolean;
  delayMs: number;
  includePages: string[];
  excludePages: string[];
  fileTypes: string[];
  excludeFiles: string[];
  minSize: number | null;
  maxSize: number | null;
  folder: string;
  keepFolders: boolean;
  offline: boolean;
  repeatHours: number | null;
  nextRun: number | null;
  lastRun: number | null;
}

export interface ProjectView extends GrabberProject {
  pagesRead: number;
  filesFound: number;
  explored: boolean;
  running: boolean;
}

export interface GrabberProgress {
  id: number;
  pagesRead: number;
  pagesFailed: number;
  pagesRefused: number;
  pagesWaiting: number;
  filesFound: number;
  finished: boolean;
}

export interface GrabberFile {
  url: string;
  from: string;
  size: number | null;
  saved: boolean;
}

export interface VideoChoice {
  label: string;
  width: number | null;
  height: number | null;
  estimatedBytes: number | null;
  seconds: number;
  extension: string;
  separateAudio: boolean;
}

export interface VideoDetails {
  choices: VideoChoice[];
  canJoin: boolean;
  suggestedName: string;
}

export interface AddVideoRequest {
  url: string;
  choice: number;
  destDir: string;
  name: string;
  later: boolean;
  captureToken?: number;
}

export interface CaptureEvent {
  token: number;
  items: CaptureItem[];
  batch: boolean;
}

export type ProxyChoice = 'system' | 'direct' | 'manual';

export interface ServerRule {
  pattern: string;
  connections: number;
}

export interface SiteLogin {
  pattern: string;
  username: string;
}

export interface Sounds {
  complete: string;
  failed: string;
  queue: string;
}

export interface ScanResult {
  id: number;
  file: string;
  problem: string;
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
