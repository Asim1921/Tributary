import { Container, Section } from "@/components/ui/container";
import { CopyButton } from "@/components/ui/copy-button";

import { Lede, SectionTitle, Typed } from "./heading";
import { getWords } from "@/lib/i18n/server";

const SYNTAX = `tributary --add URL [--folder FOLDER] [--name NAME] [--silent] [--later] [--quit-when-done]
tributary --start-queue
tributary --minimized`;

/** The options, by the words that explain each. */
const OPTIONS = [
  ["--add URL", "add"],
  ["--folder FOLDER", "folder"],
  ["--name NAME", "name"],
  ["--silent", "silent"],
  ["--later", "later"],
  ["--start-queue", "startQueue"],
  ["--minimized", "minimized"],
  ["--quit-when-done", "quitWhenDone"],
] as const;

/** The real options, for scripts and for people who live in a terminal. */
export async function CommandLine() {
  const { w } = await getWords();
  const c = w.commandLine;
  return (
    <Section id="command-line" labelledBy="command-line-title">
      <Container>
        <div className="lg:ps-24">
          <SectionTitle id="command-line-title" className="max-w-[22ch]">
            {c.title}
          </SectionTitle>
          <Lede>{c.lede}</Lede>
          <div className="mt-10 grid gap-x-12 gap-y-10 lg:grid-cols-12">
            <figure className="overflow-hidden rounded-m border border-bank bg-channel lg:col-span-6 lg:self-start">
              <figcaption className="flex min-h-12 items-center justify-between gap-3 border-b border-bank py-1 ps-4 pe-1.5">
                <span className="text-caption text-silt">{c.label}</span>
                <CopyButton value={SYNTAX} label={c.copy} copyText={w.site.copy} copiedLabel={w.site.copied} />
              </figcaption>
              <pre dir="ltr" className="whitespace-pre-wrap px-4 py-4 font-code text-[0.88rem] leading-[1.8] text-foam [overflow-wrap:anywhere]">
                <code>{SYNTAX}</code>
              </pre>
            </figure>
            <dl className="border-t border-bank lg:col-span-6">
              {OPTIONS.map(([option, which]) => (
                <div key={option} className="grid gap-1 border-b border-bank py-3.5 sm:grid-cols-[11.5rem_1fr] sm:gap-4">
                  <dt>
                    <span dir="ltr">
                      <Typed>{option}</Typed>
                    </span>
                  </dt>
                  <dd className="text-silt">{c.options[which]}</dd>
                </div>
              ))}
            </dl>
          </div>
        </div>
      </Container>
    </Section>
  );
}
