import type { Metadata } from "next";

import { formatDate } from "@/lib/release";
import { site } from "@/lib/site";
import { fill, localPath } from "@/lib/i18n/languages";
import { alternatesFor } from "@/lib/i18n/metadata";
import { Rich } from "@/lib/i18n/rich";
import { getWords } from "@/lib/i18n/server";
import { DocSection } from "@/components/pages/doc-section";
import { PageHeader } from "@/components/pages/page-header";
import { FlowList, InlineCode } from "@/components/pages/text";

const updated = "2026-11-21";

export async function generateMetadata(): Promise<Metadata> {
  const { lang, w } = await getWords();
  return {
    title: w.meta.privacyTitle,
    description: w.meta.privacyDescription,
    alternates: alternatesFor(lang, "/privacy"),
    openGraph: {
      type: "website",
      siteName: site.name,
      url: localPath(lang, "/privacy"),
      title: w.meta.privacyOgTitle,
      description: w.meta.privacyDescription,
    },
  };
}

// The website rows match backend/app/main.py (counting), nginx/nginx.conf (log format) and
// docker-compose.yml (json-file logs, max-size 10m, max-file 3).
export default async function PrivacyPage() {
  const { lang, w } = await getWords();
  const p = w.privacy;
  return (
    <>
      <PageHeader title={p.title} lede={p.lede}>
        <p className="mt-5 text-caption text-silt">
          {fill(p.updated, { date: formatDate(updated, lang) })}
        </p>
      </PageHeader>

      <DocSection id="website" title={p.websiteTitle} intro={p.websiteIntro}>
        <dl className="divide-y divide-bank border-y border-bank">
          {p.kept.map((row) => (
            <div key={row.what} className="grid gap-x-8 gap-y-2 py-6 sm:grid-cols-[minmax(0,1fr)_12.5rem]">
              <dt className="font-semibold text-foam sm:col-start-1">{row.what}</dt>
              <dd className="text-silt sm:col-start-1">{row.detail}</dd>
              <dd className="mt-2 sm:col-start-2 sm:row-span-2 sm:row-start-1 sm:mt-0">
                <span className="block text-caption font-semibold text-silt">{p.howLong}</span>
                <span className="text-[0.97rem] text-foam">{row.howLong}</span>
              </dd>
            </div>
          ))}
        </dl>
      </DocSection>

      <DocSection id="not-done" title={p.notDoneTitle}>
        <FlowList className="text-foam" items={p.notDone} />
      </DocSection>

      <DocSection id="app" title={p.appTitle} intro={p.appIntro}>
        <FlowList
          className="text-silt"
          items={p.app.map((item) => (
            <Rich key={item} text={item} code={(inner) => <InlineCode>{inner}</InlineCode>} />
          ))}
        />
      </DocSection>
    </>
  );
}
