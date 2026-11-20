/** The checksum each length of hex is. */
const KINDS: Record<number, string> = { 32: 'md5', 40: 'sha1', 64: 'sha256', 128: 'sha512' };

/** A name in front of the hex, as in `sha256:` or `SHA-256 `. */
const NAMED = /^(md5|sha-?1|sha-?256|sha-?512)[\s:=]*/i;

/**
 * Whether pasted text is a checksum Tributary can check, read the way the app reads it:
 * the hex alone or with its name in front, ignoring case and spaces. Blank is fine,
 * because a checksum is optional.
 */
export function looksLikeChecksum(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return true;

  const named = NAMED.exec(trimmed);
  const hex = trimmed.slice(named?.[0].length ?? 0).replace(/\s+/g, '');
  const kind = KINDS[hex.length];
  if (!kind || !/^[0-9a-f]+$/i.test(hex)) return false;
  // A name that doesn't fit the length means one of the two is a mistake.
  return !named || (named[1] ?? '').toLowerCase().replace('-', '') === kind;
}

export const NOT_A_CHECKSUM =
  "That isn't a checksum Tributary can check. Paste the MD5, SHA-1, SHA-256 or SHA-512 code the site gives for the file.";
