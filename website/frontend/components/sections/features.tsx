import Image from "next/image";
import type { ReactNode } from "react";

import { Branch } from "@/components/channel/branch";
import { Container, Section } from "@/components/ui/container";

import { BlockTitle, Key, Lede, SectionTitle } from "./heading";

/** One tributary: a feature whose heading sends a branch into the channel on wide screens. */
function Tributary({ title, children, className = "" }: { title: string; children: ReactNode; className?: string }) {
  return (
    <div className={`relative ${className}`}>
      <Branch />
      <BlockTitle className="max-w-[24ch]">{title}</BlockTitle>
      {children}
    </div>
  );
}

/** A note the reader should weigh before using a feature: terms, ownership, limits. */
function Note({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <p className={`border-l-2 border-bank pl-4 text-silt ${className}`}>{children}</p>;
}

function Still({ src, width, height, alt, caption }: { src: string; width: number; height: number; alt: string; caption: string }) {
  return (
    <figure>
      <div className="overflow-hidden rounded-l border border-bank">
        <Image src={src} width={width} height={height} alt={alt} className="block h-auto w-full" />
      </div>
      <figcaption className="mt-3 text-caption text-silt">{caption}</figcaption>
    </figure>
  );
}

const body = "mt-4 max-w-[62ch] text-foam";

/** The six things that feed downloads into Tributary, each joining the channel. */
export function Features() {
  return (
    <Section id="features" labelledBy="features-title">
      <Container>
        <div className="lg:pl-24">
          <SectionTitle id="features-title" className="max-w-[22ch]">
            Downloads from wherever you find them
          </SectionTitle>
          <Lede>
            From the browser, from the video you&rsquo;re watching, from a page of photos or a whole website. It all
            joins one list, and runs when and as fast as you say.
          </Lede>

          <div className="mt-20 space-y-24 lg:mt-28 lg:space-y-36">
            {/* Text with the real dialog, wide. */}
            <div className="grid gap-x-10 gap-y-10 lg:grid-cols-12">
              <Tributary title="Takes over from Chrome and Edge" className="lg:col-span-5">
                <p className={body}>
                  Start a download in Chrome or Edge and Tributary takes it, for the file types you choose. Its
                  extension works in both browsers.
                </p>
                <ul className="mt-5 max-w-[62ch] list-disc space-y-2.5 pl-5 text-foam marker:text-silt">
                  <li>
                    Hold <Key>Ctrl</Key> as you click to send a download to Tributary, or <Key>Alt</Key> to leave it
                    with the browser. Both keys can be changed.
                  </li>
                  <li>Right-click a link, a selection, or a page to send all of its links.</li>
                  <li>It can watch the clipboard for links you copy, and takes links you drag onto its window.</li>
                </ul>
              </Tributary>
              <div className="lg:col-span-7 lg:pt-2">
                <Still
                  src="/media/still-takeover.webp"
                  width={815}
                  height={624}
                  alt="Tributary's Download file dialog for node-v22.11.0.tar.gz from nodejs.org: 92.4 MB, resume supported, with a name, folder, category and queue to choose, and Cancel, Download later and Start download buttons."
                  caption="A download clicked in Chrome, taken over by Tributary."
                />
              </div>
            </div>

            {/* Text and the terms note, beside the quality dialog. */}
            <div className="grid gap-x-10 gap-y-10 lg:grid-cols-12">
              <Tributary title="Saves video from the page you watch" className="lg:col-span-6">
                <p className={body}>
                  A button appears beside video players. Tributary saves HLS and DASH streams itself and joins
                  picture and sound into one file with ffmpeg, in qualities up to 4K at 60 fps where the site offers
                  them.
                </p>
                <p className={body}>
                  YouTube and sites like it feed their player by script, so there&rsquo;s no file to see. For those,
                  Tributary hands the page to yt-dlp, a free open-source tool it can install for you, checked against
                  its published checksum.
                </p>
                <Note className="mt-6 max-w-[58ch]">
                  Saving video may go against a site&rsquo;s terms, and YouTube&rsquo;s forbid it, so it&rsquo;s your
                  decision to make. Protected (DRM or encrypted), live, paid and login-only videos are refused, and
                  yt-dlp is never given your cookies.
                </Note>
              </Tributary>
              <div className="lg:col-span-5 lg:col-start-8 lg:pt-2">
                <Still
                  src="/media/still-quality.webp"
                  width={815}
                  height={806}
                  alt="Tributary's Save video dialog for a YouTube video, listing qualities from 2160p60 at about 1.3 GB down to 144p, with the note that saving video may go against a site's terms and that protected, paid and login-only videos are refused."
                  caption="Choosing a quality. The dialog says the same about terms."
                />
              </div>
            </div>

            {/* Text with its ownership note set in the margin. */}
            <div className="grid gap-x-10 gap-y-6 lg:grid-cols-12">
              <Tributary title="Saves every picture on a page" className="lg:col-span-7">
                <p className={body}>
                  On a page full of photos or posts, choose <strong>Save all images and videos on this page</strong>.
                  Tributary scrolls the page as you would, takes each picture at the largest size the page offers,
                  and stops at the end or at a login wall.
                </p>
                <p className={body}>
                  Then you choose what to keep, and it saves them through a queue, four at a time, into a folder
                  named after the page.
                </p>
              </Tributary>
              <Note className="self-end lg:col-span-4 lg:col-start-9">
                The pictures belong to the people who posted them, and some sites&rsquo; terms, Instagram&rsquo;s and
                Facebook&rsquo;s among them, forbid collecting them automatically. Tributary never gets around a login
                wall, a private account or a site&rsquo;s limits.
              </Note>
            </div>

            {/* Text alone. */}
            <Tributary title="Explores a site for you" className="max-w-[62ch]">
              <p className={body}>
                The site grabber reads a website one page at a time, always following its robots.txt, and gathers the
                pages and files you ask for. Choose <strong>Whole site</strong> to keep it for reading offline, with its
                links pointing at the saved copies.
              </p>
              <p className={body}>
                A project can be stopped and carried on later, scheduled, or repeated to save whatever is new while the
                app is open.
              </p>
            </Tributary>

            {/* A definition list, across the column. */}
            <Tributary title="Queues, schedules, limits">
              <dl className="mt-8 grid gap-x-12 border-t border-bank sm:grid-cols-2">
                {[
                  ["Queues", "Several of them, each running as many files at once as you choose."],
                  ["Schedules", "Start and stop times, and the computer woken for a scheduled start."],
                  ["When a queue finishes", "Shut the computer down, or run a program."],
                  ["Speed limit", "A ceiling on how fast Tributary downloads."],
                  ["Quotas", "So much per period. Downloads pause when it's used up and carry on by themselves."],
                  ["Categories", "The list sorted by kind: compressed, documents, music, programs and video."],
                ].map(([term, detail]) => (
                  <div key={term} className="grid gap-1 border-b border-bank py-4 sm:grid-cols-[11rem_1fr] sm:gap-4">
                    <dt className="font-semibold text-foam">{term}</dt>
                    <dd className="text-silt">{detail}</dd>
                  </div>
                ))}
              </dl>
            </Tributary>

            {/* A closing statement. */}
            <Tributary title="Keeps your logins off the disk" className="max-w-[44rem]">
              <p className="mt-4 text-lead text-foam">
                Tributary downloads over HTTP, HTTPS and FTP, signs in to servers that ask, and works through a
                proxy. The passwords are kept in Windows Credential Manager, never in a file, and cookies from your
                browser are held in memory only.
              </p>
              <p className="mt-4 max-w-[62ch] text-silt">
                So after a restart, a download taken from the browser may need its link again. There&rsquo;s no
                account and no telemetry, and if you like, each finished download is scanned for viruses.
              </p>
            </Tributary>
          </div>
        </div>
      </Container>
    </Section>
  );
}
