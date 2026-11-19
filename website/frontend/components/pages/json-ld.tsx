import { serializeJsonLd, type JsonLdObject } from "@/lib/jsonld";

/** Structured data for search engines. Not executed: a data block, so the CSP doesn't apply. */
export function JsonLd({ data }: { data: JsonLdObject }) {
  return <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: serializeJsonLd(data) }} />;
}
