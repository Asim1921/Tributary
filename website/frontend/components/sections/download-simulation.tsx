"use client";

import { useReducedMotion } from "motion/react";
import { useEffect, useReducer, useState } from "react";

import { CheckIcon } from "@/components/ui/icons";

/*
 * A simulation of Tributary's download engine, in the browser. It follows the engine's real
 * rules (crates/engine/src/segments.rs): the file is split into one part per connection (eight
 * by default); a connection that finishes takes over half of the largest part still going, never
 * bytes already received, and parts under a minimum size aren't split; a dropped connection
 * carries on from where it stopped; a file that changes on the server means starting again.
 */

const SIZE = 1200; // MB
const NAME = "example.iso";
/** MB per simulated second. The second connection is slow, so the others come to help. */
const SPEEDS = [5.4, 1.2, 4.6, 3.9, 5.8, 4.4, 3.3, 5.1];
/** How far each part has come when the page opens, paused. */
const HEAD_START = [0.44, 0.1, 0.38, 0.3, 0.47, 0.34, 0.26, 0.41];
const MIN_SPLIT = 24; // MB
const RECONNECT_AFTER = 5; // simulated seconds
const CHANGED_FOR = 4; // simulated seconds before it starts again
const PACE = 2; // simulated seconds per real second

type Part = { id: number; owner: number; start: number; end: number; got: number; takenOver: boolean };
type Connection = { part: number | null; down: number };
type Phase = "downloading" | "changed" | "complete";
type Sim = {
  phase: Phase;
  parts: Part[];
  connections: Connection[];
  time: number;
  wait: number;
  nextId: number;
  message: string;
  saidAt: number;
  dropped: number | null;
};
type Action = { type: "tick"; dt: number } | { type: "drop" } | { type: "change" } | { type: "restart" };

const READY = "Paused. The file is split into eight parts, each fetched over its own connection.";

const left = (p: Part) => p.end - p.start - p.got;

function fresh(headStart: boolean, time: number, message: string): Sim {
  const each = SIZE / SPEEDS.length;
  return {
    phase: "downloading",
    parts: SPEEDS.map((_, i) => ({
      id: i,
      owner: i,
      start: i * each,
      end: (i + 1) * each,
      got: headStart ? HEAD_START[i] * each : 0,
      takenOver: false,
    })),
    connections: SPEEDS.map((_, i) => ({ part: i, down: 0 })),
    time,
    wait: 0,
    nextId: SPEEDS.length,
    message,
    saidAt: time,
    dropped: null,
  };
}

function tick(s: Sim, dt: number): Sim {
  if (s.phase === "complete") return s;
  const time = s.time + dt;
  if (s.phase === "changed") {
    const wait = s.wait - dt;
    if (wait > 0) return { ...s, time, wait };
    return fresh(false, time, "Starting again from the beginning, with the new version of the file.");
  }

  const parts = s.parts.map((p) => ({ ...p }));
  const connections = s.connections.map((c) => ({ ...c }));
  let { nextId, message, saidAt, dropped } = s;
  // Events worth announcing always are; takeovers only when nothing was said for a while.
  const say = (text: string, always: boolean) => {
    if (always || time - saidAt > 5) {
      message = text;
      saidAt = time;
    }
  };

  connections.forEach((connection, i) => {
    if (connection.down > 0) {
      connection.down = Math.max(0, connection.down - dt);
      if (connection.down === 0) {
        say(`Connection ${i + 1} is back, and carried on from where it stopped.`, true);
        if (dropped === i) dropped = null;
      }
      return;
    }
    if (connection.part === null) return;
    const part = parts.find((p) => p.id === connection.part);
    if (!part) return;
    // Real connections never hold a steady speed.
    const speed = SPEEDS[i] * (0.8 + 0.4 * Math.abs(Math.sin(time * 1.3 + i * 2.1)));
    part.got = Math.min(part.end - part.start, part.got + speed * dt);
    if (left(part) > 0) return;

    // This connection's part is done: it takes over half of the largest part still going.
    connection.part = null;
    let largest: Part | undefined;
    for (const p of parts) if (left(p) > MIN_SPLIT && (!largest || left(p) > left(largest))) largest = p;
    if (!largest) return;
    const middle = largest.start + largest.got + left(largest) / 2;
    parts.push({ id: nextId, owner: i, start: middle, end: largest.end, got: 0, takenOver: true });
    largest.end = middle;
    connection.part = nextId;
    nextId += 1;
    say(`Connection ${i + 1} finished its part and took over half of what connection ${largest.owner + 1} had left.`, false);
  });

  if (parts.every((p) => left(p) <= 0)) {
    return {
      ...s,
      phase: "complete",
      parts,
      connections,
      time,
      nextId,
      dropped: null,
      message: "Download complete. Every part arrived, joined as one file.",
      saidAt: time,
    };
  }
  return { ...s, parts, connections, time, nextId, message, saidAt, dropped };
}

function reduce(s: Sim, action: Action): Sim {
  switch (action.type) {
    case "tick":
      return tick(s, action.dt);
    case "restart":
      return fresh(false, s.time, "Started again. Downloading over eight connections.");
    case "change":
      if (s.phase !== "downloading") return s;
      return {
        ...s,
        phase: "changed",
        wait: CHANGED_FOR,
        dropped: null,
        message:
          "The file changed on the server. Tributary noticed, and starts again rather than join parts of two different files.",
        saidAt: s.time,
      };
    case "drop": {
      if (s.phase !== "downloading") return s;
      // The connection with the most still to fetch, so the stall shows.
      let pick = -1;
      let most = -1;
      s.connections.forEach((c, i) => {
        const part = c.down === 0 && c.part !== null ? s.parts.find((p) => p.id === c.part) : undefined;
        if (part && left(part) > most) {
          most = left(part);
          pick = i;
        }
      });
      if (pick < 0) return s;
      return {
        ...s,
        connections: s.connections.map((c, i) => (i === pick ? { ...c, down: RECONNECT_AFTER } : c)),
        dropped: pick,
        message: `Connection ${pick + 1} dropped. Tributary reconnects it, and it carries on from where it stopped.`,
        saidAt: s.time,
      };
    }
  }
}

const button =
  "inline-flex min-h-11 items-center justify-center gap-2 rounded-s border px-4 text-[0.95rem] font-semibold " +
  "transition-[background-color,border-color,color] duration-[160ms] ease-flow " +
  "disabled:cursor-not-allowed disabled:opacity-45";
const primary = `${button} border-stream bg-stream text-on-current hover:bg-[color-mix(in_oklab,var(--current)_88%,var(--foam))]`;
const secondary = `${button} border-bank bg-transparent text-foam enabled:hover:border-stream enabled:hover:text-stream`;

/** The file bar of a download in Tributary, which the reader can disturb. */
export function DownloadSimulation() {
  const [sim, dispatch] = useReducer(reduce, undefined, () => fresh(true, 0, READY));
  const [playing, setPlaying] = useState(false);
  const reduced = useReducedMotion();
  const running = playing && sim.phase !== "complete";

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
    dispatch({ type });
    setPlaying(true);
  }

  const received = sim.parts.reduce((sum, p) => sum + p.got, 0);
  const percent = Math.min(100, Math.floor((received / SIZE) * 100));
  const parts = [...sim.parts].sort((a, b) => a.start - b.start);
  const canDrop =
    sim.phase === "downloading" && sim.connections.some((c) => c.down === 0 && c.part !== null);
  const dropped = sim.dropped === null ? undefined : parts.find((p) => p.id === sim.connections[sim.dropped!]?.part);

  return (
    <div className="rounded-m border border-bank bg-riverbed p-4 sm:p-6 lg:p-8">
      <p className="text-caption text-silt">
        <span className="font-semibold text-foam">Simulation.</span> It runs in your browser and downloads
        nothing.
      </p>

      <div className="mt-5 flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1">
        <p className="font-semibold text-foam">{NAME}</p>
        <p className="tabular text-caption text-silt">
          {Math.round(received).toLocaleString("en-GB")} of {SIZE.toLocaleString("en-GB")} MB, {percent}%
        </p>
      </div>

      {/* What just happened, written over the part it happened to. */}
      <div className="relative mt-3 h-6 text-caption font-semibold" aria-hidden="true">
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
        className="relative mt-1.5 h-10 overflow-hidden rounded-s bg-channel sm:h-12"
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

      <p role="status" className="mt-6 min-h-[3.2em] max-w-[62ch] text-foam">
        {sim.message}
      </p>

      <div className="mt-4 flex flex-wrap items-center gap-3">
        {sim.phase === "complete" ? (
          <button type="button" className={primary} onClick={() => dispatch({ type: "restart" })}>
            Download it again
          </button>
        ) : (
          <button
            type="button"
            className={`${primary} min-w-[6.5rem]`}
            onClick={() => setPlaying((on) => !on)}
            aria-label={playing ? "Pause the simulation" : "Play the simulation"}
          >
            {playing ? "Pause" : "Play"}
          </button>
        )}
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
  );
}
