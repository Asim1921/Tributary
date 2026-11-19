"use client";

import dynamic from "next/dynamic";
import { Component, type ReactNode, useEffect, useRef, useState, useSyncExternalStore } from "react";

import { ConfluenceStatic } from "./confluence-static";

// The 3D view is its own chunk, fetched after the page is up; the drawing holds its place.
const ConfluenceCanvas = dynamic(() => import("./confluence-canvas"), { ssr: false });

const REDUCE = "(prefers-reduced-motion: reduce)";

function subscribeMotion(changed: () => void) {
  const query = window.matchMedia(REDUCE);
  query.addEventListener("change", changed);
  return () => query.removeEventListener("change", changed);
}
const prefersReducedMotion = () => window.matchMedia(REDUCE).matches;

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

/**
 * The hero's confluence in a fixed 4:5 box, so nothing moves while it loads. Decorative: the
 * hero's words never depend on it. The line drawing shows first; the live view fades in over it.
 */
export function ConfluenceVisual() {
  const box = useRef<HTMLDivElement>(null);
  const reduced = useSyncExternalStore(subscribeMotion, prefersReducedMotion, () => false);
  const pageVisible = useSyncExternalStore(subscribeVisibility, pageIsVisible, () => true);
  const [stage, setStage] = useState<Stage>("drawing");
  const [inView, setInView] = useState(true);

  // Pause while scrolled out of sight.
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting), {
      rootMargin: "64px",
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  // Start the 3D view once the page is idle, and only where WebGL 2 works.
  useEffect(() => {
    let cancelled = false;
    const start = () => {
      if (!cancelled) setStage(supportsWebGL2() ? "loading" : "fallback");
    };
    if (typeof window.requestIdleCallback === "function") {
      const id = window.requestIdleCallback(start, { timeout: 1500 });
      return () => {
        cancelled = true;
        window.cancelIdleCallback(id);
      };
    }
    const id = window.setTimeout(start, 300);
    return () => {
      cancelled = true;
      window.clearTimeout(id);
    };
  }, []);

  const live = stage === "live";
  const fade = "transition-opacity duration-700 ease-flow";
  return (
    <div ref={box} aria-hidden="true" className="relative aspect-[4/5] w-full select-none">
      <ConfluenceStatic className={`absolute inset-0 size-full ${fade} ${live ? "opacity-0" : "opacity-100"}`} />
      {(stage === "loading" || live) && (
        <KeepDrawing onError={() => setStage("fallback")}>
          <ConfluenceCanvas
            animate={live && inView && pageVisible && !reduced}
            still={reduced}
            onReady={() => setStage((current) => (current === "loading" ? "live" : current))}
            onLost={() => setStage("fallback")}
            className={`${fade} ${live ? "opacity-100" : "opacity-0"}`}
          />
        </KeepDrawing>
      )}
    </div>
  );
}
