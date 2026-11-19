import * as THREE from "three";

import { CONTROL, SERVER, SLAB, bowFor, moment, portOf, slabX, streamLength } from "./geometry";
import {
  guideFragment,
  guideVertex,
  headFragment,
  headVertex,
  particleFragment,
  particleVertex,
  plateFragment,
  plateVertex,
  slabFragment,
  slabVertex,
} from "./shaders";
import { CONNECTIONS, SPEEDS, left, speedOf, type Part, type Sim } from "./simulation";

/** An sRGB colour, 0 to 1 per channel, exactly as the CSS token gives it. */
export type Rgb = readonly [number, number, number];

export type Palette = {
  dark: boolean;
  riverbed: Rgb;
  channel: Rgb;
  bank: Rgb;
  current: Rgb;
  estuary: Rgb;
  foam: Rgb;
  ok: Rgb;
  warn: Rgb;
  fault: Rgb;
};

/** Pieces of the file in flight per stream, and the most parts the slab can show. */
const PER_STREAM = 110;
const CAPACITY = 48;
/** An emission window that holds nothing, and one that holds everything. */
const NONE = -1e6;
const ALL = 1e6;
const MAX_SPEED = Math.max(...SPEEDS) * 1.2;

const mixRgb = (a: Rgb, b: Rgb, t: number): Rgb => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];
const vec = (c: Rgb) => new THREE.Vector3(c[0], c[1], c[2]);

/** A small seeded generator, so the streams look the same on every visit and in every test. */
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

type Stream = {
  /** Trips made so far: what moves the pieces along. */
  flow: number;
  /** Where the stream lands now, eased towards the part's write head. */
  x: number;
  emitting: boolean | null;
  /** The last two spans of flow during which the server sent nothing: [from, to, from, to]. */
  cut: [number, number, number, number];
  colour: THREE.Vector3;
  head: number;
  guide: number;
  port: number;
};

function material(
  vertexShader: string,
  fragmentShader: string,
  uniforms: Record<string, THREE.IUniform>,
  options: Partial<THREE.ShaderMaterialParameters> = {},
) {
  return new THREE.ShaderMaterial({
    vertexShader,
    fragmentShader,
    uniforms,
    transparent: true,
    depthWrite: false,
    ...options,
  });
}

/**
 * The live picture of the simulation. Every frame it reads the simulation's state and turns it
 * into uniforms; the pieces of the file in flight are placed on the GPU from those numbers.
 */
export class MechanismScene {
  readonly root = new THREE.Group();
  readonly background = new THREE.Color();

  private readonly streams: Stream[];
  private readonly ports = Array.from({ length: CONNECTIONS }, (_, i) => portOf(i));
  private readonly shared: Record<string, THREE.IUniform>;
  private readonly particleUniforms: Record<string, THREE.IUniform>;
  private readonly guideUniforms: Record<string, THREE.IUniform>;
  private readonly headUniforms: Record<string, THREE.IUniform>;
  private readonly slabUniforms: Record<string, THREE.IUniform>;
  private readonly plateUniforms: Record<string, THREE.IUniform>;
  private readonly particleMaterial: THREE.ShaderMaterial;
  private readonly headMaterial: THREE.ShaderMaterial;
  private readonly slab: THREE.InstancedBufferGeometry;
  private readonly span: THREE.InstancedBufferAttribute;
  private readonly tint: THREE.InstancedBufferAttribute;
  private readonly fx: THREE.InstancedBufferAttribute;
  private readonly disposables: { dispose(): void }[] = [];

  private colours = {
    current: [0.184, 0.769, 0.698] as Rgb,
    estuary: [0.298, 0.608, 0.82] as Rgb,
    warn: [0.992, 0.69, 0.133] as Rgb,
    fault: [0.976, 0.439, 0.4] as Rgb,
    track: [0.094, 0.227, 0.259] as Rgb,
  };
  private readonly scratch = new THREE.Vector3();
  private time = 0;
  private started = false;
  private run = -1;
  /** When each part of this run appeared, by id, for the flash where it was split off. */
  private readonly born = new Map<number, number>();

  constructor() {
    this.streams = this.ports.map((port, i) => ({
      flow: 0.31 + i * 0.137,
      x: port[0],
      emitting: null,
      cut: [NONE, NONE, NONE, NONE],
      colour: new THREE.Vector3(),
      head: 1,
      guide: 1,
      port: 1,
    }));

    // Shared by the particles, the guides and the head lights, so all three bend together.
    this.shared = {
      uSrc: { value: this.ports.map((p) => new THREE.Vector3(p[0], p[1], p[2])) },
      uDst: { value: this.ports.map((p) => new THREE.Vector3(p[0], SLAB.fill, 0)) },
      uBow: { value: new Array<number>(CONNECTIONS).fill(0) },
      uControl: { value: CONTROL },
      uColor: { value: this.streams.map((s) => s.colour) },
      uScale: { value: 1000 },
    };

    // --- the pieces of the file in flight ---
    const random = seeded(1921);
    const total = PER_STREAM * CONNECTIONS;
    const stream = new Float32Array(total);
    const phase = new Float32Array(total);
    const keep = new Float32Array(total);
    const seed = new Float32Array(total * 4);
    for (let n = 0; n < total; n += 1) {
      const s = Math.floor(n / PER_STREAM);
      const k = n % PER_STREAM;
      stream[n] = s;
      // Evenly spaced with a little jitter: an even flow, not clumps and gaps.
      phase[n] = (k + random() * 0.9) / PER_STREAM;
      keep[n] = random();
      seed.set([random(), random(), random(), random()], n * 4);
    }
    const particles = new THREE.BufferGeometry();
    particles.setAttribute("position", new THREE.BufferAttribute(new Float32Array(total * 3), 3));
    particles.setAttribute("aStream", new THREE.BufferAttribute(stream, 1));
    particles.setAttribute("aPhase", new THREE.BufferAttribute(phase, 1));
    particles.setAttribute("aKeep", new THREE.BufferAttribute(keep, 1));
    particles.setAttribute("aSeed", new THREE.BufferAttribute(seed, 4));
    this.particleUniforms = {
      ...this.shared,
      uFlow: { value: new Array<number>(CONNECTIONS).fill(0) },
      uCut: { value: this.streams.map(() => new THREE.Vector4(NONE, NONE, NONE, NONE)) },
      uDensity: { value: new Array<number>(CONNECTIONS).fill(1) },
      uFoam: { value: new THREE.Vector3() },
      uTime: { value: 0 },
      uSize: { value: 0.05 },
      uDark: { value: 1 },
      uOpacity: { value: 1 },
    };
    this.particleMaterial = material(particleVertex, particleFragment, this.particleUniforms);
    const points = new THREE.Points(particles, this.particleMaterial);
    points.frustumCulled = false;
    points.renderOrder = 2;

    // --- each stream's course, as a hairline ---
    const SEGMENTS = 40;
    const guideStream: number[] = [];
    const guideU: number[] = [];
    for (let s = 0; s < CONNECTIONS; s += 1) {
      for (let k = 0; k < SEGMENTS; k += 1) {
        guideStream.push(s, s);
        guideU.push(k / SEGMENTS, (k + 1) / SEGMENTS);
      }
    }
    const guides = new THREE.BufferGeometry();
    guides.setAttribute("position", new THREE.BufferAttribute(new Float32Array(guideU.length * 3), 3));
    guides.setAttribute("aStream", new THREE.BufferAttribute(new Float32Array(guideStream), 1));
    guides.setAttribute("aU", new THREE.BufferAttribute(new Float32Array(guideU), 1));
    this.guideUniforms = { ...this.shared, uGuide: { value: new Array<number>(CONNECTIONS).fill(1) }, uOpacity: { value: 0.3 } };
    const guideMaterial = material(guideVertex, guideFragment, this.guideUniforms);
    const lines = new THREE.LineSegments(guides, guideMaterial);
    lines.frustumCulled = false;
    lines.renderOrder = 1;

    // --- a light where each stream lands ---
    const heads = new THREE.BufferGeometry();
    heads.setAttribute("position", new THREE.BufferAttribute(new Float32Array(CONNECTIONS * 3), 3));
    heads.setAttribute("aStream", new THREE.BufferAttribute(new Float32Array(this.ports.map((_, i) => i)), 1));
    this.headUniforms = {
      ...this.shared,
      uHead: { value: new Array<number>(CONNECTIONS).fill(0) },
      uSize: { value: 0.5 },
      uStrength: { value: 0.5 },
    };
    this.headMaterial = material(headVertex, headFragment, this.headUniforms, { depthTest: false });
    const headPoints = new THREE.Points(heads, this.headMaterial);
    headPoints.frustumCulled = false;
    headPoints.renderOrder = 3;

    // --- the file: a track and a fill per part, drawn as instances of one box ---
    const box = new THREE.BoxGeometry(1, 1, 1);
    box.translate(0.5, 0.5, 0);
    this.slab = new THREE.InstancedBufferGeometry();
    this.slab.index = box.index;
    this.slab.setAttribute("position", box.getAttribute("position"));
    this.slab.setAttribute("normal", box.getAttribute("normal"));
    const instanced = (size: number) => {
      const attribute = new THREE.InstancedBufferAttribute(new Float32Array(CAPACITY * 2 * size), size);
      attribute.setUsage(THREE.DynamicDrawUsage);
      return attribute;
    };
    this.span = instanced(4);
    this.tint = instanced(4);
    this.fx = instanced(4);
    this.slab.setAttribute("iSpan", this.span);
    this.slab.setAttribute("iColor", this.tint);
    this.slab.setAttribute("iFx", this.fx);
    this.slab.instanceCount = 0;
    this.slabUniforms = {
      uDepth: { value: SLAB.depth },
      uEdge: { value: new THREE.Vector3() },
      uEdgeAlpha: { value: 0.5 },
      uLight: { value: new THREE.Vector3() },
      uWarn: { value: new THREE.Vector3() },
      uOk: { value: new THREE.Vector3() },
      uFront: { value: 0.7 },
      uSide: { value: 0.55 },
      uFuse: { value: 0 },
      uSweep: { value: 0 },
      uOkMix: { value: 0 },
      uHalfLength: { value: SLAB.length / 2 },
      uTime: { value: 0 },
    };
    const slabMaterial = new THREE.ShaderMaterial({
      vertexShader: slabVertex,
      fragmentShader: slabFragment,
      uniforms: this.slabUniforms,
    });
    const slab = new THREE.Mesh(this.slab, slabMaterial);
    slab.frustumCulled = false;
    slab.renderOrder = 0;

    // --- the server, with one port per connection ---
    const halfX = SLAB.length / 2 + SERVER.overhang;
    const halfZ = SERVER.depth / 2;
    const plateGeometry = new THREE.PlaneGeometry(2 * halfX + 0.6, 2 * halfZ + 0.6);
    plateGeometry.rotateX(-Math.PI / 2);
    plateGeometry.translate(0, SERVER.y, SERVER.z);
    this.plateUniforms = {
      uCentre: { value: new THREE.Vector2(0, SERVER.z) },
      uHalf: { value: new THREE.Vector2(halfX, halfZ) },
      uSlot: { value: new THREE.Vector2(SERVER.slot[0], SERVER.slot[1]) },
      uPortX: { value: this.ports.map((p) => p[0]) },
      uPortColor: { value: this.streams.map(() => new THREE.Vector3()) },
      uPortGlow: { value: new Array<number>(CONNECTIONS).fill(1) },
      uFill: { value: new THREE.Vector3() },
      uFillAlpha: { value: 0.3 },
      uLine: { value: new THREE.Vector3() },
      uLineAlpha: { value: 0.7 },
      uWarn: { value: new THREE.Vector3() },
      uFlash: { value: 0 },
      uDark: { value: 1 },
    };
    const plateMaterial = material(plateVertex, plateFragment, this.plateUniforms, { depthTest: false });
    const plate = new THREE.Mesh(plateGeometry, plateMaterial);
    plate.frustumCulled = false;
    plate.renderOrder = 4;

    this.root.add(slab, lines, points, headPoints, plate);
    this.disposables.push(
      particles,
      this.particleMaterial,
      guides,
      guideMaterial,
      heads,
      this.headMaterial,
      box,
      this.slab,
      slabMaterial,
      plateGeometry,
      plateMaterial,
    );
  }

  /** Takes the theme's colours. Light added to light vanishes, so the day theme uses ink. */
  setPalette(p: Palette) {
    const dark = p.dark;
    this.background.setRGB(p.riverbed[0], p.riverbed[1], p.riverbed[2], THREE.SRGBColorSpace);
    const track = dark ? mixRgb(p.bank, p.riverbed, 0.12) : mixRgb(p.bank, p.channel, 0.2);
    // The states keep their meaning but not their full shout: parts being thrown away are the
    // fault colour dulled towards the empty track; the finished file is ok leaning to the stream.
    this.colours = {
      current: p.current,
      estuary: p.estuary,
      warn: p.warn,
      fault: mixRgb(p.fault, track, 0.3),
      track,
    };

    const u = this.particleUniforms;
    u.uFoam.value = vec(p.foam);
    u.uDark.value = dark ? 1 : 0;
    u.uSize.value = dark ? 0.052 : 0.042;
    u.uOpacity.value = dark ? 1 : 0.92;
    this.particleMaterial.blending = dark ? THREE.AdditiveBlending : THREE.NormalBlending;

    this.guideUniforms.uOpacity.value = dark ? 0.26 : 0.34;

    this.headUniforms.uSize.value = dark ? 0.48 : 0.36;
    this.headUniforms.uStrength.value = dark ? 0.46 : 0.16;
    this.headMaterial.blending = dark ? THREE.AdditiveBlending : THREE.NormalBlending;

    const s = this.slabUniforms;
    s.uEdge.value = vec(dark ? mixRgb(p.bank, p.foam, 0.3) : mixRgb(p.bank, p.foam, 0.32));
    s.uEdgeAlpha.value = dark ? 0.75 : 0.8;
    s.uLight.value = vec(dark ? mixRgb(p.foam, p.current, 0.2) : mixRgb(p.current, p.channel, 0.55));
    s.uWarn.value = vec(p.warn);
    s.uOk.value = vec(mixRgb(p.ok, p.current, 0.22));
    s.uFront.value = dark ? 0.66 : 0.84;
    s.uSide.value = dark ? 0.5 : 0.72;

    const plate = this.plateUniforms;
    plate.uFill.value = vec(dark ? mixRgb(p.channel, p.bank, 0.5) : p.channel);
    plate.uFillAlpha.value = dark ? 0.55 : 0.9;
    plate.uLine.value = vec(dark ? mixRgb(p.bank, p.foam, 0.28) : mixRgb(p.bank, p.foam, 0.3));
    plate.uLineAlpha.value = 0.9;
    plate.uWarn.value = vec(p.warn);
    plate.uDark.value = dark ? 1 : 0;
  }

  /**
   * Draws the simulation as it stands. `advance` moves the water on by `delta` seconds; `still`
   * (reduced motion) shows each state as one fixed picture, with nothing easing or flowing.
   */
  frame(delta: number, sim: Sim, advance: boolean, still: boolean, bufferHeight: number, fov: number) {
    if (advance) this.time += delta;
    const first = !this.started;
    const snap = still || first;
    const ease = snap ? 1 : 1 - Math.exp(-delta * 6.5);
    const fade = snap ? 1 : 1 - Math.exp(-delta * 4);
    this.started = true;
    if (sim.run !== this.run) {
      this.run = sim.run;
      this.born.clear();
    }

    // --- the moments that take a while, from the simulation's own clock ---
    const { flash, drain, discard, fuse, sweep, ok, wipe } = moment(sim);

    // --- the parts, left to right: a track for each, and a fill for what has arrived ---
    const gap = SLAB.gap * (1 - fuse);
    const parts = [...sim.parts].sort((a, b) => a.start - b.start);
    const heads = new Map<number, number>();
    let n = 0;
    parts.forEach((part, index) => {
      if (n >= CAPACITY * 2 - 1) return;
      // Parts already there when the view first draws were split before anyone could see it.
      if (part.takenOver && !this.born.has(part.id)) this.born.set(part.id, first ? -Infinity : this.time);
      const x0 = slabX(part.start) + gap / 2;
      const x1 = Math.max(x0, slabX(part.end) - gap / 2);
      const owner = sim.connections[part.owner];
      const writing = owner?.part === part.id && sim.phase === "downloading";
      const stalled = writing && owner.down > 0;
      const share = (part.got / (part.end - part.start)) * (1 - drain);
      const xf = Math.min(x0 + (x1 - x0) * share, wipe);
      heads.set(part.id, Math.max(x0, xf));
      const inner = (index > 0 ? 1 : 0) + (index < parts.length - 1 ? 2 : 0);

      this.write(n, [x0, x1, SLAB.track, inner], this.colours.track, 0, [stalled ? 1 : 0, 0, 0, 0]);
      n += 1;
      if (xf - x0 > 1e-4) {
        const own = part.takenOver ? this.colours.estuary : this.colours.current;
        const colour = mixRgb(own, this.colours.fault, discard);
        const light = stalled
          ? still
            ? 0.7
            : 0.55 + 0.25 * Math.sin(this.time * 5)
          : writing && left(part) > 0
            ? this.streams[part.owner].head
            : 0;
        const age = this.born.get(part.id);
        const split = still || age === undefined ? 0 : Math.exp(-(this.time - age) * 2.4);
        this.write(n, [x0, xf, SLAB.fill + fuse * 0.025, inner], colour, light, [0, split, 1, stalled ? 1 : 0]);
        n += 1;
      }
    });
    this.slab.instanceCount = n;
    this.span.needsUpdate = true;
    this.tint.needsUpdate = true;
    this.fx.needsUpdate = true;

    const s = this.slabUniforms;
    s.uFuse.value = fuse;
    s.uSweep.value = sweep;
    s.uOkMix.value = ok;
    s.uTime.value = this.time;

    // --- the streams ---
    const byId = new Map<number, Part>(sim.parts.map((p) => [p.id, p]));
    const shared = this.shared;
    const p = this.particleUniforms;
    const plate = this.plateUniforms;
    this.streams.forEach((stream, i) => {
      const connection = sim.connections[i];
      const part = connection.part === null ? undefined : byId.get(connection.part);
      const emitting = sim.phase === "downloading" && part !== undefined && connection.down === 0 && left(part) > 0;
      if (stream.emitting === null) stream.emitting = emitting;
      if (emitting !== stream.emitting) {
        // A break opens a span of flow with nothing in it; picking up again closes it.
        if (emitting) stream.cut[1] = stream.flow;
        else stream.cut = [stream.flow, ALL, stream.cut[0], stream.cut[1]];
        stream.emitting = emitting;
      }

      const target = part ? (heads.get(part.id) ?? stream.x) : stream.x;
      stream.x += (target - stream.x) * ease;
      const src = this.ports[i];
      const bow = bowFor(stream.x - src[0]);
      const dst: [number, number, number] = [stream.x, SLAB.fill + fuse * 0.025, 0];

      const speed = speedOf(i, sim.time);
      if (advance) stream.flow += (delta * (0.75 + 0.28 * speed)) / streamLength(src, dst, bow);

      const colour =
        connection.down > 0 ? this.colours.warn : part?.takenOver ? this.colours.estuary : this.colours.current;
      stream.colour.lerp(this.scratch.set(colour[0], colour[1], colour[2]), ease);

      // The pieces landing now left the server about one trip ago.
      const cut = stream.cut;
      const sent = stream.flow - 0.97;
      const inCut = (sent >= cut[0] && sent < cut[1]) || (sent >= cut[2] && sent < cut[3]);
      const landing = still ? (emitting ? 1 : 0) : inCut ? 0 : 1;
      stream.head += (landing - stream.head) * fade;
      stream.guide += ((emitting || landing ? 1 : 0) - stream.guide) * fade;
      const blink = still ? 0.8 : 0.5 + 0.5 * Math.sin(this.time * 6);
      const portTarget = emitting ? 1 : connection.down > 0 && sim.phase === "downloading" ? blink : 0.1;
      stream.port += (portTarget - stream.port) * (connection.down > 0 ? 1 : fade);

      (shared.uDst.value as THREE.Vector3[])[i].set(dst[0], dst[1], dst[2]);
      (shared.uBow.value as number[])[i] = bow;
      (p.uFlow.value as number[])[i] = stream.flow;
      (p.uDensity.value as number[])[i] = 0.45 + 0.55 * Math.min(1, speed / MAX_SPEED);
      const spans = p.uCut.value as THREE.Vector4[];
      if (still) spans[i].set(emitting ? NONE : -ALL, emitting ? NONE : ALL, NONE, NONE);
      else spans[i].set(cut[0], cut[1], cut[2], cut[3]);
      (this.guideUniforms.uGuide.value as number[])[i] = stream.guide;
      (this.headUniforms.uHead.value as number[])[i] = stream.head;
      (plate.uPortColor.value as THREE.Vector3[])[i].copy(stream.colour);
      (plate.uPortGlow.value as number[])[i] = stream.port;
    });

    p.uTime.value = this.time;
    shared.uScale.value = bufferHeight / (2 * Math.tan((fov * Math.PI) / 360));
    plate.uFlash.value = still ? (flash > 0 ? 0.85 : 0) : flash;
  }

  private write(index: number, span: number[], colour: Rgb, light: number, fx: number[]) {
    this.span.array.set(span, index * 4);
    this.tint.array.set([colour[0], colour[1], colour[2], light], index * 4);
    this.fx.array.set(fx, index * 4);
  }

  dispose() {
    for (const item of this.disposables) item.dispose();
  }
}
