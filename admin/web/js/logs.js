// Pure view model for a stack's logs tab (feat-ops-4). The dashboard's
// server asks Loki; the page picks the window, the app and a text to look
// for, and all three live in the address.

/**
 * @typedef {{ts_ms: number, source: string, stream: string, level: string,
 *   line: string}} LogLine
 */

/** The windows a person picks from, seconds. */
export const SINCE = /** @type {const} */ ([
  { value: "900", label: "15 min" },
  { value: "3600", label: "1 hour" },
  { value: "21600", label: "6 hours" },
  { value: "86400", label: "24 hours" },
  { value: "604800", label: "7 days" },
]);

/** The value of the app choice that means the stack's system journal. */
export const JOURNAL = "journal";

/**
 * The page's log settings from its query string.
 * @param {URLSearchParams} params
 */
export function logSettings(params) {
  /** @type {string} */
  const since =
    SINCE.find((s) => s.value === params.get("since"))?.value ?? "3600";
  const app = params.get("app") ?? "";
  const q = params.get("q") ?? "";
  return { since, app, q, follow: params.get("follow") === "1" };
}

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
 * A level as a badge tone.
 * @param {string} level
 * @returns {"ok" | "warn" | "bad" | ""}
 */
export function levelTone(level) {
  const l = canonicalLevel(level);
  if (l === "error" || l === "critical") return "bad";
  if (l === "warn") return "warn";
  if (l === "") return "";
  return "ok";
}

/**
 * The time of a line to the second, dd/mm HH:MM:SS (not the viewer's
 * locale, Kenny 2026-10-02 — same fixed order as `formatDateTime`, with
 * seconds added since a log line needs them).
 * @param {number} ms unix milliseconds
 * @param {import("./format.js").TimeOptions} [opts]
 */
export function lineTime(ms, opts = {}) {
  const parts = new Intl.DateTimeFormat(opts.locale, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
    timeZone: opts.timeZone,
  }).formatToParts(new Date(ms));
  const get = (/** @type {string} */ t) =>
    parts.find((p) => p.type === t)?.value ?? "";
  return `${get("day")}/${get("month")} ${get("hour")}:${get("minute")}:${get("second")}`;
}

/**
 * The app choices: every app the stack runs, and its journal.
 * @param {string[]} apps
 */
export function appChoices(apps) {
  return [
    { value: "", label: "Every app and the journal" },
    ...[...apps].sort().map((a) => ({ value: a, label: a })),
    { value: JOURNAL, label: "System journal" },
  ];
}

/**
 * The table's rows, newest first.
 * @param {LogLine[]} lines
 * @param {import("./format.js").TimeOptions} [opts]
 */
export function logRows(lines, opts) {
  return [...lines]
    .sort((a, b) => b.ts_ms - a.ts_ms)
    .map((l) => ({
      ms: l.ts_ms,
      time: lineTime(l.ts_ms, opts),
      source: l.source || "—",
      level: canonicalLevel(l.level) || "—",
      tone: levelTone(l.level),
      stream: l.stream,
      line: l.line,
    }));
}
