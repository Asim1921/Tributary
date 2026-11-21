import { lang as rootLang } from "next/root-params";

import { DEFAULT_LANG, isLang, type Lang } from "./languages";
import { WORDS } from "./words";

/** The language of the page being drawn, from its address (`app/[lang]`). */
export async function currentLang(): Promise<Lang> {
  const value = await rootLang();
  return value && isLang(value) ? value : DEFAULT_LANG;
}

/** The page's language and its words, for Server Components. */
export async function getWords() {
  const lang = await currentLang();
  return { lang, w: WORDS[lang] };
}
