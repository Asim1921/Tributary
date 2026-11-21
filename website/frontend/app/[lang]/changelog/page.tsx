import type { Metadata } from "next";

import { getReleases } from "@/lib/release";
import { facts, site } from "@/lib/site";
import { fill, localPath } from "@/lib/i18n/languages";
import { alternatesFor } from "@/lib/i18n/metadata";
import { getWords } from "@/lib/i18n/server";
import { History } from "@/components/pages/changelog/history";
import { ReleaseEntry } from "@/components/pages/changelog/release-entry";
import { DocSection } from "@/components/pages/doc-section";
import { PageHeader } from "@/components/pages/page-header";

// Rendered per request (nginx caches it for 10 s), so the live size and checksum are there
// from the first visit after a deploy; the image is built with no API to ask.
export const dynamic = "force-dynamic";


export async function generateMetadata(): Promise<Metadata> {
  const { lang, w } = await getWords();
  return {
    title: w.meta.changelogTitle,
    description: w.meta.changelogDescription,
    alternates: alternatesFor(lang, "/changelog"),
    openGraph: {
      type: "website",
      siteName: site.name,
      url: localPath(lang, "/changelog"),
      title: w.meta.changelogOgTitle,
      description: w.meta.changelogDescription,
    },
  };
}

export default async function ChangelogPage() {
  const releases = await getReleases();
  const { w } = await getWords();
  const first = releases.at(-1);

  return (
    <>
      <PageHeader
        title={w.changelog.title}
        lede={w.changelog.lede}
      />

      {releases.length > 0 ? (
        releases.map((release, index) => (
          <ReleaseEntry key={release.version} version={release.version} release={release} latest={index === 0} />
        ))
      ) : (
        <ReleaseEntry version={facts.fallbackVersion} release={null} latest />
      )}

      <DocSection
        id="history"
        title={fill(w.changelog.builtTitle, { version: first?.version ?? facts.fallbackVersion })}
        intro={w.changelog.builtIntro}
      >
        <History version={first?.version ?? facts.fallbackVersion} published={first?.published} />
      </DocSection>
    </>
  );
}
