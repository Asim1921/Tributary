import Image from "next/image";

import { Container, Section } from "@/components/ui/container";

import { DownloadSimulation } from "./download-simulation";
import { Lede, SectionTitle } from "./heading";

/** What the engine does with a download, told plainly, then shown in a simulation to disturb. */
export function HowItWorks() {
  return (
    <Section id="how-it-works" labelledBy="how-it-works-title" className="border-y border-bank bg-channel">
      <Container>
        <div className="grid gap-x-6 gap-y-12 lg:grid-cols-12">
          <div className="lg:col-span-6">
            <SectionTitle id="how-it-works-title">Built for connections that drop</SectionTitle>
            <Lede>
              Tributary asks the server for a file in up to eight parts and fetches them at once. A connection that
              finishes early takes over half of the largest part still going, so one slow connection can&rsquo;t
              hold up the rest.
            </Lede>
            <p className="mt-4 max-w-[62ch] text-lead text-silt">
              If a connection drops, or the computer crashes or shuts down, it carries on from where it stopped.
              If the file changes on the server mid-download, Tributary notices and starts again, rather than join
              parts of two different files.
            </p>
          </div>
          <figure className="self-end lg:col-span-5 lg:col-start-8">
            <div className="overflow-hidden rounded-m border border-bank">
              <Image
                src="/media/still-connections.webp"
                width={775}
                height={316}
                className="block h-auto w-full"
                alt="Tributary's download window: downloading, 58.6%, 762 MB of 1.3 GB at 3.2 MB/s, 2 minutes 47 seconds left, resume supported, and a Connections bar of eight filled parts with gaps between them."
              />
            </div>
            <figcaption className="mt-3 text-caption text-silt">
              The real thing: a 1.3 GB video over eight connections, in Tributary&rsquo;s download window.
            </figcaption>
          </figure>
        </div>
        <div className="mt-14 lg:mt-16">
          <DownloadSimulation />
        </div>
      </Container>
    </Section>
  );
}
