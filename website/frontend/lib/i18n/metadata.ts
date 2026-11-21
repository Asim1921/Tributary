import { LANGUAGES, localPath, type Lang } from "./languages";

/**
 * A page's own address in its language, and the same page in every other one, so search
 * engines send each reader to theirs. English is also the default for everyone else.
 */
export function alternatesFor(lang: Lang, path: string) {
  return {
    canonical: localPath(lang, path),
    languages: {
      ...Object.fromEntries(LANGUAGES.map((language) => [language.code, localPath(language.code, path)])),
      "x-default": path,
    },
  };
}
