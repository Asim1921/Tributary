// Watches for downloads, and hands the right ones to the app.

import {
  addMedia,
  baseName,
  cookieHeader,
  downloadUrl,
  headerValue,
  keepDocument,
  mediaKey,
  mediaKind,
  mediaLabel,
  responseLength,
  shouldCapture,
  usableLinks,
} from './rules.js';

const HOST = 'app.tributary.capture';
const SETTINGS_MAX_AGE_MS = 15_000;

let settingsAt = 0;
let lastClick = null;

/** Downloads already dealt with, because more than one event can report the same one. */
const handled = new Set();

const ask = (message) => chrome.runtime.sendNativeMessage(HOST, message);

async function refreshSettings() {
  settingsAt = Date.now();
  try {
    const reply = await ask({ type: 'settings' });
    if (reply?.ok) {
      await chrome.storage.local.set({ settings: reply });
      return reply;
    }
  } catch {
    // The saved settings stay in use. If the app really has gone, handing over fails
    // and the browser simply carries on.
  }
  return null;
}

/**
 * The last settings the app gave. Saved rather than fetched, because this extension
 * sleeps when idle and a download mustn't wait for the host program to start.
 */
async function currentSettings() {
  const { settings } = await chrome.storage.local.get('settings');
  if (!settings) return refreshSettings();

  if (Date.now() - settingsAt > SETTINGS_MAX_AGE_MS) void refreshSettings();
  return settings;
}

async function describe(url, fields = {}) {
  let cookies = [];
  try {
    cookies = await chrome.cookies.getAll({ url });
  } catch {
    // Some addresses have no cookies to read.
  }
  return { url, cookies: cookieHeader(cookies), userAgent: navigator.userAgent, ...fields };
}

/**
 * Offers a browser download to the app. The browser keeps downloading meanwhile and is
 * only stopped once the app confirms it has the file, so if the app can't be reached
 * nothing was ever interrupted. Pausing first would save a moment of transfer, but a
 * download paused this early can fail to resume, and then it would simply be lost.
 */
async function takeOver(item) {
  try {
    const file = await describe(downloadUrl(item), {
      referrer: item.referrer || undefined,
      fileName: baseName(item.filename) || undefined,
      size: item.fileSize > 0 ? item.fileSize : undefined,
      mime: item.mime || undefined,
    });
    const reply = await ask({ type: 'download', ...file });
    if (!reply?.ok) return;
  } catch {
    return;
  }

  await chrome.downloads.cancel(item.id).catch(() => {});

  // A small file can finish first. The app has it now, so without this there would be
  // two copies.
  const [current] = await chrome.downloads.search({ id: item.id });
  if (current?.state === 'complete') {
    await chrome.downloads.removeFile(item.id).catch(() => {});
  }
  await chrome.downloads.erase({ id: item.id }).catch(() => {});
}

async function consider(item) {
  if (handled.has(item.id)) return;

  const chosen = await currentSettings();
  if (handled.has(item.id) || !shouldCapture(item, chosen, lastClick, Date.now())) return;

  handled.add(item.id);
  await takeOver(item);
}

// These two events wake this extension when it's asleep, which is why they're used
// rather than `onDeterminingFilename`.
chrome.downloads.onCreated.addListener((item) => void consider(item));

// The address alone doesn't always show the file type, so the name is checked as well
// once the server has given it.
chrome.downloads.onChanged.addListener((delta) => {
  if (!delta.filename?.current || handled.has(delta.id)) return;

  void chrome.downloads.search({ id: delta.id }).then(([item]) => {
    if (item) void consider(item);
  });
});

chrome.runtime.onMessage.addListener((message, sender) => {
  if (message?.type === 'click') {
    lastClick = { ctrl: message.ctrl, alt: message.alt, shift: message.shift, at: Date.now() };
  } else if (message?.type === 'save-media') {
    void saveMedia(message.tabId ?? sender.tab?.id, message.index);
  } else if (message?.type === 'page-start' && sender.tab) {
    void updateMedia(sender.tab.id, (items) => keepDocument(items, sender.documentId)).catch(
      () => {},
    );
  }
});

// Writes wait their turn, because several finds can arrive at once and each one reads
// the list before changing it.
let mediaWrites = Promise.resolve();

function updateMedia(tabId, change) {
  const written = mediaWrites.then(async () => {
    const key = mediaKey(tabId);
    const { [key]: items = [] } = await chrome.storage.session.get(key);
    const next = change(items);
    if (next === items) return null;

    await chrome.storage.session.set({ [key]: next });
    await chrome.action
      .setBadgeText({ tabId, text: next.length > 0 ? String(next.length) : '' })
      .catch(() => {});
    return next;
  });

  mediaWrites = written.catch(() => {});
  return written;
}

async function showPanel(tabId, frameId, items) {
  const { settings } = await chrome.storage.local.get('settings');
  if (settings?.videoPanel === false) return;

  const message = { type: 'media', labels: items.map(mediaLabel) };
  await chrome.tabs.sendMessage(tabId, message, { frameId }).catch(() => {});
}

function findMedia(details) {
  if (details.tabId < 0 || (details.statusCode !== 200 && details.statusCode !== 206)) return;

  const kind = mediaKind({
    url: details.url,
    type: details.type,
    contentType: headerValue(details.responseHeaders, 'content-type'),
    length: responseLength(details.responseHeaders),
  });
  if (!kind) return;

  const found = { url: details.url, kind, documentId: details.documentId };
  void updateMedia(details.tabId, (items) => addMedia(items, found))
    .then((items) => (items ? showPanel(details.tabId, details.frameId, items) : undefined))
    .catch(() => {});
}

// This only watches. Nothing here can block or change a request.
chrome.webRequest.onHeadersReceived.addListener(
  findMedia,
  { urls: ['<all_urls>'], types: ['media', 'xmlhttprequest', 'other'] },
  ['responseHeaders'],
);

chrome.tabs.onRemoved.addListener((tabId) => {
  void chrome.storage.session.remove(mediaKey(tabId));
});

/** Offers a find to the app, which asks the user what to do with it. */
async function saveMedia(tabId, index) {
  const key = mediaKey(tabId);
  const { [key]: items = [] } = await chrome.storage.session.get(key);
  const item = items[index];
  if (!item) return;

  const tab = await chrome.tabs.get(tabId).catch(() => null);
  const stream = item.kind === 'stream';
  const file = await describe(item.url, {
    referrer: tab?.url,
    fileName: stream ? tab?.title : undefined,
    video: stream,
  });
  await ask({ type: 'download', ...file }).catch(() => {});
}

const MENU = {
  link: 'tributary-link',
  selection: 'tributary-selection',
  page: 'tributary-page',
};

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: MENU.link, title: 'Download with Tributary', contexts: ['link'] });
  chrome.contextMenus.create({
    id: MENU.selection,
    title: 'Download selected links with Tributary',
    contexts: ['selection'],
  });
  chrome.contextMenus.create({
    id: MENU.page,
    title: 'Download all links with Tributary',
    contexts: ['page'],
  });
});

async function sendLinks(tab, mode) {
  const urls = await chrome.tabs.sendMessage(tab.id, { type: 'collect', mode });
  const links = usableLinks(urls ?? []);
  if (links.length === 0) return;

  const items = await Promise.all(links.map((url) => describe(url, { referrer: tab.url })));
  await ask({ type: 'batch', items });
}

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === MENU.link) {
    void describe(info.linkUrl, { referrer: info.pageUrl }).then((file) =>
      ask({ type: 'download', ...file }).catch(() => {}),
    );
  } else if (tab && info.menuItemId === MENU.selection) {
    void sendLinks(tab, 'selection').catch(() => {});
  } else if (tab && info.menuItemId === MENU.page) {
    void sendLinks(tab, 'page').catch(() => {});
  }
});
