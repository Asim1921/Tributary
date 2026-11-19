/*
 * A simulation of Tributary's download engine, in the browser. It follows the engine's real
 * rules (crates/engine/src/segments.rs): the file is split into one part per connection (eight
 * by default); a connection that finishes takes over half of the largest part still going, never
 * bytes already received, and parts under a minimum size aren't split; a dropped connection
 * carries on from where it stopped; a file that changes on the server means starting again.
 *
 * Pure data and functions, no React and no three.js: the HTML bar (the accessible truth) and the
 * 3D picture of it both read the same state.
 */

export const SIZE = 1200; // MB
export const NAME = "example.iso";
/** MB per simulated second. The second connection is slow, so the others come to help. */
export const SPEEDS = [5.4, 1.2, 4.6, 3.9, 5.8, 4.4, 3.3, 5.1] as const;
export const CONNECTIONS = SPEEDS.length;
/** How far each part has come when the page opens. */
const HEAD_START = [0.44, 0.1, 0.38, 0.3, 0.47, 0.34, 0.26, 0.41];
const MIN_SPLIT = 24; // MB
const RECONNECT_AFTER = 7; // simulated seconds
/** Simulated seconds between noticing the change and starting again. */
export const CHANGED_FOR = 4;
/** Simulated seconds the finished file is shown before the download runs again. */
export const DONE_FOR = 6;
/** Simulated seconds per real second. */
export const PACE = 2;

export type Part = { id: number; owner: number; start: number; end: number; got: number; takenOver: boolean };
export type Connection = { part: number | null; down: number };
export type Phase = "downloading" | "changed" | "complete";
export type Sim = {
  phase: Phase;
  parts: Part[];
  connections: Connection[];
  time: number;
  /** Simulated seconds left in the "changed" or "complete" phase. */
  wait: number;
  nextId: number;
  message: string;
  saidAt: number;
  dropped: number | null;
  /** Counts every fresh start, so a picture of the download can tell a new run from the old. */
  run: number;
};
export type Action = { type: "tick"; dt: number } | { type: "drop" } | { type: "change" };

export const READY = "Paused. The file is split into eight parts, each fetched over its own connection.";
export const UNDER_WAY = "Downloading over eight connections, each fetching its own part of the file.";

export const left = (p: Part) => p.end - p.start - p.got;

/** A connection's speed at a moment, in MB per simulated second. Real connections never hold steady. */
export const speedOf = (connection: number, time: number) =>
  SPEEDS[connection] * (0.8 + 0.4 * Math.abs(Math.sin(time * 1.3 + connection * 2.1)));

export function fresh(headStart: boolean, time: number, message: string, run = 0): Sim {
  const each = SIZE / CONNECTIONS;
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
    nextId: CONNECTIONS,
    message,
    saidAt: time,
    dropped: null,
    run,
  };
}

function tick(s: Sim, dt: number): Sim {
  const time = s.time + dt;
  if (s.phase === "changed") {
    const wait = s.wait - dt;
    if (wait > 0) return { ...s, time, wait };
    return fresh(false, time, "Starting again from the beginning, with the new version of the file.", s.run + 1);
  }
  if (s.phase === "complete") {
    const wait = s.wait - dt;
    if (wait > 0) return { ...s, time, wait };
    return fresh(false, time, "Downloading it again, over eight connections.", s.run + 1);
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
    part.got = Math.min(part.end - part.start, part.got + speedOf(i, time) * dt);
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
      wait: DONE_FOR,
      nextId,
      dropped: null,
      message: "Download complete. Every part arrived, joined as one file.",
      saidAt: time,
    };
  }
  return { ...s, parts, connections, time, nextId, message, saidAt, dropped };
}

export function reduce(s: Sim, action: Action): Sim {
  switch (action.type) {
    case "tick":
      return tick(s, action.dt);
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

export const initialSim = () => fresh(true, 0, READY);

/**
 * The latest state, for a picture that draws itself outside React's renders (the 3D view reads
 * it every frame). `changed` tells listeners there is something new to draw.
 */
export type SimFeed = { current: Sim; listeners: Set<() => void> };

export const createFeed = (sim: Sim): SimFeed => ({ current: sim, listeners: new Set() });

export function publish(feed: SimFeed, sim: Sim) {
  feed.current = sim;
  for (const listener of feed.listeners) listener();
}
