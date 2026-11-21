import Image from "next/image";

import { Container, Section } from "@/components/ui/container";

import { DownloadSimulation } from "./download-simulation";
import { Lede, SectionTitle } from "./heading";
import { getWords } from "@/lib/i18n/server";

/** What the engine does with a download, told plainly, then shown in a simulation to disturb. */
export async function HowItWorks() {
  const { w } = await getWords();
  return (
    <Section id="how-it-works" labelledBy="how-it-works-title" className="border-y border-bank bg-channel">
      <Container>
        <div className="grid gap-x-6 gap-y-12 lg:grid-cols-12">
          <div className="lg:col-span-6">
            <SectionTitle id="how-it-works-title">{w.how.title}</SectionTitle>
            <Lede>{w.how.lede}</Lede>
            <p className="mt-4 max-w-[62ch] text-lead text-silt">{w.how.more}</p>
          </div>
          <figure className="self-end lg:col-span-5 lg:col-start-8">
            <div className="overflow-hidden rounded-m border border-bank">
              <Image
                src="/media/still-connections.webp"
                width={775}
                height={316}
                className="block h-auto w-full"
                alt={w.how.alt}
              />
            </div>
            <figcaption className="mt-3 text-caption text-silt">{w.how.caption}</figcaption>
          </figure>
        </div>
        <div className="mt-14 lg:mt-16">
          <DownloadSimulation words={w.sim} />
        </div>
      </Container>
    </Section>
  );
}
