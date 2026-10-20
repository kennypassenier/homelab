// Pure view model for a stack's logs tab (feat-ops-4). The dashboard's
// server asks Loki; the page picks the window, the app and a text to look
// for, and all three live in the address.

import { formatDateTime } from "./format.js";

/**
 * @typedef {{ts_ms: number, source: string, stream: string, level: string,
 *   line: string}} LogLine
 */

/**
 * The route to ask.
 * @param {string} stack
 * @param {{since: string, app: string, q: string}} s
 * @param {number} [limit]
 */
export function logsUrl(stack, s, limit = 1000) {
  const p = new URLSearchParams({
    stack,
    since: s.since,
    limit: String(limit),
  });
  if (s.app) p.set("app", s.app);
  if (s.q) p.set("q", s.q);
  return `/data/logs?${p}`;
}

/** Spellings of one level that sources use interchangeably (syslog's
 * "informational", Go's "warning", "err"), folded to one name so a table or
 * a filter never shows the same level twice (Kenny, 2026-10-01). */
/** @type {Record<string, string>} */
const LEVEL_ALIASES = {
  information: "info",
  informational: "info",
  inf: "info",
  notice: "info",
  warning: "warn",
  wrn: "warn",
  err: "error",
  eror: "error",
  crit: "critical",
  fatal: "critical",
  alert: "critical",
  emerg: "critical",
  emergency: "critical",
  panic: "critical",
  dbg: "debug",
  trc: "trace",
};

/**
 * One canonical lower-case name per level; empty stays empty.
 * @param {string | null | undefined} level
 * @returns {string}
 */
export function canonicalLevel(level) {
  const l = (level ?? "").trim().toLowerCase();
  return LEVEL_ALIASES[l] ?? l;
}

/**
 * The time of a line to the second: "Sat 3 Oct, 14:43:05" (redesign-final
 * X4: one date format, with seconds since a log line needs them).
 * @param {number} ms unix milliseconds
 * @param {import("./format.js").TimeOptions} [opts]
 */
export function lineTime(ms, opts = {}) {
  return formatDateTime(ms / 1000, { ...opts, seconds: true });
}

// ── feat-logs-1: the level filter and the copy text every log shares ────

/** The four levels a log can be filtered by, in their fixed order. */
export const LOG_LEVELS = /** @type {const} */ ([
  { value: "debug", label: "Debug" },
  { value: "info", label: "Info" },
  { value: "warning", label: "Warning" },
  { value: "critical", label: "Critical" },
]);

/** @typedef {"debug" | "info" | "warning" | "critical"} LogLevel */

/** What is on until a person chooses otherwise: everything but debug. */
export const DEFAULT_LOG_LEVELS = /** @type {readonly LogLevel[]} */ ([
  "info",
  "warning",
  "critical",
]);

/**
 * Which of the four a source's level falls under: trace and debug are
 * debug, warn is warning, error, fatal and the rest of the severe
 * spellings are critical, everything else (or no level) is info.
 * @param {string | null | undefined} level
 * @returns {LogLevel}
 */
export function levelBucket(level) {
  switch (canonicalLevel(level)) {
    case "debug":
    case "trace":
      return "debug";
    case "warn":
      return "warning";
    case "error":
    case "critical":
      return "critical";
    default:
      return "info";
  }
}

/** Where the chosen levels are remembered (one choice for every log). */
export const LOG_LEVELS_KEY = "homelab.log-levels";

/**
 * The remembered levels, or the defaults.
 * @param {Pick<Storage, "getItem"> | undefined} [store]
 * @returns {Set<LogLevel>}
 */
export function readLogLevels(store = globalThis.localStorage) {
  try {
    const raw = JSON.parse(store?.getItem(LOG_LEVELS_KEY) ?? "null");
    if (Array.isArray(raw)) {
      const ok = LOG_LEVELS.map((l) => l.value).filter((v) => raw.includes(v));
      return new Set(ok);
    }
  } catch {
    // An unreadable memory is no memory.
  }
  return new Set(DEFAULT_LOG_LEVELS);
}

/**
 * Remember the chosen levels.
 * @param {Iterable<string>} levels
 * @param {Pick<Storage, "setItem"> | undefined} [store]
 */
export function saveLogLevels(levels, store = globalThis.localStorage) {
  try {
    store?.setItem(LOG_LEVELS_KEY, JSON.stringify([...levels]));
  } catch {
    // Private mode: the choice lasts for the page.
  }
}

/**
 * Whether the level filter shows a line of this level.
 * @param {string | null | undefined} level
 * @param {ReadonlySet<string>} chosen
 */
export const levelShown = (level, chosen) => chosen.has(levelBucket(level));

/**
 * The lines `GET /data/actions/jobs/{job}/log` answered, whatever shape
 * (`{lines}`, a bare array); null when it is neither.
 * @param {any} body
 * @returns {import("./jobs.js").LogLine[] | null}
 */
export function wholeLogLines(body) {
  const lines = Array.isArray(body) ? body : body?.lines;
  return Array.isArray(lines) ? lines : null;
}
