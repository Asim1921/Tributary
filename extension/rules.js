// The decisions the extension makes, with no browser calls so they can be tested alone.

/** How long after a click its keys still count. */
export const CLICK_WINDOW_MS = 3000;

/** Direct media smaller than this is a sound effect or a preview, not worth offering. */
export const MIN_MEDIA_BYTES = 256 * 1024;

const MAX_MEDIA_ITEMS = 20;

/**
 * A player smaller than this is a thumbnail or a preview, not worth a panel. Measured on
 * its long and short sides, so an upright player counts as well as a wide one.
 */
export const MIN_PLAYER_LONG_SIDE = 320;
export const MIN_PLAYER_SHORT_SIDE = 180;

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
  if (!box || box.hidden) return false;
  const long = Math.max(box.width, box.height);
  const short = Math.min(box.width, box.height);
  return long >= MIN_PLAYER_LONG_SIDE && short >= MIN_PLAYER_SHORT_SIDE;
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
export function pageDownload({ url, page, parent, host, title, userAgent }) {
  if (!webAddress(url)) return null;
  const holder = webAddress(host);

  return {
    type: 'download',
    url,
    // For a player embedded in another page, that page, as the browser named it to the
    // player. Players often answer only when asked from the page they're embedded in.
    referrer: webAddress(parent)?.href ?? webAddress(page)?.href ?? url,
    // The page holding the player, to read instead when the player can't be read alone.
    host: holder && holder.href !== url ? holder.href : undefined,
    userAgent,
    fileName: (title ?? '').trim() || undefined,
    video: true,
    page: true,
  };
}

/**
 * The `Referer`, and for a script's request the `Origin`, that a stream was asked for
 * with. A player embedded from another site asks for its video as that site, and a video
 * server that checks this turns away anything else, such as the page the player sits in.
 * `initiator` is the origin the browser reports for the request.
 */
export function streamHeaders({ initiator, request } = {}, tabUrl) {
  const tab = webAddress(tabUrl);
  const from = webAddress(initiator);
  if (!from || (tab && from.origin === tab.origin)) return { referrer: tab?.href };

  return {
    referrer: `${from.origin}/`,
    origin: request === 'xmlhttprequest' ? from.origin : undefined,
  };
}

/** A picture smaller than this on either side is an icon, an avatar or a tracking pixel. */
export const MIN_IMAGE_SIDE = 200;

/** The most a page's media can come to in one go. */
export const MAX_PAGE_MEDIA = 1000;

const IMAGE_ENDINGS = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'avif', 'bmp', 'heic'];
const VIDEO_ENDINGS = ['mp4', 'webm', 'mov', 'm4v', 'mkv'];

/** Whether an address is a picture or a video file, going by its name. */
export function mediaFileKind(url) {
  const address = webAddress(url);
  if (!address) return null;
  const ending = address.pathname.toLowerCase().match(/\.([a-z0-9]+)$/)?.[1];
  if (IMAGE_ENDINGS.includes(ending)) return 'image';
  if (VIDEO_ENDINGS.includes(ending)) return 'video';
  return null;
}

/**
 * The biggest picture a `srcset` offers, by width, then by density, or the one the
 * browser chose when there's no choice. Relative addresses are read against the page.
 */
export function largestCandidate(srcset, current, page) {
  const resolve = (text) => {
    try {
      const url = new URL(text, page);
      return url.protocol === 'http:' || url.protocol === 'https:' ? url.href : null;
    } catch {
      return null;
    }
  };
  let best = null;
  let bestScore = -1;
  // Entries are split at a comma followed by space, so a comma inside an address stays.
  for (const entry of (srcset ?? '').split(/,\s+/)) {
    const [address, descriptor = '1x'] = entry.trim().split(/\s+/);
    if (!address) continue;
    const width = /^(\d+)w$/.exec(descriptor);
    const density = /^(\d+(?:\.\d+)?)x$/.exec(descriptor);
    // Widths are compared among themselves, and always beat a density.
    const score = width ? 1e6 + Number(width[1]) : density ? Number(density[1]) : 0;
    const url = resolve(address);
    if (url && score > bestScore) {
      best = url;
      bestScore = score;
    }
  }
  return best ?? (current ? resolve(current) : null);
}

/**
 * Whether a picture is worth saving: big enough to be content, and a real file. Its own
 * size counts when the browser knows it, and how big it's shown when it doesn't.
 */
export function worthSaving({ url, width = 0, height = 0, naturalWidth = 0, naturalHeight = 0 }) {
  const address = webAddress(url);
  if (!address || /\.svg$/i.test(address.pathname)) return false;
  const [w, h] = naturalWidth > 0 && naturalHeight > 0 ? [naturalWidth, naturalHeight] : [width, height];
  return Math.min(w, h) >= MIN_IMAGE_SIDE;
}

/** Whether an address is a single video's own page, as reels, shorts and watch pages are. */
export function looksLikeVideoPage(url) {
  const address = webAddress(url);
  if (!address) return false;
  const path = address.pathname.toLowerCase();
  if (path === '/watch' || path.startsWith('/watch/')) return true;
  return /\/(reels?|videos?|shorts|tv)\/[^/]+/.test(path);
}

/**
 * Turns what a page showed into the list to offer, in the order it was found, each
 * address once, up to a limit. Pictures and video files are fetched as they are. A video
 * fed by script goes by its own page, which is read without any cookie.
 *
 * `found` holds what the content script saw: `images` ({ srcset, src, width, height,
 * naturalWidth, naturalHeight }), `videos` ({ src, named, watching, link }) and `links`
 * ({ href, holdsPicture }).
 */
export function pageMediaBatch({ images = [], videos = [], links = [] } = {}, page) {
  const here = webAddress(page);
  if (!here) return [];

  // Keyed by what each one is, so a photo shown at two sizes is offered once, at the bigger.
  const items = new Map();
  const add = (url, kind, size = 0) => {
    const address = webAddress(url);
    if (!address) return;
    const key = kind === 'image' ? photoKey(address) : address.href;
    const held = items.get(key);
    if (held) {
      if (size > held.size) Object.assign(held, { url: address.href, size });
      return;
    }
    if (items.size < MAX_PAGE_MEDIA) items.set(key, { url: address.href, kind, size });
  };

  for (const image of images) {
    const best = largestCandidate(image.srcset, image.src, here.href);
    // The picture as loaded is no bigger than the largest offered, and an address often
    // says its size, which counts too when the picture hasn't loaded yet.
    const size = Math.max((image.naturalWidth ?? 0) * (image.naturalHeight ?? 0), statedSize(best ?? ''));
    if (best && worthSaving({ ...image, url: best })) add(best, 'image', size);
  }
  for (const video of videos) {
    // A feed that rewrites its address as it scrolls, so each video is read on the page it
    // was seen on, as long as that's this site.
    const seenOn = webAddress(video.page);
    const seenHere = seenOn && sameSite(seenOn, here) ? seenOn.href : here.href;
    const source = webAddress(video.src);
    // A playlist is only text. The page it plays on is what yt-dlp can read.
    if (source && !/\.(m3u8|mpd)$/i.test(source.pathname)) {
      add(video.src, 'video');
    } else {
      add(videoPageAddress({ page: seenHere, named: video.named, watching: video.watching, link: video.link }), 'page');
    }
  }
  for (const link of links) {
    const kind = mediaFileKind(link.href);
    if (kind) {
      add(link.href, kind);
    } else if (link.holdsPicture && looksLikeVideoPage(link.href)) {
      const address = webAddress(link.href);
      if (address && sameSite(address, here)) add(address.href, 'page');
    }
  }
  return [...items.values()].map(({ url, kind }) => ({ url, kind }));
}

/**
 * Photo sites name each photo's file by a long id, and serve its sizes from that one path
 * with different settings after the `?`. Such a path is one photo, however it's sized.
 * A short name such as `photo.jpg` says nothing, so the whole address counts then.
 */
function photoKey(address) {
  const name = address.pathname.split('/').pop() ?? '';
  return name.length >= 16 && /\d{6,}/.test(name) ? `${address.origin}${address.pathname}` : address.href;
}

/**
 * The size in pixels an address says its picture is, as image servers write it: `s960x960`,
 * `1080x1350` or `w=2000`. Only ever compared between sizes of one photo.
 */
export function statedSize(url) {
  let largest = 0;
  for (const [, width, height] of url.matchAll(/(\d{2,5})x(\d{2,5})/g)) largest = Math.max(largest, Number(width) * Number(height));
  const wide = /[?&](?:w|width)=(\d{2,5})\b/.exec(url);
  if (wide) largest = Math.max(largest, Number(wide[1]) ** 2);
  return largest;
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

/**
 * Whether the extension should reload itself to pick up the files a newer app installed.
 * Only once for each version, so a copy loaded from somewhere else can't reload forever.
 */
export function shouldReload(appVersion, ownVersion, reloadedFor) {
  return Boolean(appVersion) && appVersion !== ownVersion && reloadedFor !== appVersion;
}

/** Whether an address is on YouTube, which the store builds leave alone. */
export function isYouTubeAddress(text) {
  try {
    const url = new URL(text);
    return isYouTube(url) || url.hostname === 'youtu.be';
  } catch {
    return false;
  }
}
