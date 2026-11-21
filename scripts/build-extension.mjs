// Builds the extension for somewhere other than a hand-loaded Chrome or Edge, from the same
// files in extension/, into dist/<target>/ and a zip beside it.
//
//   node scripts/build-extension.mjs firefox   Firefox: background scripts instead of a
//                                               service worker, and its add-on ID.
//   node scripts/build-extension.mjs store     The Chrome Web Store and Edge Add-ons: the
//                                               same, but with no button on YouTube, which
//                                               their rules forbid.
//
// Neither is signed or submitted here: Mozilla's add-ons site signs the Firefox one, and
// the stores review theirs. Those accounts are the publisher's own.
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..');
const SOURCE = join(ROOT, 'extension');

/** Fixed, so the app can name it as the only add-on allowed to talk to it. */
export const FIREFOX_ID = '{8e47a714-dbc1-470d-a416-9dddedafd3a6}';

const FILES = ['background.js', 'content.js', 'popup.html', 'popup.js', 'rules.js', 'icons'];

/** The manifest for each target, from the one hand-loaded copies use. */
export function manifestFor(target, manifest) {
  const copy = structuredClone(manifest);
  // Only Chrome reads the key, to keep the ID the app expects for a hand-loaded copy.
  delete copy.key;
  if (target === 'firefox') {
    copy.background = { scripts: ['background.js'], type: 'module' };
    copy.browser_specific_settings = { gecko: { id: FIREFOX_ID, strict_min_version: '128.0' } };
  }
  return copy;
}

/** Turns the store build's switch on, in a file that has one. */
export function forStore(source) {
  return source.replace('const STORE_BUILD = false;', 'const STORE_BUILD = true;');
}

function build(target) {
  const out = join(ROOT, 'dist', target);
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  for (const file of FILES) cpSync(join(SOURCE, file), join(out, file), { recursive: true });

  const manifest = JSON.parse(readFileSync(join(SOURCE, 'manifest.json'), 'utf8'));
  writeFileSync(join(out, 'manifest.json'), `${JSON.stringify(manifestFor(target, manifest), null, 2)}\n`);
  if (target === 'store') {
    for (const file of ['background.js', 'content.js']) {
      const path = join(out, file);
      const changed = forStore(readFileSync(path, 'utf8'));
      if (!changed.includes('const STORE_BUILD = true;')) throw new Error(`${file} has no store switch`);
      writeFileSync(path, changed);
    }
  }

  const zip = join(dirname(out), `tributary-${target}-${manifest.version}.zip`);
  rmSync(zip, { force: true });
  // Windows' own tar writes zip files.
  execFileSync('tar', ['-a', '-c', '-f', zip, '-C', out, '.'], { stdio: 'inherit' });
  console.log(`built ${out}\n  and ${zip}`);
}

if (import.meta.main ?? process.argv[1] === import.meta.filename) {
  const target = process.argv[2];
  if (!['firefox', 'store'].includes(target)) {
    console.error('usage: node scripts/build-extension.mjs firefox|store');
    process.exit(2);
  }
  build(target);
}
