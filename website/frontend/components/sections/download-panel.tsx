import { Fragment } from "react";

import { ChannelOutlet } from "@/components/channel/outlet";
import { ButtonLink } from "@/components/ui/button";
import { Container, Section } from "@/components/ui/container";
import { CopyButton } from "@/components/ui/copy-button";
import { DownloadIcon } from "@/components/ui/icons";
import { formatBytes, formatDate, type Release } from "@/lib/release";
import { facts, site } from "@/lib/site";
import { fill, localPath } from "@/lib/i18n/languages";
import { getWords } from "@/lib/i18n/server";

import { BlockTitle, Lede, SectionTitle } from "./heading";


/** Where the channel arrives: the installer, its checksum, and what to do with it. */
export async function DownloadPanel({ release }: { release: Release | null }) {
  const { lang, w } = await getWords();
  const p = w.panel;
  const version = release?.version ?? facts.fallbackVersion;
  const groups = release?.sha256.match(/.{1,8}/g) ?? [];
  const meta = release
    ? [
        [p.version, version],
        [p.size, formatBytes(release.size)],
        [p.published, formatDate(release.published, lang)],
      ]
    : [
        [p.version, version],
        [p.file, facts.fallbackFile],
      ];
  // The API's requirements are in English; other languages say their own.
  const requirements =
    lang === "en" && release?.requirements.length ? release.requirements : w.facts.requirements;

  return (
    <Section id="download" labelledBy="download-title" className="border-t border-bank lg:border-t-0">
      <Container>
        <div className="grid gap-x-12 gap-y-10 lg:grid-cols-[20rem_minmax(0,1fr)] lg:grid-rows-[auto_1fr]">
          <div className="lg:col-start-2 lg:row-start-1">
            <SectionTitle id="download-title">{fill(p.title, { version })}</SectionTitle>
            <Lede>{p.lede}</Lede>
          </div>

          <div className="lg:col-start-1 lg:row-span-2 lg:row-start-1">
            <ChannelOutlet>
              <ButtonLink href={site.downloadPath} download size="lg" icon={<DownloadIcon />} className="relative lg:w-full">
                {w.site.downloadForWindows}
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
                <BlockTitle>{p.checksum}</BlockTitle>
                {release ? (
                  <CopyButton value={release.sha256} label={p.copyChecksum} copyText={w.site.copy} copiedLabel={w.site.copied} />
                ) : null}
              </div>
              {release ? (
                <p className="mt-3">
                  <code dir="ltr" className="block font-code text-[0.9rem] leading-[1.75] text-foam">
                    {groups.map((group, index) => (
                      <Fragment key={index}>
                        {index > 0 ? <wbr /> : null}
                        <span className="mr-[0.7ch] inline-block last:mr-0">{group}</span>
                      </Fragment>
                    ))}
                  </code>
                </p>
              ) : (
                <p className="mt-3 max-w-[62ch] text-silt">{p.noChecksum}</p>
              )}
            </div>

            <div>
              <BlockTitle>{p.need}</BlockTitle>
              <ul className="mt-3 max-w-[62ch] list-disc space-y-2 ps-5 text-silt marker:text-silt">
                {requirements.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </div>

            <div>
              <BlockTitle>{p.install}</BlockTitle>
              <ol className="mt-5 max-w-[62ch]">
                {p.steps.map(([title, body], index) => (
                  <li
                    key={title}
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
                        {title}
                      </h4>
                      <p className="mt-1.5 text-silt">{body}</p>
                    </div>
                  </li>
                ))}
              </ol>
              <p className="mt-6">
                <ButtonLink href={localPath(lang, "/download")} variant="secondary">
                  {p.guide}
                </ButtonLink>
              </p>
            </div>
          </div>
        </div>
      </Container>
    </Section>
  );
}
