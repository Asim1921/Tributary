/** What the grabber can start from, in the order offered. */
export const TEMPLATES = [
  { id: 'allFiles', label: 'All files', hint: 'Every file the site links to' },
  { id: 'pictures', label: 'Pictures', hint: 'Pictures on the site' },
  { id: 'documents', label: 'Documents', hint: 'PDFs, office files and text' },
  { id: 'audioAndVideo', label: 'Audio and video', hint: 'Music and video files' },
  { id: 'offlineSite', label: 'Whole site', hint: 'The pages too, to read with no connection' },
] as const;

export type TemplateId = (typeof TEMPLATES)[number]['id'];

/** One entry per line, as typed into a box, without the blanks. */
export function fromLines(text: string): string[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export const toLines = (items: string[]): string => items.join('\n');

/** The last part of an address, for showing in a list. */
export function fileNameOf(url: string): string {
  try {
    const name = new URL(url).pathname.split('/').pop() ?? '';
    return decodeURIComponent(name) || url;
  } catch {
    return url;
  }
}

/** Reads a size typed in kilobytes. Blank means no limit. */
export function kilobytesToBytes(text: string): number | null {
  const value = Number(text.trim());
  return text.trim() === '' || !Number.isFinite(value) || value < 0 ? null : Math.round(value * 1024);
}

export const bytesToKilobytes = (bytes: number | null): string =>
  bytes === null ? '' : String(bytes / 1024);
