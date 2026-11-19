"use client";

import * as m from "motion/react-m";
import { useRef, type ReactNode } from "react";

import { Arrival } from "@/components/finale/arrival";

import styles from "./channel.module.css";
import { useLevel } from "./use-level";

/**
 * Where the channel arrives: through the section's top padding, into the room above the
 * download button (the children), where it opens into lanes and the file builds into one block
 * resting on the button. On narrow screens only the button shows.
 *
 * The room is 16rem tall (OUTLET in components/finale/arrival-geometry.ts).
 */
export function ChannelOutlet({ children }: { children: ReactNode }) {
  const leadRef = useRef<HTMLDivElement>(null);
  const ref = useRef<HTMLDivElement>(null);
  const lead = useLevel(leadRef);
  const level = useLevel(ref);

  return (
    <div ref={ref} className="relative flex flex-col items-stretch sm:items-start lg:items-stretch lg:pt-[16rem]">
      <div ref={leadRef} aria-hidden="true" className={`${styles.leadIn} hidden lg:block`}>
        <m.div className={styles.fill} style={{ scaleY: lead }} />
      </div>
      <Arrival level={level} />
      {children}
    </div>
  );
}
