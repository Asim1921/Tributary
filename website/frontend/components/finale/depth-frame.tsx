"use client";

import { useMotionTemplate, useMotionValue, useSpring, useTransform } from "motion/react";
import * as m from "motion/react-m";
import { type PointerEvent, type ReactNode, useSyncExternalStore } from "react";

import styles from "./depth-frame.module.css";

const SPRING = { stiffness: 150, damping: 21, mass: 0.7 };

const REDUCE = "(prefers-reduced-motion: reduce)";
function subscribeMotion(changed: () => void) {
  const query = window.matchMedia(REDUCE);
  query.addEventListener("change", changed);
  return () => query.removeEventListener("change", changed);
}
const prefersReducedMotion = () => window.matchMedia(REDUCE).matches;

/**
 * Frames a still or the demo video. Under a mouse or pen it turns a little towards the pointer,
 * by `tilt` degrees at most, and a soft light follows it across the picture. A playing video is
 * left alone, so watching it stays steady. Touch and reduced motion get the frame alone.
 */
export function DepthFrame({ children, tilt = 3, className = "" }: { children: ReactNode; tilt?: number; className?: string }) {
  // Read after hydration, so the server's markup and the first client render agree.
  const reduce = useSyncExternalStore(subscribeMotion, prefersReducedMotion, () => false);
  const x = useMotionValue(0.5);
  const y = useMotionValue(0.5);
  const over = useMotionValue(0);
  const sx = useSpring(x, SPRING);
  const sy = useSpring(y, SPRING);
  const lit = useSpring(over, SPRING);
  const rotateY = useTransform(() => (sx.get() - 0.5) * 2 * tilt * lit.get());
  const rotateX = useTransform(() => (0.5 - sy.get()) * 2 * tilt * lit.get());
  const lightX = useTransform(sx, (v) => `${(v * 100).toFixed(2)}%`);
  const lightY = useTransform(sy, (v) => `${(v * 100).toFixed(2)}%`);
  const light = useMotionTemplate`radial-gradient(36rem circle at ${lightX} ${lightY}, var(--sheen), transparent 60%)`;

  const move = (event: PointerEvent<HTMLDivElement>) => {
    if (reduce || event.pointerType === "touch") return;
    const video = event.currentTarget.querySelector("video");
    if (video && !video.paused) {
      over.set(0);
      return;
    }
    const box = event.currentTarget.getBoundingClientRect();
    x.set((event.clientX - box.left) / box.width);
    y.set((event.clientY - box.top) / box.height);
    over.set(1);
  };
  const leave = () => over.set(0);

  return (
    <div className={`${styles.stage} ${className}`} onPointerMove={move} onPointerLeave={leave} onPlayCapture={leave}>
      <m.div className={styles.frame} style={{ rotateX, rotateY }}>
        <div className={styles.picture}>
          {children}
          <m.div aria-hidden="true" className={styles.light} style={{ background: light, opacity: lit }} />
        </div>
      </m.div>
    </div>
  );
}
