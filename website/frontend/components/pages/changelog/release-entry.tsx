import Link from "next/link";

import type { Release } from "@/lib/release";
import { formatBytes, formatDate } from "@/lib/release";
import { ButtonLink } from "@/components/ui/button";
import { Container } from "@/components/ui/container";
import { DownloadIcon } from "@/components/ui/icons";

import { FlowList } from "../text";

/**
 * One release: its version and date on the left, what it brought on the right, and a way to
 * download exactly that version. `release` is null when the API couldn't be reached.
 */
export function ReleaseEntry({
  version,
  release,
  latest,
}: {
  version: string;
  release: Release | null;
  latest: boolean;
}) {
  const headingId = `version-${version.replace(/\./g, "-")}`;
  return (
    <section aria-labelledby={headingId}>
      <Container>
        <div className="grid gap-x-8 gap-y-6 border-t border-bank py-[clamp(3rem,6vw,4.75rem)] lg:grid-cols-12">
          <div className="lg:col-span-4">
            <h2 id={headingId} className="font-display text-h3 font-[660] tracking-[-0.026em]">
              Version {version}
            </h2>
            <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-2 text-silt">
              {release ? <time dateTime={release.published}>{formatDate(release.published)}</time> : null}
              {latest ? (
                <span className="rounded-full border border-(--current) px-2.5 py-0.5 text-caption font-semibold text-(--current)">
                  Latest release
                </span>
              ) : null}
            </div>
          </div>

          <div className="min-w-0 max-w-[64ch] lg:col-span-8">
            {release && release.notes.length > 0 ? (
              <FlowList items={release.notes} className="text-foam" />
            ) : (
              <p className="text-silt">
                The notes for this release didn&apos;t load. Reload the page in a minute to see them; the
                history below shows what went into it.
              </p>
            )}
            <div className="mt-8 flex flex-wrap items-center gap-x-6 gap-y-3">
              <ButtonLink
                href={`/api/download/${encodeURIComponent(version)}`}
                variant="secondary"
                icon={<DownloadIcon className="text-[1.1em]" />}
                download
              >
                {release ? `Download ${version} (${formatBytes(release.size)})` : `Download ${version}`}
              </ButtonLink>
              {latest ? (
                <Link
                  href="/download#check"
                  className="inline-flex min-h-11 items-center text-[0.95rem] text-estuary underline decoration-1 underline-offset-[0.2em] hover:text-foam"
                >
                  Check its checksum
                </Link>
              ) : null}
            </div>
          </div>
        </div>
      </Container>
    </section>
  );
}
