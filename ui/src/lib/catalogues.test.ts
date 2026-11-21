import { describe, expect, it } from 'vitest';

// @ts-expect-error: a plain script, shared with `node scripts/text-keys.mjs`.
import { LANGUAGES, catalogue, textKeys } from '../../../scripts/text-keys.mjs';
import { placeholders, pluralsComplete } from './i18n';

const keys = [...(textKeys() as Map<string, Set<string>>).keys()];
const tags = (text: string) => [...text.matchAll(/<\/?(\w+)/g)].map((match) => match[0]).sort();
const sorted = (names: string[]) => [...names].sort();

describe('the English texts', () => {
  it('are found, and each plural has its fallback', () => {
    expect(keys.length).toBeGreaterThan(400);
    for (const key of keys) expect(pluralsComplete(key), key).toBe(true);
  });
});

describe.each(LANGUAGES as string[])('the %s catalogue', (language) => {
  const own = catalogue(language) as Record<string, string>;

  it('says every text', () => {
    const missing = keys.filter((key) => !own[key]?.trim());
    expect(missing).toEqual([]);
  });

  it('has nothing the app no longer says', () => {
    const stale = Object.keys(own).filter((key) => !keys.includes(key));
    expect(stale).toEqual([]);
  });

  it('fills in the same names, keeps its plurals whole and its tags the same', () => {
    for (const [key, text] of Object.entries(own)) {
      if (!keys.includes(key)) continue;
      expect(sorted(placeholders(text)), `${key} → ${text}`).toEqual(sorted(placeholders(key)));
      expect(pluralsComplete(text), text).toBe(true);
      expect(tags(text), text).toEqual(tags(key));
    }
  });
});
