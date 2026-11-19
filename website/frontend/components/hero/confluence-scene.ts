import * as THREE from "three";

import {
  BAR,
  CAMERA,
  CHEVRON,
  CONTROL,
  LANE,
  MERGE,
  STREAMS,
  SWING,
  TIP,
  timing,
} from "./confluence-geometry";
import {
  arrowFragment,
  barFragment,
  glowFragment,
  guideFragment,
  guideVertex,
  particleFragment,
  particleVertex,
  planeVertex,
} from "./confluence-shaders";

/** An sRGB colour, 0 to 1 per channel, exactly as the CSS token gives it. */
export type Rgb = readonly [number, number, number];

export type Palette = {
  dark: boolean;
  riverbed: Rgb;
  bank: Rgb;
  current: Rgb;
  estuary: Rgb;
  foam: Rgb;
};

/** Particles per second of each stream's trip: the same density on every stream. */
const PARTICLES_PER_SECOND = 44;
/** The moment shown when motion is reduced: the bar full, the water mid-flow. */
const STILL_TIME = 7.4;
/** The bar fills once, the first time the scene is seen: start and length in seconds. */
const FILL = { start: 0.9, seconds: 5.4 } as const;

const vec = (c: Rgb) => new THREE.Vector3(c[0], c[1], c[2]);
const mixRgb = (a: Rgb, b: Rgb, t: number): Rgb => [
  a[0] + (b[0] - a[0]) * t,
  a[1] + (b[1] - a[1]) * t,
  a[2] + (b[2] - a[2]) * t,
];
const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);

/** A small seeded generator, so the water looks the same on every visit and in every test. */
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

function plane(x0: number, x1: number, y0: number, y1: number) {
  const geometry = new THREE.PlaneGeometry(x1 - x0, y1 - y0);
  geometry.translate((x0 + x1) / 2, (y0 + y1) / 2, 0);
  return geometry;
}

function material(fragmentShader: string, uniforms: Record<string, THREE.IUniform>, vertexShader = planeVertex) {
  return new THREE.ShaderMaterial({
    vertexShader,
    fragmentShader,
    uniforms,
    transparent: true,
    depthTest: false,
    depthWrite: false,
  });
}

/**
 * The live confluence. Every particle's place is worked out on the GPU from its stream, its
 * phase and the time, so a frame costs a few uniform writes whatever the particle count.
 */
export class ConfluenceScene {
  readonly root = new THREE.Group();
  readonly background = new THREE.Color();
  /** The streams swing to show their depth; the arrow and bar hold still, square to the page. */
  private readonly water = new THREE.Group();

  private readonly path: Record<string, THREE.IUniform>;
  private readonly particleUniforms: Record<string, THREE.IUniform>;
  private readonly guideUniforms: Record<string, THREE.IUniform>;
  private readonly glowUniforms: Record<string, THREE.IUniform>;
  private readonly arrowUniforms: Record<string, THREE.IUniform>;
  private readonly barUniforms: Record<string, THREE.IUniform>;
  private readonly particleMaterial: THREE.ShaderMaterial;
  private readonly glowMaterial: THREE.ShaderMaterial;
  private readonly disposables: { dispose(): void }[] = [];

  private time = 0;
  private dark = true;
  private readonly lean = new THREE.Vector2();

  constructor() {
    const timings = STREAMS.map(timing);

    // Shared by the particles and the guide lines, so both bend together.
    this.path = {
      uSource: { value: STREAMS.map((s) => new THREE.Vector3(...s.source)) },
      uSplit: { value: timings.map((t) => t.split) },
      uMerge: { value: new THREE.Vector3(...MERGE) },
      uTip: { value: new THREE.Vector3(...TIP) },
      uControl: { value: CONTROL },
      uLane: { value: LANE },
      uBend: { value: 0 },
    };

    // --- particles ---
    const random = seeded(1921);
    const counts = timings.map((t) => Math.round(t.period * PARTICLES_PER_SECOND));
    const total = counts.reduce((a, b) => a + b, 0);
    const stream = new Float32Array(total);
    const phase = new Float32Array(total);
    const seed = new Float32Array(total * 4);
    let n = 0;
    counts.forEach((count, s) => {
      for (let i = 0; i < count; i += 1, n += 1) {
        stream[n] = s;
        // Evenly spaced with a little jitter: an even flow, not clumps and gaps.
        phase[n] = (i + random() * 0.9) / count;
        seed.set([random(), random(), random(), random()], n * 4);
      }
    });
    const particles = new THREE.BufferGeometry();
    particles.setAttribute("position", new THREE.BufferAttribute(new Float32Array(total * 3), 3));
    particles.setAttribute("aStream", new THREE.BufferAttribute(stream, 1));
    particles.setAttribute("aPhase", new THREE.BufferAttribute(phase, 1));
    particles.setAttribute("aSeed", new THREE.BufferAttribute(seed, 4));

    this.particleUniforms = {
      ...this.path,
      uTime: { value: 0 },
      uRate: { value: timings.map((t) => 1 / t.period) },
      uHue: { value: STREAMS.map((s) => s.hue) },
      uEstuary: { value: new THREE.Vector3() },
      uCurrent: { value: new THREE.Vector3() },
      uFoam: { value: new THREE.Vector3() },
      uChannelColor: { value: new THREE.Vector3() },
      uSize: { value: 0.027 },
      uScale: { value: 1000 },
      uDistance: { value: CAMERA.distance },
      uDark: { value: 1 },
      uOpacity: { value: 1 },
    };
    this.particleMaterial = material(particleFragment, this.particleUniforms, particleVertex);
    const points = new THREE.Points(particles, this.particleMaterial);
    points.frustumCulled = false;
    points.renderOrder = 2;

    // --- guide lines: each stream's course, drawn as a hairline ---
    const SEGMENTS = 48;
    const guideStream: number[] = [];
    const guideU: number[] = [];
    timings.forEach((t, s) => {
      for (let k = 0; k < SEGMENTS; k += 1) {
        guideStream.push(s, s);
        guideU.push((t.split * k) / SEGMENTS, (t.split * (k + 1)) / SEGMENTS);
      }
    });
    const guides = new THREE.BufferGeometry();
    guides.setAttribute("position", new THREE.BufferAttribute(new Float32Array(guideU.length * 3), 3));
    guides.setAttribute("aStream", new THREE.BufferAttribute(new Float32Array(guideStream), 1));
    guides.setAttribute("aU", new THREE.BufferAttribute(new Float32Array(guideU), 1));
    this.guideUniforms = {
      ...this.path,
      uDistance: { value: CAMERA.distance },
      uColor: { value: new THREE.Vector3() },
      uOpacity: { value: 0.2 },
    };
    const guideMaterial = material(guideFragment, this.guideUniforms, guideVertex);
    const lines = new THREE.LineSegments(guides, guideMaterial);
    lines.frustumCulled = false;
    lines.renderOrder = 1;

    // --- the glow where the streams meet ---
    const glowGeometry = plane(-0.8, 0.8, TIP[1] - 0.2, MERGE[1] + 0.7);
    this.glowUniforms = {
      uMergePos: { value: new THREE.Vector2(MERGE[0], MERGE[1]) },
      uTipPos: { value: new THREE.Vector2(TIP[0], TIP[1]) },
      uColor: { value: new THREE.Vector3() },
      uStrength: { value: 0.3 },
    };
    this.glowMaterial = material(glowFragment, this.glowUniforms);
    const glow = new THREE.Mesh(glowGeometry, this.glowMaterial);
    glow.renderOrder = 0;

    // --- the download arrow and its base bar ---
    const armY = TIP[1] + CHEVRON.height;
    const arrowGeometry = plane(-CHEVRON.halfWidth - 0.16, CHEVRON.halfWidth + 0.16, TIP[1] - 0.16, armY + 0.16);
    this.arrowUniforms = {
      uTipPos: { value: new THREE.Vector2(TIP[0], TIP[1]) },
      uArm: { value: new THREE.Vector2(CHEVRON.halfWidth, CHEVRON.height) },
      uRadius: { value: CHEVRON.radius },
      uColor: { value: new THREE.Vector3() },
      uHalo: { value: 0.2 },
      uOpacity: { value: 0.94 },
    };
    const arrow = new THREE.Mesh(arrowGeometry, material(arrowFragment, this.arrowUniforms));
    arrow.renderOrder = 3;

    const reach = BAR.halfLength + BAR.radius + 0.16;
    const barGeometry = plane(-reach, reach, BAR.y - 0.16, BAR.y + 0.16);
    this.barUniforms = {
      uCentre: { value: new THREE.Vector2(0, BAR.y) },
      uHalf: { value: BAR.halfLength },
      uRadius: { value: BAR.radius },
      uFill: { value: 0 },
      uTrack: { value: new THREE.Vector3() },
      uFillColor: { value: new THREE.Vector3() },
      uHalo: { value: 0 },
    };
    const bar = new THREE.Mesh(barGeometry, material(barFragment, this.barUniforms));
    bar.renderOrder = 3;

    // The channel ends on the swing's axis, so the still arrow always meets the moving water.
    this.water.add(lines, points);
    this.root.add(glow, this.water, arrow, bar);
    this.disposables.push(
      particles,
      this.particleMaterial,
      guides,
      guideMaterial,
      glowGeometry,
      this.glowMaterial,
      arrowGeometry,
      arrow.material as THREE.Material,
      barGeometry,
      bar.material as THREE.Material,
    );
  }

  /** Takes the theme's colours. Light added to light vanishes, so the day theme uses ink. */
  setPalette(p: Palette) {
    this.dark = p.dark;
    this.background.setRGB(p.riverbed[0], p.riverbed[1], p.riverbed[2], THREE.SRGBColorSpace);
    const u = this.particleUniforms;
    u.uEstuary.value = vec(p.estuary);
    u.uCurrent.value = vec(p.current);
    u.uFoam.value = vec(p.foam);
    u.uChannelColor.value = vec(p.dark ? mixRgb(p.current, p.foam, 0.28) : mixRgb(p.current, p.estuary, 0.25));
    u.uDark.value = p.dark ? 1 : 0;
    u.uSize.value = p.dark ? 0.027 : 0.022;
    u.uOpacity.value = p.dark ? 1 : 0.95;
    this.particleMaterial.blending = p.dark ? THREE.AdditiveBlending : THREE.NormalBlending;

    // Each stream's course as a drawn hairline: the water runs along a plan, not at random.
    this.guideUniforms.uColor.value = vec(p.dark ? p.current : p.estuary);
    this.guideUniforms.uOpacity.value = p.dark ? 0.3 : 0.4;

    this.glowUniforms.uColor.value = vec(p.current);
    this.glowUniforms.uStrength.value = p.dark ? 0.3 : 0.05;
    this.glowMaterial.blending = p.dark ? THREE.AdditiveBlending : THREE.NormalBlending;

    // Light on the night river; by day a deep teal ink, so the arrow doesn't outweigh the water.
    this.arrowUniforms.uColor.value = vec(p.dark ? p.foam : mixRgb(p.foam, p.current, 0.4));
    this.arrowUniforms.uHalo.value = p.dark ? 0.16 : 0;

    this.barUniforms.uTrack.value = vec(p.bank);
    this.barUniforms.uFillColor.value = vec(p.current);
  }

  /**
   * Moves the water on by `delta` seconds and eases the view towards the pointer (x and y from
   * -1 to 1). With `still`, it shows one fixed moment instead.
   */
  frame(delta: number, pointer: { x: number; y: number }, bufferHeight: number, still: boolean) {
    if (!still) this.time += delta;
    const t = still ? STILL_TIME : this.time;

    const ease = still ? 1 : 1 - Math.exp(-delta * 2.2);
    this.lean.x += ((still ? 0 : pointer.x) - this.lean.x) * ease;
    this.lean.y += ((still ? 0 : pointer.y) - this.lean.y) * ease;

    // A slow swing shows the depth; the pointer adds to it. Together they stay within SWING.
    const swing = still ? 0 : Math.sin(t * 0.16) * SWING * 0.5;
    this.water.rotation.y = swing + this.lean.x * SWING * 0.5;
    this.path.uBend.value = this.lean.x * 0.09;

    this.particleUniforms.uTime.value = t;
    this.particleUniforms.uScale.value = bufferHeight / (2 * Math.tan((CAMERA.fov * Math.PI) / 360));

    const progress = still ? 1 : Math.min(1, Math.max(0, (t - FILL.start) / FILL.seconds));
    this.barUniforms.uFill.value = easeInOut(progress);
    const sinceFull = t - (FILL.start + FILL.seconds);
    this.barUniforms.uHalo.value = this.dark && !still && sinceFull > 0 ? 0.55 * Math.exp(-sinceFull * 1.6) : 0;
  }

  dispose() {
    for (const item of this.disposables) item.dispose();
  }
}
