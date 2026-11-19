import { STREAMS } from "./confluence-geometry";

/**
 * GLSL for the confluence. The path functions mirror onStream and onChannel in
 * confluence-geometry.ts; keep the two in step. Colours arrive as sRGB values straight from the
 * CSS tokens and are written out unchanged, so the scene matches the page exactly.
 */

const path = /* glsl */ `
#define STREAMS ${STREAMS.length}
uniform vec3 uSource[STREAMS];
uniform float uSplit[STREAMS];
uniform vec3 uMerge;
uniform vec3 uTip;
uniform float uControl;
uniform float uLane;
uniform float uBend;

vec3 cubic(vec3 a, vec3 b, vec3 c, vec3 d, float t) {
  float s = 1.0 - t;
  return s * s * s * a + 3.0 * s * s * t * b + 3.0 * s * t * t * c + t * t * t * d;
}

// u runs 0..1 down stream i, then down the channel to the tip. info.x is 1 in the channel,
// info.y how far down the channel (0..1).
vec3 flowPoint(int i, float u, out vec2 info) {
  vec3 src = uSource[i];
  vec3 lane = vec3(src.x, 0.0, src.z) * uLane;
  vec3 m = uMerge + lane;
  float split = uSplit[i];
  if (u < split) {
    float t = u / split;
    float yc = mix(src.y, m.y, uControl);
    vec3 p = cubic(src, vec3(src.x, yc, src.z), vec3(m.x, yc, m.z), m, t);
    // The pointer leans on the streams; sources and the confluence stay put.
    p.x += uBend * sin(3.14159265 * t) * (0.55 + 0.3 * abs(src.x));
    info = vec2(0.0, 0.0);
    return p;
  }
  float c = (u - split) / (1.0 - split);
  info = vec2(1.0, c);
  float k = 1.0 - smoothstep(0.0, 1.0, c);
  return mix(uMerge, uTip, c) + lane * k;
}
`;

export const particleVertex = /* glsl */ `
${path}
attribute float aStream;
attribute float aPhase;
attribute vec4 aSeed; // pace, size, angle, radius: each 0..1
uniform float uTime;
uniform float uRate[STREAMS];
uniform float uHue[STREAMS];
uniform vec3 uEstuary;
uniform vec3 uCurrent;
uniform vec3 uFoam;
uniform vec3 uChannelColor;
uniform float uSize;
uniform float uScale;
uniform float uDistance;
uniform float uDark;
varying vec3 vColor;
varying float vAlpha;

void main() {
  int i = int(aStream + 0.5);
  float pace = 0.9 + 0.2 * aSeed.x;
  float u = fract(aPhase + uTime * uRate[i] * pace);
  vec2 info;
  vec3 p = flowPoint(i, u, info);
  float c = info.x * info.y;

  // A thread rather than a line: each particle keeps its own place around the path and turns
  // slowly about it. The channel draws the water together towards the tip.
  float turn = aSeed.z * 6.2831853 + uTime * (0.3 + 0.45 * aSeed.x);
  float spread = mix(0.05, 0.034, info.x) * (1.0 - 0.88 * smoothstep(0.4, 1.0, c));
  float r = sqrt(aSeed.w) * spread;
  p.x += cos(turn) * r;
  p.z += sin(turn) * r;

  vec4 mv = modelViewMatrix * vec4(p, 1.0);
  gl_Position = projectionMatrix * mv;

  // Most particles are fine mist; one in eight is a larger carrier, lit on the dark theme.
  // On the light theme carriers stay the stream's own colour, so they never clump into beads.
  float carrier = step(0.875, aSeed.y);
  float boost = mix(1.2 + 0.25 * aSeed.y, 1.45 + 0.5 * aSeed.y, uDark);
  float size = uSize * mix(0.5 + 0.55 * aSeed.y, boost, carrier);
  gl_PointSize = size * uScale / -mv.z;

  vec3 hue = mix(uEstuary, uCurrent, uHue[i]);
  vec3 colour = mix(hue, uChannelColor, smoothstep(0.0, 0.45, c));
  vColor = mix(colour, uFoam, carrier * 0.5 * uDark);

  float nearness = smoothstep(uDistance + 1.4, uDistance - 1.2, -mv.z);
  float fadeIn = smoothstep(0.0, 0.07, u);
  float fadeOut = 1.0 - smoothstep(0.84, 1.0, c);
  vAlpha = fadeIn * fadeOut * mix(0.42, 1.0, nearness) * mix(0.62, 1.0, carrier);
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
  float disc = (1.0 - smoothstep(0.35, 1.0, d)) * 0.78;
  gl_FragColor = vec4(vColor, mix(disc, glow, uDark) * vAlpha * uOpacity);
}
`;

export const guideVertex = /* glsl */ `
${path}
attribute float aStream;
attribute float aU;
uniform float uDistance;
varying float vAlpha;

void main() {
  vec2 info;
  vec3 p = flowPoint(int(aStream + 0.5), aU, info);
  vec4 mv = modelViewMatrix * vec4(p, 1.0);
  gl_Position = projectionMatrix * mv;
  float nearness = smoothstep(uDistance + 1.4, uDistance - 1.2, -mv.z);
  vAlpha = smoothstep(0.0, 0.12, aU) * (1.0 - info.x) * mix(0.45, 1.0, nearness);
}
`;

export const guideFragment = /* glsl */ `
uniform vec3 uColor;
uniform float uOpacity;
varying float vAlpha;

void main() {
  gl_FragColor = vec4(uColor, vAlpha * uOpacity);
}
`;

/** Flat shapes in the z = 0 plane pass their position on, for the signed-distance shaders. */
export const planeVertex = /* glsl */ `
varying vec2 vPos;

void main() {
  vPos = position.xy;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`;

const segment = /* glsl */ `
float segment(vec2 p, vec2 a, vec2 b) {
  vec2 pa = p - a;
  vec2 ba = b - a;
  float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * h);
}
`;

/** The glow where the streams meet and down the channel. */
export const glowFragment = /* glsl */ `
${segment}
uniform vec2 uMergePos;
uniform vec2 uTipPos;
uniform vec3 uColor;
uniform float uStrength;
varying vec2 vPos;

void main() {
  float pool = exp(-pow(length(vPos - uMergePos) / 0.42, 2.0));
  float along = clamp((uMergePos.y - vPos.y) / (uMergePos.y - uTipPos.y), 0.0, 1.0);
  float run = exp(-pow(segment(vPos, uMergePos, uTipPos) / (0.16 * (1.0 - 0.6 * along) + 0.02), 2.0));
  float a = (pool * 0.85 + run * 0.6 * (1.0 - along * 0.7)) * uStrength;
  gl_FragColor = vec4(uColor, a);
}
`;

/** The download arrow: two arms meeting at the tip, round-capped like the icon's strokes. */
export const arrowFragment = /* glsl */ `
${segment}
uniform vec2 uTipPos;
uniform vec2 uArm;
uniform float uRadius;
uniform vec3 uColor;
uniform float uHalo;
uniform float uOpacity;
varying vec2 vPos;

void main() {
  float d = min(
    segment(vPos, uTipPos + vec2(-uArm.x, uArm.y), uTipPos),
    segment(vPos, uTipPos + vec2(uArm.x, uArm.y), uTipPos)
  ) - uRadius;
  float aa = fwidth(d);
  float solid = 1.0 - smoothstep(-aa, aa, d);
  float halo = exp(-max(d, 0.0) * 22.0) * uHalo;
  gl_FragColor = vec4(uColor, max(solid, halo) * uOpacity);
}
`;

/** The base bar: a track, filled from the middle outwards as the file arrives. */
export const barFragment = /* glsl */ `
${segment}
uniform vec2 uCentre;
uniform float uHalf;
uniform float uRadius;
uniform float uFill;
uniform vec3 uTrack;
uniform vec3 uFillColor;
uniform float uHalo;
varying vec2 vPos;

void main() {
  vec2 q = vPos - uCentre;
  float track = segment(q, vec2(-uHalf, 0.0), vec2(uHalf, 0.0)) - uRadius;
  float reach = uHalf * uFill;
  float fill = segment(q, vec2(-reach, 0.0), vec2(reach, 0.0)) - uRadius * smoothstep(0.0, 0.08, uFill);
  float aa = fwidth(track);
  float inTrack = 1.0 - smoothstep(-aa, aa, track);
  float inFill = 1.0 - smoothstep(-aa, aa, fill);
  // The fill over the track, then (dark theme, once full) a halo outside it in the fill colour.
  vec3 colour = mix(uTrack, uFillColor, inFill);
  float alpha = max(inTrack, inFill);
  float halo = exp(-max(fill, 0.0) * 20.0) * uHalo * (1.0 - inTrack);
  colour = mix(colour, uFillColor, halo / max(alpha + halo, 0.0001));
  gl_FragColor = vec4(colour, max(alpha, halo));
}
`;
