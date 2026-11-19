import type { MetadataRoute } from "next";

import { site } from "@/lib/site";

export default function robots(): MetadataRoute.Robots {
  return {
    rules: [
      {
        userAgent: "*",
        allow: "/",
        // The API is for the pages, not for search results; downloads are counted there.
        disallow: "/api/",
      },
    ],
    sitemap: new URL("/sitemap.xml", site.url).toString(),
  };
}
