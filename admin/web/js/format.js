// Pure formatting helpers (ui-units): every duration and moment a person
// reads is in human units and in the viewer's own locale, never an ISO
// string or a bare count of seconds.

import {
  agoText as kpAgoText,
  humanDuration as kpHumanDuration,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * A span of seconds in the unit a person reads best, kp-themes' own
 * `humanDuration` (freshness.js: seconds below a minute, minutes and seconds
 * below an hour, hours and minutes below a day, days and hours above; a zero
 * second part is left out, "2 min"); a span that is not a number, or
 * negative, reads as a dash.
 * @param {number | null | undefined} seconds
 * @returns {string}
 */
export function humanDuration(seconds) {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "—";
  return kpHumanDuration(seconds);
}

/**
 * @typedef {{locale?: string, timeZone?: string}} TimeOptions
 * `locale` and `timeZone` default to the viewer's; tests pin both.
 */

/**
 * The zone every moment is written in (Kenny, 2026-10-02, REGISTER
 * fix-216): Europe/Brussels, not the viewer's own, so a moment reads the
 * same on every screen it is driven from.
 */
export const ZONE = "Europe/Brussels";

/**
 * The wall-clock parts of a unix moment in `timeZone` (Europe/Brussels by
 * default), read with numeric fields only so no locale reorders or names
 * them.
 * @param {number} unix seconds
 * @param {string} [timeZone]
 */
function wall(unix, timeZone = ZONE) {
  /** @type {Record<string, string>} */
  const p = {};
  for (const x of new Intl.DateTimeFormat("en-GB", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
    timeZone,
  }).formatToParts(new Date(unix * 1000)))
    p[x.type] = x.value;
  return {
    dd: p.day.padStart(2, "0"),
    mm: p.month.padStart(2, "0"),
    yyyy: p.year,
    hh: p.hour.padStart(2, "0").replace("24", "00"),
    mi: p.minute.padStart(2, "0"),
    ss: p.second.padStart(2, "0"),
  };
}

/**
 * @typedef {TimeOptions & {seconds?: boolean}} DateOptions
 * `seconds` adds them to the clock (a log line needs them).
 */

/**
 * The day of a moment as every page writes it: dd/mm/yyyy (Kenny,
 * 2026-10-02, REGISTER fix-216: a fixed field width a column aligns on,
 * 24-hour, Europe/Brussels; never the locale's order, never a weekday or a
 * month's name). redesign-final X4: one date format on every page, from
 * here; `formatDateTime` adds the clock, `formatClock` is the clock alone.
 * A guard test (finalreview_dates.test.js) refuses a date formatter
 * anywhere else.
 * @param {number | null | undefined} unix seconds
 * @param {DateOptions} [opts]
 * @returns {string}
 */
export function formatDay(unix, opts = {}) {
  if (unix == null || !Number.isFinite(unix)) return "—";
  const w = wall(unix, opts.timeZone);
  return `${w.dd}/${w.mm}/${w.yyyy}`;
}

/**
 * A moment's clock, 24-hour: "00:00" (or "00:00:05" with `seconds`).
 * @param {number | null | undefined} unix seconds
 * @param {DateOptions} [opts]
 * @returns {string}
 */
export function formatClock(unix, opts = {}) {
  if (unix == null || !Number.isFinite(unix)) return "—";
  const w = wall(unix, opts.timeZone);
  return `${w.hh}:${w.mi}${opts.seconds ? `:${w.ss}` : ""}`;
}

/**
 * A moment as every page writes it: "03/10/2026 00:00" (dd/mm/yyyy HH:MM,
 * 24-hour, Europe/Brussels; Kenny's rule, REGISTER fix-216).
 * @param {number | null | undefined} unix seconds
 * @param {DateOptions} [opts]
 * @returns {string}
 */
export function formatDateTime(unix, opts = {}) {
  if (unix == null || !Number.isFinite(unix) || unix <= 0) return "—";
  return `${formatDay(unix, opts)} ${formatClock(unix, opts)}`;
}

/**
 * "measured 12 s ago", "read 2 min 5 s ago" (feat-overview-4): how old a
 * reading is, in the words of kp-themes' freshness line (`agoText`). A
 * moment in the future (a clock ahead of this one) reads as 0 s.
 * @param {string} verb what happened at `at` ("measured", "read")
 * @param {number | null | undefined} at unix seconds
 * @param {number} now unix seconds
 * @returns {string}
 */
export function agoText(verb, at, now) {
  return kpAgoText(verb, at == null ? null : at * 1000, now * 1000);
}

/**
 * root-disk-decimals-1 (Kenny, 2026-10-22: "81.39999999999999 GB / 96 GB"):
 * a figure a person reads, rounded to at most `decimals` places (2 by
 * default) with no trailing zeros: 81.4, 96, 0.25. The one formatter every
 * bar, tile and sentence puts a raw number through; a sum of floats never
 * reaches the screen with its tail. Not a number reads as a dash.
 * @param {number | null | undefined} n
 * @param {number} [decimals]
 * @returns {string}
 */
export function figure(n, decimals = 2) {
  if (n == null || !Number.isFinite(n)) return "—";
  const f = 10 ** decimals;
  const t = (Math.round((n + Number.EPSILON) * f) / f).toFixed(decimals);
  const out = t.includes(".") ? t.replace(/\.?0+$/, "") : t;
  return out === "-0" ? "0" : out;
}

/**
 * A size in gigabytes as a person reads it: `81.4 GB`.
 * @param {number | null | undefined} n
 */
export const gbText = (n) => (n == null ? "—" : `${figure(n)} GB`);
