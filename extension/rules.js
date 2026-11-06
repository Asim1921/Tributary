// The decisions the extension makes, with no browser calls so they can be tested alone.

/** How long after a click its keys still count. */
export const CLICK_WINDOW_MS = 3000;

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
