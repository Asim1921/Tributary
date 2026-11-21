// Translation without a framework. English text is the key: a catalogue maps it to the
// same text in another language, and anything missing falls back to English.
//
// Text can hold `{name}`, filled from the values given, and
// `{count, plural, one {# file} other {# files}}`, chosen by the language's own plural
// rules (`=0 {…}` matches an exact number first). `#` is the number itself.

export type Values = Record<string, string | number | null | undefined>;
export type Catalogue = Record<string, string>;

export interface Language {
  code: string;
  /** Its own name, as people who speak it write it. */
  name: string;
  /** English name, for the code and the tests. */
  english: string;
  dir: 'ltr' | 'rtl';
}

export const LANGUAGES: readonly Language[] = [
  { code: 'en', name: 'English', english: 'English', dir: 'ltr' },
  { code: 'ur', name: 'اردو', english: 'Urdu', dir: 'rtl' },
  { code: 'ar', name: 'العربية', english: 'Arabic', dir: 'rtl' },
  { code: 'hi', name: 'हिन्दी', english: 'Hindi', dir: 'ltr' },
  { code: 'es', name: 'Español', english: 'Spanish', dir: 'ltr' },
];

export function languageOf(code: string): Language {
  return LANGUAGES.find((language) => language.code === code) ?? LANGUAGES[0]!;
}

/**
 * The language to show: the one chosen in Options, or else the first of Windows' own
 * that Tributary has, or else English.
 */
export function pickLanguage(chosen: string | null | undefined, system: readonly string[]): string {
  const known = (code: string) => LANGUAGES.some((language) => language.code === code);
  if (chosen && known(chosen)) return chosen;
  for (const tag of system) {
    const base = tag.split(/[-_]/, 1)[0]?.toLowerCase() ?? '';
    if (known(base)) return base;
  }
  return 'en';
}

const pluralRules = new Map<string, Intl.PluralRules>();
const numberFormats = new Map<string, Intl.NumberFormat>();

function pluralCategory(lang: string, count: number): string {
  let rules = pluralRules.get(lang);
  if (!rules) {
    rules = new Intl.PluralRules(lang);
    pluralRules.set(lang, rules);
  }
  return rules.select(count);
}

/** Numbers keep Western digits in every language, like the sizes and times beside them. */
export function formatNumber(value: number, lang: string): string {
  let format = numberFormats.get(lang);
  if (!format) {
    format = new Intl.NumberFormat(lang, { numberingSystem: 'latn', maximumFractionDigits: 2 });
    numberFormats.set(lang, format);
  }
  return format.format(value);
}

/** The index of the `}` closing the `{` at `open`, or -1. */
function closing(text: string, open: number): number {
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    if (text[index] === '{') depth += 1;
    else if (text[index] === '}') {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  return -1;
}

/** A plural's choices, `selector {text}` after the second comma. */
function choices(text: string): Map<string, string> {
  const found = new Map<string, string>();
  let index = 0;
  while (index < text.length) {
    while (index < text.length && /\s/.test(text[index]!)) index += 1;
    const open = text.indexOf('{', index);
    if (open < 0) break;
    const selector = text.slice(index, open).trim();
    const end = closing(text, open);
    if (end < 0) break;
    found.set(selector, text.slice(open + 1, end));
    index = end + 1;
  }
  return found;
}

/**
 * Text filled into a right-to-left sentence keeps its own direction, so a server's
 * "404 Not Found" or a file name doesn't come out back to front. The marks are Unicode's
 * first-strong isolate and pop directional isolate, and don't show.
 */
export function isolate(text: string, lang: string): string {
  return languageOf(lang).dir === 'rtl' ? `\u2068${text}\u2069` : text;
}

function show(value: Values[string], name: string, lang: string): string {
  if (value === null || value === undefined) return `{${name}}`;
  return typeof value === 'number' ? formatNumber(value, lang) : isolate(value, lang);
}

function placeholder(inner: string, lang: string, values: Values): string {
  const parts = inner.split(',');
  const name = parts[0]!.trim();
  if (parts.length < 3 || parts[1]!.trim() !== 'plural') return show(values[name], name, lang);

  const rest = inner.slice(inner.indexOf(',', inner.indexOf(',') + 1) + 1);
  const options = choices(rest);
  const count = Number(values[name] ?? 0);
  const chosen =
    options.get(`=${count}`) ?? options.get(pluralCategory(lang, count)) ?? options.get('other') ?? '';
  return fill(chosen.replaceAll('#', formatNumber(count, lang)), lang, values);
}

/** Fills a text's placeholders, choosing plurals by `lang`'s rules. */
export function fill(template: string, lang: string, values: Values = {}): string {
  let out = '';
  let index = 0;
  while (index < template.length) {
    const open = template.indexOf('{', index);
    if (open < 0) {
      out += template.slice(index);
      break;
    }
    out += template.slice(index, open);
    const end = closing(template, open);
    if (end < 0) {
      out += template.slice(open);
      break;
    }
    out += placeholder(template.slice(open + 1, end), lang, values);
    index = end + 1;
  }
  return out;
}

/** `text` in `lang`, from its catalogue, or in English if it has none. */
export function translate(
  catalogues: Readonly<Record<string, Catalogue>>,
  lang: string,
  text: string,
  values?: Values,
): string {
  const translated = lang === 'en' ? undefined : catalogues[lang]?.[text];
  return translated ? fill(translated, lang, values) : fill(text, 'en', values);
}

/** The names a text fills in, in order of first use, for checking translations. */
export function placeholders(template: string): string[] {
  const names = new Set<string>();
  const walk = (text: string) => {
    let index = 0;
    while (index < text.length) {
      const open = text.indexOf('{', index);
      if (open < 0) return;
      const end = closing(text, open);
      if (end < 0) return;
      const inner = text.slice(open + 1, end);
      const parts = inner.split(',');
      names.add(parts[0]!.trim());
      if (parts.length >= 3 && parts[1]!.trim() === 'plural') {
        const rest = inner.slice(inner.indexOf(',', inner.indexOf(',') + 1) + 1);
        for (const option of choices(rest).values()) walk(option);
      }
      index = end + 1;
    }
  };
  walk(template);
  return [...names];
}

/** Whether every plural in a text has the `other` choice every language falls back on. */
export function pluralsComplete(template: string): boolean {
  let complete = true;
  const walk = (text: string) => {
    let index = 0;
    while (index < text.length) {
      const open = text.indexOf('{', index);
      if (open < 0) return;
      const end = closing(text, open);
      if (end < 0) {
        complete = false;
        return;
      }
      const inner = text.slice(open + 1, end);
      const parts = inner.split(',');
      if (parts.length >= 3 && parts[1]!.trim() === 'plural') {
        const options = choices(inner.slice(inner.indexOf(',', inner.indexOf(',') + 1) + 1));
        if (!options.has('other')) complete = false;
        for (const option of options.values()) walk(option);
      }
      index = end + 1;
    }
  };
  walk(template);
  return complete;
}

/** A catalogue's texts with `{names}` in them, as patterns to find in English from the app. */
interface Pattern {
  text: string;
  names: string[];
  expression: RegExp;
  literal: number;
}

const patternCache = new WeakMap<Catalogue, Pattern[]>();

function patternsOf(catalogue: Catalogue): Pattern[] {
  let patterns = patternCache.get(catalogue);
  if (patterns) return patterns;
  patterns = [];
  for (const text of Object.keys(catalogue)) {
    // Plurals are written by the window itself, never sent by the app.
    if (!text.includes('{') || text.includes(', plural,')) continue;
    const names: string[] = [];
    let source = '';
    let literal = 0;
    for (const part of text.split(/(\{\w+\})/)) {
      const name = /^\{(\w+)\}$/.exec(part)?.[1];
      if (name) {
        names.push(name);
        source += '([\\s\\S]+?)';
      } else {
        literal += part.replace(/[^\p{L}]/gu, '').length;
        source += part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      }
    }
    // Without words to anchor on, or with names side by side, it would match anything.
    if (literal === 0 || /\}\{/.test(text)) continue;
    patterns.push({ text, names, expression: new RegExp(`^${source}$`), literal });
  }
  patterns.sort((a, b) => b.literal - a.literal);
  patternCache.set(catalogue, patterns);
  return patterns;
}

/**
 * English from the app, such as an error, in `lang`: the catalogue's own text if it has
 * it, or else the pattern it follows, with the parts filled in translated too. Text the
 * catalogue doesn't know, such as a server's own words, stays as it is.
 */
export function describe(
  catalogues: Readonly<Record<string, Catalogue>>,
  lang: string,
  error: unknown,
): string {
  const raw = (error instanceof Error ? error.message : String(error)).trim();
  const catalogue = catalogues[lang];
  if (lang === 'en' || !catalogue) return raw;
  const say = (text: string, depth: number): string => {
    const own = catalogue[text];
    if (own) return fill(own, lang);
    if (depth > 4) return text;
    for (const pattern of patternsOf(catalogue)) {
      const match = pattern.expression.exec(text);
      if (!match) continue;
      const values: Values = {};
      pattern.names.forEach((name, index) => {
        values[name] = say(match[index + 1]!, depth + 1);
      });
      return fill(catalogue[pattern.text]!, lang, values);
    }
    return text;
  };
  // Each line on its own, as several messages can come together.
  return raw
    .split('\n')
    .map((line) => say(line, 0))
    .join('\n');
}
