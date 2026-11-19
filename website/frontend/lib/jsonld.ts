/**
 * Structured data (schema.org) for search engines. Every value comes from the release record
 * or from lib/site, so it stays as true as the page it sits on.
 */
import type { Release } from "@/lib/release";
import { formatBytes } from "@/lib/release";
import { facts, site } from "@/lib/site";

type JsonLdValue = string | number | boolean | null | JsonLdObject | JsonLdValue[];
export type JsonLdObject = { [key: string]: JsonLdValue | undefined };

/** An absolute address on this site, from a path such as "/download". */
export function absoluteUrl(path: string): string {
  return new URL(path, site.url).toString();
}

/** Tributary as a SoftwareApplication: name, platform, version, price and where to get it. */
export function softwareApplication(release: Release | null): JsonLdObject {
  return {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: site.name,
    description: site.description,
    applicationCategory: "UtilitiesApplication",
    operatingSystem: "Windows 10, Windows 11",
    softwareVersion: release?.version ?? facts.fallbackVersion,
    downloadUrl: absoluteUrl(site.downloadPath),
    installUrl: absoluteUrl("/download"),
    url: absoluteUrl("/"),
    image: absoluteUrl("/icon.svg"),
    screenshot: absoluteUrl("/media/still-connections.webp"),
    ...(release ? { fileSize: formatBytes(release.size), datePublished: release.published } : {}),
    softwareRequirements: (release?.requirements.length ? release.requirements : facts.requirements).join("; "),
    isAccessibleForFree: true,
    offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
  };
}

/**
 * The JSON text for a <script type="application/ld+json"> tag. "<" is escaped so no value can
 * close the tag early (Next.js's JSON-LD guidance).
 */
export function serializeJsonLd(data: JsonLdObject): string {
  return JSON.stringify(data).replace(/</g, "\\u003c");
}
