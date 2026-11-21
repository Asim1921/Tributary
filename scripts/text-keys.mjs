// Finds every text the user can read, so each language's catalogue in ui/src/locales can be
// checked against it: in the window's code, what's passed to t(), mark() and tMarkup(); in
// the Rust code, what's in text!(), en!(), say!() and #[error(...)], leaving out tests.
//
//   node scripts/text-keys.mjs           lists what each language is missing
//   node scripts/text-keys.mjs --keys    prints every text as JSON
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

export const ROOT = fileURLToPath(new URL('..', import.meta.url));
export const LANGUAGES = ['ur', 'ar', 'hi', 'es'];

const WINDOW_TEXT = /\b(?:t|mark|tMarkup)\(\s*(['"])((?:\\.|(?!\1)[^\\])*)\1/g;
const RUST_TEXT = /\b(?:text|en|say)!\(\s*"((?:\\.|[^"\\])*)"/g;
const RUST_ERROR = /#\[error\(\s*"((?:\\.|[^"\\])*)"/g;

function files(folder, extensions) {
  const found = [];
  for (const entry of readdirSync(folder, { withFileTypes: true })) {
    const path = join(folder, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'node_modules' && entry.name !== 'tests') found.push(...files(path, extensions));
    } else if (extensions.some((extension) => entry.name.endsWith(extension))) {
      found.push(path);
    }
  }
  return found;
}

function unescapeWindow(text) {
  return text.replace(/\\(.)/g, (_, character) => (character === 'n' ? '\n' : character));
}

function unescapeRust(text) {
  return text
    .replace(/\\\r?\n\s*/g, '')
    .replace(/\\(.)/g, (_, character) => (character === 'n' ? '\n' : character));
}

/** Every text, with where it's used. */
export function textKeys() {
  const keys = new Map();
  const add = (key, where) => {
    if (!keys.has(key)) keys.set(key, new Set());
    keys.get(key).add(relative(ROOT, where).replaceAll('\\', '/'));
  };

  for (const path of files(join(ROOT, 'ui', 'src'), ['.svelte', '.ts'])) {
    if (path.endsWith('.test.ts')) continue;
    const source = readFileSync(path, 'utf8');
    for (const match of source.matchAll(WINDOW_TEXT)) add(unescapeWindow(match[2]), path);
  }

  const rustFolders = [join(ROOT, 'src-tauri', 'src')];
  for (const crate of readdirSync(join(ROOT, 'crates'))) rustFolders.push(join(ROOT, 'crates', crate, 'src'));
  for (const folder of rustFolders) {
    for (const path of files(folder, ['.rs'])) {
      let source = readFileSync(path, 'utf8');
      // Tests sit at the end of each file.
      const tests = source.indexOf('#[cfg(test)]');
      if (tests >= 0) source = source.slice(0, tests);
      // The macros' own definitions aren't texts.
      source = source.replace(/macro_rules! \w+ \{[\s\S]*?\n\}/g, '');
      for (const match of source.matchAll(RUST_TEXT)) add(unescapeRust(match[1]), path);
      for (const match of source.matchAll(RUST_ERROR)) {
        // `{0}` alone is someone else's words, passed on as they are.
        if (!/^\{\w*\}$/.test(match[1])) add(unescapeRust(match[1]), path);
      }
    }
  }
  return keys;
}

export function catalogue(language) {
  return JSON.parse(readFileSync(join(ROOT, 'ui', 'src', 'locales', `${language}.json`), 'utf8'));
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const keys = textKeys();
  if (process.argv.includes('--keys')) {
    console.log(JSON.stringify([...keys.keys()].sort(), null, 2));
  } else {
    console.log(`${keys.size} texts`);
    for (const language of LANGUAGES) {
      const own = catalogue(language);
      const missing = [...keys.keys()].filter((key) => !own[key]);
      const stale = Object.keys(own).filter((key) => !keys.has(key));
      console.log(`${language}: ${missing.length} missing, ${stale.length} no longer used`);
    }
  }
}
