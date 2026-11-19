"use client";

import { useMotionValueEvent, useTransform, type MotionValue } from "motion/react";
import * as m from "motion/react-m";
import dynamic from "next/dynamic";
import { Component, type ReactNode, useEffect, useRef, useState, useSyncExternalStore } from "react";

import { BLOCK_SPAN, FACES, LANE_LIST, LANE_SPAN, OUTLET, lanePath } from "./arrival-geometry";
import styles from "./arrival.module.css";

// The 3D view is its own chunk, fetched after the page is up, and only where the room shows.
const ArrivalCanvas = dynamic(() => import("./arrival-canvas"), { ssr: false });

const REDUCE = "(prefers-reduced-motion: reduce)";
const WIDE = "(min-width: 64rem)";

function watchQuery(query: string) {
  return (changed: () => void) => {
    const list = window.matchMedia(query);
    list.addEventListener("change", changed);
    return () => list.removeEventListener("change", changed);
  };
}
const subscribeMotion = watchQuery(REDUCE);
const subscribeWidth = watchQuery(WIDE);
const prefersReducedMotion = () => window.matchMedia(REDUCE).matches;
const isWide = () => window.matchMedia(WIDE).matches;

function subscribeVisibility(changed: () => void) {
  document.addEventListener("visibilitychange", changed);
  return () => document.removeEventListener("visibilitychange", changed);
}
const pageIsVisible = () => document.visibilityState === "visible";

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

const PATHS = LANE_LIST.map(lanePath);
const rem = (units: number) => `${units / 16}rem`;

/**
 * Where the river arrives: the channel opens into one lane per connection and the file builds
 * from them into one block above the download button, then rests in a slow glow. `level` is
 * how far the reader's water has come through the outlet; the file starts to arrive when it
 * reaches the room. Wide screens only, and decorative: the button and the facts carry it all.
 */
export function Arrival({ level }: { level: MotionValue<number> }) {
  const room = useRef<HTMLDivElement>(null);
  const wide = useSyncExternalStore(subscribeWidth, isWide, () => false);
  const reduced = useSyncExternalStore(subscribeMotion, prefersReducedMotion, () => false);
  const pageVisible = useSyncExternalStore(subscribeVisibility, pageIsVisible, () => true);
  const [stage, setStage] = useState<Stage>("drawing");
  const [inView, setInView] = useState(false);
  const [arrived, setArrived] = useState(false);

  const lanes = useTransform(level, LANE_SPAN, [0, 1]);
  const lanesShown = useTransform(lanes, (v) => (v > 0.002 ? 1 : 0));
  const block = useTransform(level, BLOCK_SPAN, [0, 1]);
  useMotionValueEvent(level, "change", (v) => {
    if (v > 0.02) setArrived(true);
  });

  // Pause while scrolled out of sight.
  useEffect(() => {
    const element = room.current;
    if (!element) return;
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting), {
      rootMargin: "64px",
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Start the 3D view once the page is idle, where the room shows and WebGL 2 works.
  useEffect(() => {
    if (!wide) return;
    let cancelled = false;
    const start = () => {
      if (!cancelled) setStage((current) => (current === "drawing" ? (supportsWebGL2() ? "loading" : "fallback") : current));
    };
    if (typeof window.requestIdleCallback === "function") {
      const id = window.requestIdleCallback(start, { timeout: 2500 });
      return () => {
        cancelled = true;
        window.cancelIdleCallback(id);
      };
    }
    const id = window.setTimeout(start, 600);
    return () => {
      cancelled = true;
      window.clearTimeout(id);
    };
  }, [wide]);

  const live = stage === "live";
  const drawn = `${styles.fade} ${live ? "opacity-0" : "opacity-100"}`;
  return (
    <div ref={room} aria-hidden="true" className={`${styles.room} hidden select-none lg:block`}>
      <svg
        viewBox={`0 0 ${OUTLET.width} ${OUTLET.height}`}
        width="100%"
        height="100%"
        fill="none"
        overflow="visible"
        focusable="false"
        strokeWidth="1.5"
        strokeLinecap="round"
        className="absolute inset-0"
      >
        {PATHS.map((d) => (
          <path key={d} d={d} className={styles.bed} />
        ))}
        <g className={drawn}>
          {PATHS.map((d) => (
            <m.path key={d} d={d} className={styles.water} style={{ pathLength: lanes, opacity: lanesShown }} />
          ))}
        </g>
      </svg>
      {(["top", "front"] as const).map((face) => (
        <div
          key={face}
          className={`${styles.face} ${styles[face]}`}
          style={{ top: rem(FACES[face].y0), height: rem(FACES[face].y1 - FACES[face].y0) }}
        >
          <m.div className={`${styles.fill} ${drawn}`} style={{ scaleX: block }} />
        </div>
      ))}
      {wide && (stage === "loading" || live) && (
        <div className={`${styles.live} ${styles.fade} ${live ? "opacity-100" : "opacity-0"}`}>
          <KeepDrawing onError={() => setStage("fallback")}>
            <ArrivalCanvas
              animate={live && inView && pageVisible && arrived && !reduced}
              arrived={arrived}
              still={reduced}
              onReady={() => setStage((current) => (current === "loading" ? "live" : current))}
              onLost={() => setStage("fallback")}
            />
          </KeepDrawing>
        </div>
      )}
    </div>
  );
}
