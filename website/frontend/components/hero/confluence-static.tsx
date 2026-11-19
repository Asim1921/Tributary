import {
  ASPECT,
  BAR,
  CHEVRON,
  MERGE,
  STREAMS,
  TIP,
  onChannel,
  onStream,
  project,
  type Vec3,
} from "./confluence-geometry";

/**
 * The confluence as a still line drawing, seen from the same camera as the live scene. It is
 * what loads first, what stays when there is no WebGL, and what returns if the 3D view fails.
 */

const W = 400;
const H = W / ASPECT;
const round = (n: number) => Math.round(n * 10) / 10;
const at = (p: Vec3) => project(p, W, H).map(round) as [number, number];

function pathThrough(points: Vec3[]): string {
  return points.map((p, i) => `${i === 0 ? "M" : "L"}${at(p).join(" ")}`).join("");
}

// Far streams are drawn fainter and thinner than near ones, as in the live view.
const streams = STREAMS.map((stream, index) => {
  const points = Array.from({ length: 41 }, (_, i) => onStream(stream, i / 40));
  const nearness = (stream.source[2] + 1) / 2; // 0 far, 1 near
  return {
    key: index,
    d: pathThrough(points),
    mix: Math.round(stream.hue * 100),
    opacity: round(0.42 + 0.4 * nearness),
    width: round(1 + 0.7 * nearness),
  };
});

const channel = pathThrough(Array.from({ length: 9 }, (_, i) => onChannel(STREAMS[3], i / 8)));
const [mergeX, mergeY] = at(MERGE);
const [tipX, tipY] = at(TIP);
const unit = at([1, TIP[1], 0])[0] - tipX; // viewBox units per scene unit near the arrow
const armY = TIP[1] + CHEVRON.height;
const chevron = pathThrough([[-CHEVRON.halfWidth, armY, 0], TIP, [CHEVRON.halfWidth, armY, 0]]);
const [barLeft, barCentre] = at([-BAR.halfLength - BAR.radius, BAR.y, 0]);
const barWidth = round(2 * (tipX - barLeft));
const barHeight = round(2 * BAR.radius * unit);

export function ConfluenceStatic({ className = "" }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="xMidYMid meet"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <radialGradient id="confluence-pool">
          <stop offset="0" style={{ stopColor: "var(--glow)", stopOpacity: 1 }} />
          <stop offset="1" style={{ stopColor: "var(--glow)", stopOpacity: 0 }} />
        </radialGradient>
      </defs>
      {/* The glow gathers where the streams meet and thins down the channel, as in the live view. */}
      <ellipse
        cx={mergeX}
        cy={round(mergeY + (tipY - mergeY) * 0.3)}
        rx={round(unit * 0.45)}
        ry={round((tipY - mergeY) * 0.62)}
        fill="url(#confluence-pool)"
      />
      <g fill="none" strokeLinecap="round" strokeLinejoin="round">
        {streams.map((s) => (
          <path
            key={s.key}
            d={s.d}
            vectorEffect="non-scaling-stroke"
            style={{
              stroke: `color-mix(in oklab, var(--current) ${s.mix}%, var(--estuary))`,
              strokeOpacity: s.opacity,
              strokeWidth: s.width,
            }}
          />
        ))}
        <path d={channel} vectorEffect="non-scaling-stroke" style={{ stroke: "var(--current)", strokeWidth: 2.4 }} />
        {/* As in the live view: light by night, a deep teal ink by day. Browsers without
            light-dark() keep the stroke attribute, the text colour. */}
        <path
          d={chevron}
          stroke="currentColor"
          style={{
            stroke: "light-dark(color-mix(in srgb, var(--foam) 60%, var(--current)), var(--foam))",
            strokeWidth: round(2 * CHEVRON.radius * unit),
            strokeOpacity: 0.92,
          }}
        />
      </g>
      <rect
        x={barLeft}
        y={round(barCentre - barHeight / 2)}
        width={barWidth}
        height={barHeight}
        rx={round(barHeight / 2)}
        style={{ fill: "var(--current)" }}
      />
    </svg>
  );
}
