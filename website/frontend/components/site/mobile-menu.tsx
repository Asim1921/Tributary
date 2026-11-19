"use client";

import Link from "next/link";
import { useEffect, useId, useRef, useState } from "react";

type Item = { href: string; label: string };

/** The header's links on narrow screens, behind one button. Escape or a choice closes it. */
export function MobileMenu({ links }: { links: readonly Item[] }) {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      button.current?.focus();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <div className="md:hidden">
      <button
        ref={button}
        type="button"
        aria-expanded={open}
        aria-controls={panelId}
        aria-label={open ? "Close the menu" : "Open the menu"}
        onClick={() => setOpen((value) => !value)}
        className="inline-flex size-11 items-center justify-center rounded-s text-foam"
      >
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
          {open ? <path d="M6 6l12 12M18 6 6 18" /> : <path d="M4 7h16M4 12h16M4 17h16" />}
        </svg>
      </button>
      <nav
        id={panelId}
        aria-label="Main"
        hidden={!open}
        className="absolute inset-x-0 top-16 border-b border-bank bg-riverbed px-[max(1rem,env(safe-area-inset-left))] pb-4"
      >
        <ul className="flex flex-col">
          <li>
            <Link
              href="/download"
              onClick={() => setOpen(false)}
              className="flex min-h-12 items-center border-b border-bank/60 text-body font-semibold text-stream"
            >
              Download for Windows
            </Link>
          </li>
          {links.map((link) => (
            <li key={link.href}>
              <Link
                href={link.href}
                onClick={() => setOpen(false)}
                className="flex min-h-12 items-center border-b border-bank/60 text-body text-foam"
              >
                {link.label}
              </Link>
            </li>
          ))}
        </ul>
      </nav>
    </div>
  );
}
