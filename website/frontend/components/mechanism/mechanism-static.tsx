import { useId, useMemo } from "react";

import {
  SERVER,
  SLAB,
  bowFor,
  fitCamera,
  moment,
  onStream,
  portOf,
  project,
  slabX,
  type CameraFit,
  type Vec3,
} from "./geometry";
import { CONNECTIONS, left, type Sim } from "./simulation";

/**
 * The mechanism as a line drawing, from the same camera and the same state as the live view.
 * It is what shows before the 3D code loads, and what stays with no WebGL or if the 3D fails.
 */

const W = 1000;
const round = (n: number) => Math.round(n * 10) / 10;

const TRACK = "color-mix(in oklab, var(--bank) 88%, var(--riverbed))";
const EDGE = "color-mix(in oklab, var(--bank) 70%, var(--foam))";
const shadeOf = (colour: string, black: number) =>
  black === 0 ? colour : `color-mix(in srgb, ${colour}, black ${black}%)`;

type Face = { key: string; points: string; fill: string; hatch?: boolean };

export function MechanismStatic({ sim, aspect, className = "" }: { sim: Sim; aspect: number; className?: string }) {
  const fit = useMemo<CameraFit>(() => fitCamera(aspect), [aspect]);
  const hatchId = useId().replace(/:/g, "");
  const H = W / aspect;
  const at = (p: Vec3) => project(fit, p, W, H).map(round) as [number, number];
  const polygon = (points: Vec3[]) => points.map((p) => at(p).join(",")).join(" ");
  const look = moment(sim);

  // The server: a plate, and a slot per connection.
  const hx = SLAB.length / 2 + SERVER.overhang;
  const plate = polygon([
    [-hx, SERVER.y, SERVER.z - SERVER.depth / 2],
    [hx, SERVER.y, SERVER.z - SERVER.depth / 2],
    [hx, SERVER.y, SERVER.z + SERVER.depth / 2],
    [-hx, SERVER.y, SERVER.z + SERVER.depth / 2],
  ]);

  // The parts, far to near (left to right), each a track and a fill: top, front and leading face.
  const gap = SLAB.gap * (1 - look.fuse);
  const faces: Face[] = [];
  const heads = new Map<number, number>();
  const box = (key: string, x0: number, x1: number, h: number, colour: string, hatch = false, from = 0) => {
    const d = SLAB.depth / 2;
    faces.push(
      { key: `${key}t`, points: polygon([[x0, h, -d], [x1, h, -d], [x1, h, d], [x0, h, d]]), fill: colour, hatch },
      { key: `${key}f`, points: polygon([[x0, 0, d], [x1, 0, d], [x1, h, d], [x0, h, d]]), fill: shadeOf(colour, 28), hatch },
      { key: `${key}s`, points: polygon([[x1, from, d], [x1, from, -d], [x1, h, -d], [x1, h, d]]), fill: shadeOf(colour, 45) },
    );
  };
  [...sim.parts]
    .sort((a, b) => a.start - b.start)
    .forEach((part) => {
      const x0 = slabX(part.start) + gap / 2;
      const x1 = Math.max(x0, slabX(part.end) - gap / 2);
      const owner = sim.connections[part.owner];
      const stalled = sim.phase === "downloading" && owner?.part === part.id && owner.down > 0;
      const share = (part.got / (part.end - part.start)) * (1 - look.drain);
      const xf = Math.min(x0 + (x1 - x0) * share, look.wipe);
      heads.set(part.id, Math.max(x0, xf));
      box(`p${part.id}`, x0, x1, SLAB.track, TRACK, stalled);
      if (xf - x0 > 1e-3) {
        const swept = look.ok > 0.5 && (x0 + xf) / 2 < slabX(0) + SLAB.length * look.sweep;
        const colour =
          look.discard > 0.5
            ? "color-mix(in oklab, var(--fault) 70%, var(--bank))"
            : swept
              ? "color-mix(in oklab, var(--ok) 78%, var(--current))"
              : part.takenOver
                ? "var(--estuary)"
                : "var(--current)";
        box(`p${part.id}x`, x0, xf, SLAB.fill, colour, false, SLAB.track);
      }
    });

  // The streams: one from each port to the write head of the part it is fetching.
  const streams = sim.connections.map((connection, i) => {
    const part = connection.part === null ? undefined : sim.parts.find((p) => p.id === connection.part);
    const flowing = sim.phase === "downloading" && part !== undefined && connection.down === 0 && left(part) > 0;
    const src = portOf(i);
    const x = part ? (heads.get(part.id) ?? src[0]) : src[0];
    const dst: Vec3 = [x, SLAB.fill, 0];
    const bow = bowFor(x - src[0]);
    const d = Array.from({ length: 25 }, (_, k) => at(onStream(src, dst, bow, k / 24)))
      .map(([px, py], k) => `${k === 0 ? "M" : "L"}${px} ${py}`)
      .join("");
    const colour = connection.down > 0 ? "var(--warn)" : part?.takenOver ? "var(--estuary)" : "var(--current)";
    return { key: i, d, colour, flowing, down: connection.down > 0 && sim.phase === "downloading" };
  });

  const slot = (i: number) => {
    const [x, y, z] = portOf(i);
    const [sx, sz] = SERVER.slot;
    return polygon([
      [x - sx, y, z - sz],
      [x + sx, y, z - sz],
      [x + sx, y, z + sz],
      [x - sx, y, z + sz],
    ]);
  };

  return (
    <svg
      className={className}
      viewBox={`0 0 ${W} ${round(H)}`}
      preserveAspectRatio="xMidYMid meet"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <pattern id={hatchId} width="9" height="9" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <rect width="4" height="9" style={{ fill: "var(--warn)", fillOpacity: 0.7 }} />
        </pattern>
      </defs>
      <polygon
        points={plate}
        style={{
          fill: look.flash > 0.05 ? "var(--warn)" : "color-mix(in oklab, var(--bank) 50%, var(--channel))",
          fillOpacity: look.flash > 0.05 ? 0.25 + 0.3 * look.flash : 0.6,
          stroke: look.flash > 0.05 ? "var(--warn)" : EDGE,
          strokeWidth: 1,
        }}
        vectorEffect="non-scaling-stroke"
      />
      {Array.from({ length: CONNECTIONS }, (_, i) => (
        <polygon
          key={i}
          points={slot(i)}
          style={{
            fill: streams[i].down || look.flash > 0.05 ? "var(--warn)" : streams[i].colour,
            fillOpacity: streams[i].flowing || streams[i].down ? 1 : 0.3,
          }}
        />
      ))}
      <g strokeLinejoin="round">
        {faces.map((face) => (
          <g key={face.key}>
            <polygon
              points={face.points}
              style={{ fill: face.fill, stroke: EDGE, strokeWidth: 0.8, strokeOpacity: 0.7 }}
              vectorEffect="non-scaling-stroke"
            />
            {face.hatch && <polygon points={face.points} fill={`url(#${hatchId})`} />}
          </g>
        ))}
      </g>
      <g fill="none" strokeLinecap="round">
        {streams
          .filter((s) => s.flowing)
          .map((s) => (
            <path
              key={s.key}
              d={s.d}
              vectorEffect="non-scaling-stroke"
              style={{ stroke: s.colour, strokeWidth: 2.2, strokeDasharray: "0.5 6", strokeOpacity: 0.9 }}
            />
          ))}
      </g>
    </svg>
  );
}
