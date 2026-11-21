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
  /** For the sound of a video saved as two parts, that video. It isn't listed. */
  partOf?: number | null;
  /** Whether this video's sound is still to be joined in. */
  addingSound?: boolean;
  /** What the finished file's checksum should be. */
  checksum?: Checksum | null;
  /** Whether the finished file matched its checksum, once checked. */
  checksumMatches?: boolean | null;
  /** Whether the finished file is being checked against its checksum right now. */
  checking?: boolean;
  /** What's being done to a finished video right now, such as `Converting to MP3…`. */
  finishing?: string | null;
  /** It failed because its link no longer works, which a fresh one would fix. */
  linkExpired?: boolean;
  /** Waiting for the browser to hand over a fresh link. */
  awaitingLink?: boolean;
  /** Other servers with the same file. */
  mirrors?: string[];
  /** Set for a torrent, whose id is below zero. */
  kind?: 'download' | 'torrent';
}

export interface Checksum {
  /** Such as `SHA-256`. */
  algorithm: string;
  hex: string;
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
  /** What the finished file's checksum should be, as pasted. */
  checksum?: string;
}

export interface EditRequest {
  url?: string;
  destDir?: string;
  description?: string;
  /** A new checksum, or empty for none. Left out, it stays as it is. */
  checksum?: string;
  /** Mirror addresses, one a line. Left out, they stay as they are. */
  mirrors?: string[];
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
  notifyInBackground: boolean;
  checkForUpdates: boolean;
  markDownloads: boolean;
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
  /** Such as `ur`; empty follows Windows. */
  language: string;
  ytdlpPath: string;
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
  page: boolean;
  /** What it is: a picture, a video file, a video read from its page, or any file. */
  kind?: 'image' | 'video' | 'page' | 'file';
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
  /** Sound with no picture. */
  audioOnly?: boolean;
}

export interface SubtitleChoice {
  language: string;
  name: string;
  /** Made by speech recognition rather than written by someone. */
  automatic: boolean;
}

export interface VideoDetails {
  choices: VideoChoice[];
  canJoin: boolean;
  suggestedName: string;
  subtitles?: SubtitleChoice[];
  hasCover?: boolean;
}

export interface AddVideoRequest {
  url: string;
  choice: number;
  destDir: string;
  name: string;
  later: boolean;
  captureToken?: number;
  /** Which of the offered subtitles to save, by position. */
  subtitles?: number[];
  cover?: boolean;
  mp3?: boolean;
  /** Keeps only this part, in seconds. */
  section?: { from: number; to: number; exact: boolean };
}

export interface CaptureEvent {
  token: number;
  items: CaptureItem[];
  batch: boolean;
  /** For every picture and video on a page, that page's title. */
  title?: string | null;
  /** Whether these are every picture and video on a page. */
  gallery?: boolean;
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

/** A newer release, fetched and checked, waiting to be installed. */
export interface UpdateInfo {
  version: string;
  notes: string;
}

/** The videos of a list, such as a playlist, as read without reading each one. */
export interface ListView {
  title: string | null;
  entries: { title: string; seconds: number | null; available: boolean }[];
}

/** Sending links from a phone on the home network. */
export interface PhoneStatus {
  running: boolean;
  /** The address for the phone, pairing code included. */
  address: string | null;
  /** That address as an SVG QR code, made by the app from the address alone. */
  qrCode: string | null;
  problem: string | null;
}

/** What a torrent holds, read before it's added. */
export interface TorrentPreview {
  name: string;
  files: { name: string; size: number }[];
}
