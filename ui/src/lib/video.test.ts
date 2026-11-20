import { describe, expect, it } from 'vitest';

import { defaultSubtitle, holdsCover, subtitleLabel } from './video';

const subtitles = [
  { language: 'de', name: 'German', automatic: false },
  { language: 'en', name: 'English', automatic: true },
  { language: 'en-GB', name: 'English (UK)', automatic: false },
];

describe('video extras', () => {
  it('offers written subtitles in the language Windows shows', () => {
    expect(defaultSubtitle(subtitles, 'en')).toBe(2);
    expect(defaultSubtitle(subtitles, 'de')).toBe(0);
  });

  it('never picks automatic captions, or anything in another language, by itself', () => {
    expect(defaultSubtitle(subtitles.slice(0, 2), 'en')).toBe(-1);
    expect(defaultSubtitle(subtitles, 'fr')).toBe(-1);
    expect(defaultSubtitle([], 'en')).toBe(-1);
  });

  it('says which captions are automatic', () => {
    expect(subtitleLabel(subtitles[1]!)).toBe('English (automatic)');
    expect(subtitleLabel(subtitles[0]!)).toBe('German');
  });

  it('knows which files can hold a cover', () => {
    expect(holdsCover('MP4')).toBe(true);
    expect(holdsCover('mkv')).toBe(true);
    expect(holdsCover('webm')).toBe(false);
    expect(holdsCover('ts')).toBe(false);
  });
});
