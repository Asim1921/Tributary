/** The languages the site is written in. English is at the site's own addresses, the
 * others under their code: `/ur/download`. The same five as the app. */
export const LANGUAGES = [
  { code: "en", name: "English", dir: "ltr" },
  { code: "ur", name: "اردو", dir: "rtl" },
  { code: "ar", name: "العربية", dir: "rtl" },
  { code: "hi", name: "हिन्दी", dir: "ltr" },
  { code: "es", name: "Español", dir: "ltr" },
] as const;

export type Lang = (typeof LANGUAGES)[number]["code"];

export const DEFAULT_LANG: Lang = "en";

export function isLang(value: string): value is Lang {
  return LANGUAGES.some((language) => language.code === value);
}

export function dirOf(lang: Lang): "ltr" | "rtl" {
  return LANGUAGES.find((language) => language.code === lang)?.dir ?? "ltr";
}

/** An address on the site, in a language: English keeps the plain one. */
export function localPath(lang: Lang, path: string): string {
  if (lang === DEFAULT_LANG) return path;
  if (path === "/") return `/${lang}`;
  // "/#features" becomes "/ur#features".
  if (path.startsWith("/#")) return `/${lang}${path.slice(1)}`;
  return `/${lang}${path}`;
}

/** The site's own address of a page, without its language: "/ur/download" is "/download". */
export function plainPath(pathname: string): string {
  const [, first, ...rest] = pathname.split("/");
  if (first && isLang(first) && first !== DEFAULT_LANG) return `/${rest.join("/")}`;
  return pathname || "/";
}

/** Fills `{name}` in a text. */
export function fill(text: string, values: Record<string, string | number>): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in values ? String(values[name]) : whole,
  );
}
