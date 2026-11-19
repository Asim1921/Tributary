"use client";

import * as m from "motion/react-m";
import { useRef } from "react";

import styles from "./channel.module.css";
import { useLevel } from "./use-level";

/** The main channel through its stretch of page, filling as the reader goes down. */
export function ChannelLine() {
  const ref = useRef<HTMLDivElement>(null);
  const level = useLevel(ref);
  return (
    <div ref={ref} aria-hidden="true" className={`${styles.line} hidden lg:block`}>
      <m.div className={styles.fill} style={{ scaleY: level }} />
    </div>
  );
}
