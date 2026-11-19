"use client";

import { useScroll, type MotionValue } from "motion/react";
import type { RefObject } from "react";

/**
 * How far the water has come through an element: 0 until its top reaches the reading line
 * (60% of the way down the screen), 1 once its bottom has passed it. Every piece of the
 * channel uses the same line, so separate pieces fill as one continuous stream.
 */
export function useLevel(target: RefObject<HTMLElement | null>): MotionValue<number> {
  return useScroll({ target, offset: ["start 60%", "end 60%"] }).scrollYProgress;
}
