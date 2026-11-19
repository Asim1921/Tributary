import type { Metadata } from "next";
import Link from "next/link";

import { ButtonLink } from "@/components/ui/button";
import { Container } from "@/components/ui/container";
import { DownloadIcon } from "@/components/ui/icons";
import { DryRiver } from "@/components/pages/dry-river";

export const metadata: Metadata = {
  title: "Page not found",
  robots: { index: false },
};

const more = [
  { href: "/changelog", label: "Changelog" },
  { href: "/privacy", label: "Privacy" },
];

export default function NotFound() {
  return (
    <Container className="py-[clamp(3.5rem,9vw,7.5rem)]">
      <div className="grid items-center gap-x-8 gap-y-14 lg:grid-cols-12">
        <div className="lg:col-span-7">
          <h1 className="max-w-[15ch] font-display text-h1 font-[690] tracking-[-0.034em]">
            There&apos;s no page at this address.
          </h1>
          <p className="mt-6 max-w-[46ch] text-lead text-silt">
            The link may be out of date, or the address mistyped. Everything on this site is a click away.
          </p>
          <div className="mt-9 flex flex-wrap items-center gap-3">
            <ButtonLink href="/" size="lg">
              Go to the home page
            </ButtonLink>
            <ButtonLink href="/download" variant="secondary" size="lg" icon={<DownloadIcon className="text-[1.1em]" />}>
              Download Tributary
            </ButtonLink>
          </div>
          <nav aria-label="More pages" className="mt-10">
            <ul className="flex flex-wrap gap-x-6 text-[0.97rem]">
              {more.map((item) => (
                <li key={item.href}>
                  <Link
                    href={item.href}
                    className="inline-flex min-h-11 items-center text-silt underline decoration-bank underline-offset-[0.3em] hover:text-foam hover:decoration-(--current)"
                  >
                    {item.label}
                  </Link>
                </li>
              ))}
            </ul>
          </nav>
        </div>
        <div className="hidden justify-center sm:flex lg:col-span-5">
          <DryRiver className="w-[min(100%,20rem)]" />
        </div>
      </div>
    </Container>
  );
}
