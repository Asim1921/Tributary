import Link from "next/link";

import { site } from "@/lib/site";
import { localPath } from "@/lib/i18n/languages";
import { getWords } from "@/lib/i18n/server";
import { ButtonLink } from "@/components/ui/button";
import { Container } from "@/components/ui/container";
import { DownloadIcon } from "@/components/ui/icons";
import { LanguageMenu } from "./language-menu";
import { Logo } from "./logo";
import { MobileMenu } from "./mobile-menu";
import { ThemeToggle } from "./theme-toggle";

export async function Header() {
  const { lang, w } = await getWords();
  const links: readonly { href: string; label: string }[] = [
    { href: localPath(lang, "/#how-it-works"), label: w.site.howItWorks },
    { href: localPath(lang, "/#features"), label: w.site.features },
    { href: localPath(lang, "/#footage"), label: w.site.footage },
    { href: localPath(lang, "/changelog"), label: w.site.changelog },
  ];

  return (
    <header className="sticky top-0 z-40 border-b border-bank/70 bg-veil backdrop-blur-md supports-[backdrop-filter]:bg-veil">
      <Container className="flex h-16 items-center gap-6">
        <Link href={localPath(lang, "/")} className="flex items-center gap-2.5 rounded-s font-display text-[1.2rem] font-[680] tracking-[-0.02em]">
          <Logo id="header" className="size-8" />
          <span>{site.name}</span>
        </Link>

        <nav aria-label={w.site.main} className="hidden flex-1 md:block">
          <ul className="flex items-center gap-1">
            {links.map((link) => (
              <li key={link.href}>
                <Link
                  href={link.href}
                  className="inline-flex min-h-11 items-center rounded-s px-3 text-[0.95rem] text-silt transition-colors duration-[160ms] hover:text-foam"
                >
                  {link.label}
                </Link>
              </li>
            ))}
          </ul>
        </nav>

        <div className="ms-auto flex items-center gap-1 md:ms-0">
          <LanguageMenu lang={lang} label={w.site.language} />
          <ThemeToggle toDark={w.site.toDark} toLight={w.site.toLight} />
          {/* On phones the hero's full-width button and the menu offer the download instead. */}
          <span className="hidden sm:block">
            <ButtonLink href={localPath(lang, "/download")} icon={<DownloadIcon />} className="ms-1">
              {w.site.download}
            </ButtonLink>
          </span>
          <MobileMenu
            links={links}
            download={{ href: localPath(lang, "/download"), label: w.site.downloadForWindows }}
            openLabel={w.site.openMenu}
            closeLabel={w.site.closeMenu}
            navLabel={w.site.main}
          />
        </div>
      </Container>
    </header>
  );
}
