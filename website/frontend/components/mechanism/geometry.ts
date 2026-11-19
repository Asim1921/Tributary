import { CHANGED_FOR, CONNECTIONS, DONE_FOR, SIZE, type Sim } from "./simulation";

/**
 * The mechanism's scene, shared by the live 3D view, the line drawing that stands in for it and
 * the labels laid over both. Nothing here imports three.js.
 *
 * Scene units, y up. The file is a long slab on the x axis, its first byte on the left. The
 * server is a plate above and a little behind it, with one port per connection. Each stream
 * falls in the app icon's own curve: it leaves its port heading straight down and lands heading
 * straight down, both control points 54% of the way (as in the hero's confluence).
 */

export type Vec3 = readonly [number, number, number];

export const SLAB = {
  length: 8,
  depth: 1,
  /** Height of a part still to come, and of what has arrived: the fill stands up out of it. */
  track: 0.07,
  fill: 0.2,
  /** Between neighbouring parts; it closes when the file is whole. */
  gap: 0.05,
} as const;

export const SERVER = { y: 2.75, z: -0.36, depth: 1.1, overhang: 0.32, slot: [0.22, 0.075] as const } as const;

export const CONTROL = 0.54;
/** A stream that crosses to a far part swings this far towards the viewer, so it reads in depth. */
export const BOW = 0.55;
export const FOV = 28;

const mix = (a: number, b: number, t: number) => a + (b - a) * t;
const clamp01 = (t: number) => Math.min(1, Math.max(0, t));
export const smoothstep = (e0: number, e1: number, x: number) => {
  const t = clamp01((x - e0) / (e1 - e0));
  return t * t * (3 - 2 * t);
};

/** Where a byte offset (in MB) lies along the slab. */
export const slabX = (mb: number) => -SLAB.length / 2 + (mb / SIZE) * SLAB.length;

/** The port a connection's stream leaves from: one above the middle of each starting part. */
export const portOf = (connection: number): Vec3 => [
  -SLAB.length / 2 + ((connection + 0.5) * SLAB.length) / CONNECTIONS,
  SERVER.y,
  SERVER.z,
];

export const bowFor = (dx: number) => BOW * smoothstep(0.9, 3.4, Math.abs(dx));

function cubic(a: Vec3, b: Vec3, c: Vec3, d: Vec3, t: number): Vec3 {
  const s = 1 - t;
  const k0 = s * s * s;
  const k1 = 3 * s * s * t;
  const k2 = 3 * s * t * t;
  const k3 = t * t * t;
  return [
    k0 * a[0] + k1 * b[0] + k2 * c[0] + k3 * d[0],
    k0 * a[1] + k1 * b[1] + k2 * c[1] + k3 * d[1],
    k0 * a[2] + k1 * b[2] + k2 * c[2] + k3 * d[2],
  ];
}

/** A point on a stream, u from 0 (its port) to 1 (where it lands). Mirrors streamPoint in the shaders. */
export function onStream(src: Vec3, dst: Vec3, bow: number, u: number): Vec3 {
  const yc = mix(src[1], dst[1], CONTROL);
  return cubic(src, [src[0], yc, src[2] + bow * 0.5], [dst[0], yc, dst[2] + bow], dst, u);
}

export function streamLength(src: Vec3, dst: Vec3, bow: number): number {
  let length = 0;
  let last = src;
  for (let i = 1; i <= 16; i += 1) {
    const next = onStream(src, dst, bow, i / 16);
    length += Math.hypot(next[0] - last[0], next[1] - last[1], next[2] - last[2]);
    last = next;
  }
  return length;
}

// --- the camera -----------------------------------------------------------------------------

export type CameraFit = { position: Vec3; target: Vec3; fov: number; distance: number };

type Basis = { position: Vec3; right: Vec3; up: Vec3; forward: Vec3 };

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a: Vec3, b: Vec3, k = 1): Vec3 => [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const normalize = (a: Vec3): Vec3 => {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};

function basis(position: Vec3, target: Vec3): Basis {
  const forward = normalize(sub(target, position));
  const right = normalize(cross(forward, [0, 1, 0]));
  return { position, forward, right, up: cross(right, forward) };
}

/** Normalised device coordinates (-1 to 1, y up) of a point, as three.js projects it. */
function toNdc(p: Vec3, b: Basis, fov: number, aspect: number): [number, number] {
  const d = sub(p, b.position);
  const depth = dot(d, b.forward);
  const t = Math.tan((fov * Math.PI) / 360);
  return [dot(d, b.right) / (depth * t * aspect), dot(d, b.up) / (depth * t)];
}

/** Every point the scene reaches, with room for the particles' spread and a bowed stream. */
function outline(): Vec3[] {
  const points: Vec3[] = [];
  const hx = SLAB.length / 2;
  const hz = SLAB.depth / 2;
  for (const x of [-hx, hx]) {
    for (const z of [-hz, hz]) points.push([x, 0, z], [x, SLAB.fill + 0.06, z]);
  }
  const px = hx + SERVER.overhang;
  const pz = SERVER.depth / 2;
  for (const x of [-px, px]) {
    for (const z of [SERVER.z - pz, SERVER.z + pz]) points.push([x, SERVER.y, z]);
  }
  points.push([-hx, SERVER.y * 0.45, BOW + 0.1], [hx, SERVER.y * 0.45, BOW + 0.1]);
  return points;
}

/** Room left free inside the frame: a little more at the top, for the server's label. */
const FRAME = { x: 0.92, top: 0.78, bottom: -0.9 } as const;

/**
 * A calm three-quarter view from the front right, so each part's leading face (where its stream
 * writes) turns towards the viewer. Wide boxes look more from the front; tall ones (phones) more
 * from the side and above, so the slab runs into the distance and still fills the frame.
 */
export function fitCamera(aspect: number): CameraFit {
  const t = clamp01((aspect - 1.2) / 1.2);
  const azimuth = mix(0.7, 0.34, t);
  const elevation = mix(0.56, 0.4, t);
  const dir: Vec3 = [
    Math.sin(azimuth) * Math.cos(elevation),
    Math.sin(elevation),
    Math.cos(azimuth) * Math.cos(elevation),
  ];
  const points = outline();
  const half = Math.tan((FOV * Math.PI) / 360);
  const wantX = 0;
  const wantY = (FRAME.top + FRAME.bottom) / 2;
  let target: Vec3 = [0, SERVER.y * 0.45, 0];
  let distance = 14;
  for (let pass = 0; pass < 6; pass += 1) {
    let lo = 3;
    let hi = 60;
    for (let i = 0; i < 40; i += 1) {
      const mid = (lo + hi) / 2;
      const b = basis(add(target, dir, mid), target);
      const ndc = points.map((p) => toNdc(p, b, FOV, aspect));
      const xs = ndc.map((n) => n[0]);
      const ys = ndc.map((n) => n[1]);
      const fits =
        Math.max(...xs) - Math.min(...xs) <= 2 * FRAME.x && Math.max(...ys) - Math.min(...ys) <= FRAME.top - FRAME.bottom;
      if (fits) hi = mid;
      else lo = mid;
    }
    distance = hi;
    // Centre the drawing in the free room: move the target by its offset in the frame.
    const b = basis(add(target, dir, distance), target);
    const ndc = points.map((p) => toNdc(p, b, FOV, aspect));
    const xs = ndc.map((n) => n[0]);
    const ys = ndc.map((n) => n[1]);
    const ox = (Math.max(...xs) + Math.min(...xs)) / 2 - wantX;
    const oy = (Math.max(...ys) + Math.min(...ys)) / 2 - wantY;
    target = add(add(target, b.right, ox * half * aspect * distance), b.up, oy * half * distance);
  }
  return { position: add(target, dir, distance), target, fov: FOV, distance };
}

/** Where a point lands in a box of the given size (y down), seen from a fit. */
export function project(fit: CameraFit, p: Vec3, width: number, height: number): [number, number] {
  const [x, y] = toNdc(p, basis(fit.position, fit.target), fit.fov, width / height);
  return [((x + 1) / 2) * width, ((1 - y) / 2) * height];
}

/**
 * How the picture shows the two moments that take a while, from the simulation's own clock (so
 * the picture keeps step with the bar, and moves in the same steps when motion is reduced).
 * The file changed on the server: the server flashes, the parts turn to the fault colour and
 * drain. The file is complete: the gaps close and the parts fuse into one block in the ok
 * colour, which then clears from the right before the download runs again.
 */
export function moment(sim: Sim) {
  const changed = sim.phase === "changed" ? 1 - sim.wait / CHANGED_FOR : -1;
  const done = sim.phase === "complete" ? 1 - sim.wait / DONE_FOR : -1;
  return {
    flash: changed >= 0 ? 1 - smoothstep(0, 0.6, changed) : 0,
    drain: changed >= 0 ? smoothstep(0.12, 0.78, changed) : 0,
    discard: changed >= 0 ? smoothstep(0, 0.15, changed) : 0,
    fuse: done >= 0 ? smoothstep(0, 0.2, done) * (1 - smoothstep(0.8, 0.93, done)) : 0,
    /** How far the ok colour has swept along the file, 0 to just past 1. */
    sweep: done >= 0 ? smoothstep(0.02, 0.4, done) * 1.12 : 0,
    ok: done >= 0 ? 1 - smoothstep(0.84, 1, done) : 0,
    /** Fills end at this x while the finished file clears away. */
    wipe: done >= 0 ? slabX(0) + SLAB.length * (1 - smoothstep(0.8, 1, done)) : Infinity,
  };
}

/** The box's shape on each screen size; the CSS aspect classes in the visual match these. */
export const ASPECT = { narrow: 6 / 5, wide: 12 / 5 } as const;

/**
 * Anchors for the two labels laid over the picture: above the server's far corner, and under
 * the slab's front edge part of the way along. The edge falls away to the right in this view,
 * so that label hangs to the left of its anchor, clear of the slab; on a phone the slab starts
 * near the box's left side, so the anchor moves further along to leave the label room.
 */
export function labelAnchors(aspect: number): { server: Vec3; file: Vec3 } {
  const along = mix(0.44, 0.3, clamp01((aspect - 1.2) / 1.2));
  return {
    server: [-SLAB.length / 2 - SERVER.overhang, SERVER.y, SERVER.z - SERVER.depth / 2],
    file: [-SLAB.length / 2 + SLAB.length * along, 0, SLAB.depth / 2],
  };
}
