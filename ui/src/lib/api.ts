import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

import type {
  AddRequest,
  CategoryChoice,
  Download,
  EditRequest,
  FileInfo,
  ProgressEvent,
  Queue,
  Settings,
  StateEvent,
} from './types';

export const listDownloads = () => invoke<Download[]>('list_downloads');
export const inspectUrl = (url: string) => invoke<FileInfo>('inspect_url', { url });
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

export const onProgress = (handler: (event: ProgressEvent) => void): Promise<UnlistenFn> =>
  listen<ProgressEvent>('download-progress', (event) => handler(event.payload));

export const onState = (handler: (event: StateEvent) => void): Promise<UnlistenFn> =>
  listen<StateEvent>('download-state', (event) => handler(event.payload));

export const onQueuesChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen('queues-changed', () => handler());

export const onShutdownPending = (handler: (seconds: number) => void): Promise<UnlistenFn> =>
  listen<number>('shutdown-pending', (event) => handler(event.payload));

/** Asks the user for a folder. Returns null if they cancel. */
export async function pickFolder(current?: string): Promise<string | null> {
  const chosen = await open({ directory: true, multiple: false, defaultPath: current });
  return typeof chosen === 'string' ? chosen : null;
}
