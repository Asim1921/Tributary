import { CONNECTIONS } from "./simulation";

/**
 * GLSL for the mechanism. streamPoint mirrors onStream in geometry.ts; keep the two in step.
 * Colours arrive as sRGB values straight from the CSS tokens and are written out unchanged, so
 * the scene matches the page (as in the hero).
 */

const streams = /* glsl */ `
#define STREAMS ${CONNECTIONS}
uniform vec3 uSrc[STREAMS];
uniform vec3 uDst[STREAMS];
uniform float uBow[STREAMS];
uniform float uControl;

vec3 cubic(vec3 a, vec3 b, vec3 c, vec3 d, float t) {
  float s = 1.0 - t;
  return s * s * s * a + 3.0 * s * s * t * b + 3.0 * s * t * t * c + t * t * t * d;
}

vec3 streamPoint(int i, float u) {
  vec3 s = uSrc[i];
  vec3 d = uDst[i];
  float yc = mix(s.y, d.y, uControl);
  return cubic(s, vec3(s.x, yc, s.z + uBow[i] * 0.5), vec3(d.x, yc, d.z + uBow[i]), d, u);
}
`;

/**
 * Each particle is one piece of the file on its way. Its place comes from its stream, its phase
 * and the stream's flow (how many trips the stream has made). The flow at which it left the
 * server tells whether the stream was cut then: a cut leaves a gap that falls away down the
 * stream, and the last pieces before it fray.
 */
export const particleVertex = /* glsl */ `
${streams}
uniform float uFlow[STREAMS];
uniform vec4 uCut[STREAMS];
uniform float uDensity[STREAMS];
uniform vec3 uColor[STREAMS];
uniform vec3 uFoam;
uniform float uTime;
uniform float uSize;
uniform float uScale;
uniform float uDark;
attribute float aStream;
attribute float aPhase;
attribute vec4 aSeed; // pace, size, angle, radius: each 0..1
attribute float aKeep;
varying vec3 vColor;
varying float vAlpha;

float within(float e, float from, float to) {
  return step(from, e) * (1.0 - step(to, e));
}

void main() {
  int i = int(aStream + 0.5);
  float pace = 0.93 + 0.14 * aSeed.x;
  float raw = aPhase + uFlow[i] * pace;
  float u = fract(raw);
  float sent = (floor(raw) - aPhase) / pace;
  vec4 cut = uCut[i];
  float gone = max(within(sent, cut.x, cut.y), within(sent, cut.z, cut.w));
  float fray = smoothstep(cut.x - 0.45, cut.x, sent) * (1.0 - step(cut.x, sent));

  vec3 p = streamPoint(i, u);
  // A thread rather than a line: each piece keeps its place around the path and turns slowly.
  float turn = aSeed.z * 6.2831853 + uTime * (0.5 + 0.6 * aSeed.x);
  float spread = 0.03 * (1.0 + 7.0 * fray * u) * (1.0 - 0.7 * smoothstep(0.8, 1.0, u));
  float r = sqrt(aSeed.w) * spread;
  p.x += cos(turn) * r;
  p.z += sin(turn) * r;
  p.y -= fray * u * u * 0.12;

  vec4 mv = modelViewMatrix * vec4(p, 1.0);
  gl_Position = projectionMatrix * mv;

  float carrier = step(0.88, aSeed.y);
  float boost = mix(1.25 + 0.25 * aSeed.y, 1.5 + 0.5 * aSeed.y, uDark);
  float size = uSize * mix(0.5 + 0.55 * aSeed.y, boost, carrier);
  gl_PointSize = size * uScale / -mv.z;

  vColor = mix(uColor[i], uFoam, carrier * 0.45 * uDark);
  float fadeIn = smoothstep(0.0, 0.08, u);
  float fadeOut = 1.0 - smoothstep(0.9, 1.0, u);
  float shown = step(aKeep, uDensity[i]);
  vAlpha = fadeIn * fadeOut * shown * (1.0 - gone) * (1.0 - 0.55 * fray) * mix(0.7, 1.0, carrier);
}
`;

export const particleFragment = /* glsl */ `
uniform float uDark;
uniform float uOpacity;
varying vec3 vColor;
varying float vAlpha;

void main() {
  float d = length(gl_PointCoord - 0.5) * 2.0;
  if (d > 1.0) discard;
  // Light added to a dark river: a soft glow. Ink on a light page: a crisp dot.
  float glow = pow(1.0 - d, 1.6);
  float disc = (1.0 - smoothstep(0.35, 1.0, d)) * 0.8;
  gl_FragColor = vec4(vColor, mix(disc, glow, uDark) * vAlpha * uOpacity);
}
`;

/** Each stream's course as a hairline, as in the hero: the water runs along a plan. */
export const guideVertex = /* glsl */ `
${streams}
uniform float uGuide[STREAMS];
uniform vec3 uColor[STREAMS];
attribute float aStream;
attribute float aU;
varying vec3 vColor;
varying float vAlpha;

void main() {
  int i = int(aStream + 0.5);
  vec3 p = streamPoint(i, aU);
  gl_Position = projectionMatrix * modelViewMatrix * vec4(p, 1.0);
  vColor = uColor[i];
  vAlpha = uGuide[i] * smoothstep(0.0, 0.1, aU) * (1.0 - smoothstep(0.86, 1.0, aU));
}
`;

export const guideFragment = /* glsl */ `
uniform float uOpacity;
varying vec3 vColor;
varying float vAlpha;

void main() {
  gl_FragColor = vec4(vColor, vAlpha * uOpacity);
}
`;

/** Where each stream lands: a soft light on the part's leading edge. */
export const headVertex = /* glsl */ `
${streams}
uniform float uHead[STREAMS];
uniform vec3 uColor[STREAMS];
uniform float uSize;
uniform float uScale;
attribute float aStream;
varying vec3 vColor;
varying float vAlpha;

void main() {
  int i = int(aStream + 0.5);
  vec4 mv = modelViewMatrix * vec4(uDst[i] + vec3(0.0, 0.015, 0.0), 1.0);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = uSize * uScale / -mv.z;
  vColor = uColor[i];
  vAlpha = uHead[i];
}
`;

export const headFragment = /* glsl */ `
uniform float uStrength;
varying vec3 vColor;
varying float vAlpha;

void main() {
  float d = length(gl_PointCoord - 0.5) * 2.0;
  if (d > 1.0) discard;
  gl_FragColor = vec4(vColor, exp(-d * d * 5.0) * vAlpha * uStrength);
}
`;

/**
 * The parts of the file, as blocks: per part a low track for what is still to come and a
 * raised fill for what has arrived. Faces are shaded flat by their direction and drawn with
 * hairline edges, an engineered drawing rather than a rendered object.
 */
export const slabVertex = /* glsl */ `
attribute vec4 iSpan;  // x from, x to, height, inner ends (1 left, 2 right)
attribute vec4 iColor; // rgb, write-head light
attribute vec4 iFx;    // stalled, split flash, fill (1) or track (0), head colour (0 light, 1 warning)
uniform float uDepth;
varying vec3 vLocal;
varying vec3 vSize;
varying vec3 vNormal;
varying vec3 vColor;
varying vec4 vFx;
varying float vHead;
varying float vWorldX;
varying vec2 vInner;

void main() {
  // A fill stands a hair proud of its track on every side, so no two faces share a plane.
  float proud = iFx.z * 0.004;
  float x0 = iSpan.x - proud;
  float w = max(iSpan.y - iSpan.x, 0.0) + 2.0 * proud;
  float depth = uDepth + 2.0 * proud;
  vec3 world = vec3(x0 + position.x * w, position.y * iSpan.z - proud, position.z * depth);
  vLocal = vec3(position.x * w, position.y * iSpan.z, (position.z + 0.5) * depth);
  vSize = vec3(w, iSpan.z, depth);
  vNormal = normal;
  vColor = iColor.rgb;
  vHead = iColor.a;
  vFx = iFx;
  vWorldX = world.x;
  vInner = vec2(mod(iSpan.w, 2.0), step(2.0, iSpan.w));
  gl_Position = projectionMatrix * modelViewMatrix * vec4(world, 1.0);
}
`;

export const slabFragment = /* glsl */ `
uniform vec3 uEdge;
uniform float uEdgeAlpha;
uniform vec3 uLight;
uniform vec3 uWarn;
uniform vec3 uOk;
uniform float uFront;
uniform float uSide;
uniform float uFuse;
uniform float uSweep;
uniform float uOkMix;
uniform float uHalfLength;
uniform float uTime;
varying vec3 vLocal;
varying vec3 vSize;
varying vec3 vNormal;
varying vec3 vColor;
varying vec4 vFx;
varying float vHead;
varying float vWorldX;
varying vec2 vInner;

float hairline(float d) {
  return 1.0 - smoothstep(0.0, fwidth(d) * 1.15, d);
}

void main() {
  vec3 n = normalize(vNormal);
  float top = step(0.5, n.y);
  float front = step(0.5, n.z);
  float leading = step(0.5, n.x);
  float shade = top > 0.5 ? 1.0 : (front > 0.5 ? uFront : (abs(n.x) > 0.5 ? uSide : uFront * 0.8));

  // The whole file, once every part is in: the ok colour sweeps from the first byte to the last.
  float along = (vWorldX + uHalfLength) / (2.0 * uHalfLength);
  float ok = uOkMix * (1.0 - smoothstep(uSweep - 0.08, uSweep, along)) * vFx.z;
  vec3 base = mix(vColor, uOk, ok);
  vec3 colour = base * shade;

  // A stalled part: warning stripes across what is still to come, as in the bar's legend.
  float diagonal = vWorldX + (top > 0.5 ? vLocal.z : vLocal.y) * 1.2;
  float stripe = step(0.55, fract(diagonal * 5.0));
  colour = mix(colour, uWarn * mix(0.9, 1.0, top), vFx.x * stripe * 0.7 * (1.0 - vFx.z));

  // The write head: the leading face lights up in the part's own colour, and the newest data
  // just behind it glows.
  float behind = vSize.x - vLocal.x;
  float trail = top * exp(-behind / 0.3) + front * exp(-behind / 0.16) * 0.5;
  float head = vHead * max(leading * 0.6, trail * 0.75);
  vec3 light = mix(mix(vColor, uLight, 0.6), uWarn, vFx.w);
  colour = mix(colour, light, head);

  // A new boundary where a part was split shines for a moment.
  colour = mix(colour, uLight, vFx.y * exp(-vLocal.x / 0.07) * max(top, front));

  // Hairline edges. Ends inside the slab fade out as the parts fuse into one block.
  float ends = min(vLocal.x + vInner.x * uFuse * 10.0, behind + vInner.y * uFuse * 10.0);
  float acrossZ = min(vLocal.z, vSize.z - vLocal.z);
  float acrossY = min(vLocal.y, vSize.y - vLocal.y);
  float d = top > 0.5 ? min(ends, acrossZ) : (front > 0.5 ? min(ends, acrossY) : min(acrossZ, acrossY));
  colour = mix(colour, uEdge, hairline(d) * uEdgeAlpha * mix(0.75, 1.0, vFx.z));

  gl_FragColor = vec4(colour, 1.0);
}
`;

/** The server: a plate with one port per connection. It flashes the warning colour when the file changes. */
export const plateVertex = /* glsl */ `
varying vec2 vPos;

void main() {
  vPos = position.xz;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`;

export const plateFragment = /* glsl */ `
#define STREAMS ${CONNECTIONS}
uniform vec2 uCentre;
uniform vec2 uHalf;
uniform vec2 uSlot;
uniform float uPortX[STREAMS];
uniform vec3 uPortColor[STREAMS];
uniform float uPortGlow[STREAMS];
uniform vec3 uFill;
uniform float uFillAlpha;
uniform vec3 uLine;
uniform float uLineAlpha;
uniform vec3 uWarn;
uniform float uFlash;
uniform float uDark;
varying vec2 vPos;

float roundBox(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

void main() {
  vec2 p = vPos - uCentre;
  float d = roundBox(p, uHalf, 0.1);
  float aa = fwidth(d);
  float inside = 1.0 - smoothstep(-aa, aa, d);
  float rim = 1.0 - smoothstep(aa * 0.5, aa * 1.8, abs(d + aa));

  vec3 colour = mix(uFill, uWarn, uFlash);
  float alpha = inside * mix(uFillAlpha, 0.5, uFlash);
  // Two rails along the plate, like a rack: quiet structure, no ornament.
  float rail = max(1.0 - smoothstep(0.0, fwidth(p.y) * 1.2, abs(p.y - uHalf.y * 0.62)),
                   1.0 - smoothstep(0.0, fwidth(p.y) * 1.2, abs(p.y + uHalf.y * 0.62)));
  rail *= inside * (1.0 - smoothstep(uHalf.x - 0.25, uHalf.x - 0.1, abs(p.x)));
  vec3 line = mix(uLine, uWarn, uFlash);
  colour = mix(colour, line, rail * 0.6);
  alpha = max(alpha, rail * uLineAlpha * 0.55);
  colour = mix(colour, line, rim);
  alpha = max(alpha, rim * mix(uLineAlpha, 1.0, uFlash));

  for (int i = 0; i < STREAMS; i++) {
    float s = roundBox(vPos - vec2(uPortX[i], uCentre.y), uSlot, uSlot.y);
    float inSlot = 1.0 - smoothstep(-aa, aa, s);
    float halo = exp(-max(s, 0.0) * 16.0) * uPortGlow[i] * mix(0.25, 0.6, uDark) * (1.0 - inSlot);
    float lit = inSlot * mix(0.35, 1.0, uPortGlow[i]);
    vec3 port = mix(uPortColor[i], uWarn, uFlash);
    colour = mix(colour, port, max(lit, halo));
    alpha = max(alpha, max(lit, halo));
  }
  gl_FragColor = vec4(colour, alpha);
}
`;
