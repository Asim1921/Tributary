/**
 * The confluence in the hero, shared by the live 3D scene and the static drawing that stands in
 * for it (before the 3D code loads, with no WebGL, or when it fails). Both use these numbers and
 * the same camera, so the drawing and the first live frame line up.
 *
 * Scene units, y up, the channel on the y axis. The curves are the app icon's own
 * (app/icon.svg: "M250 212 C 250 420, 512 420, 512 600"): each stream leaves its source heading
 * straight down and joins the channel heading straight down, with both control points 54% of the
 * way down. The channel runs on into the icon's download arrow and base bar.
 *
 * Nothing here imports three.js, so the static drawing costs no 3D code.
 */

export type Vec3 = readonly [number, number, number];

export type Stream = {
  /** Where the stream rises. Sources spread in depth (z) as well as across (x). */
  source: Vec3;
  /** Its colour, from 0 (the estuary blue) to 1 (the current teal). */
  hue: number;
};

/** Width over height of the box the confluence is drawn in, on every screen size. */
export const ASPECT = 4 / 5;

/** The main stem comes from the top centre, as in the icon; six tributaries join it. */
export const STREAMS: readonly Stream[] = [
  { source: [-1.3, 1.5, 0.5], hue: 0.12 },
  { source: [-0.9, 1.74, -0.9], hue: 0.72 },
  { source: [-0.48, 1.6, 0.88], hue: 0.4 },
  { source: [0, 1.86, -0.32], hue: 1 },
  { source: [0.5, 1.66, 0.66], hue: 0.86 },
  { source: [0.93, 1.79, -0.78], hue: 0.26 },
  { source: [1.32, 1.52, 0.22], hue: 0.58 },
];

/** Where the streams meet. */
export const MERGE: Vec3 = [0, 0.04, 0];
/** Where the channel ends: the point of the download arrow. */
export const TIP: Vec3 = [0, -1.2, 0];
/** The icon's control points sit 54% of the way down each curve (212 to 420 of 212 to 600). */
export const CONTROL = 0.54;
/** Streams arrive side by side: each keeps this share of its source's offset as a lane. */
export const LANE = 0.045;

/** The arrow's arms run from (±halfWidth, TIP + height) to the tip, in the icon's 176:200. */
export const CHEVRON = { halfWidth: 0.5, height: 0.568, radius: 0.026 } as const;
/** The icon's base bar under the arrow, as a capsule: centre height, half length, radius. */
export const BAR = { y: -1.402, halfLength: 0.6, radius: 0.036 } as const;

/** Scene units per second: the channel carries the water faster than the streams. */
export const SPEED = { stream: 0.27, channel: 0.42 } as const;

export const CAMERA_FOV = 30;
/** How far the view swings, so the fit below leaves room for it (radians about the y axis). */
export const SWING = 0.2;

// --- small vector helpers ------------------------------------------------------------------

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
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
const mix = (a: number, b: number, t: number) => a + (b - a) * t;
const smoothstep = (e0: number, e1: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
};

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

/** The lane a stream keeps where it meets the channel (x and z). */
export function laneOf(stream: Stream): [number, number] {
  return [stream.source[0] * LANE, stream.source[2] * LANE];
}

/** A point on the stream's own curve, t from 0 (source) to 1 (where it meets the channel). */
export function onStream(stream: Stream, t: number): Vec3 {
  const [lx, lz] = laneOf(stream);
  const src = stream.source;
  const end: Vec3 = [MERGE[0] + lx, MERGE[1], MERGE[2] + lz];
  const yc = mix(src[1], end[1], CONTROL);
  return cubic(src, [src[0], yc, src[2]], [end[0], yc, end[2]], end, t);
}

/** A point down the channel, c from 0 (the confluence) to 1 (the tip); lanes close up. */
export function onChannel(stream: Stream, c: number): Vec3 {
  const [lx, lz] = laneOf(stream);
  const k = 1 - smoothstep(0, 1, c);
  return [mix(MERGE[0], TIP[0], c) + lx * k, mix(MERGE[1], TIP[1], c), mix(MERGE[2], TIP[2], c) + lz * k];
}

/**
 * How a particle's time divides between the stream and the channel. `split` is the share of
 * the trip spent on the stream; `period` is the whole trip in seconds.
 */
export function timing(stream: Stream): { split: number; period: number } {
  let length = 0;
  let last = onStream(stream, 0);
  for (let i = 1; i <= 64; i += 1) {
    const next = onStream(stream, i / 64);
    length += Math.hypot(next[0] - last[0], next[1] - last[1], next[2] - last[2]);
    last = next;
  }
  const onStreamSeconds = length / SPEED.stream;
  const onChannelSeconds = (MERGE[1] - TIP[1]) / SPEED.channel;
  const period = onStreamSeconds + onChannelSeconds;
  return { split: onStreamSeconds / period, period };
}

// --- the camera -----------------------------------------------------------------------------

export type CameraFit = { position: Vec3; target: Vec3; fov: number; distance: number };

type Basis = { position: Vec3; right: Vec3; up: Vec3; forward: Vec3 };

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

const rotateY = (p: Vec3, a: number): Vec3 => [
  p[0] * Math.cos(a) + p[2] * Math.sin(a),
  p[1],
  -p[0] * Math.sin(a) + p[2] * Math.cos(a),
];

/** Every point the scene can reach, with a margin for the particles' spread around each path. */
function outline(): Vec3[] {
  const points: Vec3[] = [];
  for (const stream of STREAMS) {
    for (let i = 0; i <= 24; i += 1) {
      const p = onStream(stream, i / 24);
      points.push([p[0] - 0.07, p[1], p[2]], [p[0] + 0.07, p[1], p[2]]);
    }
    points.push([stream.source[0], stream.source[1] + 0.07, stream.source[2]]);
  }
  const armY = TIP[1] + CHEVRON.height;
  points.push([-CHEVRON.halfWidth, armY, 0], [CHEVRON.halfWidth, armY, 0]);
  points.push([-BAR.halfLength - BAR.radius, BAR.y - BAR.radius, 0]);
  points.push([BAR.halfLength + BAR.radius, BAR.y - BAR.radius, 0]);
  return points;
}

/**
 * Places the camera a little above the scene, looking slightly down, as close as it can be with
 * everything (and the swing) inside 90% of the frame, and centres the drawing in the box.
 */
function fitCamera(aspect: number): CameraFit {
  const fov = CAMERA_FOV;
  const elevation = 0.15; // radians above the horizon
  const points = outline().flatMap((p) => [p, rotateY(p, SWING), rotateY(p, -SWING)]);
  let target: Vec3 = [0, 0.2, 0];
  let distance = 8;
  let position: Vec3 = [0, 0, 0];
  for (let pass = 0; pass < 4; pass += 1) {
    let lo = 3;
    let hi = 30;
    for (let i = 0; i < 40; i += 1) {
      const mid = (lo + hi) / 2;
      const pos: Vec3 = [0, target[1] + Math.sin(elevation) * mid, Math.cos(elevation) * mid];
      const b = basis(pos, target);
      const fits = points.every((p) => {
        const [x, y] = toNdc(p, b, fov, aspect);
        return Math.abs(x) <= 0.9 && Math.abs(y) <= 0.9;
      });
      if (fits) hi = mid;
      else lo = mid;
    }
    distance = hi;
    position = [0, target[1] + Math.sin(elevation) * distance, Math.cos(elevation) * distance];
    // Centre vertically: move the target by the drawing's offset from the middle of the frame.
    const b = basis(position, target);
    const ys = points.map((p) => toNdc(p, b, fov, aspect)[1]);
    const offset = (Math.max(...ys) + Math.min(...ys)) / 2;
    const unitsPerNdc = Math.tan((fov * Math.PI) / 360) * distance;
    target = [0, target[1] + offset * unitsPerNdc, 0];
  }
  return { position, target, fov, distance };
}

export const CAMERA: CameraFit = fitCamera(ASPECT);

/** Where a point lands in a viewBox of the given size (y down), seen from CAMERA unswung. */
export function project(p: Vec3, width: number, height: number): [number, number] {
  const [x, y] = toNdc(p, basis(CAMERA.position, CAMERA.target), CAMERA.fov, width / height);
  return [((x + 1) / 2) * width, ((1 - y) / 2) * height];
}
