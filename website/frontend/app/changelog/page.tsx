import type { Metadata } from "next";

import { getReleases } from "@/lib/release";
import { facts, site } from "@/lib/site";
import { History } from "@/components/pages/changelog/history";
import { ReleaseEntry } from "@/components/pages/changelog/release-entry";
import { DocSection } from "@/components/pages/doc-section";
import { PageHeader } from "@/components/pages/page-header";

// Rendered per request (nginx caches it for 10 s), so the live size and checksum are there
// from the first visit after a deploy; the image is built with no API to ask.
export const dynamic = "force-dynamic";


const description =
  "Every release of Tributary, the download manager for Windows, newest first, and the milestones that built the first one.";

export const metadata: Metadata = {
  title: "Changelog",
  description,
  alternates: { canonical: "/changelog" },
  openGraph: { type: "website", siteName: site.name, url: "/changelog", title: "Tributary changelog", description },
};

export default async function ChangelogPage() {
  const releases = await getReleases();
  const first = releases.at(-1);

  return (
    <>
      <PageHeader
        title="Changelog"
        lede="What each release of Tributary brought, newest first, and how the first one was built."
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
        title={`How ${first?.version ?? facts.fallbackVersion} was built`}
        intro="Milestones from the project's commit history, oldest first."
      >
        <History version={first?.version ?? facts.fallbackVersion} published={first?.published} />
      </DocSection>
    </>
  );
}
