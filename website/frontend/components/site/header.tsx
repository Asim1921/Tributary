import Link from "next/link";

import { site } from "@/lib/site";
import { ButtonLink } from "@/components/ui/button";
import { Container } from "@/components/ui/container";
import { DownloadIcon } from "@/components/ui/icons";

import { Logo } from "./logo";
import { MobileMenu } from "./mobile-menu";
import { ThemeToggle } from "./theme-toggle";

const links: readonly { href: string; label: string }[] = [
  { href: "/#how-it-works", label: "How it works" },
  { href: "/#features", label: "Features" },
  { href: "/#footage", label: "See it work" },
  { href: "/changelog", label: "Changelog" },
];

export function Header() {
  return (
    <header className="sticky top-0 z-40 border-b border-bank/70 bg-veil backdrop-blur-md supports-[backdrop-filter]:bg-veil">
      <Container className="flex h-16 items-center gap-6">
        <Link href="/" className="flex items-center gap-2.5 rounded-s font-display text-[1.2rem] font-[680] tracking-[-0.02em]">
          <Logo id="header" className="size-8" />
          <span>{site.name}</span>
        </Link>
        <nav aria-label="Main" className="hidden flex-1 md:block">
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
        <div className="ml-auto flex items-center gap-1 md:ml-0">
          <ThemeToggle />
          {/* On phones the hero's full-width button and the menu offer the download instead. */}
          <span className="hidden sm:block">
            <ButtonLink href="/download" icon={<DownloadIcon />} className="ml-1">
              Download
            </ButtonLink>
          </span>
          <MobileMenu links={links} />
        </div>
      </Container>
    </header>
  );
}
