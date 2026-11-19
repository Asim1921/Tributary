"use client";

import { useEffect, useReducer, useRef, useState, useSyncExternalStore } from "react";

import { MechanismVisual } from "@/components/mechanism/mechanism-visual";
import {
  NAME,
  PACE,
  READY,
  SIZE,
  UNDER_WAY,
  createFeed,
  initialSim,
  publish,
  reduce,
} from "@/components/mechanism/simulation";
import { CheckIcon } from "@/components/ui/icons";

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

/** How much of the panel must be in sight before the simulation starts by itself. */
const AUTOPLAY_AT = 0.35;

const button =
  "inline-flex min-h-11 items-center justify-center gap-2 rounded-s border px-4 text-[0.95rem] font-semibold " +
  "transition-[background-color,border-color,color] duration-[160ms] ease-flow " +
  "disabled:cursor-not-allowed disabled:opacity-45";
const primary = `${button} border-stream bg-stream text-on-current hover:bg-[color-mix(in_oklab,var(--current)_88%,var(--foam))]`;
const secondary = `${button} border-bank bg-transparent text-foam enabled:hover:border-stream enabled:hover:text-stream`;

/**
 * A download in Tributary, which the reader can disturb: a picture of the connections at work,
 * and under it the file bar, the status and the controls, which carry everything the picture
 * shows. It plays by itself once in sight (not with reduced motion), and pauses out of sight.
 */
export function DownloadSimulation() {
  const [sim, dispatch] = useReducer(reduce, undefined, initialSim);
  const [feed] = useState(() => createFeed(sim));
  const [playing, setPlaying] = useState(false);
  // Until the reader uses a control, the simulation's steps aren't read out to them.
  const [touched, setTouched] = useState(false);
  const [inView, setInView] = useState(false);
  const reduced = useSyncExternalStore(subscribeMotion, prefersReducedMotion, () => false);
  const pageVisible = useSyncExternalStore(subscribeVisibility, pageIsVisible, () => true);
  const panel = useRef<HTMLDivElement>(null);
  const autoplay = useRef({ allowed: false, done: false });
  const running = playing && inView && pageVisible;

  // The picture reads every step from the feed, outside React's renders.
  useEffect(() => publish(feed, sim), [feed, sim]);

  useEffect(() => {
    autoplay.current.allowed = !reduced && !touched;
  }, [reduced, touched]);

  // Out of sight it waits; the first time it is well in sight, it plays by itself.
  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        setInView(entry.isIntersecting);
        const state = autoplay.current;
        if (entry.intersectionRatio >= AUTOPLAY_AT && state.allowed && !state.done) {
          state.done = true;
          setPlaying(true);
        }
      },
      { threshold: [0, AUTOPLAY_AT] },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!running) return;
    // Reduced motion: the same download, moving in one-second steps with nothing animated.
    if (reduced) {
      const id = window.setInterval(() => dispatch({ type: "tick", dt: PACE }), 1000);
      return () => window.clearInterval(id);
    }
    let last = performance.now();
    let frame = requestAnimationFrame(function step(now) {
      dispatch({ type: "tick", dt: Math.min(0.1, (now - last) / 1000) * PACE });
      last = now;
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [running, reduced]);

  function act(type: "drop" | "change") {
    setTouched(true);
    dispatch({ type });
    setPlaying(true);
  }

  const received = sim.parts.reduce((sum, p) => sum + p.got, 0);
  const percent = Math.min(100, Math.floor((received / SIZE) * 100));
  const parts = [...sim.parts].sort((a, b) => a.start - b.start);
  const canDrop =
    sim.phase === "downloading" && sim.connections.some((c) => c.down === 0 && c.part !== null);
  const dropped = sim.dropped === null ? undefined : parts.find((p) => p.id === sim.connections[sim.dropped!]?.part);
  const message = sim.message === READY && playing ? UNDER_WAY : sim.message;
  const engage = () => setTouched(true);

  return (
    <div ref={panel} className="overflow-hidden rounded-m border border-bank bg-riverbed">
      <p className="px-4 pt-4 text-caption text-silt sm:px-6 sm:pt-6 lg:px-8 lg:pt-7">
        <span className="font-semibold text-foam">Simulation.</span> It runs in your browser and downloads
        nothing.
      </p>

      <MechanismVisual sim={sim} feed={feed} animate={running && !reduced} still={reduced} />

      <div className="px-4 pb-4 sm:px-6 sm:pb-6 lg:px-8 lg:pb-8">
        <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1">
          <p className="font-semibold text-foam">{NAME}</p>
          <p className="tabular text-caption text-silt">
            {Math.round(received).toLocaleString("en-GB")} of {SIZE.toLocaleString("en-GB")} MB, {percent}%
          </p>
        </div>

        {/* What just happened, written over the part it happened to. */}
        <div className="relative mt-2 h-6 text-caption font-semibold" aria-hidden="true">
          {sim.phase === "complete" && (
            <span className="absolute left-0 inline-flex items-center gap-1.5 text-ok">
              <CheckIcon /> Complete
            </span>
          )}
          {sim.phase === "changed" && <span className="absolute left-0 text-fault">Changed on the server</span>}
          {sim.phase === "downloading" && dropped && sim.dropped !== null && (
            <span
              className="absolute whitespace-nowrap text-warn"
              style={
                (dropped.start + dropped.end) / 2 < SIZE / 2
                  ? { left: `${(dropped.start / SIZE) * 100}%` }
                  : { right: `${100 - (dropped.end / SIZE) * 100}%` }
              }
            >
              Connection {sim.dropped + 1} reconnecting
            </span>
          )}
        </div>

        <div
          role="progressbar"
          aria-label={`Simulated download of ${NAME}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
          className="relative mt-1.5 h-9 overflow-hidden rounded-s bg-channel sm:h-10"
        >
          {sim.phase === "complete" ? (
            <div className="absolute inset-0 bg-ok" />
          ) : (
            parts.map((part) => {
              const owner = sim.connections[part.owner];
              const down = owner?.part === part.id && owner.down > 0;
              const fill =
                sim.phase === "changed" ? "bg-fault" : down ? "bg-warn" : part.takenOver ? "bg-estuary" : "bg-stream";
              return (
                <div
                  key={part.id}
                  className="absolute inset-y-0"
                  style={{
                    left: `calc(${(part.start / SIZE) * 100}% + 1px)`,
                    width: `max(0px, calc(${((part.end - part.start) / SIZE) * 100}% - 2px))`,
                  }}
                >
                  <div
                    className={
                      down
                        ? "absolute inset-0 bg-[repeating-linear-gradient(135deg,var(--warn)_0_3px,transparent_3px_9px)] opacity-60"
                        : "absolute inset-0 bg-bank"
                    }
                  />
                  <div
                    className={`absolute inset-y-0 left-0 ${fill}`}
                    style={{ width: `${(part.got / (part.end - part.start)) * 100}%` }}
                  />
                </div>
              );
            })
          )}
        </div>

        <ul className="mt-4 flex flex-wrap gap-x-6 gap-y-2 text-caption text-silt">
          <li className="inline-flex items-center gap-2">
            <span aria-hidden="true" className="size-3 rounded-[3px] bg-stream" />
            Downloading its own part
          </li>
          <li className="inline-flex items-center gap-2">
            <span aria-hidden="true" className="size-3 rounded-[3px] bg-estuary" />
            Took over from a slower connection
          </li>
          <li className="inline-flex items-center gap-2">
            <span
              aria-hidden="true"
              className="size-3 rounded-[3px] bg-[repeating-linear-gradient(135deg,var(--warn)_0_2px,transparent_2px_5px)] ring-1 ring-warn"
            />
            Reconnecting
          </li>
        </ul>

        <p role="status" aria-live={touched ? "polite" : "off"} className="mt-6 min-h-[3.2em] max-w-[62ch] text-foam">
          {message}
        </p>

        <div
          className="mt-4 flex flex-wrap items-center gap-3"
          onFocusCapture={engage}
          onPointerDownCapture={engage}
        >
          <button
            type="button"
            className={`${primary} min-w-[6.5rem]`}
            onClick={() => setPlaying((on) => !on)}
            aria-label={playing ? "Pause the simulation" : "Play the simulation"}
          >
            {playing ? "Pause" : "Play"}
          </button>
          <button type="button" className={secondary} disabled={!canDrop} onClick={() => act("drop")}>
            Drop a connection
          </button>
          <button
            type="button"
            className={secondary}
            disabled={sim.phase !== "downloading"}
            onClick={() => act("change")}
          >
            Change the file on the server
          </button>
        </div>
      </div>
    </div>
  );
}
