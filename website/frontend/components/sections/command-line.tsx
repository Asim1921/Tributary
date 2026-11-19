import { Container, Section } from "@/components/ui/container";
import { CopyButton } from "@/components/ui/copy-button";

import { Lede, SectionTitle, Typed } from "./heading";

const SYNTAX = `tributary --add URL [--folder FOLDER] [--name NAME] [--silent] [--later] [--quit-when-done]
tributary --start-queue
tributary --minimized`;

const OPTIONS: [string, string][] = [
  ["--add URL", "Adds a download. Without --silent, you're asked first."],
  ["--folder FOLDER", "Where to save it."],
  ["--name NAME", "What to call it."],
  ["--silent", "Starts with no questions. A video stream saves its best quality."],
  ["--later", "Puts it in the main queue instead of starting it."],
  ["--start-queue", "Starts the main queue."],
  ["--minimized", "Opens in the tray."],
  [
    "--quit-when-done",
    "Closes the app when this download finishes. Needs --add and --silent. The app stays open if the download fails, another one is running, or virus scanning is on.",
  ],
];

/** The real options, for scripts and for people who live in a terminal. */
export function CommandLine() {
  return (
    <Section id="command-line" labelledBy="command-line-title">
      <Container>
        <div className="lg:pl-24">
          <SectionTitle id="command-line-title" className="max-w-[22ch]">
            Drive it from a script
          </SectionTitle>
          <Lede>
            Add downloads, start the queue or open the app in the tray from a terminal, a shortcut or a scheduled
            task. If Tributary is already running, the command goes to the running copy.
          </Lede>
          <div className="mt-10 grid gap-x-12 gap-y-10 lg:grid-cols-12">
            <figure className="overflow-hidden rounded-m border border-bank bg-channel lg:col-span-6 lg:self-start">
              <figcaption className="flex min-h-12 items-center justify-between gap-3 border-b border-bank py-1 pl-4 pr-1.5">
                <span className="text-caption text-silt">Command line</span>
                <CopyButton value={SYNTAX} label="Copy the command line syntax" />
              </figcaption>
              <pre className="whitespace-pre-wrap px-4 py-4 font-code text-[0.88rem] leading-[1.8] text-foam [overflow-wrap:anywhere]">
                <code>{SYNTAX}</code>
              </pre>
            </figure>
            <dl className="border-t border-bank lg:col-span-6">
              {OPTIONS.map(([option, detail]) => (
                <div key={option} className="grid gap-1 border-b border-bank py-3.5 sm:grid-cols-[11.5rem_1fr] sm:gap-4">
                  <dt>
                    <Typed>{option}</Typed>
                  </dt>
                  <dd className="text-silt">{detail}</dd>
                </div>
              ))}
            </dl>
          </div>
        </div>
      </Container>
    </Section>
  );
}
