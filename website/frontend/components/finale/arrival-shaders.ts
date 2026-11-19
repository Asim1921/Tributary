import { BLOCK, LANES } from "./arrival-geometry";

/**
 * GLSL for the arrival. The lane curve mirrors onLane in arrival-geometry.ts; keep the two in
 * step. The scene's y runs up, so the room's y (down) is negated. Colours arrive as sRGB values
 * straight from the CSS tokens and are written out unchanged, so the scene matches the page.
 */

const lane = /* glsl */ `
#define LANES ${LANES}
uniform float uLaneX[LANES];
uniform float uLandY;
uniform float uControlY;

float cubic(float a, float b, float c, float d, float t) {
  float s = 1.0 - t;
  return s * s * s * a + 3.0 * s * s * t * b + 3.0 * s * t * t * c + t * t * t * d;
}

// A point down lane i, t from 0 (the channel) to 1 (the block's top), in scene units.
vec3 lanePoint(int i, float t) {
  float x = uLaneX[i];
  return vec3(cubic(0.0, 0.0, x, x, t), -cubic(0.0, uControlY, uControlY, uLandY, t), 0.0);
}
`;

/** The water down the lanes: points placed on the GPU from their lane, phase and the time. */
export const waterVertex = /* glsl */ `
${lane}
attribute float aLane;
attribute float aPhase;
attribute vec4 aSeed; // pace, size, turn, keep: each 0..1
uniform float uTime;
uniform float uFlow;
uniform float uRate[LANES];
uniform float uHue[LANES];
uniform vec3 uEstuary;
uniform vec3 uCurrent;
uniform vec3 uFoam;
uniform vec3 uChannelColor;
uniform float uSize;
uniform float uScale;
uniform float uDark;
varying vec3 vColor;
varying float vAlpha;

void main() {
  int i = int(aLane + 0.5);
  float pace = 0.88 + 0.24 * aSeed.x;
  float travelled = uTime * uRate[i] * pace;
  float u = fract(aPhase + travelled);
  vec3 p = lanePoint(i, u);

  // A thread, not a line: each drop keeps a small place of its own across the lane.
  float turn = aSeed.z * 6.2831853 + uTime * (0.5 + 0.7 * aSeed.x);
  p.x += cos(turn) * (0.4 + 1.9 * fract(aSeed.z * 7.31)) * (0.3 + 0.7 * u);
  p.y += sin(turn) * 0.6;

  gl_Position = projectionMatrix * modelViewMatrix * vec4(p, 1.0);

  // Most drops are fine; one in eight is a larger carrier, lit on the dark theme.
  float carrier = step(0.875, aSeed.y);
  float size = uSize * mix(0.55 + 0.5 * aSeed.y, mix(1.3, 1.7, uDark), carrier);
  gl_PointSize = size * uScale;

  // The merged water leaves the channel in its own colour and takes on each lane's.
  vec3 hue = mix(uEstuary, uCurrent, uHue[i]);
  vec3 colour = mix(uChannelColor, hue, smoothstep(0.06, 0.42, u));
  vColor = mix(colour, uFoam, carrier * 0.45 * uDark);

  // The water arrives: a drop shows only once it has come down from the channel since the
  // start. Then the flow thins to a trickle as the file completes (keep against uFlow).
  float arrived = step(u, travelled) * step(0.0, uTime);
  float keep = smoothstep(aSeed.w - 0.06, aSeed.w + 0.06, uFlow);
  float fadeIn = smoothstep(0.0, 0.05, u);
  float fadeOut = 1.0 - smoothstep(0.86, 1.0, u);
  vAlpha = arrived * keep * fadeIn * fadeOut * mix(0.55, 1.0, carrier);
}
`;

export const waterFragment = /* glsl */ `
uniform float uDark;
varying vec3 vColor;
varying float vAlpha;

void main() {
  float d = length(gl_PointCoord - 0.5) * 2.0;
  if (d > 1.0) discard;
  // Light added to a dark river: a soft glow. Ink on a light page: a crisp dot.
  float glow = pow(1.0 - d, 1.5);
  float disc = (1.0 - smoothstep(0.4, 1.0, d)) * 0.8;
  gl_FragColor = vec4(vColor, mix(disc, glow, uDark) * vAlpha);
}
`;

/**
 * The pieces of the file. Each one leaves the channel small and turning, rides its lane down,
 * hops into its place in the block and settles square. Everything is worked out here from the
 * time, so a frame costs a few uniform writes.
 */
export const pieceVertex = /* glsl */ `
${lane}
attribute vec3 aSlot;   // the piece's place, in the block's own axes
attribute vec3 aGrid;   // column, row, depth
attribute vec4 aTiming; // launch, ride, hop (seconds), lane
attribute vec4 aSpin;   // axis (xyz) and turns per second
uniform float uTime;
uniform float uFuse;
uniform mat3 uTilt;
uniform vec3 uCentre;
uniform vec3 uCell;
varying vec3 vLocal;
varying vec3 vNormal;
varying vec3 vFacing;
varying vec3 vGrid;
varying float vFlash;
varying float vX;

vec3 turnAbout(vec3 v, vec3 axis, float angle) {
  float c = cos(angle);
  float s = sin(angle);
  return v * c + cross(axis, v) * s + axis * dot(axis, v) * (1.0 - c);
}

void main() {
  float age = uTime - aTiming.x;
  float ride = aTiming.y;
  float hop = aTiming.z;
  int i = int(aTiming.w + 0.5);
  vec3 slot = uCentre + uTilt * aSlot;
  vec3 landing = lanePoint(i, 1.0);

  vec3 p = slot;
  float scale = 1.0;
  float spin = 0.0;
  float flash = 0.0;
  if (age < 0.0) {
    scale = 0.0;
  } else if (age < ride) {
    // Carried down the lane by the water, unseen.
    scale = 0.0;
  } else {
    // It condenses out of the water at the end of its lane, turning, and hops into its place:
    // a short arc, higher for a longer hop.
    float h = clamp((age - ride) / hop, 0.0, 1.0);
    float e = h * h * (3.0 - 2.0 * h);
    vec3 lift = vec3(0.0, 5.0 + 0.16 * distance(landing.xy, slot.xy), 0.0);
    vec3 mid = (landing + slot) * 0.5 + lift;
    p = mix(mix(landing, mid, e), mix(mid, slot, e), e);
    float back = 1.0 + 2.2 * (h - 1.0) * (h - 1.0) * (h - 1.0) + 1.2 * (h - 1.0) * (h - 1.0);
    scale = mix(0.1, 1.0, back);
    spin = 1.0 - e;
    // A glint as it appears, and again as it lands.
    flash = max(exp(-(age - ride) * 7.0), step(1.0, h) * exp(-(age - ride - hop) * 3.5));
  }

  float angle = aSpin.w * 6.2831853 * (age - ride + 0.6) * spin;
  vec3 local = turnAbout(position, aSpin.xyz, angle);
  // The pieces sit apart while they arrive and close up into one block.
  float inset = mix(0.86, 1.0, uFuse);
  vec3 world = p + uTilt * (local * uCell * scale * inset);
  gl_Position = projectionMatrix * modelViewMatrix * vec4(world, 1.0);

  vLocal = position;
  vNormal = normal;
  vFacing = uTilt * turnAbout(normal, aSpin.xyz, angle);
  vGrid = aGrid;
  vFlash = flash;
  vX = world.x;
}
`;

export const pieceFragment = /* glsl */ `
#define COLS ${BLOCK.cols}.0
#define ROWS ${BLOCK.rows}.0
#define DEEP ${BLOCK.deep}.0
uniform vec3 uFront;
uniform vec3 uTop;
uniform vec3 uSide;
uniform vec3 uEdge;
uniform float uFuse;
uniform float uEdgeLevel;
uniform float uSweepX;
uniform float uSweep;
varying vec3 vLocal;
varying vec3 vNormal;
varying vec3 vFacing;
varying vec3 vGrid;
varying float vFlash;
varying float vX;

void main() {
  vec3 n = normalize(vFacing);
  // Lit from above: the top face is the lightest, the face to the reader the base, sides darker.
  vec3 base = mix(uFront, uTop, smoothstep(-0.1, 0.7, n.y));
  base = mix(base, uSide, abs(n.x));
  // The light falls off down the block's front, so it reads as a solid, not a band.
  float height = (vGrid.y + vLocal.y + 0.5) / ROWS;
  base = mix(base, uSide, (1.0 - height) * 0.55 * (1.0 - smoothstep(0.2, 0.7, n.y)));

  // The face's own edges. Between pieces they fade as the block closes up; the block's outline
  // stays.
  vec3 a = abs(vNormal);
  vec2 uv;
  vec4 outline;
  float firstCol = step(vGrid.x, 0.5);
  float lastCol = step(COLS - 1.5, vGrid.x);
  if (a.z > 0.5) {
    uv = vLocal.xy;
    outline = vec4(firstCol, lastCol, step(vGrid.y, 0.5), step(ROWS - 1.5, vGrid.y));
  } else if (a.y > 0.5) {
    uv = vLocal.xz;
    outline = vec4(firstCol, lastCol, step(vGrid.z, 0.5), step(DEEP - 1.5, vGrid.z));
  } else {
    uv = vLocal.zy;
    outline = vec4(1.0);
  }
  vec4 d = vec4(uv.x + 0.5, 0.5 - uv.x, uv.y + 0.5, 0.5 - uv.y);
  vec2 w = max(fwidth(uv), vec2(1e-5));
  vec4 lines = 1.0 - smoothstep(0.55, 1.55, d / vec4(w.x, w.x, w.y, w.y));
  // Faintly, the pieces still show in the whole file.
  vec4 weight = mix(vec4(1.0 - 0.8 * uFuse), vec4(1.0), outline);
  vec4 shown = lines * weight;
  float edge = max(max(shown.x, shown.y), max(shown.z, shown.w));

  // One slow light across the finished block, and a brief glint as each piece lands.
  float sweep = uSweep * exp(-pow((vX - uSweepX) / 30.0, 2.0));
  float light = clamp(edge * uEdgeLevel + vFlash * 0.6 + sweep * 0.5, 0.0, 1.0);
  gl_FragColor = vec4(mix(base, uEdge, light), 1.0);
}
`;

/** Flat shapes pass their scene position on, for the glow's distance field. */
export const planeVertex = /* glsl */ `
varying vec2 vPos;

void main() {
  vPos = position.xy;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`;

/** The glow the finished block rests in, breathing slowly. */
export const glowFragment = /* glsl */ `
uniform vec2 uBoxCentre;
uniform vec2 uBoxHalf;
uniform vec3 uColor;
uniform float uStrength;
uniform vec3 uWindow; // the canvas's left and right edges, and the button's top
varying vec2 vPos;

void main() {
  vec2 q = abs(vPos - uBoxCentre) - uBoxHalf;
  float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
  float a = exp(-pow(max(d, 0.0) / 13.0, 1.25)) * uStrength;
  // Gone before the canvas's edges, and before the button, which sits on the block's light.
  a *= smoothstep(uWindow.x, uWindow.x + 20.0, vPos.x) * (1.0 - smoothstep(uWindow.y - 20.0, uWindow.y, vPos.x));
  a *= smoothstep(uWindow.z, uWindow.z + 11.0, vPos.y);
  gl_FragColor = vec4(uColor, a);
}
`;
