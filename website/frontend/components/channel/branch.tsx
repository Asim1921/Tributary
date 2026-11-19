"use client";

import { useTransform } from "motion/react";
import * as m from "motion/react-m";
import { useRef } from "react";

import styles from "./channel.module.css";
import { useLevel } from "./use-level";

// The icon's stream: it leaves its source heading straight down and meets the channel straight
// down, bending between (both control points at 54% of the drop, as in the icon).
const PATH = "M80 0 C80 64 0 64 0 120";

/**
 * A tributary: one feature's branch flowing into the channel, drawn as the reader reaches it.
 * Place it first inside a `relative` block whose left edge is 6rem right of the channel.
 */
export function Branch() {
  const ref = useRef<HTMLDivElement>(null);
  const level = useLevel(ref);
  // A zero-length stroke still draws its round cap, so hide it until water arrives.
  const shown = useTransform(level, (v) => (v > 0.002 ? 1 : 0));
  return (
    <div ref={ref} aria-hidden="true" className={`${styles.branch} hidden lg:block`}>
      <svg
        viewBox="0 0 80 120"
        width="100%"
        height="100%"
        fill="none"
        overflow="visible"
        focusable="false"
        strokeWidth="2"
        strokeLinecap="round"
      >
        <path d={PATH} className={styles.bed} />
        <m.path d={PATH} className={styles.stream} style={{ pathLength: level, opacity: shown }} />
      </svg>
    </div>
  );
}
