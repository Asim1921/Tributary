// The language the window speaks, and `t` to say things in it. The app decides the
// language (the one chosen in Options, or else Windows' own) and tells the window.

import ar from '../locales/ar.json';
import es from '../locales/es.json';
import hi from '../locales/hi.json';
import ur from '../locales/ur.json';
import {
  type Catalogue,
  type Values,
  describe,
  formatNumber,
  languageOf,
  translate,
} from './i18n';

const catalogues: Record<string, Catalogue> = { ar, es, hi, ur };

const current = $state({ code: 'en' });

/** `text` in the window's language. English text is the key. */
export function t(text: string, values?: Values): string {
  return translate(catalogues, current.code, text, values);
}

const ENTITIES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

/**
 * Like `t`, for a sentence with `<strong>`, `<code>` or `<kbd>` in it, to show with
 * `{@html}`. The values are escaped; the text is the app's own, and a test checks each
 * translation keeps the same tags.
 */
export function tMarkup(text: string, values: Values = {}): string {
  const safe: Values = {};
  for (const [name, value] of Object.entries(values)) {
    safe[name] = typeof value === 'string' ? value.replace(/[&<>"']/g, (c) => ENTITIES[c]!) : value;
  }
  return t(text, safe);
}

/**
 * Marks text for translation where it's written, to be passed through `t` where it's
 * shown, such as a toolbar button's label kept in a list.
 */
export function mark<T extends string>(text: T): T {
  return text;
}

/** Any error, or a message from the app, in the window's language. */
export function sayError(error: unknown): string {
  return describe(catalogues, current.code, error);
}

export function number(value: number): string {
  return formatNumber(value, current.code);
}

export function languageCode(): string {
  return current.code;
}

/** Speaks `code` from now on, and lays the page out right to left if it reads that way. */
export function useLanguage(code: string) {
  const language = languageOf(code);
  current.code = language.code;
  if (typeof document !== 'undefined') {
    document.documentElement.lang = language.code;
    document.documentElement.dir = language.dir;
  }
}
