import type { MetadataRoute } from "next";

import { LANGUAGES, localPath } from "@/lib/i18n/languages";
import { facts, site } from "@/lib/site";

// The newest milestone is when the pages last changed in substance.
const lastModified = new Date(`${facts.history[facts.history.length - 1].date}T00:00:00Z`);

/** Every page in every language, each naming its versions in the others. */
export default function sitemap(): MetadataRoute.Sitemap {
  const address = (path: string) => new URL(path, site.url).toString();
  const pages: [string, number][] = [
    ["/", 1],
    ["/download", 0.9],
    ["/changelog", 0.6],
    ["/privacy", 0.3],
  ];
  return pages.flatMap(([path, priority]) => {
    const languages = Object.fromEntries(
      LANGUAGES.map((language) => [language.code, address(localPath(language.code, path))]),
    );
    return LANGUAGES.map((language) => ({
      url: address(localPath(language.code, path)),
      lastModified,
      changeFrequency: "monthly" as const,
      priority,
      alternates: { languages },
    }));
  });
}
