// Drives the real app through WebView2's debugging port, against the local test server.
//
//   npm run e2e
//
// Uses a temporary data folder, so real downloads and settings are never touched.
// Screenshots go to the folder in E2E_SCREENSHOTS, or are skipped if it isn't set.

import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
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
const APP = process.env.E2E_APP ?? join(ROOT, 'target', 'release', 'tributary.exe');
const SERVER = join(ROOT, 'target', 'release', 'testserver.exe');
const HOST = join(ROOT, 'target', 'release', 'tributary-host.exe');
const PIPE = `\\\\.\\pipe\\tributary-e2e-${process.pid}`;
const SERVER_PORT = 8791;
const DEBUG_PORT = 9333;
const BASE = `http://127.0.0.1:${SERVER_PORT}`;
const SCREENSHOTS = process.env.E2E_SCREENSHOTS;

const dataDir = mkdtempSync(join(tmpdir(), 'tributary-e2e-data-'));
const downloadDir = mkdtempSync(join(tmpdir(), 'tributary-e2e-files-'));

const results = [];
const pageErrors = [];
let activePage;
let server;
let app;
let browser;

async function step(name, run) {
  const started = Date.now();
  try {
    await run();
    results.push({ name, passed: true });
    console.log(`ok   ${name} (${Date.now() - started} ms)`);
  } catch (error) {
    results.push({ name, passed: false });
    console.log(`FAIL ${name}\n     ${String(error.message ?? error).split('\n').join('\n     ')}`);
    if (activePage) {
      await shot(activePage, 'failure').catch(() => {});
      const rows = await activePage.locator('tbody tr').allInnerTexts().catch(() => []);
      console.log(`     rows: ${rows.map((row) => row.replace(/\s+/g, ' ')).join(' | ')}`);
      const failed = await activePage
        .evaluate(() => window.__TAURI_INTERNALS__.invoke('list_downloads'))
        .catch(() => []);
      for (const download of failed.filter((item) => item.error)) {
        console.log(`     ${download.url}: ${download.error}`);
      }
    }
    throw error;
  }
}

async function sha256(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

function expectedSha256(name) {
  return execFileSync(SERVER, ['sha256', name], { encoding: 'utf8' }).trim();
}

async function shot(page, name) {
  if (!SCREENSHOTS) return;
  mkdirSync(SCREENSHOTS, { recursive: true });
  await page.screenshot({ path: join(SCREENSHOTS, `${name}.png`) });
}

async function connect() {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try {
      const connected = await chromium.connectOverCDP(`http://127.0.0.1:${DEBUG_PORT}`);
      const page = connected.contexts()[0]?.pages()[0];
      if (page) return { connected, page };
      await connected.close();
    } catch {
      // The app is still starting.
    }
    await sleep(500);
  }
  throw new Error('the app never opened its debugging port');
}

/** Adds a URL through the same dialogs a person would use. */
async function addThroughDialogs(page, url) {
  await page.getByRole('button', { name: 'Add URL' }).first().click();
  await page.getByLabel('Address').fill(url);
  await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByLabel('Folder').fill(downloadDir);
  await page.getByRole('button', { name: 'Start download' }).click();
}

const rowFor = (page, name) => page.locator('tbody tr', { hasText: name });

/** Sends one message through the real host program, exactly as a browser does. */
function askHost(message, env = { ...process.env, TRIBUTARY_PIPE: PIPE }) {
  return new Promise((resolve, reject) => {
    const host = spawn(HOST, [], { stdio: ['pipe', 'pipe', 'ignore'], env });
    const chunks = [];

    host.stdout.on('data', (data) => {
      chunks.push(data);
      const bytes = Buffer.concat(chunks);
      if (bytes.length >= 4 && bytes.length >= 4 + bytes.readUInt32LE(0)) {
        host.stdin.end();
        resolve(JSON.parse(bytes.subarray(4, 4 + bytes.readUInt32LE(0)).toString()));
      }
    });
    host.on('error', reject);

    const body = Buffer.from(JSON.stringify(message));
    const length = Buffer.alloc(4);
    length.writeUInt32LE(body.length);
    host.stdin.write(Buffer.concat([length, body]));
  });
}

/** Runs a PowerShell clipboard command. Clipboard access needs a single-threaded host. */
function clipboard(command) {
  return execFileSync('pwsh', ['-NoProfile', '-STA', '-Command', command], {
    encoding: 'utf8',
  }).trim();
}

const RUN_KEY = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run';

/** What Windows will run at sign-in for this app, or an empty string if nothing. */
function startupEntry() {
  try {
    const output = execFileSync('reg', ['query', RUN_KEY], { encoding: 'utf8' });
    return output.split('\n').find((line) => /tributary/i.test(line))?.trim() ?? '';
  } catch {
    return '';
  }
}

try {
  assert.ok(existsSync(APP), `the app isn't built: ${APP}`);
  assert.ok(existsSync(SERVER), `the test server isn't built: ${SERVER}`);

  // Set before the app starts, so a test can never write to the real Downloads folder.
  writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ downloadDir }));

  server = spawn(SERVER, ['--port', String(SERVER_PORT)], { stdio: ['ignore', 'pipe', 'ignore'] });
  const ftpBase = await new Promise((resolve, reject) => {
    server.stdout.on('data', (data) => {
      const found = /ftp on (\S+)/.exec(String(data));
      if (found) resolve(found[1]);
    });
    server.on('exit', () => reject(new Error('the test server stopped')));
  });

  const appEnv = {
    ...process.env,
    TRIBUTARY_DATA_DIR: dataDir,
    TRIBUTARY_PIPE: PIPE,
    WEBVIEW2_USER_DATA_FOLDER: join(dataDir, 'webview'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${DEBUG_PORT}`,
  };
  app = spawn(APP, [], { stdio: 'ignore', env: appEnv });

  /** Starts the app a second time, which passes its command line to the running copy. */
  const launch = (...args) =>
    execFileSync(APP, args, { env: appEnv, stdio: 'ignore', timeout: 15_000 });

  const { connected, page } = await connect();
  browser = connected;
  activePage = page;
  page.setDefaultTimeout(20_000);
  page.on('pageerror', (error) => pageErrors.push(error.message));

  await step('opens on an empty list', async () => {
    await page.getByRole('heading', { name: 'No downloads yet' }).waitFor();
    await shot(page, '01-empty');
  });

  await step('the Add URL dialog refuses an address it cannot download', async () => {
    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill('ftp-not-a-web-address');
    assert.equal(await page.getByRole('button', { name: 'Continue' }).isDisabled(), true);
    await page.getByRole('button', { name: 'Cancel' }).click();
  });

  await step('the start dialog reports the size before downloading', async () => {
    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/24M.bin`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByText('24.0 MB').waitFor();
    await page.getByText('Supported').waitFor();
    assert.equal(await page.getByLabel('Save as').inputValue(), '24M.bin');
    await shot(page, '02-start-dialog');
  });

  await step('a download finishes with the correct content', async () => {
    await page.getByLabel('Folder').fill(downloadDir);
    await page.getByRole('button', { name: 'Start download' }).click();
    await page.getByRole('heading', { name: 'Download complete' }).waitFor();
    await shot(page, '03-complete');
    assert.equal(await sha256(join(downloadDir, '24M.bin')), expectedSha256('24M.bin'));
    await page.getByRole('button', { name: 'Close' }).click();
    await rowFor(page, '24M.bin').getByText('Complete').waitFor();
  });

  await step('shows live progress, then stops and resumes to a correct file', async () => {
    await addThroughDialogs(page, `${BASE}/file/40M.bin?rate=2M`);
    const row = rowFor(page, '40M.bin');
    await row.getByText('Downloading').waitFor();

    await row.dblclick();
    await page.getByRole('img', { name: 'Progress of each connection' }).waitFor();
    await page.getByText('/s', { exact: false }).first().waitFor();
    await sleep(1200);
    await shot(page, '04-progress');

    await page.getByRole('dialog').getByRole('button', { name: 'Stop' }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Resume' }).waitFor();
    assert.ok(
      readdirSync(downloadDir).some((name) => name.endsWith('.part')),
      'a partial file should exist while stopped',
    );

    await page.getByRole('dialog').getByRole('button', { name: 'Resume' }).click();
    await page.getByRole('heading', { name: 'Download complete' }).waitFor({ timeout: 30_000 });
    await page.getByRole('button', { name: 'Close' }).click();
    assert.equal(await sha256(join(downloadDir, '40M.bin')), expectedSha256('40M.bin'));
  });

  await step('a failed download shows its reason', async () => {
    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/missing/file.zip`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByText("Couldn't check this file", { exact: false }).waitFor();
    await page.getByRole('button', { name: 'Cancel' }).click();
  });

  await step('sorting by a column works, and filters narrow the list', async () => {
    await page.getByRole('button', { name: 'File name' }).click();
    const names = await page.locator('tbody tr td:first-child').allInnerTexts();
    assert.deepEqual(names, ['24M.bin', '40M.bin']);

    await page.getByRole('button', { name: 'File name' }).click();
    const reversed = await page.locator('tbody tr td:first-child').allInnerTexts();
    assert.deepEqual(reversed, ['40M.bin', '24M.bin']);

    await page.getByRole('button', { name: /^Unfinished/ }).click();
    assert.equal(await page.locator('tbody tr').count(), 0);
    await page.getByRole('button', { name: /^All downloads/ }).click();
  });

  await step('the right-click menu and properties save a description', async () => {
    await rowFor(page, '24M.bin').click({ button: 'right' });
    await shot(page, '05-menu');
    await page.getByRole('menuitem', { name: 'Properties' }).click();
    await page.getByLabel('Description').fill('checked by the test');
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    await rowFor(page, '24M.bin').click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Properties' }).click();
    assert.equal(await page.getByLabel('Description').inputValue(), 'checked by the test');
    await page.getByRole('button', { name: 'Cancel' }).click();
  });

  await step('options change the theme and are saved to disk', async () => {
    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByLabel('Theme').selectOption('dark');
    await page.getByRole('button', { name: 'Save' }).click();
    await page.waitForFunction(() => document.documentElement.dataset.theme === 'dark');
    await shot(page, '06-dark');

    const saved = JSON.parse(readFileSync(join(dataDir, 'settings.json'), 'utf8'));
    assert.equal(saved.theme, 'dark');
  });

  await step('turning off the start dialog downloads straight away', async () => {
    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: 'Downloads' }).click();
    await page.getByLabel('Default folder').fill(downloadDir);
    await page.getByLabel('Ask before starting a download').uncheck();
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/3M.bin`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('heading', { name: 'Download complete' }).waitFor();
    await page.getByRole('button', { name: 'Close' }).click();
    assert.equal(await sha256(join(downloadDir, '3M.bin')), expectedSha256('3M.bin'));
  });

  await step('hiding a toolbar button and a column takes effect', async () => {
    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: 'Toolbar' }).click();
    await page.getByRole('checkbox', { name: 'Clear finished' }).uncheck();
    await page.getByRole('tab', { name: 'Columns' }).click();
    await page.getByRole('checkbox', { name: 'Speed' }).uncheck();
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    // The dialog closes before saving finishes, so these wait rather than check at once.
    await page.getByRole('toolbar').getByRole('button', { name: 'Clear finished' }).waitFor({ state: 'detached', timeout: 5000 });
    await page.locator('thead').getByRole('button', { name: 'Speed' }).waitFor({ state: 'detached', timeout: 5000 });
    await shot(page, '07-customised');
  });

  await step('deleting can remove the file from disk too', async () => {
    await rowFor(page, '3M.bin').click();
    await page.keyboard.press('Delete');
    await page.getByLabel('Also delete the finished file from disk').check();
    await page.getByRole('button', { name: 'Delete' }).last().click();
    await rowFor(page, '3M.bin').waitFor({ state: 'detached' });
    assert.equal(existsSync(join(downloadDir, '3M.bin')), false);
  });

  await step('start with Windows can be turned on and off', async () => {
    const setStartup = async (on) => {
      await page.getByRole('button', { name: 'Options' }).click();
      await page.getByLabel('Start with Windows').setChecked(on);
      await page.getByRole('button', { name: 'Save' }).click();
      await page.getByRole('dialog').waitFor({ state: 'hidden' });
      await sleep(500);
    };

    await setStartup(true);
    assert.match(startupEntry(), /tributary\.exe.*--minimized/i);
    await setStartup(false);
    assert.equal(startupEntry(), '');
  });

  await step('warns before saving a web page as a file', async () => {
    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: 'Downloads' }).click();
    await page.getByLabel('Ask before starting a download').check();
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    await sleep(300);

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByText('This address is a web page', { exact: false }).waitFor();
    await shot(page, '08-web-page-warning');
    await page.getByRole('button', { name: 'Cancel' }).click();
  });

  await step('what you type is kept when the file check finishes afterwards', async () => {
    const typed = join(downloadDir, 'chosen by hand');

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/1M.typed?delay=1500`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByLabel('Folder').fill(typed);
    await page.getByLabel('Save as').fill('my own name.bin');

    await page.getByText('1.0 MB').waitFor();
    await sleep(300);
    assert.equal(await page.getByLabel('Folder').inputValue(), typed);
    assert.equal(await page.getByLabel('Save as').inputValue(), 'my own name.bin');
    await page.getByRole('button', { name: 'Cancel' }).click();
  });

  await step('files are sorted into categories, which filter the list', async () => {
    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/2M.mp4`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByLabel('Save as').waitFor();
    await page.waitForFunction(() => !document.body.innerText.includes('Checking the file'));
    assert.equal(await page.getByLabel('Category').inputValue(), 'Video');
    await page.getByRole('button', { name: 'Start download' }).click();

    // Also turns off the complete message, which would get in the way of the queue tests.
    await page.getByRole('heading', { name: 'Download complete' }).waitFor();
    await page.getByLabel("Don't show this again").check();
    await page.getByRole('button', { name: 'Close' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    await page.getByRole('button', { name: /^Video/ }).click();
    await rowFor(page, '2M.mp4').waitFor();
    assert.equal(await page.locator('tbody tr').count(), 1);
    await page.getByRole('button', { name: /^All downloads/ }).click();
  });

  await step('the speed limit is respected in the real app', async () => {
    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: 'Connection' }).click();
    await page.getByLabel('Limit download speed').check();
    await page.getByLabel('Speed limit').fill('2048');
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    await sleep(300);

    const began = Date.now();
    await addThroughDialogs(page, `${BASE}/file/6M.zip`);
    await rowFor(page, '6M.zip').getByText('Complete').waitFor();
    const seconds = (Date.now() - began) / 1000;
    assert.ok(seconds > 2.6 && seconds < 4, `6 MiB at 2 MiB/s took ${seconds}s, expected about 3s`);
    assert.equal(await sha256(join(downloadDir, '6M.zip')), expectedSha256('6M.zip'));

    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: 'Connection' }).click();
    await page.getByLabel('Limit download speed').uncheck();
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
  });

  await step('a new queue runs its files and then its after-queue program', async () => {
    const marker = join(dataDir, 'queue finished.txt');

    await page.getByRole('button', { name: 'Scheduler' }).click();
    await page.getByRole('button', { name: 'New queue' }).click();
    await page.getByLabel('Queue name').fill('Evening');
    await page.getByLabel('Files at once').fill('1');
    await page.getByRole('tab', { name: 'When it ends' }).click();
    await page.getByLabel('Run a program or open a file').fill(`echo done> "${marker}"`);
    await page.getByRole('dialog').getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Start now' }).waitFor();
    await shot(page, '09-scheduler');
    await page.getByRole('button', { name: 'Close' }).click();

    for (const name of ['3M.qa', '3M.qb']) {
      await page.getByRole('button', { name: 'Add URL' }).first().click();
      await page.getByLabel('Address').fill(`${BASE}/file/${name}?rate=4M&run=queue`);
      await page.getByRole('button', { name: 'Continue' }).click();
      await page.getByRole('button', { name: 'Download later' }).waitFor();
      await page.waitForFunction(() => !document.body.innerText.includes('Checking the file'));
      await page.getByLabel('Folder').fill(downloadDir);
      await page.getByLabel('Queue for later').selectOption({ label: 'Evening' });
      await page.getByRole('button', { name: 'Download later' }).click();
      await page.getByRole('dialog').waitFor({ state: 'hidden' });
    }

    await page.getByRole('button', { name: /^Evening/ }).click();
    assert.equal(await page.locator('tbody tr').count(), 2);

    await page.getByRole('toolbar').getByRole('button', { name: 'Start queue' }).click();
    await page.getByRole('button', { name: /^All downloads/ }).click();
    await rowFor(page, '3M.qa').getByText('Complete').waitFor();
    await rowFor(page, '3M.qb').getByText('Complete').waitFor();

    for (let attempt = 0; attempt < 50 && !existsSync(marker); attempt += 1) await sleep(100);
    assert.ok(existsSync(marker), 'the after-queue program should have run');
    assert.equal(await sha256(join(downloadDir, '3M.qa')), expectedSha256('3M.qa'));
    assert.equal(await sha256(join(downloadDir, '3M.qb')), expectedSha256('3M.qb'));
    assert.equal(
      await page.getByRole('button', { name: /^Evening/ }).locator('.count').innerText(),
      '0',
      'finished files should leave the queue',
    );
  });

  // Waits for the clock, so it can be skipped while working on other steps.
  await step('a queue really starts by itself at its scheduled time', async () => {
    if (process.env.E2E_SKIP_TIMED) return;

    const start = new Date(Date.now() + 70_000);
    const time = `${String(start.getHours()).padStart(2, '0')}:${String(start.getMinutes()).padStart(2, '0')}`;

    await page.getByRole('button', { name: 'Scheduler' }).click();
    await page.getByRole('button', { name: /^Evening/ }).last().click();
    await page.getByLabel('Start at').fill(time);
    await page.getByRole('dialog').getByRole('button', { name: 'Save' }).click();
    await sleep(300);
    await page.getByRole('button', { name: 'Close' }).click();

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/2M.timed`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('button', { name: 'Download later' }).waitFor();
    await page.waitForFunction(() => !document.body.innerText.includes('Checking the file'));
    await page.getByLabel('Folder').fill(downloadDir);
    await page.getByLabel('Queue for later').selectOption({ label: 'Evening' });
    await page.getByRole('button', { name: 'Download later' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    await rowFor(page, '2M.timed').waitFor();
    assert.equal(existsSync(join(downloadDir, '2M.timed')), false, 'it must not start early');

    await rowFor(page, '2M.timed').getByText('Complete').waitFor({ timeout: 135_000 });
    const finished = new Date();
    const scheduled = new Date(start);
    scheduled.setSeconds(0, 0);
    assert.ok(finished >= scheduled, `finished at ${finished}, scheduled for ${scheduled}`);
    assert.equal(await sha256(join(downloadDir, '2M.timed')), expectedSha256('2M.timed'));
  });

  /** Opens Options on a tab, changes things, and saves. */
  async function inOptions(tab, change) {
    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: tab }).click();
    await change();
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    await sleep(300);
  }

  await step('downloads over FTP', async () => {
    await addThroughDialogs(page, `${ftpBase}pub/4M.ftp`);
    await rowFor(page, '4M.ftp').getByText('Complete').waitFor();
    assert.equal(await sha256(join(downloadDir, '4M.ftp')), expectedSha256('4M.ftp'));
  });

  await step('the start dialog asks for a login, and the download works with it', async () => {
    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/2M.login?basic=alice:secret`);
    await page.getByRole('button', { name: 'Continue' }).click();

    await page.getByLabel('User name').waitFor();
    await page.getByText('needs a login', { exact: false }).waitFor();
    await shot(page, '10-login');

    await page.getByLabel('User name').fill('alice');
    await page.getByLabel('Password').fill('secret');
    await page.getByRole('button', { name: 'Try again' }).click();
    await page.getByRole('dialog').getByText('2.0 MB').waitFor();

    await page.getByRole('button', { name: 'Start download' }).click();
    await rowFor(page, '2M.login').getByText('Complete').waitFor();
    assert.equal(await sha256(join(downloadDir, '2M.login')), expectedSha256('2M.login'));
  });

  await step('a download that fails for a login prompts for one', async () => {
    await inOptions('Downloads', () => page.getByLabel('Ask before starting a download').uncheck());

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/2M.prompt?digest=bob:%68unter2`);
    await page.getByRole('button', { name: 'Continue' }).click();

    await page.getByRole('heading', { name: 'Login needed' }).waitFor();
    await page.getByLabel('User name').fill('bob');
    await page.getByLabel('Password').fill('wrong password');
    await page.getByRole('button', { name: 'Try again' }).click();

    // A wrong password asks again rather than failing quietly.
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    await page.getByRole('heading', { name: 'Login needed' }).waitFor();
    await page.getByLabel('User name').fill('bob');
    await page.getByLabel('Password').fill('hunter2');
    await page.getByRole('button', { name: 'Try again' }).click();

    await rowFor(page, '2M.prompt').getByText('Complete').waitFor();
    assert.equal(await sha256(join(downloadDir, '2M.prompt')), expectedSha256('2M.prompt'));
  });

  await step('a saved site login is used without asking', async () => {
    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: 'Logins' }).click();
    await page.getByLabel('Site').fill('127.0.0.*');
    await page.getByLabel('User name').fill('carol');
    await page.getByLabel('Password').fill('saved-pass');
    await page.getByRole('button', { name: 'Add login' }).click();
    await page.getByText('127.0.0.* (carol)').waitFor();
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/1M.saved?basic=carol:%73aved-pass`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await rowFor(page, '1M.saved').getByText('Complete').waitFor();

    const settings = readFileSync(join(dataDir, 'settings.json'), 'utf8');
    assert.ok(settings.includes('carol'));
    assert.ok(!settings.includes('saved-pass'), 'a password reached the settings file');
    const database = readFileSync(join(dataDir, 'downloads.db'));
    assert.ok(!database.includes('saved-pass'), 'a password reached the database');

    await page.getByRole('button', { name: 'Options' }).click();
    await page.getByRole('tab', { name: 'Logins' }).click();
    await page.getByRole('button', { name: 'Remove 127.0.0.*' }).click();
    await page.getByText('127.0.0.* (carol)').waitFor({ state: 'detached' });
    await page.getByRole('button', { name: 'Save' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
  });

  await step('downloads through a proxy set in Options', async () => {
    await inOptions('Proxy', async () => {
      await page.getByLabel('Connect through').selectOption('manual');
      await page.getByLabel('Proxy address').fill(BASE);
    });

    // This host doesn't exist, so finishing proves the proxy was used.
    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill('http://tributary.invalid/file/1M.proxied');
    await page.getByRole('button', { name: 'Continue' }).click();
    await rowFor(page, '1M.proxied').getByText('Complete').waitFor();

    await inOptions('Proxy', () => page.getByLabel('Connect through').selectOption('system'));
  });

  await step('a bad proxy address is refused with a clear message', async () => {
    await inOptions('Proxy', async () => {
      await page.getByLabel('Connect through').selectOption('manual');
      await page.getByLabel('Proxy address').fill('not a proxy');
    });
    await page.getByRole('heading', { name: 'Something went wrong' }).waitFor();
    await page.getByText("isn't valid", { exact: false }).waitFor();
    await page.getByRole('button', { name: 'Close' }).click();
  });

  await step('the virus scanner runs and a warning is shown', async () => {
    await inOptions('Security', async () => {
      await page.getByLabel('Scan finished downloads for viruses').check();
      await page.getByLabel('Scanner program').fill('C:\\Windows\\System32\\cmd.exe');
      await page.getByLabel('Scanner arguments').fill('/c exit 2');
    });

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/1M.scanned`);
    await page.getByRole('button', { name: 'Continue' }).click();

    await page.getByRole('heading', { name: 'Virus scan warning' }).waitFor();
    await page.getByText('code 2', { exact: false }).waitFor();
    await shot(page, '11-scan-warning');
    await page.getByRole('button', { name: 'Close' }).click();

    await inOptions('Security', () =>
      page.getByLabel('Scan finished downloads for viruses').uncheck(),
    );
  });

  await step('settings and the list can be exported and imported', async () => {
    const invoke = (command, args) =>
      page.evaluate(([name, values]) => window.__TAURI_INTERNALS__.invoke(name, values), [command, args]);
    const file = join(dataDir, 'export.json');

    const exported = await invoke('export_data', { path: file });
    assert.ok(exported > 5, `exported ${exported} downloads`);

    const text = readFileSync(file, 'utf8');
    assert.ok(text.includes('2M.login'));
    assert.ok(!text.includes('saved-pass') && !text.includes('hunter2'), 'a password was exported');

    assert.equal(await invoke('import_data', { path: file }), 0, 'finished files are not re-added');
  });

  await step('the complete message buttons work, and a missing file is explained', async () => {
    await inOptions('Downloads', () =>
      page.getByLabel('Show a message when a download finishes').check(),
    );

    await page.getByRole('button', { name: 'Add URL' }).first().click();
    await page.getByLabel('Address').fill(`${BASE}/file/1M.gone`);
    await page.getByRole('button', { name: 'Continue' }).click();
    await page.getByRole('heading', { name: 'Download complete' }).waitFor();

    // Removing the file means pressing Open can't launch anything on the desktop.
    rmSync(join(downloadDir, '1M.gone'));
    await page.getByRole('button', { name: 'Open file' }).click();
    await page.getByRole('heading', { name: 'Something went wrong' }).waitFor();
    await page.getByText('no longer there', { exact: false }).waitFor();
    await page.getByRole('button', { name: 'Close' }).click();
  });

  await step('the host program reports the capture settings', async () => {
    await inOptions('Downloads', () => page.getByLabel('Ask before starting a download').check());

    const reply = await askHost({ type: 'settings' });

    assert.equal(reply.ok, true);
    assert.equal(reply.enabled, true);
    assert.ok(reply.fileTypes.includes('zip'));
    assert.equal(reply.forceKey, 'ctrl');
    assert.equal(reply.skipKey, 'alt');
  });

  await step('a captured download keeps its cookies, without the window ever seeing them', async () => {
    const reply = await askHost({
      type: 'download',
      url: `${BASE}/file/2M.cap?require_cookie=session%3D%63aptured-cookie-7`,
      cookies: 'session=captured-cookie-7',
      userAgent: 'Test Browser/1.0',
      referrer: `${BASE}/`,
      fileName: '2M.cap',
    });
    assert.equal(reply.ok, true);

    await page.getByRole('heading', { name: 'Download file' }).waitFor();
    await page.getByRole('dialog').getByText('2.0 MB').waitFor();
    assert.ok(
      !(await page.content()).includes('captured-cookie-7'),
      'the cookie reached the window',
    );
    await shot(page, '12-captured');

    await page.getByRole('button', { name: 'Start download' }).click();
    await page.getByRole('heading', { name: 'Download complete' }).waitFor();
    await page.getByRole('button', { name: 'Close' }).click();
    assert.equal(await sha256(join(downloadDir, '2M.cap')), expectedSha256('2M.cap'));

    const database = readFileSync(join(dataDir, 'downloads.db'));
    assert.ok(!database.includes('captured-cookie-7'), 'the cookie reached the database');
  });

  await step('declining a captured download forgets it', async () => {
    await askHost({ type: 'download', url: `${BASE}/file/1M.declined` });
    await page.getByRole('heading', { name: 'Download file' }).waitFor();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    assert.equal(await rowFor(page, '1M.declined').count(), 0);
  });

  await step('several links can be chosen from', async () => {
    await inOptions('Downloads', () =>
      page.getByLabel('Show a message when a download finishes').uncheck(),
    );

    const reply = await askHost({
      type: 'batch',
      items: ['1M.ba', '1M.bb', '1M.bc'].map((name) => ({ url: `${BASE}/file/${name}` })),
    });
    assert.equal(reply.ok, true);

    await page.getByRole('heading', { name: 'Download links' }).waitFor();
    await page.getByRole('checkbox', { name: '1M.bc' }).uncheck();
    await shot(page, '13-batch');
    await page.getByRole('button', { name: 'Start now' }).click();

    await rowFor(page, '1M.ba').getByText('Complete').waitFor();
    await rowFor(page, '1M.bb').getByText('Complete').waitFor();
    assert.equal(await rowFor(page, '1M.bc').count(), 0, 'the unchecked link was downloaded');
    assert.equal(await sha256(join(downloadDir, '1M.ba')), expectedSha256('1M.ba'));
  });

  await step('the host refuses what it should', async () => {
    for (const url of ['file:///C:/Windows/win.ini', 'javascript:alert(1)', 'not a url']) {
      const reply = await askHost({ type: 'download', url });
      assert.equal(reply.ok, false, `${url} was accepted`);
    }
    assert.equal((await askHost({ type: 'nonsense' })).ok, false);
    assert.equal(await page.getByRole('dialog').count(), 0, 'nothing should have opened');
  });

  await step('a link dragged onto the window starts a download', async () => {
    await page.evaluate((url) => {
      const data = new DataTransfer();
      data.setData('text/uri-list', url);
      window.dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true }));
    }, `${BASE}/file/1M.dragged`);

    await page.getByRole('heading', { name: 'Download file' }).waitFor();
    await page.getByRole('dialog').getByText('1.0 MB').waitFor();
    await page.getByRole('button', { name: 'Cancel' }).click();
  });

  await step('the drop target window works', async () => {
    await inOptions('Browser', () => page.getByLabel('Show a small window to drop links on').check());

    let target;
    for (let attempt = 0; attempt < 50 && !target; attempt += 1) {
      target = browser.contexts()[0].pages().find((item) => item.url().endsWith('#drop'));
      if (!target) await sleep(100);
    }
    assert.ok(target, 'the drop target window never opened');

    await target.getByRole('region', { name: 'Drop a link here' }).waitFor();
    await target.evaluate((url) => {
      const data = new DataTransfer();
      data.setData('text/uri-list', url);
      document
        .querySelector('.target')
        .dispatchEvent(new DragEvent('drop', { dataTransfer: data, bubbles: true }));
    }, `${BASE}/file/1M.droptarget`);

    await page.getByRole('heading', { name: 'Download file' }).waitFor();
    await page.getByRole('button', { name: 'Cancel' }).click();

    await inOptions('Browser', () =>
      page.getByLabel('Show a small window to drop links on').uncheck(),
    );
  });

  await step('a copied link is offered, and the clipboard is put back', async () => {
    const kind = clipboard(
      `Add-Type -AssemblyName System.Windows.Forms;
       $c = [System.Windows.Forms.Clipboard];
       if ($c::ContainsImage() -or $c::ContainsFileDropList()) { 'other' }
       elseif ($c::ContainsText()) { 'text' } else { 'empty' }`,
    );
    if (kind === 'other') {
      console.log('     skipped: something other than text is on the clipboard');
      return;
    }
    const before = kind === 'text' ? clipboard('Get-Clipboard -Raw') : null;

    try {
      await inOptions('Browser', () =>
        page.getByLabel('Offer to download links copied to the clipboard').check(),
      );

      clipboard(`Set-Clipboard -Value '${BASE}/file/1M.zip'`);
      await page.getByRole('heading', { name: 'Download file' }).waitFor({ timeout: 8000 });
      await page.getByRole('button', { name: 'Cancel' }).click();

      // Ordinary text must not open anything.
      clipboard("Set-Clipboard -Value 'just some ordinary copied words'");
      await sleep(2500);
      assert.equal(await page.getByRole('dialog').count(), 0);

      await inOptions('Browser', () =>
        page.getByLabel('Offer to download links copied to the clipboard').uncheck(),
      );
    } finally {
      if (before === null) {
        clipboard("Set-Clipboard -Value $null");
      } else {
        writeFileSync(join(dataDir, 'clipboard.txt'), before);
        clipboard(`Get-Content -Raw -LiteralPath '${join(dataDir, 'clipboard.txt')}' | Set-Clipboard`);
      }
    }
  });

  await step('a second launch hands its command line to the running app', async () => {
    await inOptions('Downloads', () =>
      page.getByLabel('Show a message when a download finishes').uncheck(),
    );

    const folder = join(dataDir, 'from-command-line');
    mkdirSync(folder);
    launch('--add', `${BASE}/file/1M.cli`, '--folder', folder, '--name', 'named.bin', '--silent');
    await rowFor(page, 'named.bin').getByText('Complete').waitFor();
    assert.equal(await sha256(join(folder, 'named.bin')), expectedSha256('1M.cli'));

    // Without --silent the user still gets to decide.
    launch('--add', `${BASE}/file/1M.asked`);
    await page.getByRole('heading', { name: 'Download file' }).waitFor();
    await page.getByRole('button', { name: 'Cancel' }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });

    // --later waits in the main queue until the queue is started.
    launch('--add', `${BASE}/file/1M.queued`, '--later');
    const queued = rowFor(page, '1M.queued');
    await queued.waitFor();
    await sleep(1000);
    assert.notEqual(await queued.getAttribute('data-state'), 'completed');

    launch('--start-queue');
    await queued.getByText('Complete').waitFor();

    // A bad command line is ignored rather than breaking the running app.
    launch('--nonsense');
    await sleep(300);
    assert.equal(app.exitCode, null);
  });

  await step('only finished rows can be dragged out, and nothing moves without a held button', async () => {
    const invoke = (command, args) =>
      page.evaluate(([name, values]) => window.__TAURI_INTERNALS__.invoke(name, values), [command, args]);

    const gone = rowFor(page, '1M.gone');
    assert.equal(await gone.getAttribute('draggable'), 'true');

    launch('--add', `${BASE}/file/1M.undone`, '--later');
    const undone = rowFor(page, '1M.undone');
    await undone.waitFor();
    assert.equal(await undone.getAttribute('draggable'), 'false');

    // Starting a drag reaches the app, which explains that the file has gone.
    await gone.dispatchEvent('dragstart');
    await page.getByRole('heading', { name: 'Something went wrong' }).waitFor();
    await page.getByText('no longer there', { exact: false }).waitFor();
    await page.getByRole('button', { name: 'Close' }).click();

    await assert.rejects(invoke('drag_files', { ids: [999_999] }), /no longer there/);

    // With no button held Windows would drop straight onto whatever is under the cursor,
    // so this must return without doing anything. A hang here would mean a stuck window.
    const rows = await invoke('list_downloads');
    const present = rows.find(
      (row) => row.state === 'completed' && row.finalPath && existsSync(row.finalPath),
    );
    assert.ok(present, 'this check needs a finished file');
    const answered = await Promise.race([
      invoke('drag_files', { ids: [present.id] }).then(() => 'returned'),
      sleep(5000).then(() => 'hung'),
    ]);
    assert.equal(answered, 'returned');
    assert.ok(existsSync(present.finalPath), 'the file should not have moved');
  });

  await step('the app stays open after a command line download if something else is running', async () => {
    launch('--add', `${BASE}/file/40M.busy?rate=2M`, '--folder', downloadDir, '--silent');
    await rowFor(page, '40M.busy').getByText('Downloading').waitFor();

    const url = `${BASE}/file/1M.notlast`;
    launch('--add', url, '--folder', downloadDir, '--silent', '--quit-when-done');
    await rowFor(page, '1M.notlast').getByText('Complete').waitFor();
    await sleep(1500);
    assert.equal(app.exitCode, null, 'the app closed during another download');

    await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('stop_all'));
  });

  await step('no script error happened anywhere in the run', async () => {
    assert.deepEqual(pageErrors, []);
  });

  await step('a file sent while the app is still starting is not lost', async () => {
    const earlyData = join(dataDir, 'early-data');
    mkdirSync(earlyData);
    writeFileSync(join(earlyData, 'settings.json'), JSON.stringify({ downloadDir }));

    const early = spawn(APP, ['--add', `${BASE}/file/1M.early`], {
      stdio: 'ignore',
      env: {
        ...process.env,
        TRIBUTARY_DATA_DIR: earlyData,
        TRIBUTARY_PIPE: `${PIPE}-early`,
        WEBVIEW2_USER_DATA_FOLDER: join(earlyData, 'webview'),
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${DEBUG_PORT + 1}`,
      },
    });

    let earlyBrowser;
    try {
      for (let attempt = 0; attempt < 60 && !earlyBrowser; attempt += 1) {
        earlyBrowser = await chromium
          .connectOverCDP(`http://127.0.0.1:${DEBUG_PORT + 1}`)
          .catch(() => null);
        if (!earlyBrowser) await sleep(250);
      }
      assert.ok(earlyBrowser, 'the second app never answered');

      const earlyPage = earlyBrowser.contexts()[0].pages()[0];
      await earlyPage.getByRole('heading', { name: 'Download file' }).waitFor({ timeout: 10_000 });
      await earlyPage.getByRole('dialog').getByText('1.0 MB').waitFor();
    } finally {
      await earlyBrowser?.close().catch(() => {});
      early.kill();
    }
  });

  await step('the browser host starts a closed app, and the download reaches it', async () => {
    const coldData = join(dataDir, 'cold-data');
    const coldPort = DEBUG_PORT + 2;
    mkdirSync(coldData);
    writeFileSync(join(coldData, 'settings.json'), JSON.stringify({ downloadDir }));
    const coldEnv = {
      ...process.env,
      TRIBUTARY_DATA_DIR: coldData,
      TRIBUTARY_PIPE: `${PIPE}-cold`,
      WEBVIEW2_USER_DATA_FOLDER: join(coldData, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${coldPort}`,
    };

    let coldBrowser;
    try {
      // The host finds the app beside itself, as it does in a real install.
      const reply = await askHost({ type: 'download', url: `${BASE}/file/1M.cold` }, coldEnv);
      assert.equal(reply.ok, true, JSON.stringify(reply));

      for (let attempt = 0; attempt < 60 && !coldBrowser; attempt += 1) {
        coldBrowser = await chromium.connectOverCDP(`http://127.0.0.1:${coldPort}`).catch(() => null);
        if (!coldBrowser) await sleep(250);
      }
      assert.ok(coldBrowser, 'the host never started the app');

      const coldPage = coldBrowser.contexts()[0].pages()[0];
      await coldPage.getByRole('heading', { name: 'Download file' }).waitFor({ timeout: 10_000 });
      await coldPage.getByRole('dialog').getByText('1.0 MB').waitFor();
    } finally {
      await coldBrowser?.close().catch(() => {});
      // The host started it, so this test has no handle to close it with.
      execFileSync(
        APP,
        ['--add', `${BASE}/file/1M.closing`, '--silent', '--quit-when-done'],
        { env: coldEnv, stdio: 'ignore', timeout: 15_000 },
      );
    }

    let closed = false;
    for (let attempt = 0; attempt < 40 && !closed; attempt += 1) {
      closed = await fetch(`http://127.0.0.1:${coldPort}/json`).then(() => false, () => true);
      if (!closed) await sleep(250);
    }
    assert.ok(closed, 'the app the host started never closed');
  });

  await step('a script can download a file with no window, and the app then closes', async () => {
    const folder = join(dataDir, 'script-files');
    const scriptData = join(dataDir, 'script-data');
    mkdirSync(folder);
    mkdirSync(scriptData);
    writeFileSync(join(scriptData, 'settings.json'), JSON.stringify({ downloadDir: folder }));

    const scripted = spawn(
      APP,
      ['--add', `${BASE}/file/2M.script`, '--silent', '--quit-when-done', '--minimized'],
      {
        stdio: 'ignore',
        env: {
          ...process.env,
          TRIBUTARY_DATA_DIR: scriptData,
          TRIBUTARY_PIPE: `${PIPE}-script`,
          WEBVIEW2_USER_DATA_FOLDER: join(scriptData, 'webview'),
          WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '',
        },
      },
    );

    const exitCode = await Promise.race([
      new Promise((done) => scripted.once('exit', done)),
      sleep(30_000).then(() => 'still running'),
    ]);
    if (exitCode === 'still running') scripted.kill();

    assert.equal(exitCode, 0);
    assert.equal(await sha256(join(folder, '2M.script')), expectedSha256('2M.script'));
  });
} finally {
  await browser?.close().catch(() => {});
  app?.kill();

  // The saved login goes to the real Credential Manager, so it's always removed.
  try {
    execFileSync('cmdkey', ['/delete:Tributary/site/127.0.0.*'], { stdio: 'ignore' });
  } catch {
    // Normally already gone.
  }

  server?.kill();
  await sleep(1000);
  rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });
  rmSync(downloadDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });

  const passed = results.filter((result) => result.passed).length;
  console.log(`\n${passed} of ${results.length} steps passed`);
}
