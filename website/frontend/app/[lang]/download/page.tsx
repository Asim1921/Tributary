import type { Metadata } from "next";
import Link from "next/link";

import { softwareApplication } from "@/lib/jsonld";
import { getLatestRelease } from "@/lib/release";
import { facts, site } from "@/lib/site";
import { fill, localPath } from "@/lib/i18n/languages";
import { alternatesFor } from "@/lib/i18n/metadata";
import { Rich } from "@/lib/i18n/rich";
import { getWords } from "@/lib/i18n/server";
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

export async function generateMetadata(): Promise<Metadata> {
  const { lang, w } = await getWords();
  return {
    title: w.meta.downloadTitle,
    description: w.meta.downloadDescription,
    alternates: alternatesFor(lang, "/download"),
    openGraph: {
      type: "website",
      siteName: site.name,
      url: localPath(lang, "/download"),
      title: w.meta.downloadOgTitle,
      description: w.meta.downloadDescription,
    },
  };
}

const link = "text-estuary underline decoration-1 underline-offset-[0.2em] hover:text-foam";

/** A sentence with bold names, things to type and keys, as the page sets them. */
function Said({ text, file }: { text: string; file?: string }) {
  return (
    <Rich
      text={file ? fill(text, { file }) : text}
      b={(inner) => <UiName>{inner}</UiName>}
      code={(inner) =>
        file && inner === file ? (
          <span className="text-foam [overflow-wrap:anywhere]">{inner}</span>
        ) : (
          <InlineCode>{inner}</InlineCode>
        )
      }
      keyboard={(inner) => <Kbd>{inner}</Kbd>}
    />
  );
}

export default async function DownloadPage() {
  const { lang, w } = await getWords();
  const d = w.download;
  const release = await getLatestRelease();
  const file = release?.file ?? facts.fallbackFile;
  // The API's requirements are in English; other languages say their own.
  const requirements =
    lang === "en" && release?.requirements.length ? release.requirements : w.facts.requirements;
  const hashCommand = `Get-FileHash .\\${file} -Algorithm SHA256`;
  const sections = Object.entries(d.sections) as [keyof typeof d.sections, string][];

  return (
    <>
      <JsonLd data={softwareApplication(release)} />
      <PageHeader title={d.title} lede={d.lede} aside={<ReleasePanel release={release} />}>
        <div className="mt-9 flex flex-wrap items-center gap-3">
          <ButtonLink href={site.downloadPath} size="lg" icon={<DownloadIcon className="text-[1.15em]" />} download>
            {w.site.downloadForWindows}
          </ButtonLink>
          <ButtonLink href={localPath(lang, "/#footage")} variant="secondary" size="lg" icon={<PlayIcon />}>
            {w.hero.demo}
          </ButtonLink>
        </div>

        <nav aria-labelledby="on-this-page" className="mt-12 max-w-[48ch]">
          <p id="on-this-page" className="text-caption font-semibold text-foam">
            {d.onThisPage}
          </p>
          <ul className="mt-1 flex flex-wrap gap-x-5 text-[0.95rem]">
            {sections.map(([id, label]) => (
              <li key={id}>
                <a
                  href={`#${id}`}
                  className="inline-flex min-h-11 items-center rounded-s text-silt underline decoration-bank underline-offset-[0.3em] transition-colors duration-[160ms] hover:text-foam hover:decoration-(--current)"
                >
                  {label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
      </PageHeader>

      <DocSection id="requirements" title={d.sections.requirements}>
        <FlowList items={requirements} className="text-foam" />
      </DocSection>

      <DocSection id="check" title={d.sections.check} intro={d.checkIntro}>
        <div className="space-y-5">
          <p className="text-silt">{d.checkRun}</p>
          <CodeBlock label="PowerShell" code={`cd ~\\Downloads\n${hashCommand}`} copyLabel={d.copyCommands} />
          <p className="text-silt">
            <Said text={d.checkHash} />
          </p>
          {release ? (
            <>
              <p className="text-silt">
                <Said text={d.checkCompare} />
              </p>
              <CodeBlock
                label="PowerShell"
                code={`(${hashCommand}).Hash -eq "${release.sha256}"`}
                copyLabel={d.copyComparison}
              />
            </>
          ) : null}
          <p className="text-silt">{d.checkMismatch}</p>
        </div>
      </DocSection>

      <DocSection id="install" title={d.sections.install}>
        <Steps
          steps={[
            {
              title: d.installSteps.downloadTitle,
              body: (
                <p>
                  <Said text={d.installSteps.download} file={file} />{" "}
                  <a className={link} href="#check">
                    {d.installSteps.checkLink}
                  </a>
                </p>
              ),
            },
            {
              title: d.installSteps.runTitle,
              body: (
                <p>
                  <Said text={d.installSteps.run} />{" "}
                  <a className={link} href="#smartscreen">
                    {d.installSteps.whyLink}
                  </a>
                </p>
              ),
            },
            {
              title: d.installSteps.followTitle,
              body: <p>{d.installSteps.follow}</p>,
            },
            {
              title: d.installSteps.openTitle,
              body: (
                <p>
                  {d.installSteps.open}{" "}
                  <a className={link} href="#browser">
                    {d.installSteps.connectLink}
                  </a>
                </p>
              ),
            },
          ]}
        />
      </DocSection>

      <DocSection id="smartscreen" title={d.sections.smartscreen} intro={d.smartscreenIntro}>
        <div className="space-y-4 text-silt">
          <p>
            <Said text={d.smartscreen} />
          </p>
          <p>
            <a className={link} href="#check">
              {d.smartscreenCheckLink}
            </a>{" "}
            <Said text={d.smartscreenCheck} />
          </p>
          <p>{d.smartscreenMismatch}</p>
        </div>
      </DocSection>

      <DocSection id="browser" title={d.sections.browser} intro={d.browserIntro}>
        <p className="mb-8 text-silt">{d.browserWhy}</p>
        <Steps
          steps={[
            { title: d.browserSteps.copyTitle, body: <p><Said text={d.browserSteps.copy} /></p> },
            { title: d.browserSteps.developerTitle, body: <p><Said text={d.browserSteps.developer} /></p> },
            { title: d.browserSteps.loadTitle, body: <p><Said text={d.browserSteps.load} /></p> },
          ]}
        />
        <FlowList
          className="mt-10 border-t border-bank pt-8 text-silt"
          items={[<>{d.browserOthers}</>, <Said key="keys" text={d.browserKeys} />]}
        />
      </DocSection>

      <DocSection
        id="update"
        title={d.sections.update}
        intro={
          <>
            {d.updateIntro}{" "}
            <Link className={link} href={localPath(lang, "/changelog")}>
              {d.changelogLink}
            </Link>
          </>
        }
      >
        <Steps
          steps={[
            { title: d.updateSteps.installTitle, body: <p>{d.updateSteps.install}</p> },
            { title: d.updateSteps.reloadTitle, body: <p><Said text={d.updateSteps.reload} /></p> },
          ]}
        />
      </DocSection>

      <DocSection id="uninstall" title={d.sections.uninstall}>
        <p className="text-silt">
          <Said text={d.uninstall} />
        </p>
        <FlowList
          className="mt-6 text-silt"
          items={d.uninstallNotes.map((note) => (
            <Said key={note} text={note} />
          ))}
        />
      </DocSection>
    </>
  );
}
