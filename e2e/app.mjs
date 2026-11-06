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
const SERVER_PORT = 8791;
const DEBUG_PORT = 9333;
const BASE = `http://127.0.0.1:${SERVER_PORT}`;
const SCREENSHOTS = process.env.E2E_SCREENSHOTS;

const dataDir = mkdtempSync(join(tmpdir(), 'tributary-e2e-data-'));
const downloadDir = mkdtempSync(join(tmpdir(), 'tributary-e2e-files-'));

const results = [];
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

  server = spawn(SERVER, ['--port', String(SERVER_PORT)], { stdio: 'ignore' });
  app = spawn(APP, [], {
    stdio: 'ignore',
    env: {
      ...process.env,
      TRIBUTARY_DATA_DIR: dataDir,
      TRIBUTARY_ALLOW_SECOND_COPY: '1',
      WEBVIEW2_USER_DATA_FOLDER: join(dataDir, 'webview'),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${DEBUG_PORT}`,
    },
  });

  const { connected, page } = await connect();
  browser = connected;
  page.setDefaultTimeout(20_000);

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

  await step('a queue really starts by itself at its scheduled time', async () => {
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

  await step('a page-level error never appears', async () => {
    assert.equal(await page.getByRole('heading', { name: 'Something went wrong' }).count(), 0);
  });
} finally {
  await browser?.close().catch(() => {});
  app?.kill();
  server?.kill();
  await sleep(1000);
  rmSync(dataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });
  rmSync(downloadDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });

  const passed = results.filter((result) => result.passed).length;
  console.log(`\n${passed} of ${results.length} steps passed`);
}
