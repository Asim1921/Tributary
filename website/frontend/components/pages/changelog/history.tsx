import Link from "next/link";

import { formatDate } from "@/lib/release";
import { facts } from "@/lib/site";

/**
 * How the first release came together, oldest first. Each milestone joins one main channel in
 * the icon's own curve, and the channel ends in the icon's arrow and base bar: the release.
 */
export function History({ version, published }: { version: string; published?: string }) {
  return (
    <div>
      <div className="relative">
        {/* The main channel, from the first junction to the arrow below the list. */}
        <span aria-hidden="true" className="absolute bottom-0 left-[11px] top-12 w-0.5 bg-bank" />
        <ol>
          {facts.history.map((milestone) => (
            <li key={milestone.date} className="relative pb-12 pl-16">
              <svg
                aria-hidden="true"
                focusable="false"
                className="absolute left-0 top-0 h-12 w-14 overflow-visible"
                viewBox="0 0 56 48"
                fill="none"
              >
                <path d="M54 13 C 30 13, 12 22, 12 48" stroke="var(--current)" strokeWidth="2" strokeLinecap="round" />
              </svg>
              <p className="text-[0.95rem] font-semibold text-(--current) tabular">
                <time dateTime={milestone.date}>{formatDate(milestone.date)}</time>
              </p>
              <h3 className="mt-1.5 font-display text-[1.42rem] font-[650] leading-snug tracking-[-0.018em]">
                {milestone.title}
              </h3>
              <p className="mt-2 max-w-[60ch] text-silt">{milestone.detail}</p>
            </li>
          ))}
        </ol>
      </div>

      {/* Where the channel ends: the icon's arrow and base bar, and the release it became. */}
      <div className="grid grid-cols-[1.5rem_minmax(0,1fr)] gap-x-10">
        <svg aria-hidden="true" focusable="false" className="h-10 w-6" viewBox="0 0 24 40" fill="none">
          <path d="M12 0 V15" stroke="var(--bank)" strokeWidth="2" />
          <path d="M3.5 9 L12 18.5 L20.5 9" stroke="var(--current)" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
          <rect x="2.5" y="27" width="19" height="3.2" rx="1.6" fill="var(--current)" />
        </svg>
        <div className="pt-1">
          <p className="font-display text-[1.42rem] font-[650] leading-snug tracking-[-0.018em]">
            Released as version {version}
            {published ? (
              <>
                {" "}
                on <time dateTime={published}>{formatDate(published)}</time>
              </>
            ) : null}
          </p>
          <p className="mt-2">
            <Link href="/download" className="text-estuary underline decoration-1 underline-offset-[0.2em] hover:text-foam">
              Get it, with its checksum and install steps
            </Link>
          </p>
        </div>
      </div>
    </div>
  );
}
