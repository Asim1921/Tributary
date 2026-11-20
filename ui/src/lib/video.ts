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
