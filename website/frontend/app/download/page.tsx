import type { Metadata } from "next";
import Link from "next/link";

import { softwareApplication } from "@/lib/jsonld";
import { getLatestRelease } from "@/lib/release";
import { facts, site } from "@/lib/site";
import { ButtonLink } from "@/components/ui/button";
import { DownloadIcon, PlayIcon } from "@/components/ui/icons";
import { CodeBlock } from "@/components/pages/code-block";
import { DocSection } from "@/components/pages/doc-section";
import { ReleasePanel } from "@/components/pages/download/release-panel";
import { JsonLd } from "@/components/pages/json-ld";
import { PageHeader } from "@/components/pages/page-header";
import { Steps } from "@/components/pages/steps";
import { FlowList, InlineCode, Kbd, UiName } from "@/components/pages/text";

// Rendered per request (nginx caches it for 10 s), so the live size and checksum are there
// from the first visit after a deploy; the image is built with no API to ask.
export const dynamic = "force-dynamic";


const description =
  "Download Tributary for Windows 10 and 11: the installer and its SHA-256 checksum, how to install it, why SmartScreen warns, and how to connect Chrome or Edge.";

export const metadata: Metadata = {
  title: "Download",
  description,
  alternates: { canonical: "/download" },
  openGraph: {
    type: "website",
    siteName: site.name,
    url: "/download",
    title: "Download Tributary for Windows",
    description,
  },
};

const sections = [
  { id: "requirements", label: "What you need" },
  { id: "check", label: "Check the download" },
  { id: "install", label: "Install" },
  { id: "smartscreen", label: "If Windows warns you" },
  { id: "browser", label: "Connect Chrome or Edge" },
  { id: "update", label: "Update" },
  { id: "uninstall", label: "Uninstall" },
];

const link = "text-estuary underline decoration-1 underline-offset-[0.2em] hover:text-foam";

export default async function DownloadPage() {
  const release = await getLatestRelease();
  const file = release?.file ?? facts.fallbackFile;
  const requirements = release?.requirements.length ? release.requirements : facts.requirements;
  const hashCommand = `Get-FileHash .\\${file} -Algorithm SHA256`;

  return (
    <>
      <JsonLd data={softwareApplication(release)} />

      <PageHeader
        title="Download Tributary"
        lede="Free, for Windows 10 and 11. It installs for your user only, so it doesn't need administrator rights."
        aside={<ReleasePanel release={release} />}
      >
        <div className="mt-9 flex flex-wrap items-center gap-3">
          <ButtonLink href={site.downloadPath} size="lg" icon={<DownloadIcon className="text-[1.15em]" />} download>
            Download for Windows
          </ButtonLink>
          <ButtonLink href="/#footage" variant="secondary" size="lg" icon={<PlayIcon />}>
            Watch the demo
          </ButtonLink>
        </div>

        <nav aria-labelledby="on-this-page" className="mt-12 max-w-[48ch]">
          <p id="on-this-page" className="text-caption font-semibold text-foam">
            On this page
          </p>
          <ul className="mt-1 flex flex-wrap gap-x-5 text-[0.95rem]">
            {sections.map((section) => (
              <li key={section.id}>
                <a
                  href={`#${section.id}`}
                  className="inline-flex min-h-11 items-center rounded-s text-silt underline decoration-bank underline-offset-[0.3em] transition-colors duration-[160ms] hover:text-foam hover:decoration-(--current)"
                >
                  {section.label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
      </PageHeader>

      <DocSection id="requirements" title="What you need">
        <FlowList items={requirements} className="text-foam" />
      </DocSection>

      <DocSection
        id="check"
        title="Check the download"
        intro="A checksum proves the file you have is the one published here, unchanged on the way."
      >
        <div className="space-y-5">
          <p className="text-silt">
            Open PowerShell and run these two lines. If you saved the installer somewhere other than your
            Downloads folder, change the folder in the first line.
          </p>
          <CodeBlock
            label="PowerShell"
            code={`cd ~\\Downloads\n${hashCommand}`}
            copyLabel="Copy the PowerShell commands"
          />
          <p className="text-silt">
            The <UiName>Hash</UiName> it prints should match the SHA-256 checksum on this page. PowerShell
            prints it in capital letters, which makes no difference.
          </p>
          {release ? (
            <>
              <p className="text-silt">
                Or let PowerShell compare them for you. It prints <UiName>True</UiName> when they match.
              </p>
              <CodeBlock
                label="PowerShell"
                code={`(${hashCommand}).Hash -eq "${release.sha256}"`}
                copyLabel="Copy the PowerShell comparison"
              />
            </>
          ) : null}
          <p className="text-silt">
            If they don&apos;t match, don&apos;t run the file. Delete it and download it again.
          </p>
        </div>
      </DocSection>

      <DocSection id="install" title="Install">
        <Steps
          steps={[
            {
              title: "Download the installer",
              body: (
                <p>
                  Choose <UiName>Download for Windows</UiName>. Your browser saves{" "}
                  <span className="text-foam [overflow-wrap:anywhere]">{file}</span>, usually in your Downloads
                  folder. To be sure it arrived intact, <a className={link} href="#check">check its checksum</a>.
                </p>
              ),
            },
            {
              title: "Run it",
              body: (
                <p>
                  Open the file. Windows SmartScreen will probably warn you, because the installer isn&apos;t
                  signed: choose <UiName>More info</UiName>, then <UiName>Run anyway</UiName>.{" "}
                  <a className={link} href="#smartscreen">Why Windows warns you</a>
                </p>
              ),
            },
            {
              title: "Follow the installer",
              body: (
                <p>
                  Tributary installs for your user account only, so it doesn&apos;t ask for administrator rights.
                  It needs Microsoft Edge WebView2, which is part of Windows 11; if it&apos;s missing, the
                  installer fetches it.
                </p>
              ),
            },
            {
              title: "Open Tributary",
              body: (
                <p>
                  Start it from the Start menu. To let it take over downloads from your browser,{" "}
                  <a className={link} href="#browser">connect Chrome or Edge</a>.
                </p>
              ),
            },
          ]}
        />
      </DocSection>

      <DocSection
        id="smartscreen"
        title="If Windows warns you"
        intro="Why SmartScreen stops the installer, and how to go on."
      >
        <div className="space-y-4 text-silt">
          <p>
            Windows SmartScreen warns about programs it doesn&apos;t know yet. Tributary&apos;s installer
            isn&apos;t signed with a code-signing certificate, so Windows can&apos;t confirm who published it
            and shows <UiName>Windows protected your PC</UiName>.
          </p>
          <p>
            First, <a className={link} href="#check">check the file&apos;s SHA-256 checksum</a>. If it matches,
            choose <UiName>More info</UiName>, then <UiName>Run anyway</UiName>.
          </p>
          <p>If it doesn&apos;t match, don&apos;t run it. Delete the file and download it again.</p>
        </div>
      </DocSection>

      <DocSection
        id="browser"
        title="Connect Chrome or Edge"
        intro="Once for each browser, so Tributary can take over downloads and save video from the pages you watch."
      >
        <p className="mb-8 text-silt">
          Chrome and Edge don&apos;t let other programs install extensions, so you add Tributary&apos;s yourself.
        </p>
        <Steps
          steps={[
            {
              title: "Copy the extension folder",
              body: (
                <p>
                  In Tributary, open <UiName>Options</UiName>, then the <UiName>Browser</UiName> tab, and copy the
                  folder shown there.
                </p>
              ),
            },
            {
              title: "Turn on Developer mode",
              body: (
                <p>
                  Open <InlineCode>chrome://extensions</InlineCode> in Chrome, or{" "}
                  <InlineCode>edge://extensions</InlineCode> in Edge, and turn on <UiName>Developer mode</UiName>.
                </p>
              ),
            },
            {
              title: "Load the extension",
              body: (
                <p>
                  Choose <UiName>Load unpacked</UiName> and pick the folder you copied.
                </p>
              ),
            },
          ]}
        />
        <FlowList
          className="mt-10 border-t border-bank pt-8 text-silt"
          items={[
            <>
              Turn off any other download manager&apos;s extension. Both would try to take the same downloads.
            </>,
            <>
              Hold <Kbd>Ctrl</Kbd> while you click a link to send it to Tributary, or <Kbd>Alt</Kbd> to leave it
              with the browser. You can change both keys in <UiName>Options</UiName>.
            </>,
          ]}
        />
      </DocSection>

      <DocSection
        id="update"
        title="Update"
        intro={
          <>
            Tributary doesn&apos;t update itself. New versions are listed in the{" "}
            <Link className={link} href="/changelog">changelog</Link>.
          </>
        }
      >
        <Steps
          steps={[
            {
              title: "Install the new version",
              body: <p>Download it from this page and run the installer over the version you have.</p>,
            },
            {
              title: "Reload the extension",
              body: (
                <p>
                  Open <InlineCode>chrome://extensions</InlineCode> (or <InlineCode>edge://extensions</InlineCode>)
                  and press the circular arrow on Tributary&apos;s card. Chrome often keeps running in the
                  background after its windows close, still holding the old extension, so new features
                  won&apos;t appear until you reload it.
                </p>
              ),
            },
          ]}
        />
      </DocSection>

      <DocSection id="uninstall" title="Uninstall">
        <p className="text-silt">
          Open Windows <UiName>Settings</UiName>, then <UiName>Apps</UiName>. Find Tributary in your installed
          apps and choose <UiName>Uninstall</UiName>. To remove your download list, queues and settings too,
          tick <UiName>Delete the application data</UiName> in the uninstaller.
        </p>
        <FlowList
          className="mt-6 text-silt"
          items={[
            <>Files you downloaded stay where you saved them.</>,
            <>
              The uninstaller disconnects the browser extension. Remove it from the browser too, at{" "}
              <InlineCode>chrome://extensions</InlineCode> or <InlineCode>edge://extensions</InlineCode>.
            </>,
            <>
              Passwords you saved for sites stay in Windows Credential Manager, under names that start with{" "}
              <InlineCode>Tributary/</InlineCode>, until you remove them there.
            </>,
          ]}
        />
      </DocSection>
    </>
  );
}
