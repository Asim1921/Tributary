import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

const FOLDER = import.meta.dirname;

/** The extension's files, which all have to reach the installed copy. */
function runtimeFiles(folder = FOLDER, prefix = '') {
  return readdirSync(folder, { withFileTypes: true }).flatMap((entry) => {
    const path = `${prefix}${entry.name}`;
    if (entry.isDirectory()) return runtimeFiles(join(folder, entry.name), `${path}/`);
    return entry.name.endsWith('.test.js') ? [] : [path];
  });
}

describe('the installer', () => {
  const config = JSON.parse(readFileSync(join(FOLDER, '../src-tauri/tauri.conf.json'), 'utf8'));
  const bundled = config.bundle.resources;

  // Tests run against this folder, so a file left off the list would only be missed
  // after installing.
  it('carries every file of the extension', () => {
    for (const file of runtimeFiles()) {
      expect(bundled[`../extension/${file}`], file).toBe(`extension/${file}`);
    }
  });

  it('lists nothing that no longer exists', () => {
    const files = new Set(runtimeFiles());
    for (const source of Object.keys(bundled).filter((path) => path.startsWith('../extension/'))) {
      expect(files.has(source.replace('../extension/', '')), source).toBe(true);
    }
  });
});

describe('the content script', () => {
  // It can't import rules.js, so it holds its own copy of the smallest player size.
  it('uses the same smallest player size as the rules', async () => {
    const { MIN_PLAYER_HEIGHT, MIN_PLAYER_WIDTH } = await import('./rules.js');
    const script = readFileSync(join(FOLDER, 'content.js'), 'utf8');

    expect(script).toContain(`const MIN_PLAYER_WIDTH = ${MIN_PLAYER_WIDTH};`);
    expect(script).toContain(`const MIN_PLAYER_HEIGHT = ${MIN_PLAYER_HEIGHT};`);
  });

  it('says where the tab is now, because the browser only knows where it started', () => {
    const script = readFileSync(join(FOLDER, 'content.js'), 'utf8');
    const background = readFileSync(join(FOLDER, 'background.js'), 'utf8');
    const savePage = background.slice(background.indexOf('async function savePage'));

    expect(script).toContain('page: location.href');
    expect(savePage.slice(0, savePage.indexOf('\n}\n'))).not.toContain('sender.url');
  });

  it('only ever sends a page for a trusted click', () => {
    const script = readFileSync(join(FOLDER, 'content.js'), 'utf8');
    const sends = script.indexOf("type: 'save-page'");
    const before = script.slice(Math.max(0, sends - 260), sends);

    expect(sends).toBeGreaterThan(-1);
    expect(before).toContain('event.isTrusted');
  });
});

describe('the manifest', () => {
  const manifest = JSON.parse(readFileSync(join(FOLDER, 'manifest.json'), 'utf8'));

  it('points at files that exist', () => {
    const files = new Set(runtimeFiles());
    const named = [
      manifest.background.service_worker,
      manifest.action.default_popup,
      ...manifest.content_scripts.flatMap((script) => script.js),
      ...Object.values(manifest.icons),
    ];

    for (const file of named) expect(files.has(file), file).toBe(true);
  });

  it('only observes web requests and never blocks or changes them', () => {
    expect(manifest.permissions).toContain('webRequest');
    expect(manifest.permissions).not.toContain('webRequestBlocking');
    expect(manifest.permissions).not.toContain('declarativeNetRequest');
  });
});
