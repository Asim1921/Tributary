import type { MetadataRoute } from "next";

import { facts, site } from "@/lib/site";

// The newest milestone is when the pages last changed in substance.
const lastModified = new Date(`${facts.history[facts.history.length - 1].date}T00:00:00Z`);

export default function sitemap(): MetadataRoute.Sitemap {
  const page = (path: string, priority: number): MetadataRoute.Sitemap[number] => ({
    url: new URL(path, site.url).toString(),
    lastModified,
    changeFrequency: "monthly",
    priority,
  });
  return [page("/", 1), page("/download", 0.9), page("/changelog", 0.6), page("/privacy", 0.3)];
}
