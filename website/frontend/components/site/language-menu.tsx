"use client";

import { usePathname } from "next/navigation";
import { useEffect, useRef } from "react";

import { LANGUAGES, localPath, plainPath, type Lang } from "@/lib/i18n/languages";

/**
 * The same page in another language. A plain disclosure, so it works before the page's
 * script has loaded; Escape, a choice or a click elsewhere closes it.
 */
export function LanguageMenu({ lang, label }: { lang: Lang; label: string }) {
  const here = plainPath(usePathname() ?? "/");
  const details = useRef<HTMLDetailsElement>(null);

  useEffect(() => {
    const close = (event: Event) => {
      const open = details.current;
      if (!open?.open) return;
      if (event instanceof KeyboardEvent ? event.key === "Escape" : !open.contains(event.target as Node)) {
        open.open = false;
      }
    };
    document.addEventListener("keydown", close);
    document.addEventListener("pointerdown", close);
    return () => {
      document.removeEventListener("keydown", close);
      document.removeEventListener("pointerdown", close);
    };
  }, []);

  const current = LANGUAGES.find((language) => language.code === lang);
  return (
    <details ref={details} className="relative">
      <summary
        aria-label={label}
        title={label}
        className="inline-flex min-h-11 cursor-pointer list-none items-center gap-1.5 rounded-s px-2.5 text-[0.95rem] text-silt hover:text-foam [&::-webkit-details-marker]:hidden"
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true">
          <circle cx="12" cy="12" r="9" />
          <path d="M3 12h18M12 3c2.6 2.7 3.9 5.7 3.9 9s-1.3 6.3-3.9 9c-2.6-2.7-3.9-5.7-3.9-9s1.3-6.3 3.9-9Z" />
        </svg>
        <span lang={lang}>{current?.name}</span>
      </summary>
      <ul className="absolute end-0 top-full z-50 mt-1 min-w-[10rem] rounded-m border border-bank bg-riverbed p-1 shadow-lg">
        {LANGUAGES.map((language) => (
          <li key={language.code}>
            <a
              href={localPath(language.code, here)}
              hrefLang={language.code}
              lang={language.code}
              aria-current={language.code === lang ? "true" : undefined}
              className="flex min-h-11 items-center rounded-s px-3 text-foam hover:bg-channel aria-[current]:text-stream"
            >
              {language.name}
            </a>
          </li>
        ))}
      </ul>
    </details>
  );
}
