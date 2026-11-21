import Link from "next/link";

import { site } from "@/lib/site";
import { localPath } from "@/lib/i18n/languages";
import { getWords } from "@/lib/i18n/server";
import { Container } from "@/components/ui/container";
import { Logo } from "./logo";

export async function Footer() {
  const { lang, w } = await getWords();
  const year = new Date().getUTCFullYear();
  return (
    <footer className="border-t border-bank">
      <Container className="grid gap-10 py-14 md:grid-cols-[1.4fr_1fr]">
        <div className="max-w-[38ch]">
          <Link href={localPath(lang, "/")} className="inline-flex items-center gap-2.5 rounded-s font-display text-[1.15rem] font-[680] tracking-[-0.02em]">
            <Logo id="footer" className="size-7" />
            <span>{site.name}</span>
          </Link>
          <p className="mt-4 text-caption text-silt">{w.site.footerAbout}</p>
        </div>

        <nav aria-label="Tributary">
          <h2 className="font-text text-caption font-semibold tracking-normal text-foam">Tributary</h2>
          <ul className="mt-3 space-y-1.5 text-caption">
            <li><Link className="text-silt hover:text-foam" href={localPath(lang, "/download")}>{w.site.download}</Link></li>
            <li><Link className="text-silt hover:text-foam" href={localPath(lang, "/changelog")}>{w.site.changelog}</Link></li>
            <li><Link className="text-silt hover:text-foam" href={localPath(lang, "/privacy")}>{w.site.privacy}</Link></li>
          </ul>
        </nav>
      </Container>
      <Container className="flex flex-wrap justify-between gap-3 border-t border-bank/60 py-6 text-caption text-silt">
        <p>&copy; {year} Tributary</p>
        <p>{w.site.notAffiliated}</p>
      </Container>
    </footer>
  );
}
