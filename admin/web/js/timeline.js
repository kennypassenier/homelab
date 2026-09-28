// Pure layout for the timeline (feat-ops-7): the host's operations, the
// nightly round's phases and the incident bundles on one time axis. No
// chart library (tech-charts): this computes positions, the page draws SVG
// coloured with kp-themes tokens.

import { entryOutcome, entryWhat, parseIncident } from "./activity.js";
import { formatTime, humanDuration } from "./format.js";

export const GUTTER = 118;
export const RIGHT = 12;
export const ROW = 18;
export const ROW_GAP = 4;
export const LANE_GAP = 14;
export const AXIS = 26;
/** Operations that overlap stack into at most this many rows. */
export const MAX_ROWS = 4;

/** The spans the axis can step by, seconds. */
const STEPS = [
  3600,
  3 * 3600,
  6 * 3600,
  12 * 3600,
  86400,
  2 * 86400,
  7 * 86400,
];

/**
 * @typedef {{kind: "op" | "phase" | "incident", x: number, y: number,
 *   w: number, h: number, tone: "ok" | "warn" | "bad" | "info",
 *   label: string, title: string, start: number}} Mark
 * @typedef {{label: string, y: number, h: number}} Lane
 * @typedef {{x: number, label: string, major: boolean}} Tick
 * @typedef {{width: number, height: number, lanes: Lane[], marks: Mark[],
 *   ticks: Tick[], counts: {ops: number, phases: number, incidents: number}}} Timeline
 * @typedef {{locale?: string, timeZone?: string,
 *   offset?: (unix: number) => number}} TimelineOptions
 *   offset: seconds east of UTC at a moment (default: the viewer's zone)
 */

/**
 * Still going: the host wrote a start and no end (the same rule as the
 * activity page's outcome).
 * @param {{start: number, end: number}} e
 */
const running = (e) => e.end === 0 || e.end < e.start;

/** @param {number} unix */
const localOffset = (unix) => -new Date(unix * 1000).getTimezoneOffset() * 60;

/**
 * Pack spans into rows so no two in a row overlap; the last row takes
 * whatever does not fit.
 * @param {{a: number, b: number}[]} spans sorted by start
 * @param {number} max
 * @returns {number[]} the row of each span
 */
export function packRows(spans, max) {
  /** @type {number[]} */
  const ends = [];
  return spans.map((s) => {
    let row = ends.findIndex((e) => e <= s.a);
    if (row === -1) row = ends.length < max ? ends.length : max - 1;
    ends[row] = Math.max(ends[row] ?? 0, s.b);
    return row;
  });
}

/**
 * The axis ticks between `from` and `to`, on local whole hours or days.
 * @param {number} from
 * @param {number} to
 * @param {number} px the axis length
 * @param {TimelineOptions} opts
 * @returns {{t: number, major: boolean, label: string}[]}
 */
export function ticks(from, to, px, opts = {}) {
  const offset = opts.offset ?? localOffset;
  const most = Math.max(2, Math.floor(px / 80));
  const step =
    STEPS.find((s) => (to - from) / s <= most) ?? STEPS[STEPS.length - 1];
  const day = new Intl.DateTimeFormat(opts.locale, {
    month: "short",
    day: "numeric",
    timeZone: opts.timeZone,
  });
  const hour = new Intl.DateTimeFormat(opts.locale, {
    hour: "2-digit",
    minute: "2-digit",
    timeZone: opts.timeZone,
  });
  const out = [];
  const o = offset(from);
  let t = Math.ceil((from + o) / step) * step - o;
  for (; t <= to; t += step) {
    const local = t + offset(t);
    const major = local % 86400 === 0;
    out.push({
      t,
      major,
      label: (major ? day : hour).format(new Date(t * 1000)),
    });
  }
  return out;
}

/**
 * @param {{entries: import("./activity.js").Entry[], incidents: string[],
 *   from: number, to: number, width: number}} input
 * @param {TimelineOptions} [opts]
 * @returns {Timeline}
 */
export function timelineModel(input, opts = {}) {
  const { from, to } = input;
  const width = Math.max(input.width, GUTTER + RIGHT + 100);
  const span = Math.max(1, to - from);
  const px = width - GUTTER - RIGHT;
  const x = (/** @type {number} */ t) =>
    GUTTER + ((Math.min(Math.max(t, from), to) - from) / span) * px;
  const timeOpts = { locale: opts.locale, timeZone: opts.timeZone };

  const ops = input.entries
    .filter((e) => e && e.kind === "op")
    .map((e) => ({ e, a: e.start, b: running(e) ? to : e.end }))
    .filter((s) => s.b >= from && s.a <= to)
    .sort((p, q) => p.a - q.a);
  const phases = input.entries
    .filter((e) => e && e.kind === "phase")
    .map((e) => ({ e, a: e.start, b: running(e) ? to : e.end }))
    .filter((s) => s.b >= from && s.a <= to)
    .sort((p, q) => p.a - q.a);
  const incidents = input.incidents
    .map((name) => ({ name, ...parseIncident(name) }))
    .filter((i) => i.at != null && i.at >= from && i.at <= to);

  const opRows = packRows(ops, MAX_ROWS);
  const opLaneRows = Math.max(1, ...opRows.map((r) => r + 1));
  const phaseRows = packRows(phases, 2);
  const phaseLaneRows = Math.max(1, ...phaseRows.map((r) => r + 1));

  /** @type {Lane[]} */
  const lanes = [];
  let y = 6;
  const lane = (/** @type {string} */ label, /** @type {number} */ rows) => {
    const h = rows * ROW + (rows - 1) * ROW_GAP;
    lanes.push({ label, y, h });
    const top = y;
    y += h + LANE_GAP;
    return top;
  };
  const opTop = lane("Operations", opLaneRows);
  const phaseTop = lane("Nightly round", phaseLaneRows);
  const incTop = lane("Incidents", 1);

  /** @type {Mark[]} */
  const marks = [];
  ops.forEach((s, i) => {
    const o = entryOutcome(s.e);
    const took = !running(s.e)
      ? humanDuration(s.e.end - s.e.start)
      : "still running";
    const x0 = x(s.a);
    marks.push({
      kind: "op",
      x: x0,
      y: opTop + opRows[i] * (ROW + ROW_GAP),
      w: Math.max(3, x(s.b) - x0),
      h: ROW,
      tone: o.tone,
      label: entryWhat(s.e),
      title: `${entryWhat(s.e)} · ${o.label} · ${formatTime(s.e.start, timeOpts)} · ${took}`,
      start: s.e.start,
    });
  });
  phases.forEach((s, i) => {
    const x0 = x(s.a);
    marks.push({
      kind: "phase",
      x: x0,
      y: phaseTop + phaseRows[i] * (ROW + ROW_GAP),
      w: Math.max(3, x(s.b) - x0),
      h: ROW,
      tone: "info",
      label: entryWhat(s.e),
      title: `${entryWhat(s.e)} · ${formatTime(s.e.start, timeOpts)} · ${humanDuration(s.b - s.a)}`,
      start: s.e.start,
    });
  });
  for (const i of incidents) {
    const at = /** @type {number} */ (i.at);
    marks.push({
      kind: "incident",
      x: x(at),
      y: incTop,
      w: 0,
      h: ROW,
      tone: "bad",
      label: i.op,
      title: `incident: ${i.op} · ${formatTime(at, timeOpts)}`,
      start: at,
    });
  }

  const height = y - LANE_GAP + AXIS;
  return {
    width,
    height,
    lanes,
    marks,
    ticks: ticks(from, to, px, opts).map((t) => ({
      x: x(t.t),
      label: t.label,
      major: t.major,
    })),
    counts: {
      ops: ops.length,
      phases: phases.length,
      incidents: incidents.length,
    },
  };
}
