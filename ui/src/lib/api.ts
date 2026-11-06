import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';

import type {
  AddRequest,
  AddVideoRequest,
  CaptureEvent,
  CategoryChoice,
  Download,
  EditRequest,
  FileInfo,
  ProgressEvent,
  Queue,
  ScanResult,
  Settings,
  StateEvent,
  VideoDetails,
} from './types';

/** What the app says when a server wants a login it doesn't have. */
export const LOGIN_NEEDED = 'this site needs a login';

export const listDownloads = () => invoke<Download[]>('list_downloads');
export const inspectUrl = (
  url: string,
  username?: string,
  password?: string,
  captureToken?: number,
) => invoke<FileInfo>('inspect_url', { url, username, password, captureToken });
export const addDownload = (request: AddRequest) => invoke<number>('add_download', { request });
export const pauseDownload = (id: number) => invoke<void>('pause_download', { id });
export const resumeDownload = (id: number) => invoke<void>('resume_download', { id });
export const stopAll = () => invoke<void>('stop_all');
export const removeDownload = (id: number, deleteFile: boolean) =>
  invoke<void>('remove_download', { id, deleteFile });
export const editDownload = (id: number, edit: EditRequest) =>
  invoke<void>('edit_download', { id, edit });
export const openFile = (id: number) => invoke<void>('open_file', { id });
export const showInFolder = (id: number) => invoke<void>('show_in_folder', { id });
export const dragFiles = (ids: number[]) => invoke<void>('drag_files', { ids });
export const windowReady = () => invoke<CaptureEvent[]>('window_ready');
export const inspectVideo = (url: string, fileName?: string, captureToken?: number) =>
  invoke<VideoDetails>('inspect_video', { url, fileName, captureToken });
export const addVideo = (request: AddVideoRequest) => invoke<void>('add_video', { request });
export const getSettings = () => invoke<Settings>('get_settings');
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings });
export const suggestCategory = (fileName: string, url: string) =>
  invoke<CategoryChoice>('suggest_category', { fileName, url });
export const listQueues = () => invoke<Queue[]>('list_queues');
export const saveQueue = (queue: Queue) => invoke<number>('save_queue', { queue });
export const deleteQueue = (id: number) => invoke<void>('delete_queue', { id });
export const startQueue = (id: number) => invoke<void>('start_queue', { id });
export const stopQueue = (id: number) => invoke<void>('stop_queue', { id });
export const setDownloadQueue = (id: number, queueId: number | null) =>
  invoke<void>('set_download_queue', { id, queueId });
export const reorderQueue = (ids: number[]) => invoke<void>('reorder_queue', { ids });
export const cancelShutdown = () => invoke<void>('cancel_shutdown');
export const loginAndResume = (
  id: number,
  username: string,
  password: string,
  remember: boolean,
) => invoke<void>('login_and_resume', { id, username, password, remember });
export const saveSiteLogin = (pattern: string, username: string, password: string) =>
  invoke<void>('save_site_login', { pattern, username, password });
export const deleteSiteLogin = (pattern: string) =>
  invoke<void>('delete_site_login', { pattern });
export const setProxyPassword = (password: string) =>
  invoke<void>('set_proxy_password', { password });
export const playSound = (path: string) => invoke<boolean>('play_sound', { path });
export const discardCapture = (token: number) => invoke<void>('discard_capture', { token });
export const addBatch = (
  token: number,
  urls: string[],
  destDir: string,
  queueId: number | null,
  paused: boolean,
) => invoke<number>('add_batch', { token, urls, destDir, queueId, paused });
export const extensionFolder = () => invoke<string>('extension_folder');
export const addDroppedLink = (url: string) => invoke<void>('add_dropped_link', { url });
export const exportData = (path: string) => invoke<number>('export_data', { path });
export const importData = (path: string) => invoke<number>('import_data', { path });

export const onProgress = (handler: (event: ProgressEvent) => void): Promise<UnlistenFn> =>
  listen<ProgressEvent>('download-progress', (event) => handler(event.payload));

export const onState = (handler: (event: StateEvent) => void): Promise<UnlistenFn> =>
  listen<StateEvent>('download-state', (event) => handler(event.payload));

export const onQueuesChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen('queues-changed', () => handler());

export const onVideoProblem = (handler: (problem: string) => void): Promise<UnlistenFn> =>
  listen<string>('video-problem', (event) => handler(event.payload));

export const onCapture = (handler: (event: CaptureEvent) => void): Promise<UnlistenFn> =>
  listen<CaptureEvent>('capture', (event) => handler(event.payload));

export const onScanFinished = (handler: (result: ScanResult) => void): Promise<UnlistenFn> =>
  listen<ScanResult>('scan-finished', (event) => handler(event.payload));

export const onShutdownPending = (handler: (seconds: number) => void): Promise<UnlistenFn> =>
  listen<number>('shutdown-pending', (event) => handler(event.payload));

/** Asks where to save an export. Returns null if they cancel. */
export async function pickSavePath(): Promise<string | null> {
  return save({
    defaultPath: 'tributary-export.json',
    filters: [{ name: 'Tributary export', extensions: ['json'] }],
  });
}

/** Asks the user for a file. Returns null if they cancel. */
export async function pickFile(name: string, extensions: string[]): Promise<string | null> {
  const chosen = await open({ multiple: false, filters: [{ name, extensions }] });
  return typeof chosen === 'string' ? chosen : null;
}

/** Asks the user for a folder. Returns null if they cancel. */
export async function pickFolder(current?: string): Promise<string | null> {
  const chosen = await open({ directory: true, multiple: false, defaultPath: current });
  return typeof chosen === 'string' ? chosen : null;
}
