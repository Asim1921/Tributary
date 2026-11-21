import type { Release } from "@/lib/release";
import { formatBytes, formatDate } from "@/lib/release";
import { facts } from "@/lib/site";
import { Logo } from "@/components/site/logo";

import { Checksum } from "../checksum";
import { getWords } from "@/lib/i18n/server";

/**
 * The release on one card: what it is, when it was published, how big it is, and the checksum
 * to check it against. Without the API the card still names the version and file, and says
 * how to get the checksum.
 */
export async function ReleasePanel({ release }: { release: Release | null }) {
  const { lang, w } = await getWords();
  const version = release?.version ?? facts.fallbackVersion;
  const file = release?.file ?? facts.fallbackFile;

  return (
    <section aria-labelledby="release-title" className="rounded-m border border-bank bg-channel">
      <div className="flex items-center gap-4 px-6 pb-5 pt-6">
        <Logo id="release" className="size-12 shrink-0" />
        <div className="min-w-0">
          <h2 id="release-title" className="font-display text-h4 font-[660] leading-tight tracking-[-0.02em]">
            Tributary {version}
          </h2>
          <p className="mt-0.5 text-caption text-silt">{w.release.forWindows}</p>
        </div>
      </div>

      <dl className="grid grid-cols-[minmax(5.5rem,auto)_minmax(0,1fr)] gap-x-6 gap-y-2.5 border-t border-bank px-6 py-5 text-[0.97rem]">
        {release ? (
          <>
            <dt className="text-silt">{w.panel.published}</dt>
            <dd>
              <time dateTime={release.published}>{formatDate(release.published, lang)}</time>
            </dd>
            <dt className="text-silt">{w.panel.size}</dt>
            <dd className="tabular">{formatBytes(release.size)}</dd>
          </>
        ) : null}
        <dt className="text-silt">{w.panel.file}</dt>
        <dd className="[overflow-wrap:anywhere]">{file}</dd>
      </dl>

      <div className="border-t border-bank px-6 pb-6 pt-5">
        {release ? (
          <>
            <Checksum value={release.sha256} />
            <p className="mt-3 text-caption text-silt">
              <a href="#check" className="text-estuary underline decoration-1 underline-offset-[0.2em] hover:text-foam">
                {w.release.howToCheck}
              </a>
            </p>
          </>
        ) : (
          <>
            <h3 className="font-text text-caption font-semibold tracking-normal text-foam">{w.panel.checksum}</h3>
            <p className="mt-2 text-caption text-silt">{w.panel.noChecksum}</p>
          </>
        )}
      </div>
    </section>
  );
}
