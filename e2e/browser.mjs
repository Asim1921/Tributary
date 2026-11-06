// Tests the real extension in real Microsoft Edge, against the real app.
//
//   npm run e2e:browser
//
// Uses a throwaway Edge profile, so the normal one isn't touched. Edge's download folder
// is a temporary one, and the browser registration is restored afterwards.

import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFileSync,
  createReadStream,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { chromium } from 'playwright-core';

const ROOT = resolve(import.meta.dirname, '..');
const APP = join(ROOT, 'target', 'release', 'tributary.exe');
const SERVER = join(ROOT, 'target', 'release', 'testserver.exe');
const HOST = join(ROOT, 'target', 'release', 'tributary-host.exe');
const EXTENSION = join(ROOT, 'extension');
const EDGE = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';

const SERVER_PORT = 8792;
const APP_DEBUG_PORT = 9336;
const EDGE_DEBUG_PORT = 9446;
const BASE = `http://127.0.0.1:${SERVER_PORT}`;
const PIPE = `\\\\.\\pipe\\tributary-browser-${process.pid}`;
const EXTENSION_ID = 'khhpmnkkeejcjhbnknnghhahncphbobk';
const REGISTRY_KEY = 'HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\app.tributary.capture';
const SCREENSHOTS = process.env.E2E_SCREENSHOTS;

const dataDir = mkdtempSync(join(tmpdir(), 'tributary-browser-data-'));
const appFolder = mkdtempSync(join(tmpdir(), 'tributary-browser-files-'));
const edgeFolder = mkdtempSync(join(tmpdir(), 'tributary-browser-edge-'));
const profile = mkdtempSync(join(tmpdir(), 'tributary-browser-profile-'));

const serverLog = join(dataDir, 'server.jsonl');
const results = [];
let appPage;

async function step(name, run) {
  const started = Date.now();
  try {
    await run();
    results.push(true);
    console.log(`ok   ${name} (${Date.now() - started} ms)`);
  } catch (error) {
    results.push(false);
    console.log(`FAIL ${name}\n     ${String(error.message ?? error).split('\n').join('\n     ')}`);
    const requests = existsSync(serverLog) ? readFileSync(serverLog, 'utf8').trim().split('\n') : [];
    for (const line of requests.slice(-8)) {
      const { target, status, body_bytes_sent: sent, outcome } = JSON.parse(line);
      console.log(`     server saw ${target}: ${status}, ${sent} bytes, ${outcome}`);
    }
    console.log(`     last click: ${JSON.stringify(edgeTools?.lastClick)}`);
    console.log(`     Edge's folder: ${readdirSync(edgeFolder).join(', ') || 'empty'}`);
    if (SCREENSHOTS) {
      mkdirSync(SCREENSHOTS, { recursive: true });
      await appPage?.screenshot({ path: join(SCREENSHOTS, 'browser-failure-app.png') }).catch(() => {});
    }
    throw error;
  }
}

async function sha256(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

const expectedSha256 = (name) => execFileSync(SERVER, ['sha256', name], { encoding: 'utf8' }).trim();

async function connect(port, pick) {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      const page = pick(browser);
      if (page) return { browser, page };
      await browser.close();
    } catch {
      // Still starting.
    }
    await sleep(500);
  }
  throw new Error(`nothing answered on port ${port}`);
}

/**
 * Plain DevTools commands. Playwright can't be used on Edge here, because connecting it
 * takes over the browser's download handling, which is the very thing under test.
 */
class DevTools {
  #socket;
  #next = 1;
  #waiting = new Map();

  static async open(url) {
    const tools = new DevTools();
    tools.#socket = new WebSocket(url);
    await new Promise((done, fail) => {
      tools.#socket.onopen = done;
      tools.#socket.onerror = () => fail(new Error(`could not connect to ${url}`));
    });
    tools.#socket.onmessage = ({ data }) => {
      const message = JSON.parse(data);
      const pending = tools.#waiting.get(message.id);
      if (pending) {
        tools.#waiting.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result);
      }
    };
    return tools;
  }

  send(method, params = {}) {
    const id = this.#next++;
    this.#socket.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => this.#waiting.set(id, { resolve, reject }));
  }

  async evaluate(expression) {
    const { result, exceptionDetails } = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? 'script failed');
    return result.value;
  }

  /** Opens a page and waits until it has finished loading, wherever it redirects to. */
  async open(url, landsOn = url) {
    await this.send('Page.navigate', { url });
    for (let attempt = 0; attempt < 100; attempt += 1) {
      const ready = await this.evaluate(
        `location.href === ${JSON.stringify(landsOn)} && document.readyState === 'complete'`,
      ).catch(() => false);
      if (ready) return;
      await sleep(50);
    }
    throw new Error(`${url} never finished loading`);
  }

  /** Waits until a script's value is true. */
  async waitFor(expression, timeoutMs = 8000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (await this.evaluate(expression).catch(() => false)) return;
      await sleep(50);
    }
    throw new Error(`never true: ${expression}`);
  }

  /** A real mouse click at a place on the page. */
  async clickAt(x, y) {
    const event = { x, y, modifiers: 0, button: 'left', clickCount: 1 };
    await this.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, modifiers: 0 });
    await this.send('Input.dispatchMouseEvent', { type: 'mousePressed', ...event });
    await this.send('Input.dispatchMouseEvent', { type: 'mouseReleased', ...event });
  }

  /** Closes the tab this is connected to. */
  async closeTarget() {
    await this.send('Page.close').catch(() => {});
    this.close();
  }

  /** A real mouse click, so the page sees any held keys. Modifiers: Alt 1, Ctrl 2, Shift 8. */
  async click(linkText, modifiers = 0) {
    const locate = `(() => {
      const link = [...document.links].find((item) => item.textContent.trim() === ${JSON.stringify(linkText)});
      if (!link) return null;
      link.scrollIntoView({ block: 'center' });
      const rect = link.getBoundingClientRect();
      const x = rect.left + rect.width / 2;
      const y = rect.top + rect.height / 2;
      window.clickReached = false;
      link.addEventListener('click', () => { window.clickReached = true; }, { capture: true, once: true });
      return {
        x,
        y,
        visibility: document.visibilityState,
        focused: document.hasFocus(),
        onLink: document.elementFromPoint(x, y) === link,
      };
    })()`;

    let target = null;
    for (let attempt = 0; attempt < 60 && !target; attempt += 1) {
      target = await this.evaluate(locate).catch(() => null);
      if (!target) await sleep(100);
    }
    if (!target) throw new Error(`no link called ${linkText}`);

    const { x, y } = target;
    const event = { x, y, modifiers, button: 'left', clickCount: 1 };
    await this.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, modifiers });
    await this.send('Input.dispatchMouseEvent', { type: 'mousePressed', ...event });
    await this.send('Input.dispatchMouseEvent', { type: 'mouseReleased', ...event });

    const reached = await this.evaluate('window.clickReached').catch(() => 'unknown');
    this.lastClick = { linkText, ...target, reached };
  }

  close() {
    this.#socket.close();
  }
}

/** Where the panel is, once the extension has shown it. */
async function waitForPanel(tools) {
  const find = `(() => {
    const panel = document.querySelector('[data-tributary-panel]');
    if (!panel) return null;
    const box = panel.getBoundingClientRect();
    return { x: box.left, y: box.top, width: box.width, height: box.height };
  })()`;

  for (let attempt = 0; attempt < 160; attempt += 1) {
    const box = await tools.evaluate(find).catch(() => null);
    if (box) return box;
    await sleep(50);
  }
  throw new Error('the panel never appeared');
}

/** Opens one of the extension's own pages in a new tab. */
async function openExtensionPage(page) {
  const address = `chrome-extension://${EXTENSION_ID}/${page}`;
  const reply = await fetch(`http://127.0.0.1:${EDGE_DEBUG_PORT}/json/new?${address}`, {
    method: 'PUT',
  });
  const target = await reply.json();
  const tools = await DevTools.open(target.webSocketDebuggerUrl);
  await tools.send('Page.enable');
  await tools.send('Runtime.enable');
  await tools.waitFor(`document.readyState === 'complete'`);
  return tools;
}

async function devToolsTargets(port) {
  const reply = await fetch(`http://127.0.0.1:${port}/json`);
  return reply.json();
}

async function findTarget(port, matches) {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const target = (await devToolsTargets(port)).find(matches);
      if (target) return target;
    } catch {
      // Still starting.
    }
    await sleep(250);
  }
  const seen = await devToolsTargets(port).catch(() => []);
  throw new Error(`target not found. Saw: ${seen.map((item) => `${item.type} ${item.url}`).join(', ')}`);
}

function savedRegistryValue() {
  try {
    const output = execFileSync('reg', ['query', REGISTRY_KEY, '/ve'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    return /REG_SZ\s+(.+)/.exec(output)?.[1]?.trim() ?? null;
  } catch {
    return null;
  }
}

function setRegistryValue(value) {
  execFileSync('reg', ['add', REGISTRY_KEY, '/ve', '/d', value, '/f'], { stdio: 'ignore' });
}

async function waitForFile(folder, name, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(join(folder, name))) return;
    await sleep(100);
  }
  throw new Error(`${name} never appeared. Found: ${readdirSync(folder).join(', ')}`);
}

const previousRegistration = savedRegistryValue();
let server;
let app;
let edge;
let appBrowser;
let edgeTools;

try {
  assert.ok(existsSync(EDGE), 'Microsoft Edge is not installed where expected');

  // A copy with no app beside it, so a closed app can't be started by accident.
  const host = join(dataDir, 'tributary-host.exe');
  copyFileSync(HOST, host);

  const manifest = join(dataDir, 'native-host.json');
  writeFileSync(
    manifest,
    JSON.stringify({
      name: 'app.tributary.capture',
      description: 'Tributary test',
      path: host,
      type: 'stdio',
      allowed_origins: [`chrome-extension://${EXTENSION_ID}/`],
    }),
  );
  setRegistryValue(manifest);

  writeFileSync(
    join(dataDir, 'settings.json'),
    JSON.stringify({ downloadDir: appFolder, showCompleteDialog: false }),
  );

  mkdirSync(join(profile, 'Default'), { recursive: true });
  writeFileSync(
    join(profile, 'Default', 'Preferences'),
    JSON.stringify({ download: { default_directory: edgeFolder, prompt_for_download: false } }),
  );

  server = spawn(SERVER, ['--port', String(SERVER_PORT), '--log', serverLog], { stdio: 'ignore' });
  app = spawn(APP, [], {
    stdio: 'ignore',
    env: {
      ...process.env,
      TRIBUTARY_DATA_DIR: dataDir,
      TRIBUTARY_PIPE: PIPE,
      WEBVIEW2_USER_DATA_FOLDER: join(dataDir, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${APP_DEBUG_PORT}`,
    },
  });

  const connectedApp = await connect(APP_DEBUG_PORT, (browser) => browser.contexts()[0]?.pages()[0]);
  appBrowser = connectedApp.browser;
  appPage = connectedApp.page;
  appPage.setDefaultTimeout(25_000);
  await appPage.getByRole('heading', { name: 'No downloads yet' }).waitFor();

  edge = spawn(
    EDGE,
    [
      `--user-data-dir=${profile}`,
      `--remote-debugging-port=${EDGE_DEBUG_PORT}`,
      `--load-extension=${EXTENSION}`,
      `--disable-extensions-except=${EXTENSION}`,
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-features=msEdgeSidebarV2,msHubApps',
      `${BASE}/`,
    ],
    { stdio: 'ignore', env: { ...process.env, TRIBUTARY_PIPE: PIPE } },
  );

  const pageTarget = await findTarget(
    EDGE_DEBUG_PORT,
    (target) => target.type === 'page' && target.url.startsWith(BASE),
  );
  edgeTools = await DevTools.open(pageTarget.webSocketDebuggerUrl);
  await edgeTools.send('Page.enable');
  await edgeTools.send('Runtime.enable');

  const rowFor = (name) => appPage.locator('tbody tr', { hasText: name });

  await step('the extension loads in Edge', async () => {
    await findTarget(
      EDGE_DEBUG_PORT,
      (item) => item.type === 'service_worker' && item.url.includes(EXTENSION_ID),
    );
    // Its first run sets up what it listens for. Only a brand-new install has this gap.
    await sleep(1500);
  });

  await step('clicking a download link hands it to the app', async () => {
    await edgeTools.open(`${BASE}/`);
    await edgeTools.click('5M.zip');

    await appPage
      .getByRole('heading', { name: 'Download file' })
      .waitFor()
      .catch((error) => {
        const inEdge = readdirSync(edgeFolder).join(', ') || 'nothing';
        throw new Error(`${error.message}\nEdge's download folder has: ${inEdge}`);
      });
    await appPage.getByRole('dialog').getByText('5.0 MB').waitFor();
    await appPage.getByRole('button', { name: 'Start download' }).click();
    await rowFor('5M.zip').getByText('Complete').waitFor();

    assert.equal(await sha256(join(appFolder, '5M.zip')), expectedSha256('5M.zip'));
    await sleep(800);
    const leftovers = readdirSync(edgeFolder).filter((name) => name.includes('5M.zip'));
    assert.deepEqual(leftovers, [], 'Edge should have given the download up');
  });

  await step('the right-click commands send links to the app', async () => {
    const target = await findTarget(
      EDGE_DEBUG_PORT,
      (item) => item.type === 'service_worker' && item.url.includes(EXTENSION_ID),
    );
    const worker = await DevTools.open(target.webSocketDebuggerUrl);

    try {
      // The browser's own menu can't be automated, so these go in just behind it: the
      // entries must exist, and the extension's handler gets what the browser would send.
      for (const id of ['tributary-link', 'tributary-selection', 'tributary-page']) {
        await worker.evaluate(`chrome.contextMenus.update('${id}', {})`);
      }

      const choose = (info) =>
        worker.evaluate(`(async () => {
          const [tab] = await chrome.tabs.query({ url: '${BASE}/*' });
          chrome.contextMenus.onClicked.dispatch(${JSON.stringify(info)}, tab);
        })()`);
      const cancel = async () => {
        await appPage.getByRole('button', { name: 'Cancel' }).click();
        await appPage.getByRole('dialog').waitFor({ state: 'hidden' });
      };
      const offered = appPage.getByRole('dialog').locator('label.check');

      await edgeTools.open(`${BASE}/`);
      await choose({
        menuItemId: 'tributary-link',
        linkUrl: `${BASE}/file/2M.menu`,
        pageUrl: `${BASE}/`,
      });
      await appPage.getByRole('heading', { name: 'Download file' }).waitFor();
      await appPage.getByRole('dialog').getByText('2.0 MB').waitFor();
      await cancel();

      await choose({ menuItemId: 'tributary-page' });
      await appPage.getByRole('heading', { name: 'Download links' }).waitFor();
      const onThePage = await edgeTools.evaluate('new Set([...document.links].map((a) => a.href)).size');
      assert.equal(await offered.count(), onThePage);
      await cancel();

      await edgeTools.evaluate(`(() => {
        const [first, second] = document.links;
        const range = document.createRange();
        range.setStartBefore(first);
        range.setEndAfter(second);
        getSelection().removeAllRanges();
        getSelection().addRange(range);
      })()`);
      await choose({ menuItemId: 'tributary-selection' });
      await appPage.getByRole('heading', { name: 'Download links' }).waitFor();
      assert.equal(await offered.count(), 2);
      await cancel();
    } finally {
      worker.close();
    }
  });

  await step('video on a page gets a panel, and the panel offers it to the app', async () => {
    await edgeTools.open(`${BASE}/video`);

    const box = await waitForPanel(edgeTools);
    assert.ok(box.width > 0 && box.height > 0, 'the panel has no size');

    // A real click near the top of the panel lands on its first entry, the stream.
    await edgeTools.clickAt(box.x + 30, box.y + 14);
    await appPage.getByRole('heading', { name: 'Save video' }).waitFor();
    await appPage.getByRole('radio', { name: '720p' }).waitFor();
    assert.equal(await appPage.getByLabel('Save as').inputValue(), 'Video test page');

    await appPage.getByRole('button', { name: 'Cancel' }).click();
    await appPage.getByRole('dialog').waitFor({ state: 'hidden' });
  });

  await step('a script on the page cannot press the panel', async () => {
    await edgeTools.evaluate(`document.querySelector('[data-tributary-panel]').click()`);
    await sleep(600);
    assert.equal(await appPage.getByRole('dialog').count(), 0);

    const reachable = await edgeTools.evaluate(
      `document.querySelector('[data-tributary-panel]').shadowRoot === null`,
    );
    assert.equal(reachable, true, 'the panel should be closed to the page');
  });

  await step('the media list shows what the page has and offers it to the app', async () => {
    const popup = await openExtensionPage('popup.html');
    try {
      const tabId = await popup.evaluate(`(async () => {
        const [tab] = await chrome.tabs.query({ url: '${BASE}/video' });
        return tab.id;
      })()`);
      await popup.open(`chrome-extension://${EXTENSION_ID}/popup.html?tab=${tabId}`);

      assert.equal(await popup.evaluate(`chrome.action.getBadgeText({ tabId: ${tabId} })`), '2');
      await popup.waitFor(`document.querySelectorAll('#items button').length === 2`);
      assert.deepEqual(
        await popup.evaluate(`[...document.querySelectorAll('#items button')].map((item) => item.textContent)`),
        ['Video stream', '2M.mp4'],
      );

      await popup.evaluate(`document.querySelectorAll('#items button')[1].click()`);
      await appPage.getByRole('heading', { name: 'Download file' }).waitFor();
      await appPage.getByRole('dialog').getByText('2.0 MB').waitFor();
      await appPage.getByRole('button', { name: 'Cancel' }).click();
      await appPage.getByRole('dialog').waitFor({ state: 'hidden' });
    } finally {
      await popup.closeTarget();
    }
  });

  await step('the panel can be hidden, and turned off altogether', async () => {
    await edgeTools.open(`${BASE}/video`);
    const box = await waitForPanel(edgeTools);

    // The last row of the panel is its Hide button.
    await edgeTools.clickAt(box.x + box.width - 14, box.y + box.height - 12);
    await edgeTools.waitFor(`document.querySelector('[data-tributary-panel]') === null`);

    const popup = await openExtensionPage('popup.html');
    const setPanel = (shown) =>
      popup.evaluate(`(async () => {
        const { settings } = await chrome.storage.local.get('settings');
        await chrome.storage.local.set({ settings: { ...settings, videoPanel: ${shown} } });
      })()`);

    try {
      await setPanel(false);
      await edgeTools.open(`${BASE}/video`);
      await sleep(1200);
      assert.equal(
        await edgeTools.evaluate(`document.querySelector('[data-tributary-panel]') === null`),
        true,
        'the panel was turned off',
      );
    } finally {
      await setPanel(true);
      await popup.closeTarget();
    }
  });

  await step('a new page starts with an empty media list', async () => {
    await edgeTools.open(`${BASE}/`);
    assert.equal(await edgeTools.evaluate(`document.querySelector('[data-tributary-panel]')`), null);

    const popup = await openExtensionPage('popup.html');
    try {
      const stored = await popup.evaluate(`(async () => {
        const [tab] = await chrome.tabs.query({ url: '${BASE}/' });
        const key = 'media-' + tab.id;
        return (await chrome.storage.session.get(key))[key] ?? [];
      })()`);
      assert.deepEqual(stored, []);
    } finally {
      await popup.closeTarget();
    }
  });

  await step('a download that needs a site cookie works through the app', async () => {
    await edgeTools.open(`${BASE}/login`, `${BASE}/`);
    await edgeTools.click('Download a file that requires it');

    await appPage.getByRole('heading', { name: 'Download file' }).waitFor();
    // The size only shows if the cookie reached the server, which answers 403 otherwise.
    await appPage.getByRole('dialog').getByText('5.0 MB').waitFor();
    await appPage.getByLabel('Save as').fill('with-cookie.zip');
    await appPage.getByRole('button', { name: 'Start download' }).click();
    await rowFor('with-cookie.zip').getByText('Complete').waitFor();

    assert.equal(await sha256(join(appFolder, 'with-cookie.zip')), expectedSha256('5M.zip'));
  });

  await step('a file type that is not listed stays with the browser', async () => {
    await edgeTools.open(`${BASE}/`);
    await edgeTools.click('5M.txt');

    await waitForFile(edgeFolder, '5M.txt');
    assert.equal(await appPage.getByRole('dialog').count(), 0, 'the app should not be involved');
  });

  await step('holding the skip key leaves a download with the browser', async () => {
    await edgeTools.open(`${BASE}/`);
    await edgeTools.click('5M.pdf', 1);

    await waitForFile(edgeFolder, '5M.pdf');
    assert.equal(await appPage.getByRole('dialog').count(), 0, 'the app should not be involved');
  });

  await step('a download is still caught after the extension has gone to sleep', async () => {
    const isAwake = async () =>
      (await devToolsTargets(EDGE_DEBUG_PORT)).some(
        (item) => item.type === 'service_worker' && item.url.includes(EXTENSION_ID),
      );

    // Browsers stop an idle extension after about 30 seconds.
    const deadline = Date.now() + 70_000;
    while (Date.now() < deadline && (await isAwake())) await sleep(1000);
    assert.equal(await isAwake(), false, 'the extension never went to sleep');

    await edgeTools.open(`${BASE}/`);
    await edgeTools.click('Name from server');

    await appPage.getByRole('heading', { name: 'Download file' }).waitFor();
    await appPage.getByRole('dialog').getByText('5.0 MB').waitFor();
    assert.equal(await appPage.getByLabel('Save as').inputValue(), 'report final.zip');
    await appPage.getByRole('button', { name: 'Cancel' }).click();
    await appPage.getByRole('dialog').waitFor({ state: 'hidden' });
  });

  await step('if the app closes, the browser keeps the download', async () => {
    await appBrowser.close().catch(() => {});
    appBrowser = null;
    appPage = null;
    app.kill();
    await sleep(1500);

    await edgeTools.open(`${BASE}/`);
    await edgeTools.click('5M.bin');

    // The extension can't reach the app, so it must carry on by itself.
    await waitForFile(edgeFolder, '5M.bin', 25_000);
    assert.equal(await sha256(join(edgeFolder, '5M.bin')), expectedSha256('5M.bin'));
  });
} finally {
  edgeTools?.close();
  await appBrowser?.close().catch(() => {});
  edge?.kill();
  app?.kill();
  server?.kill();

  if (previousRegistration) {
    setRegistryValue(previousRegistration);
  } else {
    try {
      execFileSync('reg', ['delete', REGISTRY_KEY, '/f'], { stdio: 'ignore' });
    } catch {
      // Nothing to remove.
    }
  }

  await sleep(1500);
  for (const folder of [dataDir, appFolder, edgeFolder, profile]) {
    rmSync(folder, { recursive: true, force: true, maxRetries: 8, retryDelay: 400 });
  }

  const passed = results.filter(Boolean).length;
  console.log(`\n${passed} of ${results.length} steps passed`);
}
