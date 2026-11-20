import { describe, expect, it } from 'vitest';

import { clock, defaultSubtitle, holdsCover, parseClock, sectionProblem, subtitleLabel } from './video';

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

describe('a part of a video', () => {
  it('reads times the way people write them', () => {
    expect(parseClock('1:05')).toBe(65);
    expect(parseClock(' 1:02:03 ')).toBe(3723);
    expect(parseClock('75')).toBe(75);
    expect(parseClock('0:07.5')).toBe(7.5);
    for (const bad of ['', 'abc', '1:-5', '1:2:3:4', '1::2']) expect(parseClock(bad), bad).toBe(null);
  });

  it('writes them back', () => {
    expect(clock(65)).toBe('1:05');
    expect(clock(3723)).toBe('1:02:03');
    expect(clock(6.9)).toBe('0:06');
  });

  it('says what is wrong with a part', () => {
    expect(sectionProblem('0:02', '0:04', 6)).toBe(null);
    expect(sectionProblem('0:04', '0:02', 6)).toContain('after the start');
    expect(sectionProblem('0:07', '0:09', 6)).toContain('after the end');
    expect(sectionProblem('soon', '0:09', 6)).toContain('such as 1:30');
    expect(sectionProblem('1:00', '2:00', 0)).toBe(null);
  });
});
