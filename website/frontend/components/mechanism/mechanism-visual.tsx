"use client";

import dynamic from "next/dynamic";
import { Component, type ReactNode, useEffect, useMemo, useRef, useState } from "react";

import { ASPECT, fitCamera, labelAnchors, project, type Vec3 } from "./geometry";
import { MechanismStatic } from "./mechanism-static";
import type { Sim, SimFeed } from "./simulation";

// The 3D view is its own chunk, fetched when the reader comes near; the drawing holds its place.
const MechanismCanvas = dynamic(() => import("./mechanism-canvas"), { ssr: false });

function supportsWebGL2() {
  try {
    const gl = document.createElement("canvas").getContext("webgl2");
    gl?.getExtension("WEBGL_lose_context")?.loseContext();
    return gl !== null;
  } catch {
    return false;
  }
}

/** If the 3D view throws, it quietly leaves the drawing in place. */
class KeepDrawing extends Component<{ onError: () => void; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  componentDidCatch() {
    this.props.onError();
  }

  render() {
    return this.state.failed ? null : this.props.children;
  }
}

type Stage = "drawing" | "loading" | "live" | "fallback";

const FADE = "transition-opacity duration-700 ease-flow";

type Props = {
  /** The two ends of the picture, in the page's language. */
  labels: { server: string; computer: string };
  sim: Sim;
  feed: SimFeed;
  /** The simulation is playing and in sight: keep the water moving. */
  animate: boolean;
  /** Reduced motion: each state as one still picture. */
  still: boolean;
};

/**
 * The picture of the simulation: the server above, the file below, a stream of its pieces for
 * each connection. Decorative: the bar, the status line and the buttons under it carry
 * everything it shows. A drawing first; the live view fades in over it.
 */
export function MechanismVisual({ sim, feed, animate, still, labels: words }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const [stage, setStage] = useState<Stage>("drawing");
  const [aspect, setAspect] = useState<number>(ASPECT.wide);
  const [near, setNear] = useState(false);
  const [settled, setSettled] = useState(false);

  // The box's shape decides the view.
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      if (width > 0 && height > 0) setAspect(Math.round((width / height) * 100) / 100);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Fetch the 3D code only when the reader comes near the section.
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        setNear(true);
        observer.disconnect();
      },
      { rootMargin: "600px 0px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Then start it once the page is idle, and only where WebGL 2 works.
  useEffect(() => {
    if (!near) return;
    let cancelled = false;
    const start = () => {
      if (!cancelled) setStage(supportsWebGL2() ? "loading" : "fallback");
    };
    if (typeof window.requestIdleCallback === "function") {
      const id = window.requestIdleCallback(start, { timeout: 1200 });
      return () => {
        cancelled = true;
        window.cancelIdleCallback(id);
      };
    }
    const id = window.setTimeout(start, 200);
    return () => {
      cancelled = true;
      window.clearTimeout(id);
    };
  }, [near]);

  // The drawing stays under the live view while it fades in, then goes.
  useEffect(() => {
    if (stage !== "live") return;
    const id = window.setTimeout(() => setSettled(true), 800);
    return () => window.clearTimeout(id);
  }, [stage]);

  const live = stage === "live";

  // The same element while nothing it shows has changed, so the simulation's steps never
  // re-render the canvas; the canvas reads each step from the feed.
  const canvas = useMemo(
    () =>
      stage === "loading" || stage === "live" ? (
        <KeepDrawing onError={() => setStage("fallback")}>
          <MechanismCanvas
            feed={feed}
            animate={stage === "live" && animate && !still}
            still={still}
            onReady={() => setStage((current) => (current === "loading" ? "live" : current))}
            onLost={() => setStage("fallback")}
            className={`${FADE} ${stage === "live" ? "opacity-100" : "opacity-0"}`}
          />
        </KeepDrawing>
      ) : null,
    [stage, feed, animate, still],
  );

  // Where the two labels sit, as shares of the box, seen from the same camera.
  const labels = useMemo(() => {
    const fit = fitCamera(aspect);
    const height = 100 / aspect;
    const place = (anchor: Vec3) => {
      const [x, y] = project(fit, anchor, 100, height);
      return { left: `${x}%`, top: `${(y / height) * 100}%` };
    };
    const anchors = labelAnchors(aspect);
    return { server: place(anchors.server), file: place(anchors.file) };
  }, [aspect]);

  return (
    <div
      ref={box}
      aria-hidden="true"
      className="relative aspect-[6/5] w-full select-none sm:aspect-[2/1] lg:aspect-[12/5]"
    >
      {!(live && settled) && (
        <MechanismStatic
          sim={sim}
          aspect={aspect}
          className={`absolute inset-0 size-full ${FADE} ${live ? "opacity-0" : "opacity-100"}`}
        />
      )}
      {canvas}
      <span
        className="pointer-events-none absolute -translate-y-[calc(100%+0.5rem)] whitespace-nowrap text-caption text-silt"
        style={labels.server}
      >
        {words.server}
      </span>
      <span
        className="pointer-events-none absolute -translate-x-full translate-y-2 whitespace-nowrap text-caption text-silt"
        style={labels.file}
      >
        {words.computer}
      </span>
    </div>
  );
}
