import { describe as group, expect, it } from 'vitest';

import {
  describe,
  fill,
  pickLanguage,
  placeholders,
  pluralsComplete,
  translate,
} from './i18n';

const FILES = '{count, plural, one {# file} other {# files}}';

group('fill', () => {
  it('fills names and leaves an unknown one showing', () => {
    expect(fill('Saved {name} in {folder}', 'en', { name: 'a.zip', folder: 'C:\\x' })).toBe(
      'Saved a.zip in C:\\x',
    );
    expect(fill('Saved {name}', 'en')).toBe('Saved {name}');
  });

  it('chooses plurals by each language’s rules', () => {
    expect(fill(FILES, 'en', { count: 1 })).toBe('1 file');
    expect(fill(FILES, 'en', { count: 0 })).toBe('0 files');
    expect(fill(FILES, 'en', { count: 1200 })).toBe('1,200 files');

    // Arabic has six forms; those missing fall back to `other`.
    const arabic =
      '{count, plural, zero {لا ملفات} one {ملف واحد} two {ملفان} few {# ملفات} many {# ملفًا} other {# ملف}}';
    expect(fill(arabic, 'ar', { count: 0 })).toBe('لا ملفات');
    expect(fill(arabic, 'ar', { count: 2 })).toBe('ملفان');
    expect(fill(arabic, 'ar', { count: 5 })).toBe('5 ملفات');
    expect(fill(arabic, 'ar', { count: 11 })).toBe('11 ملفًا');
    expect(fill(arabic, 'ar', { count: 100 })).toBe('100 ملف');

    // Hindi counts 0 as singular, as English doesn't.
    expect(fill('{count, plural, one {# फ़ाइल} other {# फ़ाइलें}}', 'hi', { count: 0 })).toBe('0 फ़ाइल');
  });

  it('matches an exact number before a category, and fills names inside a choice', () => {
    const text = '{count, plural, =0 {Nothing in {queue}} one {# file in {queue}} other {# files in {queue}}}';
    expect(fill(text, 'en', { count: 0, queue: 'Main' })).toBe('Nothing in Main');
    expect(fill(text, 'en', { count: 3, queue: 'Main' })).toBe('3 files in Main');
  });

  it('keeps Western digits, as sizes and times do', () => {
    expect(fill('{count} left', 'ar', { count: 1234 })).toMatch(/^1.234 left$|^1,234 left$|^1٬234 left$/);
    expect(fill('{count} left', 'hi', { count: 7 })).toBe('7 left');
  });
});

group('translate', () => {
  const catalogues = { es: { 'Add URL': 'Añadir URL', [FILES]: '{count, plural, one {# archivo} other {# archivos}}' } };

  it('uses the catalogue, and falls back to English', () => {
    expect(translate(catalogues, 'es', 'Add URL')).toBe('Añadir URL');
    expect(translate(catalogues, 'es', FILES, { count: 2 })).toBe('2 archivos');
    expect(translate(catalogues, 'es', 'Not translated yet')).toBe('Not translated yet');
    expect(translate(catalogues, 'ur', 'Add URL')).toBe('Add URL');
    expect(translate(catalogues, 'en', FILES, { count: 1 })).toBe('1 file');
  });
});

group('placeholders', () => {
  it('lists names inside plurals too', () => {
    expect(placeholders('Saved {name} to {folder}')).toEqual(['name', 'folder']);
    expect(placeholders('{count, plural, one {# in {queue}} other {# in {queue}}}')).toEqual([
      'count',
      'queue',
    ]);
  });

  it('finds a plural with no fallback', () => {
    expect(pluralsComplete(FILES)).toBe(true);
    expect(pluralsComplete('{count, plural, one {# file}}')).toBe(false);
    expect(pluralsComplete('{count, plural, one {# file} other {# files}')).toBe(false);
  });
});

group('isolate', () => {
  it('keeps what fills a right-to-left sentence in its own direction', () => {
    expect(fill('{name} محفوظ ہو گیا', 'ur', { name: 'a.zip' })).toBe('\u2068a.zip\u2069 محفوظ ہو گیا');
    expect(fill('Saved {name}', 'en', { name: 'a.zip' })).toBe('Saved a.zip');
    // Numbers read the same either way.
    expect(fill('{count} منٹ', 'ur', { count: 3 })).toBe('3 منٹ');
  });
});

group('pickLanguage', () => {
  it('prefers the choice, then Windows’ languages, then English', () => {
    expect(pickLanguage('ur', ['en-US'])).toBe('ur');
    expect(pickLanguage(null, ['fr-FR', 'es-MX'])).toBe('es');
    expect(pickLanguage('', ['ar_SA'])).toBe('ar');
    expect(pickLanguage('xx', ['de-DE'])).toBe('en');
  });
});

group('describe', () => {
  const catalogues = {
    ur: {
      "the torrent couldn't be added: {error}": 'ٹورینٹ شامل نہیں ہو سکا: {error}',
      'the server answered {status}': 'سرور نے جواب دیا: {status}',
      'the file is no longer there': 'فائل اب وہاں نہیں ہے',
      '{name}: {reason}': '{name}: {reason}',
    },
  };

  it('finds the pattern English from the app follows, and translates the parts too', () => {
    const error = "the torrent couldn't be added: the server answered 404 Not Found";
    expect(describe(catalogues, 'ur', error)).toBe(
      'ٹورینٹ شامل نہیں ہو سکا: \u2068سرور نے جواب دیا: \u2068404 Not Found\u2069\u2069',
    );
    expect(describe(catalogues, 'ur', 'the file is no longer there')).toBe('فائل اب وہاں نہیں ہے');
    expect(describe(catalogues, 'en', error)).toBe(error);
  });

  it('leaves text it doesn’t know as it is, and ignores patterns with no words to anchor on', () => {
    expect(describe(catalogues, 'ur', 'yt-dlp: ERROR: video unavailable')).toBe(
      'yt-dlp: ERROR: video unavailable',
    );
    expect(describe(catalogues, 'ur', new Error('boom'))).toBe('boom');
    expect(describe(catalogues, 'ur', 'a.zip: gone')).toBe('a.zip: gone');
  });

  it('says each line of several on its own', () => {
    expect(describe(catalogues, 'ur', 'the file is no longer there\n\nsomething else')).toBe(
      'فائل اب وہاں نہیں ہے\n\nsomething else',
    );
  });
});
