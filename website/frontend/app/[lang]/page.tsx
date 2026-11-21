import { ChannelRun } from "@/components/channel/channel-run";
import { Hero } from "@/components/hero/hero";
import { CommandLine } from "@/components/sections/command-line";
import { DownloadPanel } from "@/components/sections/download-panel";
import { Faq } from "@/components/sections/faq";
import { Features } from "@/components/sections/features";
import { Footage } from "@/components/sections/footage";
import { HowItWorks } from "@/components/sections/how-it-works";
import { ProofLine } from "@/components/sections/proof-line";
import { getLatestRelease } from "@/lib/release";
import { getWords } from "@/lib/i18n/server";

// Rendered per request (nginx caches it for 10 s), so the live size and checksum are there
// from the first visit after a deploy; the image is built with no API to ask.
export const dynamic = "force-dynamic";


export default async function Home() {
  const release = await getLatestRelease();
  const { w } = await getWords();
  return (
    <>
      <Hero release={release} />
      <ProofLine />
      <HowItWorks />
      <ChannelRun>
        <Features />
        <Footage />
        <CommandLine />
      </ChannelRun>
      <DownloadPanel release={release} />
      <Faq title={w.faq.title} items={w.faq.items} />
    </>
  );
}
