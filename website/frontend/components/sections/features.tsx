import Image from "next/image";
import type { ReactNode } from "react";

import { Branch } from "@/components/channel/branch";
import { DepthFrame } from "@/components/finale/depth-frame";
import { Container, Section } from "@/components/ui/container";
import { Rich } from "@/lib/i18n/rich";
import { getWords } from "@/lib/i18n/server";
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
  return <p className={`border-s-2 border-bank ps-4 text-silt ${className}`}>{children}</p>;
}

/** A real still from the demo, in a frame that answers the pointer. */
function Still({ src, width, height, alt, caption }: { src: string; width: number; height: number; alt: string; caption: string }) {
  return (
    <figure>
      <DepthFrame>
        <Image src={src} width={width} height={height} alt={alt} className="block h-auto w-full" />
      </DepthFrame>
      <figcaption className="mt-3.5 text-caption text-silt">{caption}</figcaption>
    </figure>
  );
}

const body = "mt-4 max-w-[62ch] text-foam";
const keys = (inner: string) => <Key>{inner}</Key>;

/** The six things that feed downloads into Tributary, each joining the channel. */
export async function Features() {
  const { w } = await getWords();
  const f = w.features;
  return (
    <Section id="features" labelledBy="features-title">
      <Container>
        <div className="lg:ps-24">
          <SectionTitle id="features-title" className="max-w-[22ch]">
            {f.title}
          </SectionTitle>
          <Lede>{f.lede}</Lede>

          <div className="mt-20 space-y-24 lg:mt-28 lg:space-y-36">
            {/* Text with the real dialog, wide. */}
            <div className="grid gap-x-10 gap-y-10 lg:grid-cols-12">
              <Tributary title={f.takeoverTitle} className="lg:col-span-5">
                <p className={body}>{f.takeover}</p>
                <ul className="mt-5 max-w-[62ch] list-disc space-y-2.5 ps-5 text-foam marker:text-silt">
                  <li>
                    <Rich text={f.takeoverKeys} keyboard={keys} />
                  </li>
                  <li>{f.takeoverMenu}</li>
                  <li>{f.takeoverClipboard}</li>
                </ul>
              </Tributary>
              <div className="lg:col-span-7 lg:pt-2">
                <Still
                  src="/media/still-takeover.webp"
                  width={815}
                  height={624}
                  alt={f.takeoverAlt}
                  caption={f.takeoverCaption}
                />
              </div>
            </div>

            {/* Text and the terms note, beside the quality dialog. */}
            <div className="grid gap-x-10 gap-y-10 lg:grid-cols-12">
              <Tributary title={f.videoTitle} className="lg:col-span-6">
                <p className={body}>{f.video}</p>
                <p className={body}>{f.videoScript}</p>
                <Note className="mt-6 max-w-[58ch]">{f.videoNote}</Note>
              </Tributary>
              <div className="lg:col-span-5 lg:col-start-8 lg:pt-2">
                <Still
                  src="/media/still-quality.webp"
                  width={815}
                  height={806}
                  alt={f.videoAlt}
                  caption={f.videoCaption}
                />
              </div>
            </div>

            {/* Text with its ownership note set in the margin. */}
            <div className="grid gap-x-10 gap-y-6 lg:grid-cols-12">
              <Tributary title={f.picturesTitle} className="lg:col-span-7">
                <p className={body}>
                  <Rich text={f.pictures} />
                </p>
                <p className={body}>{f.picturesKeep}</p>
              </Tributary>
              <Note className="self-end lg:col-span-4 lg:col-start-9">{f.picturesNote}</Note>
            </div>

            {/* Text alone. */}
            <Tributary title={f.grabberTitle} className="max-w-[62ch]">
              <p className={body}>
                <Rich text={f.grabber} />
              </p>
              <p className={body}>{f.grabberProjects}</p>
            </Tributary>

            {/* A definition list, across the column. */}
            <Tributary title={f.queuesTitle}>
              <dl className="mt-8 grid gap-x-12 border-t border-bank sm:grid-cols-2">
                {f.queues.map(([term, detail]) => (
                  <div key={term} className="grid gap-1 border-b border-bank py-4 sm:grid-cols-[11rem_1fr] sm:gap-4">
                    <dt className="font-semibold text-foam">{term}</dt>
                    <dd className="text-silt">{detail}</dd>
                  </div>
                ))}
              </dl>
            </Tributary>

            {/* A closing statement. */}
            <Tributary title={f.loginsTitle} className="max-w-[44rem]">
              <p className="mt-4 text-lead text-foam">{f.logins}</p>
              <p className="mt-4 max-w-[62ch] text-silt">{f.loginsMore}</p>
            </Tributary>
          </div>
        </div>
      </Container>
    </Section>
  );
}
