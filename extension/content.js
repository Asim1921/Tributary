// Runs in web pages. Notes which keys are held on a click, lists links on request, and
// shows the panel over video players.

// Words in the browser's own language, from _locales, and the way they read.
const say = (name, substitutions) => chrome.i18n.getMessage(name, substitutions);
const DIRECTION = chrome.i18n.getMessage('@@bidi_dir') || 'ltr';

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
    padding: 5px 8px; overflow: hidden; font: inherit; color: inherit; text-align: start;
    text-overflow: ellipsis; white-space: nowrap; cursor: pointer;
    background: transparent; border: 0; border-radius: 4px;
  }
  button:hover { background: #26b5a433; }
  .hide { align-self: flex-end; padding: 0 6px; opacity: 0.7; }
`;

// These match `MIN_PLAYER_LONG_SIDE` and `MIN_PLAYER_SHORT_SIDE` in rules.js, which a test
// checks. A content script can't import that file, so the two numbers are kept here too.
// Set by scripts/build-extension.mjs for the browser stores, whose rules don't allow
// saving YouTube's videos: no button on YouTube there.
const STORE_BUILD = false;

const MIN_PLAYER_LONG_SIDE = 320;
const MIN_PLAYER_SHORT_SIDE = 180;

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
/**
 * What a player shows about the video in it, for working out that video's own address.
 * Sites like YouTube change page without reloading, so the address bar can show a list, or
 * the home page, while a video plays.
 */
function describePlayer(video) {
  if (!video) return {};
  const main = video.closest('#movie_player');
  return {
    // YouTube's players name their video in a title link. It's empty in the main one.
    named: video.closest('.html5-video-player')?.querySelector('a.ytp-title-link')?.href || undefined,
    // The main player keeps playing in a corner when the page moves on, and the watch page
    // behind it keeps the id of what's playing.
    watching: main
      ? (document.querySelector('ytd-watch-flexy')?.getAttribute('video-id') ?? undefined)
      : undefined,
    // A preview in a feed sits inside a link to its own page.
    link: video.closest('a[href]')?.href || undefined,
  };
}

const saysWhich = (facts) => Boolean(facts.named || facts.watching || facts.link);

/**
 * What the player beside the panel said about its video while it played. A preview, like
 * YouTube's on the home page, is taken away as soon as the pointer leaves it for the panel,
 * so by the time the button is pressed there may be nothing left to ask.
 */
let lastPlayer = { facts: {}, page: '', at: 0 };

function rememberPlayer() {
  const facts = describePlayer(largestVideo());
  if (saysWhich(facts)) lastPlayer = { facts, page: location.href, at: Date.now() };
}

/** What the player says now, or what it said a moment ago if it has just been taken away. */
function currentPlayer() {
  const facts = describePlayer(largestVideo());
  if (saysWhich(facts)) return facts;
  const recent = lastPlayer.page === location.href && Date.now() - lastPlayer.at < 60_000;
  return recent ? lastPlayer.facts : facts;
}

/** The panel stays while the pointer is on it, even if the preview it belongs to has gone. */
let pointerOnPanel = false;

function playerWorthPanel() {
  if (STORE_BUILD && /(^|\.)youtube\.com$|^youtu\.be$/.test(location.hostname)) return false;
  const video = largestVideo();
  if (!video) return false;

  const box = video.getBoundingClientRect();
  const style = getComputedStyle(video);
  const hidden = style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0';
  // Measured on the long and short sides, so an upright player counts too.
  const long = Math.max(box.width, box.height);
  const short = Math.min(box.width, box.height);
  return long >= MIN_PLAYER_LONG_SIDE && short >= MIN_PLAYER_SHORT_SIDE && !hidden;
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
  pointerOnPanel = false;
  panel.addEventListener('pointerenter', () => {
    pointerOnPanel = true;
  });
  panel.addEventListener('pointerleave', () => {
    pointerOnPanel = false;
  });

  // Closed, so the page can't reach in and press anything.
  const shadow = panel.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = PANEL_STYLE;

  const box = document.createElement('div');
  box.className = 'panel';
  box.dir = DIRECTION;

  if (entries) {
    entries.forEach((label, index) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = say('panelSave', [label]);
      button.addEventListener('click', (event) => {
        if (event.isTrusted) void chrome.runtime.sendMessage({ type: 'save-media', index });
      });
      box.append(button);
    });
  } else {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = say('panelDownloadVideo');
    button.addEventListener('click', (event) => {
      if (!event.isTrusted) return;
      void chrome.runtime.sendMessage({
        type: 'save-page',
        page: location.href,
        // The page this player is embedded in, as the browser named it to the player.
        parent: window === window.top ? undefined : document.referrer || undefined,
        title: document.title,
        player: currentPlayer(),
      });
    });
    box.append(button);
  }

  const hide = document.createElement('button');
  hide.type = 'button';
  hide.className = 'hide';
  hide.textContent = say('panelHide');
  hide.addEventListener('click', (event) => {
    if (!event.isTrusted) return;
    panelHidden = true;
    panel?.remove();
    panel = null;
    pointerOnPanel = false;
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

  // Left alone while it's about to be pressed, because a preview stops as the pointer moves
  // off it onto the panel.
  if (pointerOnPanel && panel?.isConnected) return;

  if (panelEnabled && playerWorthPanel()) {
    rememberPlayer();
    showPanel(found);
  } else if (panel) {
    panel.remove();
    panel = null;
    shownFor = '';
    pointerOnPanel = false;
  }
}

// Players appear, start and resize after the page loads, so this keeps looking.
setInterval(refreshPanel, 1500);
for (const name of ['loadedmetadata', 'playing', 'resize']) {
  document.addEventListener(name, refreshPanel, { capture: true, passive: true });
}
refreshPanel();

// Saving every picture and video on the page. Only the extension can start this: a page's
// own scripts can't send it messages. What's collected here are plain facts about what
// the page shows. The background decides what's worth offering, by rules it can test.

// Matches \`MIN_IMAGE_SIDE\` in rules.js, which a test checks. Only used for the running count.
const MIN_IMAGE_SIDE = 200;
const SCROLL_PAUSE_MS = 1500;
const QUIET_STEPS = 3;
const MAX_SCROLL_STEPS = 60;
const MAX_COLLECTED = 300;

// The overlay sits in the bottom corner, with its two buttons a fixed size at its right.
const COLLECT_STYLE = `
  .box {
    display: flex; gap: 10px; align-items: center;
    padding: 10px 12px; font: 13px/1.3 system-ui, sans-serif; color: #eef3f2;
    background: #15201ff2; border: 1px solid #26b5a4; border-radius: 8px;
    box-shadow: 0 4px 16px #0006;
  }
  span { max-width: 260px; }
  button {
    box-sizing: border-box; width: 96px; padding: 5px 0; font: inherit; color: inherit;
    cursor: pointer; background: #26b5a433; border: 1px solid #26b5a4; border-radius: 5px;
  }
  button.quiet { background: transparent; border-color: transparent; opacity: 0.8; }
`;

let collecting = null;

/** The element that scrolls the page: the page itself, or the biggest scrolling box in it. */
function scroller() {
  const page = document.scrollingElement ?? document.documentElement;
  if (page.scrollHeight > page.clientHeight + 4) return page;
  let best = page;
  let room = 0;
  for (const element of document.querySelectorAll('*')) {
    const style = getComputedStyle(element);
    if (!/(auto|scroll)/.test(style.overflowY)) continue;
    const extra = element.scrollHeight - element.clientHeight;
    if (extra > room) {
      best = element;
      room = extra;
    }
  }
  return best;
}

/** Adds what the page shows now to what's been seen, so rows a site drops still count. */
function lookAround(seen) {
  for (const image of document.images) {
    const src = image.currentSrc || image.src;
    const key = image.srcset || src;
    if (!key) continue;
    const box = image.getBoundingClientRect();
    const known = seen.images.get(key);
    if (known) {
      // A lazy picture first seen before it loaded is bigger now, so keep the largest.
      known.width = Math.max(known.width, Math.round(box.width));
      known.height = Math.max(known.height, Math.round(box.height));
      known.naturalWidth = Math.max(known.naturalWidth, image.naturalWidth);
      known.naturalHeight = Math.max(known.naturalHeight, image.naturalHeight);
      continue;
    }
    seen.images.set(key, {
      srcset: image.srcset || image.closest('picture')?.querySelector('source[srcset]')?.srcset || '',
      src,
      width: Math.round(box.width),
      height: Math.round(box.height),
      naturalWidth: image.naturalWidth,
      naturalHeight: image.naturalHeight,
    });
  }
  for (const video of document.querySelectorAll('video')) {
    const src = video.currentSrc || video.src || video.querySelector('source')?.src || '';
    const player = describePlayer(video);
    const key = /^https?:/.test(src) ? src : player.named || player.watching || player.link || location.href;
    if (!seen.videos.has(key)) seen.videos.set(key, { src, ...player, page: location.href });
  }
  for (const link of document.links) {
    if (seen.links.has(link.href)) continue;
    seen.links.set(link.href, { href: link.href, holdsPicture: Boolean(link.querySelector('img, video, picture')) });
  }
}

/** A rough count for the overlay. The background makes the exact list. */
function tally(seen) {
  let pictures = 0;
  for (const image of seen.images.values()) {
    const [w, h] = image.naturalWidth > 0 ? [image.naturalWidth, image.naturalHeight] : [image.width, image.height];
    if (Math.min(w, h) >= MIN_IMAGE_SIDE) pictures += 1;
  }
  const videos = seen.videos.size + [...seen.links.values()].filter((link) => link.holdsPicture && /\/(reels?|videos?|shorts|watch)\b/.test(link.href)).length;
  return { pictures, videos };
}

/**
 * A small box in the page's bottom corner, out of the page's reach, with a few words and
 * buttons that only a real click presses.
 */
function showOverlay(text, buttons) {
  const host = document.createElement('div');
  host.setAttribute('data-tributary-collecting', '');
  host.style.cssText = 'all: initial; position: fixed; right: 16px; bottom: 16px; z-index: 2147483647;';
  const shadow = host.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = COLLECT_STYLE;
  const box = document.createElement('div');
  box.className = 'box';
  box.dir = DIRECTION;
  const words = document.createElement('span');
  words.textContent = text;
  box.append(words);
  for (const { label, quiet, press } of buttons) {
    const button = document.createElement('button');
    button.type = 'button';
    if (quiet) button.className = 'quiet';
    button.textContent = label;
    button.addEventListener('click', (event) => {
      if (event.isTrusted) press();
    });
    box.append(button);
  }
  shadow.append(style, box);
  document.documentElement.append(host);
  return {
    say: (text) => {
      words.textContent = text;
    },
    remove: () => host.remove(),
  };
}

/** Waits while the tab is out of sight, because a page loads nothing more until it's seen. */
async function untilSeen(overlay) {
  if (document.visibilityState !== 'hidden') return;
  overlay.say(say('collectWaiting'));
  // The browser says when, so a tab left for a long while carries on straight away. Its
  // timers then run only once a minute.
  await new Promise((done) => {
    const back = () => {
      if (document.visibilityState === 'hidden') return;
      document.removeEventListener('visibilitychange', back);
      done();
    };
    document.addEventListener('visibilitychange', back);
  });
}

/**
 * Looks through the page for pictures and videos, scrolling down a screen at a time as a
 * person would, until nothing new turns up, the end, the limit, or Stop. Then puts the
 * page back where it was and hands what it found to the extension.
 */
async function collectMedia({ scroll }) {
  if (collecting) return;
  const seen = { images: new Map(), videos: new Map(), links: new Map() };
  const state = { stopped: false, cancelled: false };
  collecting = state;
  const target = scroller();
  const start = target.scrollTop;
  const overlay = showOverlay(say('collectLooking'), [
    {
      label: say('collectStop'),
      press: () => {
        state.stopped = true;
      },
    },
    {
      label: say('collectCancel'),
      quiet: true,
      press: () => {
        state.cancelled = true;
      },
    },
  ]);
  const report = () => {
    const { pictures, videos } = tally(seen);
    overlay.say(say('collectFound', [String(pictures), String(videos)]));
  };

  try {
    lookAround(seen);
    report();
    let quiet = 0;
    for (let step = 0; scroll && step < MAX_SCROLL_STEPS && !state.stopped && !state.cancelled; step += 1) {
      await untilSeen(overlay);
      if (state.stopped || state.cancelled) break;
      const before = seen.images.size + seen.videos.size + seen.links.size;
      const height = target.scrollHeight;
      target.scrollTop += Math.round(target.clientHeight * 0.9);
      await new Promise((done) => setTimeout(done, SCROLL_PAUSE_MS));
      lookAround(seen);
      report();
      if (tally(seen).pictures + tally(seen).videos >= MAX_COLLECTED) break;
      const grew = seen.images.size + seen.videos.size + seen.links.size > before || target.scrollHeight > height;
      // A step spent out of sight proves nothing: the page had no chance to load more.
      if (document.visibilityState === 'hidden') continue;
      const atEnd = target.scrollTop + target.clientHeight >= target.scrollHeight - 4;
      // The end of the page is a quiet step too, as a slow feed may still add more.
      quiet = grew ? 0 : quiet + 1;
      // A login wall, the end, or a page that's stopped loading more.
      if (quiet >= QUIET_STEPS) break;
    }
  } finally {
    target.scrollTop = start;
    overlay.remove();
    collecting = null;
  }
  if (state.cancelled) return;

  void chrome.runtime.sendMessage({
    type: 'page-media',
    page: location.href,
    title: document.title,
    images: [...seen.images.values()],
    videos: [...seen.videos.values()],
    links: [...seen.links.values()],
  });
}

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.type === 'media') {
    found = message.labels;
    refreshPanel();
    return undefined;
  }
  if (message?.type === 'media-none') {
    const notice = showOverlay(say('collectNothing'), [
      { label: say('collectClose'), quiet: true, press: () => notice.remove() },
    ]);
    setTimeout(() => notice.remove(), 6000);
    return undefined;
  }
  if (message?.type === 'collect-media') {
    // Only the top page collects. A player's frame is reached through its own page.
    if (window !== window.top) return undefined;
    void collectMedia({ scroll: message.scroll !== false });
    // Says the page heard, so the popup can tell a page it can't reach.
    respond({ started: true });
    return undefined;
  }
  if (message?.type !== 'collect') return undefined;

  const links = message.mode === 'selection' ? linksInSelection() : [...document.links];
  respond(links.map((link) => link.href));
  return undefined;
});
