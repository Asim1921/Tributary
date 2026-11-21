"use client";

import * as Accordion from "@radix-ui/react-accordion";

import { Container, Section } from "@/components/ui/container";

import { Rich } from "@/lib/i18n/rich";
import type { Words } from "@/lib/i18n/words";
import { SectionTitle, Typed } from "./heading";


/** Straight answers, one open at a time. */
export function Faq({ title, items }: { title: string; items: Words["faq"]["items"] }) {
  return (
    <Section id="faq" labelledBy="faq-title" className="border-t border-bank">
      <Container>
        <div className="grid gap-x-12 gap-y-10 lg:grid-cols-12">
          <SectionTitle id="faq-title" className="lg:col-span-4">
            {title}
          </SectionTitle>
          <Accordion.Root type="single" collapsible className="border-t border-bank lg:col-span-8">
            {items.map((item) => (
              <Accordion.Item key={item.id} value={item.id} className="border-b border-bank">
                <Accordion.Header asChild>
                  <h3>
                    <Accordion.Trigger className="group flex min-h-14 w-full items-center justify-between gap-6 py-4 text-start font-display text-h4 font-[620] tracking-[-0.018em] text-foam hover:text-stream">
                      {item.q}
                      <span
                        aria-hidden="true"
                        className="relative block size-4 shrink-0 before:absolute before:left-0 before:top-1/2 before:h-0.5 before:w-full before:-translate-y-1/2 before:bg-stream after:absolute after:left-1/2 after:top-0 after:h-full after:w-0.5 after:-translate-x-1/2 after:bg-stream after:transition-transform group-data-[state=open]:after:scale-y-0"
                      />
                    </Accordion.Trigger>
                  </h3>
                </Accordion.Header>
                <Accordion.Content className="max-w-[62ch] pb-6 pe-10 text-silt">
                  <Rich text={item.a} code={(inner) => <Typed>{inner}</Typed>} />
                </Accordion.Content>
              </Accordion.Item>
            ))}
          </Accordion.Root>
        </div>
      </Container>
    </Section>
  );
}
