import { ButtonLink } from "@/components/ui/button";
import { Container } from "@/components/ui/container";
import { DownloadIcon, GitHubIcon } from "@/components/ui/icons";
import { formatBytes, type Release } from "@/lib/release";
import { facts, site } from "@/lib/site";

import { ConfluenceVisual } from "./confluence-visual";

/**
 * The first screen: what Tributary is, the two ways in, the facts a downloader checks first,
 * and the live confluence beside them. The words never depend on the picture.
 */
export function Hero({ release }: { release: Release | null }) {
  const size = release ? formatBytes(release.size) : "";
  const details = [
    { term: "Version", detail: release?.version ?? facts.fallbackVersion },
    // The size comes only from the release manifest; without it, the row is left out.
    ...(size ? [{ term: "Download size", detail: size }] : []),
    { term: "Runs on", detail: "Windows 10 and 11, 64-bit" },
  ];

  return (
    <section aria-labelledby="hero-title" className="relative">
      <Container className="grid items-center gap-x-6 gap-y-8 sm:gap-y-14 pb-[clamp(3.5rem,8vw,6rem)] pt-[clamp(2.5rem,6vw,5rem)] lg:grid-cols-12">
        <div className="@container lg:col-span-6">
          <h1
            id="hero-title"
            className="font-display text-[length:clamp(2.5rem,14.6cqi,5.06rem)] font-[690] leading-[0.98] tracking-[-0.035em] text-foam"
          >
            <span className="block">Many streams.</span>
            <span className="block">One file.</span>
          </h1>
          <p className="mt-7 max-w-[34em] text-lead text-silt">
            Tributary is a free download manager for Windows that splits each file across several
            connections and picks up where it left off after a crash, a shutdown or a dropped
            connection.
          </p>
          <div className="mt-10 flex flex-col gap-3 sm:flex-row sm:flex-wrap">
            <ButtonLink href={site.downloadPath} download size="lg" icon={<DownloadIcon className="text-[1.15em]" />}>
              Download for Windows
            </ButtonLink>
            <ButtonLink
              href={site.repo}
              external
              variant="secondary"
              size="lg"
              icon={<GitHubIcon className="text-[1.05em]" />}
            >
              View the source<span className="sr-only"> (opens in a new tab)</span>
            </ButtonLink>
          </div>
          <dl className="mt-12 flex flex-wrap gap-x-10 gap-y-5 border-t border-bank pt-6">
            {details.map(({ term, detail }) => (
              <div key={term}>
                <dt className="text-caption text-silt">{term}</dt>
                <dd className="tabular mt-1 text-body text-foam">{detail}</dd>
              </div>
            ))}
          </dl>
        </div>
        <div className="mx-auto w-full max-w-[28rem] lg:col-span-6 lg:max-w-none lg:pl-[6%]">
          <ConfluenceVisual />
        </div>
      </Container>
    </section>
  );
}
