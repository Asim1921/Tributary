"use client";

import * as Accordion from "@radix-ui/react-accordion";
import type { ReactNode } from "react";

import { Container, Section } from "@/components/ui/container";
import { site } from "@/lib/site";

import { SectionTitle, Typed } from "./heading";

const link = "text-estuary underline decoration-1 underline-offset-[0.2em] hover:text-foam";

const ITEMS: { id: string; q: string; a: ReactNode }[] = [
  {
    id: "free",
    q: "Is Tributary free?",
    a: <>Yes. There&rsquo;s no price, no account and no advertising, and the source is available on GitHub.</>,
  },
  {
    id: "safe",
    q: "Is the installer safe to run?",
    a: (
      <>
        The installer isn&rsquo;t signed, so Windows SmartScreen warns about it. Check that your download&rsquo;s
        SHA-256 matches the one on this page, then choose <strong>More info</strong> and{" "}
        <strong>Run anyway</strong>. You can read the code it&rsquo;s built from at{" "}
        <a className={link} href={site.repo} target="_blank" rel="noreferrer">
          github.com/Asim1921/Tributary
        </a>
        .
      </>
    ),
  },
  {
    id: "youtube",
    q: "Does it download YouTube videos?",
    a: (
      <>
        Yes, through yt-dlp, a free open-source tool Tributary can install for you and check against its published
        checksum. Saving video may go against a site&rsquo;s terms, and YouTube&rsquo;s forbid it, so that&rsquo;s your
        decision to make. Protected (DRM or encrypted), live, paid and login-only videos are refused, and yt-dlp is
        never given your cookies.
      </>
    ),
  },
  {
    id: "privacy",
    q: "What does it know about me?",
    a: (
      <>
        There&rsquo;s no account and no telemetry. Passwords for servers are kept in Windows Credential Manager, never
        on disk, and cookies from your browser are held in memory only.
      </>
    ),
  },
  {
    id: "browsers",
    q: "Which browsers does it work with?",
    a: (
      <>
        Chrome and Edge, through an extension you add yourself, because browsers don&rsquo;t let other programs
        install extensions. The steps are in the download section, and in full on the download page.
      </>
    ),
  },
  {
    id: "sites",
    q: "Can it save a whole website?",
    a: (
      <>
        Yes. The site grabber reads a site one page at a time and always follows its <Typed>robots.txt</Typed>.
        Choose <strong>Whole site</strong> to keep the pages for offline reading.
      </>
    ),
  },
  {
    id: "update",
    q: "I updated Tributary. Why is a new feature missing?",
    a: (
      <>
        Reload the extension. Open <Typed>chrome://extensions</Typed> (or <Typed>edge://extensions</Typed>) and press
        the circular arrow on Tributary&rsquo;s card. The browser often keeps running in the background and still holds
        the old extension.
      </>
    ),
  },
];

/** Straight answers, one open at a time. */
export function Faq() {
  return (
    <Section id="faq" labelledBy="faq-title" className="border-t border-bank">
      <Container>
        <div className="grid gap-x-12 gap-y-10 lg:grid-cols-12">
          <SectionTitle id="faq-title" className="lg:col-span-4">
            Questions people ask
          </SectionTitle>
          <Accordion.Root type="single" collapsible className="border-t border-bank lg:col-span-8">
            {ITEMS.map((item) => (
              <Accordion.Item key={item.id} value={item.id} className="border-b border-bank">
                <Accordion.Header asChild>
                  <h3>
                    <Accordion.Trigger className="group flex min-h-14 w-full items-center justify-between gap-6 py-4 text-left font-display text-h4 font-[620] tracking-[-0.018em] text-foam hover:text-stream">
                      {item.q}
                      <span
                        aria-hidden="true"
                        className="relative block size-4 shrink-0 before:absolute before:left-0 before:top-1/2 before:h-0.5 before:w-full before:-translate-y-1/2 before:bg-stream after:absolute after:left-1/2 after:top-0 after:h-full after:w-0.5 after:-translate-x-1/2 after:bg-stream after:transition-transform group-data-[state=open]:after:scale-y-0"
                      />
                    </Accordion.Trigger>
                  </h3>
                </Accordion.Header>
                <Accordion.Content className="max-w-[62ch] pb-6 pr-10 text-silt">{item.a}</Accordion.Content>
              </Accordion.Item>
            ))}
          </Accordion.Root>
        </div>
      </Container>
    </Section>
  );
}
