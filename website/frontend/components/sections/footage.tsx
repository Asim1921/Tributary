import { DepthFrame } from "@/components/finale/depth-frame";
import { Container, Section } from "@/components/ui/container";

import { Lede, SectionTitle } from "./heading";

/** The author's own demo, as it is: real footage with controls, never autoplaying. */
export function Footage() {
  return (
    <Section id="footage" labelledBy="footage-title">
      <Container>
        <div className="lg:pl-24">
          <SectionTitle id="footage-title" className="max-w-[22ch]">
            See it work on a real download
          </SectionTitle>
          <Lede>
            Sixty-four seconds, recorded in the app: a download taken over from Chrome, the connections filling, a
            video quality chosen, and the finished list.
          </Lede>
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
                Your browser can&rsquo;t play this video. It shows Tributary taking over a download from Chrome and
                saving a video.
              </video>
            </DepthFrame>
            <figcaption id="footage-caption" className="mt-3.5 max-w-[62ch] text-caption text-silt">
              The demo is silent. It saves Big Buck Bunny, a short film that is free to copy. Big Buck Bunny (c)
              Blender Foundation, CC BY 3.0.
            </figcaption>
          </figure>
        </div>
      </Container>
    </Section>
  );
}
