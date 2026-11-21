import type { Metadata } from "next";
import type { ReactNode } from "react";

import { formatDate } from "@/lib/release";
import { site } from "@/lib/site";
import { DocSection } from "@/components/pages/doc-section";
import { PageHeader } from "@/components/pages/page-header";
import { FlowList, InlineCode } from "@/components/pages/text";

const updated = "2026-11-21";

const description =
  "What Tributary's website keeps (a download count and short server logs, no cookies, no analytics) and what the app keeps: everything stays on your computer.";

export const metadata: Metadata = {
  title: "Privacy",
  description,
  alternates: { canonical: "/privacy" },
  openGraph: { type: "website", siteName: site.name, url: "/privacy", title: "Tributary privacy", description },
};

type Kept = { what: string; detail: ReactNode; howLong: ReactNode };

// Matches backend/app/main.py (counting), nginx/nginx.conf (log format) and
// docker-compose.yml (json-file logs, max-size 10m, max-file 3).
const kept: Kept[] = [
  {
    what: "Download counts",
    detail:
      "Each download of the installer is recorded with its version, the date and time, and whether your browser said it runs on Windows. Nothing else is recorded with it.",
    howLong: "For as long as the site runs, as the number of downloads of each version",
  },
  {
    what: "A code that stops double counting",
    detail:
      "So that one person downloading twice in a day counts once, the site makes a one-way code (a salted hash) from your IP address, your browser's user agent and the date. Your IP address itself is not stored.",
    howLong: "One day, then it is deleted",
  },
  {
    what: "Server logs",
    detail:
      "For each request, nginx, the web server in front of the site, logs the time, your IP address, the address you asked for, the response code and size, and how long it took. It doesn't log which browser you use or which page you came from.",
    howLong:
      "Up to three files of 10 MB; when they are full, the oldest is deleted, so how long depends on how busy the site is",
  },
  {
    what: "Your theme",
    detail:
      "If you switch between the light and dark theme, your browser remembers the choice on your device. It is never sent to the site.",
    howLong: "Until you clear this site's data in your browser",
  },
];

export default function PrivacyPage() {
  return (
    <>
      <PageHeader
        title="Privacy"
        lede="This website counts downloads and keeps short server logs. The app keeps everything on your computer, with no account and no telemetry."
      >
        <p className="mt-5 text-caption text-silt">
          Last updated <time dateTime={updated}>{formatDate(updated)}</time>
        </p>
      </PageHeader>

      <DocSection id="website" title="What this website keeps" intro="As little as it can, and only for as long as it needs.">
        <dl className="divide-y divide-bank border-y border-bank">
          {kept.map((row) => (
            <div key={row.what} className="grid gap-x-8 gap-y-2 py-6 sm:grid-cols-[minmax(0,1fr)_12.5rem]">
              <dt className="font-semibold text-foam sm:col-start-1">{row.what}</dt>
              <dd className="text-silt sm:col-start-1">{row.detail}</dd>
              <dd className="mt-2 sm:col-start-2 sm:row-span-2 sm:row-start-1 sm:mt-0">
                <span className="block text-caption font-semibold text-silt">How long</span>
                <span className="text-[0.97rem] text-foam">{row.howLong}</span>
              </dd>
            </div>
          ))}
        </dl>
      </DocSection>

      <DocSection id="not-done" title="What it doesn't do">
        <FlowList
          className="text-foam"
          items={[
            "It sets no cookies.",
            "It runs no analytics, tracking or advertising scripts.",
            "It loads nothing from other companies: the fonts, pictures and video all come from this site.",
          ]}
        />
      </DocSection>

      <DocSection
        id="app"
        title="What the app keeps"
        intro="Tributary keeps what it knows on your computer. There is no account, and no telemetry: it doesn't report what you download or how you use it."
      >
        <FlowList
          className="text-silt"
          items={[
            <>
              Your download list, queues and settings are in <InlineCode>%APPDATA%\app.tributary.desktop</InlineCode>.
            </>,
            "Passwords for sites and servers are kept in Windows Credential Manager, never in a file.",
            "Cookies that your browser passes on for a download are held in memory only and never written to disk, so after a restart such a download may need its link again.",
            "The browser extension talks only to Tributary on your computer.",
            "Tributary connects to the sites you download from, and to the place yt-dlp is published when you ask it to install or update yt-dlp. yt-dlp is never given your cookies or logins.",
            "Once a day, Tributary asks this website whether there's a newer version. It sends only the version it is and that it runs on 64-bit Windows, and you can turn this off under Options, General.",
            "When you press Find in its start dialog, Tributary asks the site you're downloading from for the checksum it publishes beside the file, without your cookies.",
            "Mirrors and Metalink servers you or a site name get the parts of the file they serve. Your cookies and logins only go to the file's own site.",
          ]}
        />
      </DocSection>

    </>
  );
}
