/**
 * The arrival at the download button, shared by the live scene and the drawing that stands in
 * for it. Units are the outlet's own: 1 unit is 1/16 rem (a CSS pixel at the default text
 * size), x to the right and y down from where the page's channel enters, at the left edge of
 * the download column. The scene's camera looks straight at this plane, so the live view and
 * the drawing line up exactly at every zoom.
 *
 * The channel opens into one lane per connection, in the icon's curve, and each lane builds its
 * share of the file: a slim block of pieces, seen a little from above, resting on the button.
 *
 * Nothing here imports three.js, so the drawing costs no 3D code.
 */

/** The room above the download button. Keep in step with the outlet's lg:pt-[16rem]. */
export const OUTLET = { width: 320, height: 256 } as const;
/** The button below it (min-h-13, 3.25rem), so the water's level can be placed in the column. */
export const BUTTON_HEIGHT = 52;
/** The live view reaches this far past the room on every side, for the glow and the spray. */
export const MARGIN = 24;

/** One lane per connection. */
export const LANES = 6;

/**
 * The file: a block of pieces (columns across, rows up, two deep), tilted back so its top
 * shows. Width and depth in units; centre is where its middle sits in the room.
 */
export const BLOCK = {
  cols: 30,
  rows: 3,
  deep: 2,
  width: OUTLET.width,
  height: 30,
  depth: 24,
  tilt: 0.68,
  centre: [OUTLET.width / 2, 222] as const,
} as const;

const cos = Math.cos(BLOCK.tilt);
const sin = Math.sin(BLOCK.tilt);

/** Where a point of the block (its own axes, y up, z towards the reader) lands in the room. */
export function onBlock(x: number, y: number, z: number): [number, number] {
  return [BLOCK.centre[0] + x, BLOCK.centre[1] - (y * cos - z * sin)];
}

const halfH = BLOCK.height / 2;
const halfD = BLOCK.depth / 2;

/** The block's two visible faces in the room, as top and bottom edges (y down). */
export const FACES = {
  top: { y0: onBlock(0, halfH, -halfD)[1], y1: onBlock(0, halfH, halfD)[1] },
  front: { y0: onBlock(0, halfH, halfD)[1], y1: onBlock(0, -halfH, halfD)[1] },
} as const;

/** Each lane lands on the middle of the block's top face, over the middle of its share. */
const LAND_Y = onBlock(0, halfH, 0)[1];
const CONTROL = 0.54; // the icon's control points, 54% of the way down each curve

export type Lane = { x: number; landY: number; controlY: number; hue: number; length: number };

/** The lanes' colours run from the estuary blue to the current teal, as in the hero. */
const HUES = [0.15, 0.62, 0.32, 0.92, 0.48, 0.78];

function cubic(a: number, b: number, c: number, d: number, t: number) {
  const s = 1 - t;
  return s * s * s * a + 3 * s * s * t * b + 3 * s * t * t * c + t * t * t * d;
}

/** A point down lane `lane`, t from 0 (the channel) to 1 (the block's top). */
export function onLane(lane: Lane, t: number): [number, number] {
  return [cubic(0, 0, lane.x, lane.x, t), cubic(0, lane.controlY, lane.controlY, lane.landY, t)];
}

export const LANE_LIST: readonly Lane[] = Array.from({ length: LANES }, (_, i) => {
  const lane = { x: ((i + 0.5) * BLOCK.width) / LANES, landY: LAND_Y, controlY: LAND_Y * CONTROL, hue: HUES[i], length: 0 };
  let length = 0;
  let last = onLane(lane, 0);
  for (let k = 1; k <= 48; k += 1) {
    const next = onLane(lane, k / 48);
    length += Math.hypot(next[0] - last[0], next[1] - last[1]);
    last = next;
  }
  return { ...lane, length };
});

/** The lane as an SVG path in the room's viewBox. */
export function lanePath(lane: Lane): string {
  const r = (n: number) => Math.round(n * 100) / 100;
  return `M0 0C0 ${r(lane.controlY)} ${r(lane.x)} ${r(lane.controlY)} ${r(lane.x)} ${r(lane.landY)}`;
}

/** Where the water is along the column as the reader scrolls: lanes first, then the block. */
const COLUMN = OUTLET.height + BUTTON_HEIGHT;
export const LANE_SPAN: [number, number] = [0, LAND_Y / COLUMN];
export const BLOCK_SPAN: [number, number] = [FACES.top.y0 / COLUMN, FACES.front.y1 / COLUMN];
