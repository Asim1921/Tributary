import type { CaptureItem } from './types';

/** What a chosen item is, in words. */
export function kindLabel(kind: CaptureItem['kind']): string {
  switch (kind) {
    case 'image':
      return 'Picture';
    case 'video':
      return 'Video';
    case 'page':
      return 'Video page';
    default:
      return 'File';
  }
}

/** Pictures and videos, counted. A video read from its page counts as a video. */
export function kindCounts(items: Pick<CaptureItem, 'kind'>[]): { pictures: number; videos: number; other: number } {
  let pictures = 0;
  let videos = 0;
  let other = 0;
  for (const item of items) {
    if (item.kind === 'image') pictures += 1;
    else if (item.kind === 'video' || item.kind === 'page') videos += 1;
    else other += 1;
  }
  return { pictures, videos, other };
}

const FALLBACK_FOLDER = 'Page media';
const MAX_FOLDER_NAME = 80;
/** Keeps the folder's whole path short enough for Windows, whatever the base folder is. */
const MAX_FOLDER_PATH = 180;
const MIN_FOLDER_NAME = 16;
/** Names Windows keeps for devices, with or without an extension, and with superscript digits. */
const DEVICE_NAME = /^(con|prn|aux|nul|(com|lpt)[0-9\u00b9\u00b2\u00b3])[ ]*(\..*)?$/i;

/**
 * A folder named after the page, inside the usual one, so a page's hundreds of pictures
 * don't spill over everything else. Characters Windows won't have in a name become spaces.
 */
export function folderForPage(defaultFolder: string, title: string | null | undefined): string {
  const base = defaultFolder.replace(/[\\/]+$/, '');
  const room = Math.min(MAX_FOLDER_NAME, Math.max(MIN_FOLDER_NAME, MAX_FOLDER_PATH - base.length - 1));
  let name =
    // Cut by characters, so a picture such as an emoji is never split in two.
    Array.from(
      (title ?? '')
        // Characters Windows forbids, and control characters.
        .replace(/[<>:"/\\|?*\u0000-\u001f]/g, ' ')
        .replace(/\s+/g, ' ')
        .trim(),
    )
      .slice(0, room)
      .join('')
      // Windows drops trailing dots and spaces from a name.
      .replace(/[. ]+$/, '') || FALLBACK_FOLDER;
  if (DEVICE_NAME.test(name)) name += ' page';
  return base ? `${base}\\${name}` : name;
}
