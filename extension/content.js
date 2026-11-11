// Runs in web pages. Notes which keys are held on a click, lists links on request, and
// shows the panel over video players.

// Tells the extension which document this tab now holds, so it can drop finds from the
// page before it.
if (window === window.top) {
  chrome.runtime.sendMessage({ type: 'page-start' }).catch(() => {});
}

addEventListener(
  'mousedown',
  (event) => {
    chrome.runtime
      .sendMessage({
        type: 'click',
        ctrl: event.ctrlKey,
        alt: event.altKey,
        shift: event.shiftKey,
      })
      .catch(() => {});
  },
  { capture: true, passive: true },
);

function linksInSelection() {
  const selection = getSelection();
  if (!selection || selection.rangeCount === 0) return [];

  const range = selection.getRangeAt(0);
  return [...document.links].filter((link) => range.intersectsNode(link));
}

const PANEL_STYLE = `
  .panel {
    display: flex; flex-direction: column; gap: 2px; min-width: 150px; max-width: 240px;
    padding: 4px; font: 13px/1.3 system-ui, sans-serif; color: #eef3f2;
    background: #15201fee; border: 1px solid #26b5a4; border-radius: 7px;
  }
  button {
    padding: 5px 8px; overflow: hidden; font: inherit; color: inherit; text-align: left;
    text-overflow: ellipsis; white-space: nowrap; cursor: pointer;
    background: transparent; border: 0; border-radius: 4px;
  }
  button:hover { background: #26b5a433; }
  .hide { align-self: flex-end; padding: 0 6px; opacity: 0.7; }
`;

// These match `MIN_PLAYER_WIDTH` and `MIN_PLAYER_HEIGHT` in rules.js, which a test checks.
// A content script can't import that file, so the two numbers are kept here as well.
const MIN_PLAYER_WIDTH = 320;
const MIN_PLAYER_HEIGHT = 180;

/** The most room a player can have, so ads and previews don't get the panel. */
function largestVideo() {
  let best = null;
  let bestArea = 0;

  for (const video of document.querySelectorAll('video')) {
    const box = video.getBoundingClientRect();
    const area = box.width * box.height;
    if (area > bestArea) {
      best = video;
      bestArea = area;
    }
  }
  return best;
}

let panel = null;
let panelHidden = false;
let shownFor = '';
let lastAddress = location.href;

/** What the background found in this page: direct streams and files, by label. */
let found = [];

function placePanel() {
  if (!panel) return;

  const box = largestVideo()?.getBoundingClientRect();
  const margin = 8;
  const top = Math.max(margin, (box?.top ?? 0) + margin);
  const right = Math.max(margin, innerWidth - (box?.right ?? innerWidth) + margin);
  panel.style.top = `${top}px`;
  panel.style.right = `${right}px`;
}

/** Whether there's a player here worth a panel, with real room and actually showing. */
function playerWorthPanel() {
  const video = largestVideo();
  if (!video) return false;

  const box = video.getBoundingClientRect();
  const style = getComputedStyle(video);
  const hidden = style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0';
  return box.width >= MIN_PLAYER_WIDTH && box.height >= MIN_PLAYER_HEIGHT && !hidden;
}

function showPanel(labels) {
  if (panelHidden) return;
  // A frame only gets a panel if its player is actually in it.
  if (!playerWorthPanel()) return;

  // With nothing found directly, the one button hands the whole page over.
  const entries = labels.length > 0 ? labels : null;
  const signature = `${entries?.join('|') ?? ''}#${location.href}`;
  if (panel && shownFor === signature) return;
  shownFor = signature;

  panel?.remove();
  panel = document.createElement('div');
  panel.setAttribute('data-tributary-panel', entries ? 'media' : 'page');
  panel.style.cssText = 'all: initial; position: fixed; z-index: 2147483647;';

  // Closed, so the page can't reach in and press anything.
  const shadow = panel.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = PANEL_STYLE;

  const box = document.createElement('div');
  box.className = 'panel';

  if (entries) {
    entries.forEach((label, index) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = `Save: ${label}`;
      button.addEventListener('click', (event) => {
        if (event.isTrusted) void chrome.runtime.sendMessage({ type: 'save-media', index });
      });
      box.append(button);
    });
  } else {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = 'Download this video';
    button.addEventListener('click', (event) => {
      if (!event.isTrusted) return;
      void chrome.runtime.sendMessage({
        type: 'save-page',
        url: location.href,
        title: document.title,
      });
    });
    box.append(button);
  }

  const hide = document.createElement('button');
  hide.type = 'button';
  hide.className = 'hide';
  hide.textContent = 'Hide';
  hide.addEventListener('click', (event) => {
    if (!event.isTrusted) return;
    panelHidden = true;
    panel?.remove();
    panel = null;
  });
  box.append(hide);

  shadow.append(style, box);
  document.documentElement.append(panel);
  placePanel();
}

addEventListener('scroll', placePanel, { capture: true, passive: true });
addEventListener('resize', placePanel, { passive: true });

/**
 * Whether the user wants the panel. Read once and then kept up to date by a listener,
 * because asking storage every moment, on every page, is needless work.
 */
let panelEnabled = true;
try {
  void chrome.storage.local.get('settings').then(({ settings }) => {
    panelEnabled = settings?.videoPanel !== false;
  });
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === 'local' && changes.settings) {
      panelEnabled = changes.settings.newValue?.videoPanel !== false;
    }
  });
} catch {
  // The extension was reloaded under this page, so the panel simply stays as it is.
}

/** Puts the panel up, or takes it down, to match whether a player is on show. */
function refreshPanel() {
  if (!/^https?:$/.test(location.protocol)) return;

  // A page that changes without reloading, as YouTube does, gets its panel back.
  if (location.href !== lastAddress) {
    lastAddress = location.href;
    panelHidden = false;
  }

  if (panelEnabled && playerWorthPanel()) {
    showPanel(found);
  } else if (panel) {
    panel.remove();
    panel = null;
    shownFor = '';
  }
}

// Players appear, start and resize after the page loads, so this keeps looking.
setInterval(refreshPanel, 1500);
for (const name of ['loadedmetadata', 'playing', 'resize']) {
  document.addEventListener(name, refreshPanel, { capture: true, passive: true });
}
refreshPanel();

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.type === 'media') {
    found = message.labels;
    refreshPanel();
    return undefined;
  }
  if (message?.type !== 'collect') return undefined;

  const links = message.mode === 'selection' ? linksInSelection() : [...document.links];
  respond(links.map((link) => link.href));
  return undefined;
});
