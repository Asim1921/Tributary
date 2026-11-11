// The decisions the extension makes, with no browser calls so they can be tested alone.

/** How long after a click its keys still count. */
export const CLICK_WINDOW_MS = 3000;

/** Direct media smaller than this is a sound effect or a preview, not worth offering. */
export const MIN_MEDIA_BYTES = 256 * 1024;

const MAX_MEDIA_ITEMS = 20;

/** A player smaller than this is a thumbnail or a preview, not worth a panel. */
export const MIN_PLAYER_WIDTH = 320;
export const MIN_PLAYER_HEIGHT = 180;

const SCHEMES = ['http:', 'https:', 'ftp:'];

/** The last part of a path, which may use either kind of slash. */
export function baseName(path) {
  return (path ?? '').split(/[\\/]/).pop() ?? '';
}

export function extensionOf(name) {
  const base = baseName(name);
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
}

/** Matches text where `*` is any run of characters and `?` is one. Case doesn't matter. */
export function wildcardMatch(pattern, text) {
  const escaped = pattern
    .toLowerCase()
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')
    .replace(/\*+/g, '.*')
    .replace(/\?/g, '.');
  return new RegExp(`^${escaped}$`).test(text.toLowerCase());
}

/** Whether a URL is on the list of sites and addresses to leave alone. */
export function isExcluded(url, exclusions) {
  let host = '';
  try {
    host = new URL(url).hostname;
  } catch {
    return false;
  }
  return (exclusions ?? []).some(
    (pattern) => pattern.trim() && (wildcardMatch(pattern.trim(), host) || wildcardMatch(pattern.trim(), url)),
  );
}

function keyHeld(key, click, now) {
  if (!click || now - click.at > CLICK_WINDOW_MS) return false;
  return Boolean(key && key !== 'none' && click[key]);
}

function hasCapturableScheme(url) {
  try {
    return SCHEMES.includes(new URL(url).protocol);
  } catch {
    return false;
  }
}

/** The address a download really comes from. */
export function downloadUrl(item) {
  return item.finalUrl || item.url;
}

/**
 * Decides whether to take a download from the browser.
 *
 * @param item the browser's download item
 * @param settings the app's capture settings, or null if it can't be reached
 * @param click the keys held at the last click, as `{ ctrl, alt, shift, at }`
 * @param now the current time in milliseconds
 */
export function shouldCapture(item, settings, click, now) {
  if (!settings?.enabled) return false;

  // Private windows stay private, and other extensions' downloads are theirs.
  if (item.incognito || item.byExtensionId) return false;

  const url = downloadUrl(item);
  if (!hasCapturableScheme(url)) return false;

  if (keyHeld(settings.skipKey, click, now)) return false;
  if (keyHeld(settings.forceKey, click, now)) return true;

  if (isExcluded(url, settings.exclusions)) return false;

  const name = baseName(item.filename) || new URL(url).pathname;
  return (settings.fileTypes ?? []).includes(extensionOf(name));
}

/** The value of a response header, whatever its case. */
export function headerValue(headers, name) {
  return (headers ?? []).find((header) => header.name.toLowerCase() === name)?.value ?? null;
}

/** The whole size of a response, which a range response gives separately. */
export function responseLength(headers) {
  const total = /\/(\d+)\s*$/.exec(headerValue(headers, 'content-range') ?? '');
  if (total) return Number(total[1]);

  const length = headerValue(headers, 'content-length');
  return length === null ? null : Number(length);
}

/**
 * What a web request holds, as far as saving it goes: `stream`, `file` or null.
 *
 * Only a media element's own request counts as a file. Sites that protect their video
 * feed the player in pieces by script, and those requests are deliberately left alone.
 */
export function mediaKind({ url, type, contentType, length }) {
  if (!hasCapturableScheme(url)) return null;

  const kind = (contentType ?? '').toLowerCase();
  const path = new URL(url).pathname.toLowerCase();
  if (
    kind.includes('mpegurl') ||
    kind.includes('dash+xml') ||
    path.endsWith('.m3u8') ||
    path.endsWith('.mpd')
  ) {
    return 'stream';
  }

  const media = /^(video|audio)\//.test(kind);
  const bigEnough = length === null || length === undefined || length >= MIN_MEDIA_BYTES;
  return type === 'media' && media && bigEnough ? 'file' : null;
}

const folderOf = (url) => url.split(/[?#]/, 1)[0].replace(/[^/]*$/, '');

/**
 * Adds a find to a page's list, with streams first. A player fetches a playlist for each
 * quality after the main one, and those sit beside it, so they aren't more videos.
 * Returns the same list if nothing changed.
 */
export function addMedia(items, found) {
  const known = items.some(
    (item) =>
      item.documentId === found.documentId &&
      (item.url === found.url ||
        (item.kind === 'stream' &&
          found.kind === 'stream' &&
          folderOf(item.url) === folderOf(found.url))),
  );
  if (known || items.length >= MAX_MEDIA_ITEMS) return items;

  const all = [...items, found];
  return [...all.filter((item) => item.kind === 'stream'), ...all.filter((item) => item.kind !== 'stream')];
}

/**
 * What's left of a tab's list once a new page has opened. The old page lives on until the
 * new one takes over, so its requests can still report in after loading has begun. Going
 * by which document each find came from is the only sure way to tell them apart.
 * Returns the same list if nothing changed.
 */
export function keepDocument(items, documentId) {
  const kept = items.filter((item) => item.documentId === documentId);
  return kept.length === items.length ? items : kept;
}

/** What to call a find in a list. */
export function mediaLabel(item) {
  if (item.kind === 'stream') return 'Video stream';

  const name = baseName(new URL(item.url).pathname);
  try {
    return decodeURIComponent(name) || 'Video';
  } catch {
    return name;
  }
}

/** Where a tab's list of finds is kept. */
export const mediaKey = (tabId) => `media-${tabId}`;

/** Whether a player is big and showing enough to put a panel beside. */
export function isWorthPanel(box) {
  return Boolean(
    box && box.width >= MIN_PLAYER_WIDTH && box.height >= MIN_PLAYER_HEIGHT && !box.hidden,
  );
}

/** A YouTube video's id: letters, digits, `-` and `_`. */
const VIDEO_ID = /^[\w-]{6,20}$/;

/** Reads an http or https address, or gives null for anything else. */
function webAddress(text) {
  try {
    const url = new URL(text);
    return url.protocol === 'http:' || url.protocol === 'https:' ? url : null;
  } catch {
    return null;
  }
}

/** Whether two addresses are on the same site, counting `www.` and other subdomains. */
function sameSite(one, other) {
  const plain = (url) => url.hostname.replace(/^www\./, '');
  const [a, b] = [plain(one), plain(other)];
  return a === b || a.endsWith(`.${b}`) || b.endsWith(`.${a}`);
}

function isYouTube(url) {
  return url.hostname === 'youtube.com' || url.hostname.endsWith('.youtube.com');
}

/**
 * Works out the address of the video a player is showing, from what the page shows:
 *
 * - `page`: where the tab is now. The browser's own record of a frame's address is the one
 *   it loaded with, which is out of date on sites that change page without reloading. On
 *   YouTube that's a playlist or the home page while a video plays.
 * - `named`: the address the player gives as its own video, as YouTube's previews do.
 * - `watching`: the id of the video in YouTube's main player. That player keeps playing in a
 *   corner when the page moves on to something else.
 * - `link`: the link a player sits inside, as previews in a feed do.
 *
 * Only addresses on the page's own site are taken from the page, so a page can't point the
 * button somewhere else. Returns null when there's no web address at all.
 */
export function videoPageAddress({ page, named, watching, link } = {}) {
  const here = webAddress(page);
  if (!here) return null;

  const fromPage = (text) => {
    const url = webAddress(text);
    return url && sameSite(url, here) ? url.href : null;
  };
  const playing =
    isYouTube(here) && VIDEO_ID.test(watching ?? '') ? `${here.origin}/watch?v=${watching}` : null;

  return fromPage(named) ?? playing ?? fromPage(link) ?? here.href;
}

/**
 * What to tell the app about a page with a player and no stream the extension could see.
 * There is deliberately no cookie in it: the app reads the page without the browser's
 * logins, and never passes them to another server. Returns null for anything but a web page.
 */
export function pageDownload({ url, page, title, userAgent }) {
  if (!webAddress(url)) return null;

  return {
    type: 'download',
    url,
    referrer: webAddress(page)?.href ?? url,
    userAgent,
    fileName: (title ?? '').trim() || undefined,
    video: true,
    page: true,
  };
}

/** Turns the browser's cookie list into a `Cookie` header. */
export function cookieHeader(cookies) {
  return (cookies ?? []).map(({ name, value }) => `${name}=${value}`).join('; ');
}

/** Keeps web links once each, in page order. */
export function usableLinks(urls) {
  const seen = new Set();
  return urls.filter((url) => {
    if (seen.has(url) || !hasCapturableScheme(url)) return false;
    seen.add(url);
    return true;
  });
}
