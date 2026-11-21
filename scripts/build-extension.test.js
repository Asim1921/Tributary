import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { FIREFOX_ID, forStore, manifestFor } from './build-extension.mjs';

const manifest = {
  manifest_version: 3,
  name: 'Tributary',
  version: '0.2.0',
  key: 'secret-looking-but-public',
  background: { service_worker: 'background.js', type: 'module' },
};

describe('building the extension elsewhere', () => {
  it('gives Firefox background scripts and its add-on ID', () => {
    const firefox = manifestFor('firefox', manifest);
    expect(firefox.background).toEqual({ scripts: ['background.js'], type: 'module' });
    expect(firefox.browser_specific_settings.gecko.id).toBe(FIREFOX_ID);
    expect(firefox.key).toBeUndefined();
  });

  it('keeps the store build as it is, without the hand-loaded key', () => {
    const store = manifestFor('store', manifest);
    expect(store.background).toEqual(manifest.background);
    expect(store.key).toBeUndefined();
    expect(manifest.key).toBeDefined();
  });

  it('turns the store switch on', () => {
    expect(forStore('const STORE_BUILD = false;\nrest')).toBe('const STORE_BUILD = true;\nrest');
    expect(forStore('nothing here')).toBe('nothing here');
  });
});

describe('the extension itself', () => {
  it('has the store switch, off, in both scripts the store build turns it on in', () => {
    for (const file of ['background.js', 'content.js']) {
      const source = readFileSync(join(import.meta.dirname, '..', 'extension', file), 'utf8');
      expect(source, file).toContain('const STORE_BUILD = false;');
      expect(forStore(source), file).toContain('const STORE_BUILD = true;');
    }
  });
});
