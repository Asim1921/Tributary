import * as THREE from "three";

import { BLOCK, FACES, LANE_LIST, LANES, MARGIN, OUTLET } from "./arrival-geometry";
import { glowFragment, pieceFragment, pieceVertex, planeVertex, waterFragment, waterVertex } from "./arrival-shaders";

/** An sRGB colour, 0 to 1 per channel, exactly as the CSS token gives it. */
export type Rgb = readonly [number, number, number];

export type Palette = {
  dark: boolean;
  riverbed: Rgb;
  channel: Rgb;
  current: Rgb;
  estuary: Rgb;
  foam: Rgb;
};

/** The room's camera: the scene's units are the room's, y up, with the margin round it. */
export const VIEW = {
  left: -MARGIN,
  right: OUTLET.width + MARGIN,
  top: MARGIN,
  bottom: -(OUTLET.height + MARGIN),
} as const;

// --- the schedule: which piece leaves when, down which lane --------------------------------

/** Seconds between pieces on a lane at pace 1, and how fast they ride (units per second). */
const INTERVAL = 0.085;
const RIDE_SPEED = 175;
/** The connections run at their own paces, as real ones do, and start a moment apart. */
const PACE = [1, 0.78, 1.16, 0.9, 1.22, 0.84];
const START = [0.45, 0.6, 0.5, 0.72, 0.48, 0.64];
/** The water's speed down the lanes, units per second. */
const WATER_SPEED = 118;
/** Drops per unit of lane length. */
const WATER_DENSITY = 0.52;

type Piece = { slot: [number, number, number]; grid: [number, number, number]; launch: number; ride: number; hop: number; lane: number };

const cell = [BLOCK.width / BLOCK.cols, BLOCK.height / BLOCK.rows, BLOCK.depth / BLOCK.deep] as const;
const perLane = BLOCK.cols / LANES;

/** A small seeded generator, so the arrival is the same on every visit and in every test. */
function seeded(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * Each lane fills its own share of the block, column by column from the bottom up. When a lane
 * runs out of work it takes over the far half of the slowest lane's remainder, as Tributary's
 * connections do, and those pieces hop across to their place.
 */
function schedule(): Piece[] {
  const queues = Array.from({ length: LANES }, (_, lane) => {
    const queue: [number, number, number][] = [];
    for (let c = lane * perLane; c < (lane + 1) * perLane; c += 1) {
      for (let r = 0; r < BLOCK.rows; r += 1) {
        for (let d = 0; d < BLOCK.deep; d += 1) queue.push([c, r, d]);
      }
    }
    return queue;
  });
  const next = [...START];
  const done = new Array<boolean>(LANES).fill(false);
  const pieces: Piece[] = [];
  const [cx, cy] = [BLOCK.centre[0], BLOCK.centre[1]];

  while (done.some((d) => !d)) {
    let lane = -1;
    for (let l = 0; l < LANES; l += 1) if (!done[l] && (lane < 0 || next[l] < next[lane])) lane = l;
    if (queues[lane].length === 0) {
      let slowest = -1;
      for (let l = 0; l < LANES; l += 1) {
        if (l !== lane && queues[l].length > 3 && (slowest < 0 || queues[l].length > queues[slowest].length)) slowest = l;
      }
      if (slowest < 0) {
        done[lane] = true;
        continue;
      }
      const take = Math.floor(queues[slowest].length / 2);
      queues[lane] = queues[slowest].splice(queues[slowest].length - take, take);
    }
    const [c, r, d] = queues[lane].shift()!;
    const slot: [number, number, number] = [
      -BLOCK.width / 2 + (c + 0.5) * cell[0],
      -BLOCK.height / 2 + (r + 0.5) * cell[1],
      -BLOCK.depth / 2 + (d + 0.5) * cell[2],
    ];
    const end = LANE_LIST[lane];
    const across = Math.abs(cx + slot[0] - end.x) + Math.abs(cy - slot[1] - end.landY);
    pieces.push({
      slot,
      grid: [c, r, d],
      launch: next[lane],
      ride: end.length / RIDE_SPEED,
      hop: 0.42 + across / 260,
      lane,
    });
    next[lane] += INTERVAL / PACE[lane];
  }
  return pieces;
}

const PIECES = schedule();
const LANDINGS = PIECES.map((p) => p.launch + p.ride + p.hop).sort((a, b) => a - b);
/** The moment the last piece is in its place. */
const LANDED = LANDINGS[LANDINGS.length - 1];
const FUSE = { start: LANDED + 0.12, seconds: 0.75 } as const;
const SWEEP = { start: FUSE.start + 0.4, seconds: 1.6 } as const;
const REST = SWEEP.start + SWEEP.seconds;
/** A breath every 6.4 seconds once the file is whole. */
const BREATH = 6.4;
/** The water keeps a trickle going once the file is whole. */
const TRICKLE = 0.16;

const smoothstep = (e0: number, e1: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
};
const vec = (c: Rgb) => new THREE.Vector3(c[0], c[1], c[2]);
const mixRgb = (a: Rgb, b: Rgb, t: number): Rgb => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];

function plane(x0: number, x1: number, y0: number, y1: number) {
  const geometry = new THREE.PlaneGeometry(x1 - x0, y1 - y0);
  geometry.translate((x0 + x1) / 2, (y0 + y1) / 2, 0);
  return geometry;
}

/**
 * The live arrival. The water and every piece are placed on the GPU from the time, so a frame
 * costs a few uniform writes.
 */
export class ArrivalScene {
  readonly root = new THREE.Group();

  private readonly laneUniforms: Record<string, THREE.IUniform>;
  private readonly waterUniforms: Record<string, THREE.IUniform>;
  private readonly pieceUniforms: Record<string, THREE.IUniform>;
  private readonly glowUniforms: Record<string, THREE.IUniform>;
  private readonly waterMaterial: THREE.ShaderMaterial;
  private readonly glowMaterial: THREE.ShaderMaterial;
  private readonly disposables: { dispose(): void }[] = [];

  /** Seconds since the water arrived; below zero, it hasn't yet. */
  private time = -1;
  private dark = true;
  private glowBase = 0.3;

  constructor() {
    this.laneUniforms = {
      uLaneX: { value: LANE_LIST.map((l) => l.x) },
      uLandY: { value: LANE_LIST[0].landY },
      uControlY: { value: LANE_LIST[0].controlY },
    };

    // --- the water: drops riding each lane ---
    const random = seeded(1921);
    const counts = LANE_LIST.map((l) => Math.round(l.length * WATER_DENSITY));
    const total = counts.reduce((a, b) => a + b, 0);
    const laneOf = new Float32Array(total);
    const phase = new Float32Array(total);
    const seed = new Float32Array(total * 4);
    let n = 0;
    counts.forEach((count, l) => {
      for (let i = 0; i < count; i += 1, n += 1) {
        laneOf[n] = l;
        phase[n] = (i + random() * 0.9) / count;
        seed.set([random(), random(), random(), random()], n * 4);
      }
    });
    const water = new THREE.BufferGeometry();
    water.setAttribute("position", new THREE.BufferAttribute(new Float32Array(total * 3), 3));
    water.setAttribute("aLane", new THREE.BufferAttribute(laneOf, 1));
    water.setAttribute("aPhase", new THREE.BufferAttribute(phase, 1));
    water.setAttribute("aSeed", new THREE.BufferAttribute(seed, 4));
    this.waterUniforms = {
      ...this.laneUniforms,
      uTime: { value: -1 },
      uFlow: { value: 0 },
      uRate: { value: LANE_LIST.map((l) => WATER_SPEED / l.length) },
      uHue: { value: LANE_LIST.map((l) => l.hue) },
      uEstuary: { value: new THREE.Vector3() },
      uCurrent: { value: new THREE.Vector3() },
      uFoam: { value: new THREE.Vector3() },
      uChannelColor: { value: new THREE.Vector3() },
      uSize: { value: 2.6 },
      uScale: { value: 1 },
      uDark: { value: 1 },
    };
    this.waterMaterial = new THREE.ShaderMaterial({
      vertexShader: waterVertex,
      fragmentShader: waterFragment,
      uniforms: this.waterUniforms,
      transparent: true,
      depthTest: false,
      depthWrite: false,
    });
    const drops = new THREE.Points(water, this.waterMaterial);
    drops.frustumCulled = false;
    drops.renderOrder = 3;

    // --- the pieces: one small box each, instanced ---
    const box = new THREE.BoxGeometry(1, 1, 1);
    const pieces = new THREE.InstancedBufferGeometry();
    pieces.index = box.index;
    pieces.setAttribute("position", box.getAttribute("position"));
    pieces.setAttribute("normal", box.getAttribute("normal"));
    pieces.instanceCount = PIECES.length;
    const spinRandom = seeded(77);
    const slots = new Float32Array(PIECES.length * 3);
    const grid = new Float32Array(PIECES.length * 3);
    const timing = new Float32Array(PIECES.length * 4);
    const spin = new Float32Array(PIECES.length * 4);
    PIECES.forEach((p, i) => {
      slots.set(p.slot, i * 3);
      grid.set(p.grid, i * 3);
      timing.set([p.launch, p.ride, p.hop, p.lane], i * 4);
      const axis = new THREE.Vector3(spinRandom() - 0.5, spinRandom() - 0.5, spinRandom() - 0.5).normalize();
      spin.set([axis.x, axis.y, axis.z, 0.35 + spinRandom() * 0.45], i * 4);
    });
    pieces.setAttribute("aSlot", new THREE.InstancedBufferAttribute(slots, 3));
    pieces.setAttribute("aGrid", new THREE.InstancedBufferAttribute(grid, 3));
    pieces.setAttribute("aTiming", new THREE.InstancedBufferAttribute(timing, 4));
    pieces.setAttribute("aSpin", new THREE.InstancedBufferAttribute(spin, 4));
    const centre = new THREE.Vector3(BLOCK.centre[0], -BLOCK.centre[1], 0);
    this.pieceUniforms = {
      ...this.laneUniforms,
      uTime: { value: -1 },
      uFuse: { value: 0 },
      uTilt: { value: new THREE.Matrix3().setFromMatrix4(new THREE.Matrix4().makeRotationX(BLOCK.tilt)) },
      uCentre: { value: centre },
      uCell: { value: new THREE.Vector3(cell[0], cell[1], cell[2]) },
      uFront: { value: new THREE.Vector3() },
      uTop: { value: new THREE.Vector3() },
      uSide: { value: new THREE.Vector3() },
      uEdge: { value: new THREE.Vector3() },
      uEdgeLevel: { value: 0.85 },
      uSweepX: { value: -100 },
      uSweep: { value: 0 },
    };
    const pieceMaterial = new THREE.ShaderMaterial({
      vertexShader: pieceVertex,
      fragmentShader: pieceFragment,
      uniforms: this.pieceUniforms,
    });
    const block = new THREE.Mesh(pieces, pieceMaterial);
    block.frustumCulled = false;
    block.renderOrder = 1;

    // --- the glow the finished block rests in, just behind it ---
    const reach = 46;
    const glowGeometry = plane(
      -reach,
      BLOCK.width + reach,
      -(BLOCK.centre[1] + BLOCK.height / 2 + reach),
      -(BLOCK.centre[1] - BLOCK.height / 2 - reach),
    );
    glowGeometry.translate(0, 0, -40);
    this.glowUniforms = {
      uBoxCentre: { value: new THREE.Vector2(centre.x, centre.y) },
      uBoxHalf: { value: new THREE.Vector2(BLOCK.width / 2, (FACES.front.y1 - FACES.top.y0) / 2) },
      uColor: { value: new THREE.Vector3() },
      uStrength: { value: 0 },
      uWindow: { value: new THREE.Vector3(VIEW.left, VIEW.right, -OUTLET.height) },
    };
    this.glowMaterial = new THREE.ShaderMaterial({
      vertexShader: planeVertex,
      fragmentShader: glowFragment,
      uniforms: this.glowUniforms,
      transparent: true,
      depthWrite: false,
    });
    const glow = new THREE.Mesh(glowGeometry, this.glowMaterial);
    glow.renderOrder = 0;

    this.root.add(glow, block, drops);
    this.disposables.push(water, this.waterMaterial, box, pieces, pieceMaterial, glowGeometry, this.glowMaterial);
  }

  /** Takes the theme's colours. Light added to light vanishes, so the day theme uses ink. */
  setPalette(p: Palette) {
    this.dark = p.dark;
    const w = this.waterUniforms;
    w.uEstuary.value = vec(p.estuary);
    w.uCurrent.value = vec(p.current);
    w.uFoam.value = vec(p.foam);
    w.uChannelColor.value = vec(p.dark ? mixRgb(p.current, p.foam, 0.28) : mixRgb(p.current, p.estuary, 0.25));
    w.uDark.value = p.dark ? 1 : 0;
    w.uSize.value = p.dark ? 3.1 : 2.4;
    this.waterMaterial.blending = p.dark ? THREE.AdditiveBlending : THREE.NormalBlending;

    // A deep teal block with lit edges by night; by day pale faces drawn in teal ink.
    const u = this.pieceUniforms;
    if (p.dark) {
      u.uFront.value = vec(mixRgb(p.channel, p.current, 0.26));
      u.uTop.value = vec(mixRgb(p.channel, p.current, 0.5));
      u.uSide.value = vec(mixRgb(p.channel, p.current, 0.14));
      u.uEdge.value = vec(mixRgb(p.current, p.foam, 0.42));
    } else {
      u.uFront.value = vec(mixRgb(p.channel, p.current, 0.16));
      u.uTop.value = vec(mixRgb(p.channel, p.current, 0.06));
      u.uSide.value = vec(mixRgb(p.channel, p.current, 0.3));
      u.uEdge.value = vec(mixRgb(p.current, p.estuary, 0.15));
    }

    this.glowUniforms.uColor.value = vec(p.current);
    this.glowBase = p.dark ? 0.3 : 0.09;
    this.glowMaterial.blending = p.dark ? THREE.AdditiveBlending : THREE.NormalBlending;
  }

  /** The water has reached the room: the file starts to arrive. */
  start() {
    if (this.time < 0) this.time = 0;
  }

  /**
   * Moves on by `delta` seconds. With `still`, it shows the file whole and at rest instead.
   * `bufferHeight` is the drawing buffer's height in pixels, for the size of the drops.
   */
  frame(delta: number, bufferHeight: number, still: boolean) {
    if (!still && this.time >= 0) this.time += delta;
    const t = still ? REST + BREATH / 4 : this.time;
    const started = still || this.time >= 0;

    const flow = still || !started ? 0 : smoothstep(0, 0.6, t) * (1 - (1 - TRICKLE) * smoothstep(LANDED - 0.6, LANDED + 1.4, t));
    this.waterUniforms.uTime.value = still ? -1 : t;
    this.waterUniforms.uFlow.value = flow;
    this.waterUniforms.uScale.value = bufferHeight / (VIEW.top - VIEW.bottom);

    const fuse = started ? smoothstep(FUSE.start, FUSE.start + FUSE.seconds, t) : 0;
    const p = this.pieceUniforms;
    p.uTime.value = started ? t : -1;
    p.uFuse.value = fuse;
    const sweep = (t - SWEEP.start) / SWEEP.seconds;
    p.uSweep.value = !still && sweep > 0 && sweep < 1 ? Math.sin(Math.PI * sweep) : 0;
    p.uSweepX.value = -40 + sweep * (BLOCK.width + 80);

    // Once whole, the block rests in a glow that rises and falls, slowly.
    const settled = still ? 1 : smoothstep(REST - 0.4, REST + 1.6, t);
    const breath = still ? 0.5 : 0.5 - 0.5 * Math.cos((2 * Math.PI * Math.max(0, t - REST)) / BREATH);
    const level = 0.62 + 0.38 * (1 - settled + settled * breath);
    p.uEdgeLevel.value = this.dark ? 0.78 + 0.22 * level : 0.92;
    this.glowUniforms.uStrength.value = this.glowBase * fuse * level;
  }

  dispose() {
    for (const item of this.disposables) item.dispose();
  }
}
