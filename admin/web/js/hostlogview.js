// redesign-console (3.71.0, the Console demo Kenny approved on 2026-10-03):
// the pure half of the host-log explorer that Console and Activity's Host
// log share. Which lines a filter shows (sources turned off one by one,
// "only" one source, a minimum level, text), the counts the side column
// prints beside each value, and the filter's place in the address.

import {
  DEFAULT_LOG_LEVELS,
  levelBucket,
  levelShown,
  readLogLevels,
} from "./logs.js";

/**
 * @typedef {import("./parity.js").HostLine} HostLine
 * @typedef {{off: Set<string>, only: string | null,
 *   levels: Set<string>, q: string}} LogFilter
 *   `off`: sources turned off; `only`: one source alone (an old
 *   `/log?source=media` link, or "only" beside a source); `levels`
 *   (feat-logs-1): the levels shown, any mix of debug, info, warning and
 *   critical, remembered between visits.
 */

/** The Level choices, in their fixed order (feat-logs-1: several at once). */
export const LEVELS = /** @type {const} */ ([
  { value: "debug", label: "Debug", dot: "info" },
  { value: "info", label: "Info", dot: "info" },
  { value: "warning", label: "Warning", dot: "warn" },
  { value: "critical", label: "Critical", dot: "bad" },
]);

/**
 * Whether a source's lines are shown.
 * @param {string} source
 * @param {LogFilter} f
 */
export const sourceOn = (source, f) =>
  f.only != null ? source === f.only : !f.off.has(source);

/**
 * Whether one line passes the filter. The text matches the source or the
 * message, any case.
 * @param {HostLine} l
 * @param {LogFilter} f
 */
export function lineShown(l, f) {
  if (!sourceOn(l.source, f)) return false;
  if (!levelShown(l.level, f.levels)) return false;
  const q = f.q.trim().toLowerCase();
  if (q && !`${l.source} ${l.msg}`.toLowerCase().includes(q)) return false;
  return true;
}

/**
 * Every source the side column lists: the host's own first, then the
 * fleet's stacks and any other source a line named, sorted.
 * @param {string[]} fleet
 * @param {Iterable<string>} seen
 */
export function sourceList(fleet, seen) {
  const all = new Set([...fleet, ...seen]);
  const host = [...all].filter((s) => s === "HOST");
  const rest = [...all].filter((s) => s && s !== "HOST").sort();
  return [...host, ...rest];
}

/**
 * How many lines each source, and each level choice, holds (the side
 * column's counts: never capped, invariant 60).
 * @param {HostLine[]} lines
 */
export function counts(lines) {
  /** @type {Map<string, number>} */
  const bySource = new Map();
  /** @type {Record<string, number>} */
  const byLevel = { debug: 0, info: 0, warning: 0, critical: 0 };
  for (const l of lines) {
    bySource.set(l.source, (bySource.get(l.source) ?? 0) + 1);
    byLevel[levelBucket(l.level)] += 1;
  }
  return { bySource, byLevel };
}

/**
 * Whether anything narrows the view.
 * @param {LogFilter} f
 */
export const narrowed = (f) =>
  f.only != null ||
  f.off.size > 0 ||
  !sameLevels(f.levels, DEFAULT_LOG_LEVELS) ||
  f.q.trim() !== "";

/**
 * @param {ReadonlySet<string>} a
 * @param {Iterable<string>} b
 */
const sameLevels = (a, b) => {
  const set = new Set(b);
  return a.size === set.size && [...a].every((x) => set.has(x));
};

/** The levels a filter starts with: everything but debug. */
export const defaultLevels = () => new Set(DEFAULT_LOG_LEVELS);

/**
 * The filter from the address: `?source=` (one source alone, as the old
 * Live log's links), `?hide=a,b`, `?level=warn|error`, `?q=`.
 * @param {URLSearchParams} p
 * @param {Set<string>} [remembered] the levels remembered between visits
 * @returns {LogFilter}
 */
export function logFilterFromParams(p, remembered = readLogLevels()) {
  // An old `?level=warn|error` link means "at least this severe"; without
  // one the levels remembered between visits apply.
  const level = p.get("level");
  const atLeast =
    level === "warn"
      ? ["warning", "critical"]
      : level === "error"
        ? ["critical"]
        : null;
  return {
    only: p.get("source") || null,
    off: new Set((p.get("hide") ?? "").split(",").filter(Boolean)),
    levels: atLeast ? new Set(atLeast) : new Set(remembered),
    q: p.get("q") ?? "",
  };
}

/**
 * @param {LogFilter} f
 * @returns {Record<string, string | null>}
 */
export const logFilterToParams = (f) => ({
  source: f.only,
  hide: f.only == null && f.off.size ? [...f.off].sort().join(",") : null,
  level: null,
  q: f.q.trim() || null,
});

/**
 * The host's own source ("HOST") reads as "the host"; a stack keeps its
 * name.
 * @param {string} s
 */
export const sourceLabel = (s) => (s === "HOST" ? "the host" : s);

/**
 * The page's lines from the snapshot and the live lines that arrived
 * around it: one line per number, in number order, the newest `keep`
 * kept (senior review, finding 1: a live line that came before the
 * snapshot used to hide every older snapshot line).
 * @param {HostLine[]} a
 * @param {HostLine[]} b
 * @param {number} keep
 * @returns {HostLine[]}
 */
export function mergeLines(a, b, keep) {
  /** @type {Map<number, HostLine>} */
  const by = new Map();
  for (const l of a) by.set(l.seq, l);
  for (const l of b) by.set(l.seq, l);
  const all = [...by.values()].sort((x, y) => x.seq - y.seq);
  return all.length > keep ? all.slice(all.length - keep) : all;
}

// ── feat-kpadopt-2 (kp-themes 11.0.0, the approved app-homelab-backups
// demo, feat-kp11demo-7): where in the 2,000-line window things happened.
// The strip and the grid of sources against five-minute columns are drawn
// from these buckets; a press on a column narrows the lines to its five
// minutes. Pure.

/** Seconds in one column of the density strip and grid. */
export const BUCKET_S = 300;

/** @typedef {"critical" | "warning" | "info" | "none"} Worst */
/** @type {Record<Worst, number>} */
const RANK = { none: 0, info: 1, warning: 2, critical: 3 };

/** The start of the five minutes a moment falls in. @param {number} ts */
export const bucketStart = (ts) => Math.floor(ts / BUCKET_S) * BUCKET_S;

/**
 * Whether a line falls in a five-minute range (`[start, start + 300)`).
 * @param {HostLine} l
 * @param {number | null} start
 */
export const inRange = (l, start) =>
  start == null || (l.ts >= start && l.ts < start + BUCKET_S);

/**
 * Every five minutes from the first line to the last, with the worst level
 * logged in them, overall and per source, and the number of the first line
 * (to jump to). Quiet five minutes are kept: a gap is information.
 * @param {HostLine[]} lines oldest first
 * @returns {{start: number, count: number, worst: Worst, first: number | null,
 *   bySource: Map<string, {count: number, worst: Worst}>}[]}
 */
export function densityBuckets(lines) {
  if (!lines.length) return [];
  const from = bucketStart(lines[0].ts);
  const to = bucketStart(lines[lines.length - 1].ts);
  /** @type {Map<number, {start: number, count: number, worst: Worst, first: number | null, bySource: Map<string, {count: number, worst: Worst}>}>} */
  const by = new Map();
  for (let t = from; t <= to; t += BUCKET_S)
    by.set(t, {
      start: t,
      count: 0,
      worst: "none",
      first: null,
      bySource: new Map(),
    });
  for (const l of lines) {
    const b = by.get(bucketStart(l.ts));
    if (!b) continue;
    const lv = levelBucket(l.level);
    const level = /** @type {Worst} */ (lv === "debug" ? "info" : lv);
    b.count++;
    b.first ??= l.seq;
    if (RANK[level] > RANK[b.worst]) b.worst = level;
    const s = b.bySource.get(l.source) ?? {
      count: 0,
      worst: /** @type {Worst} */ ("none"),
    };
    s.count++;
    if (RANK[level] > RANK[s.worst]) s.worst = level;
    b.bySource.set(l.source, s);
  }
  return [...by.values()];
}
