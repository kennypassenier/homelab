// ask-kp11be-2: the pure half of every uptime strip the dashboard draws from
// the watch history (`/data/uptime`) and the SMART history
// (`/data/drive-history`): Brussels day edges for the request, and the
// server's buckets as kp's strip periods. No DOM, so `node --test` drives it.

import { ZONE, formatClock, formatDay } from "./format.js";
import { uptimePercent } from "/static/kp/dist/kp-themes.min.js";

/**
 * Seconds Brussels is ahead of UTC at `unix` (the request's `offset_s`, so a
 * day bucket is the reader's day as every page writes it).
 * @param {number} unix
 */
export function zoneOffsetS(unix) {
  const wall = new Date(unix * 1000)
    .toLocaleString("sv-SE", { timeZone: ZONE })
    .replace(" ", "T");
  return Math.round((Date.parse(`${wall}Z`) - unix * 1000) / 1000);
}

/** The query for a 90-day strip of daily periods, or a 24 h strip of hourly. @param {"day" | "hour"} unit @param {number} now */
export function uptimeQuery(unit, now) {
  const hours = unit === "day" ? 24 * 90 : 24;
  return `hours=${hours}&bucket_s=${unit === "day" ? 86400 : 3600}&offset_s=${zoneOffsetS(now)}`;
}

/**
 * @typedef {{start: number, state: "up" | "degraded" | "down" | "none",
 *   uptime: number | null, up?: number, dep?: number, down?: number,
 *   ms_avg?: number | null, ms_max?: number | null}} Bucket
 * @typedef {import("/static/kp/dist/kp-themes.min.js").UptimePeriod} UptimePeriod
 */

/**
 * One server bucket as a strip cell. A day reads dd/mm/yyyy, an hour the
 * day and clock; the tip says what was measured, in whole numbers.
 * @param {Bucket} b @param {"day" | "hour"} unit @param {string} [what]
 * @returns {UptimePeriod}
 */
export function periodOf(b, unit, what = "answered") {
  const label =
    unit === "day"
      ? formatDay(b.start + 43200)
      : `${formatDay(b.start)} ${formatClock(b.start)}`;
  const total = (b.up ?? 0) + (b.dep ?? 0) + (b.down ?? 0);
  /** @type {string[]} */
  const details = [];
  if (b.state === "none") details.push("No reading in this period.");
  else {
    if (b.down) details.push(`${b.down} of ${total} readings had no answer.`);
    if (b.dep) details.push(`${b.dep} readings fell in a deploy.`);
    if (b.ms_avg != null)
      details.push(
        `Round trip ${b.ms_avg} ms on average, ${b.ms_max} ms at most.`,
      );
  }
  return {
    key: new Date(b.start * 1000).toISOString(),
    label,
    state: b.state,
    ...(b.uptime != null ? { uptime: b.uptime } : {}),
    tip:
      b.state === "none"
        ? `${label}: no reading`
        : `${label}: ${what} ${uptimePercent(b.uptime ?? 0)} % of ${total} readings`,
    details,
  };
}

/**
 * A key's periods, oldest first.
 * @param {{periods?: Bucket[]} | undefined} entry @param {"day" | "hour"} unit
 * @param {string} [what]
 * @returns {UptimePeriod[]}
 */
export function periodsOf(entry, unit, what) {
  return (entry?.periods ?? []).map((b) => periodOf(b, unit, what));
}

/**
 * The watch key an app tile's strip is read from: its own probe, else its
 * stack's container (the same fallback the tile's state word uses).
 * @param {Map<string, unknown>} keys @param {{host: string, stack: string}} t
 */
export function tileKey(keys, t) {
  return keys.has(`tile:${t.host}`) ? `tile:${t.host}` : `stack:${t.stack}`;
}

/**
 * "99.8 % over 90 days" or "no readings yet".
 * @param {number | null | undefined} share @param {string} window
 */
export function shareText(share, window) {
  return share == null
    ? "no readings yet"
    : `${uptimePercent(share)} % over ${window}`;
}

/**
 * The history a tile draws: its own probe's buckets, and where that probe
 * measured nothing (it is younger than the strip) the buckets of its stack's
 * container, the same fallback the tile's state word uses. Both answers were
 * asked with the same edges, so a bucket lines up by its start. Without this
 * a tile whose own key is a day old showed 89 days of "no reading" while its
 * stack had months of cells.
 * @param {Map<string, {periods?: Bucket[], uptime?: number | null}>} keys
 * @param {{host: string, stack: string}} t
 * @returns {{periods: Bucket[], uptime: number | null} | undefined}
 */
export function tileEntry(keys, t) {
  const own = keys.get(`tile:${t.host}`);
  const stack = keys.get(`stack:${t.stack}`);
  if (!own || !stack) {
    const only = own ?? stack;
    return only
      ? { periods: only.periods ?? [], uptime: only.uptime ?? null }
      : undefined;
  }
  const byStart = new Map((stack.periods ?? []).map((b) => [b.start, b]));
  const periods = (own.periods ?? []).map((b) =>
    b.state === "none" ? (byStart.get(b.start) ?? b) : b,
  );
  let up = 0;
  let total = 0;
  for (const b of periods) {
    const n = (b.up ?? 0) + (b.dep ?? 0) + (b.down ?? 0);
    up += b.up ?? 0;
    total += n;
  }
  return { periods, uptime: total > 0 ? up / total : null };
}
