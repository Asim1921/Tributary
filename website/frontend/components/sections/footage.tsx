import { DepthFrame } from "@/components/finale/depth-frame";
import { Container, Section } from "@/components/ui/container";

import { Lede, SectionTitle } from "./heading";
import { getWords } from "@/lib/i18n/server";

/** The author's own demo, as it is: real footage with controls, never autoplaying. */
export async function Footage() {
  const { w } = await getWords();
  return (
    <Section id="footage" labelledBy="footage-title">
      <Container>
        <div className="lg:ps-24">
          <SectionTitle id="footage-title" className="max-w-[22ch]">
            {w.footage.title}
          </SectionTitle>
          <Lede>{w.footage.lede}</Lede>
          <figure className="mt-10 max-w-[64rem]">
            <DepthFrame tilt={1.4}>
              <video
                controls
                muted
                playsInline
                preload="none"
                poster="/media/tributary-demo-poster.webp"
                width={1280}
                height={720}
                className="block h-auto w-full bg-riverbed"
                aria-describedby="footage-caption"
              >
                <source src="/media/tributary-demo.mp4" type="video/mp4" />
                {w.footage.fallback}
              </video>
            </DepthFrame>
            <figcaption id="footage-caption" className="mt-3.5 max-w-[62ch] text-caption text-silt">
              {w.footage.caption}
            </figcaption>
          </figure>
        </div>
      </Container>
    </Section>
  );
}
