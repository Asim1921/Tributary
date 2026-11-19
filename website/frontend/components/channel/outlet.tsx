"use client";

import { motion, useTransform } from "motion/react";
import { useRef, type ReactNode } from "react";

import styles from "./channel.module.css";
import { useLevel } from "./use-level";

// Geometry in px at a 16px root (all of it scales with rem). The channel arrives at x = 0 and
// bends, in the icon's curve, to the middle of this 20rem column, then ends in the icon's arrow.
const BEND = "M0 0 C0 107 160 107 160 200 L160 234";
const ARROW = "M134 210 L160 238 L186 210";
// Where each piece sits in the column's height (bend 0-236, arrow 210-238, button 252-304,
// base bar 318-322 of 322), so the water reaches each as the reading line does.
const BEND_SPAN = [0, 0.735];
const ARROW_SPAN = [0.65, 0.745];
const TRAY_SPAN = [0.8, 1];

/**
 * Where the channel arrives: it bends into the icon's arrow, above the download button (the
 * children), with the icon's base bar below. On narrow screens only the button shows.
 */
export function ChannelOutlet({ children }: { children: ReactNode }) {
  const leadRef = useRef<HTMLDivElement>(null);
  const ref = useRef<HTMLDivElement>(null);
  const lead = useLevel(leadRef);
  const level = useLevel(ref);
  const bend = useTransform(level, BEND_SPAN, [0, 1]);
  const arrow = useTransform(level, ARROW_SPAN, [0, 1]);
  const tray = useTransform(level, TRAY_SPAN, [0, 1]);
  const bendShown = useTransform(bend, (v) => (v > 0.002 ? 1 : 0));
  const arrowShown = useTransform(arrow, (v) => (v > 0.002 ? 1 : 0));

  return (
    <div ref={ref} className="relative flex flex-col items-stretch sm:items-start lg:items-center lg:pt-[15.75rem]">
      <div ref={leadRef} aria-hidden="true" className={`${styles.leadIn} hidden lg:block`}>
        <motion.div className={styles.fill} style={{ scaleY: lead }} />
      </div>
      <div aria-hidden="true" className={`${styles.outlet} hidden lg:block`}>
        <svg
          viewBox="0 0 320 240"
          width="100%"
          height="100%"
          fill="none"
          overflow="visible"
          focusable="false"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d={BEND} className={styles.bed} />
          <path d={ARROW} className={styles.bed} />
          <motion.path d={BEND} className={styles.water} style={{ pathLength: bend, opacity: bendShown }} />
          <motion.path d={ARROW} className={styles.water} style={{ pathLength: arrow, opacity: arrowShown }} />
        </svg>
      </div>
      {children}
      <div aria-hidden="true" className={`${styles.tray} hidden lg:block`}>
        <motion.div className={styles.trayFill} style={{ scaleX: tray }} />
      </div>
    </div>
  );
}
