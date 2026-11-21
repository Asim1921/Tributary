import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

const FOLDER = import.meta.dirname;
const LANGUAGES = readdirSync(join(FOLDER, '_locales'));
const messages = (language) =>
  JSON.parse(readFileSync(join(FOLDER, '_locales', language, 'messages.json'), 'utf8'));
const english = messages('en');

describe('the extension speaks', () => {
  it('English and the four other languages the app does', () => {
    expect(LANGUAGES.sort()).toEqual(['ar', 'en', 'es', 'hi', 'ur']);
  });

  it('every message the code asks for, in English', () => {
    const asked = new Set();
    for (const file of ['background.js', 'content.js', 'popup.js', 'rules.js', 'popup.html', 'manifest.json']) {
      const source = readFileSync(join(FOLDER, file), 'utf8');
      for (const match of source.matchAll(/getMessage\('(\w+)'|say\('(\w+)'|data-say="(\w+)"|__MSG_(\w+)__/g)) {
        asked.add(match.slice(1).find(Boolean));
      }
    }
    expect(asked.size).toBeGreaterThan(15);
    for (const name of asked) expect(english[name], name).toBeDefined();
  });

  it.each(LANGUAGES)('every message in %s, with the same placeholders', (language) => {
    const own = messages(language);
    expect(Object.keys(own).sort()).toEqual(Object.keys(english).sort());
    for (const [name, entry] of Object.entries(english)) {
      expect(own[name].message.trim(), name).not.toBe('');
      expect(own[name].placeholders ?? null, name).toEqual(entry.placeholders ?? null);
      const used = (text) => [...text.matchAll(/\$(\w+)\$/g)].map((match) => match[1].toLowerCase()).sort();
      expect(used(own[name].message), name).toEqual(used(entry.message));
    }
  });
});
