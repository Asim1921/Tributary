import type { SubtitleChoice } from './types';

/** The language Windows shows, such as `en` for `en-GB`. */
export function ownLanguage(): string {
  return (navigator.language || 'en').split('-')[0]?.toLowerCase() ?? 'en';
}

/**
 * The subtitles to offer first: written ones in the language Windows shows, if the video
 * has them. Automatic captions are never picked by themselves, because they're often
 * wrong. `-1` means none.
 */
export function defaultSubtitle(subtitles: SubtitleChoice[], language = ownLanguage()): number {
  return subtitles.findIndex(
    (item) => !item.automatic && item.language.split('-')[0]?.toLowerCase() === language,
  );
}

/** What the menu calls a subtitle track. */
export function subtitleLabel(item: SubtitleChoice): string {
  return item.automatic ? `${item.name} (automatic)` : item.name;
}

/** Kinds of file that can't hold a cover, so it's saved beside them. */
export function holdsCover(extension: string): boolean {
  return ['mp4', 'm4v', 'mov', 'mkv', 'm4a', 'mp3'].includes(extension.toLowerCase());
}

/** Reads a time such as `1:05`, `1:02:03` or `75` as seconds, or null if it isn't one. */
export function parseClock(text: string): number | null {
  const parts = text.trim().split(':');
  if (parts.length > 3 || parts.some((part) => !/^\d+(\.\d+)?$/.test(part))) return null;
  return parts.reduce((total, part) => total * 60 + Number(part), 0);
}

/** A number of seconds as `m:ss` or `h:mm:ss`. */
export function clock(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = String(whole % 60).padStart(2, '0');
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, '0')}:${rest}` : `${minutes}:${rest}`;
}

/** What's wrong with a part chosen from a video `seconds` long, if anything. */
export function sectionProblem(from: string, to: string, seconds: number): string | null {
  const start = parseClock(from);
  const end = parseClock(to);
  if (start === null || end === null) return 'Write the times as minutes and seconds, such as 1:30.';
  if (end <= start) return 'The end has to come after the start.';
  if (seconds > 0 && start >= seconds) return 'The start is after the end of the video.';
  return null;
}
