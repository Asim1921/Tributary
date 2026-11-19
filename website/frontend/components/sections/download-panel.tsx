import { Fragment } from "react";

import { ChannelOutlet } from "@/components/channel/outlet";
import { ButtonLink } from "@/components/ui/button";
import { Container, Section } from "@/components/ui/container";
import { CopyButton } from "@/components/ui/copy-button";
import { DownloadIcon } from "@/components/ui/icons";
import { formatBytes, formatDate, type Release } from "@/lib/release";
import { facts, site } from "@/lib/site";

import { BlockTitle, Lede, SectionTitle } from "./heading";

const STEPS = [
  {
    title: "Download the installer",
    body: "Save it anywhere. Check its SHA-256 against the one above if you like.",
  },
  {
    title: "Open it and get past SmartScreen",
    body: "The installer isn't signed, so Windows warns about it. Choose More info, then Run anyway.",
  },
  {
    title: "Let it install",
    body: "It installs for your user only and needs no administrator rights. If Microsoft Edge WebView2 is missing, it fetches it.",
  },
  {
    title: "Add the browser extension",
    body: "Chrome and Edge don't let other programs install extensions, so you add Tributary's yourself. The download page has every step.",
  },
];

/** Where the channel arrives: the installer, its checksum, and what to do with it. */
export function DownloadPanel({ release }: { release: Release | null }) {
  const version = release?.version ?? facts.fallbackVersion;
  const groups = release?.sha256.match(/.{1,8}/g) ?? [];
  const meta = release
    ? [
        ["Version", version],
        ["Size", formatBytes(release.size)],
        ["Published", formatDate(release.published)],
      ]
    : [
        ["Version", version],
        ["File", facts.fallbackFile],
      ];

  return (
    <Section id="download" labelledBy="download-title" className="border-t border-bank lg:border-t-0">
      <Container>
        <div className="grid gap-x-12 gap-y-10 lg:grid-cols-[20rem_minmax(0,1fr)] lg:grid-rows-[auto_1fr]">
          <div className="lg:col-start-2 lg:row-start-1">
            <SectionTitle id="download-title">Download Tributary {version}</SectionTitle>
            <Lede>Free, for Windows 10 and 11. One installer, no account.</Lede>
          </div>

          <div className="lg:col-start-1 lg:row-span-2 lg:row-start-1">
            <ChannelOutlet>
              <ButtonLink href={site.downloadPath} download size="lg" icon={<DownloadIcon />}>
                Download for Windows
              </ButtonLink>
            </ChannelOutlet>
            <dl className="mt-8 grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-[0.97rem]">
              {meta.map(([term, value]) => (
                <Fragment key={term}>
                  <dt className="text-silt">{term}</dt>
                  <dd className="tabular text-foam [overflow-wrap:anywhere]">{value}</dd>
                </Fragment>
              ))}
            </dl>
          </div>

          <div className="space-y-12 lg:col-start-2 lg:row-start-2">
            <div>
              <div className="flex items-center justify-between gap-3">
                <BlockTitle>SHA-256 checksum</BlockTitle>
                {release ? <CopyButton value={release.sha256} label="Copy the SHA-256 checksum" /> : null}
              </div>
              {release ? (
                <p className="mt-3">
                  <code className="block font-code text-[0.9rem] leading-[1.75] text-foam">
                    {groups.map((group, index) => (
                      <Fragment key={index}>
                        {index > 0 ? <wbr /> : null}
                        <span className="mr-[0.7ch] inline-block last:mr-0">{group}</span>
                      </Fragment>
                    ))}
                  </code>
                </p>
              ) : (
                <p className="mt-3 max-w-[62ch] text-silt">
                  The release details didn&rsquo;t load, so the checksum can&rsquo;t be shown. Reload the page in a
                  minute to see it before you run the installer.
                </p>
              )}
            </div>

            <div>
              <BlockTitle>What you need</BlockTitle>
              <ul className="mt-3 max-w-[62ch] list-disc space-y-2 pl-5 text-silt marker:text-silt">
                {(release?.requirements.length ? release.requirements : facts.requirements).map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>

            <div>
              <BlockTitle>Install it</BlockTitle>
              <ol className="mt-5 max-w-[62ch]">
                {STEPS.map((step, index) => (
                  <li
                    key={step.title}
                    className="grid grid-cols-[2.75rem_minmax(0,1fr)] gap-x-4 border-t border-bank py-4 first:border-t-0 first:pt-0"
                  >
                    <span
                      aria-hidden="true"
                      className="tabular flex size-9 items-center justify-center rounded-full border border-bank font-display text-[1rem] font-[650] text-stream"
                    >
                      {index + 1}
                    </span>
                    <div>
                      <h4 className="font-display text-[1.15rem] font-[640] leading-snug tracking-[-0.015em] text-foam">
                        {step.title}
                      </h4>
                      <p className="mt-1.5 text-silt">{step.body}</p>
                    </div>
                  </li>
                ))}
              </ol>
              <p className="mt-6">
                <ButtonLink href="/download" variant="secondary">
                  Read the full install guide
                </ButtonLink>
              </p>
            </div>
          </div>
        </div>
      </Container>
    </Section>
  );
}
