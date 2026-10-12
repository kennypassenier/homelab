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
 * Turn one source on or off (a plain click; several may be off). An
 * "only" filter becomes "everything else off" first, so the click means
 * what it shows.
 * @param {LogFilter} f
 * @param {string} source
 * @param {string[]} sources every source known
 * @returns {LogFilter}
 */
export function toggleSource(f, source, sources) {
  const off =
    f.only != null
      ? new Set(sources.filter((s) => s !== f.only))
      : new Set(f.off);
  if (off.has(source)) off.delete(source);
  else off.add(source);
  return { ...f, off, only: null };
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
 * The words of the count in the toolbar's right zone.
 * @param {number} shown
 * @param {number} total
 */
export const countText = (shown, total) =>
  `${shown} of ${total} ${total === 1 ? "line" : "lines"}`;

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

/**
 * What a key does on the explorer: "/" to the search box, Space pauses
 * or resumes, End back to the tail. Nothing while typing, and Space and
 * End leave a focused button or link alone (anchored: MAIN or LABEL are
 * not buttons; senior review, finding 12).
 * @param {string} key
 * @param {string} tag the focused element's tag name
 * @param {string | null} role its role attribute
 * @returns {"search" | "pause" | "tail" | null}
 */
export function keyAction(key, tag, role) {
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(tag);
  if (key === "/") return typing ? null : "search";
  if (typing || /^(BUTTON|A)$/.test(tag) || role === "button") return null;
  if (key === " ") return "pause";
  if (key === "End") return "tail";
  return null;
}
