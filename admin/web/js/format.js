// Pure formatting helpers (ui-units): every duration and moment a person
// reads is in human units and in the viewer's own locale, never an ISO
// string or a bare count of seconds.

/**
 * @param {number} n
 * @param {string} one
 * @param {string} many
 */
const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

/**
 * A span of seconds in the unit a person reads best: seconds below a
 * minute, minutes and seconds below an hour, hours and minutes below a day,
 * days and hours above. A zero second part is left out ("2 min").
 * @param {number | null | undefined} seconds
 * @returns {string}
 */
export function humanDuration(seconds) {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "—";
  const d = Math.round(seconds);
  if (d < 60) return `${d} s`;
  if (d < 3600) {
    const s = d % 60;
    return s === 0
      ? `${Math.floor(d / 60)} min`
      : `${Math.floor(d / 60)} min ${s} s`;
  }
  if (d < 86400) {
    const m = Math.floor((d % 3600) / 60);
    return m === 0
      ? `${Math.floor(d / 3600)} h`
      : `${Math.floor(d / 3600)} h ${m} min`;
  }
  const days = Math.floor(d / 86400);
  const h = Math.floor((d % 86400) / 3600);
  return h === 0
    ? plural(days, "day", "days")
    : `${plural(days, "day", "days")} ${h} h`;
}

/**
 * @typedef {{locale?: string, timeZone?: string}} TimeOptions
 * `locale` and `timeZone` default to the viewer's; tests pin both.
 */

/**
 * A unix moment as date and time in the viewer's locale.
 * @param {number | null | undefined} unix seconds
 * @param {TimeOptions} [opts]
 * @returns {string}
 */
export function formatTime(unix, opts = {}) {
  if (unix == null || !Number.isFinite(unix) || unix <= 0) return "—";
  return new Intl.DateTimeFormat(opts.locale, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: opts.timeZone,
  }).format(new Date(unix * 1000));
}

/**
 * "measured 12 s ago", "read 2 min 5 s ago" (feat-overview-4): how old a
 * reading is, in the units `humanDuration` uses. A moment in the future (a
 * clock ahead of this one) reads as 0 s.
 * @param {string} verb what happened at `at` ("measured", "read")
 * @param {number | null | undefined} at unix seconds
 * @param {number} now unix seconds
 * @returns {string}
 */
export function agoText(verb, at, now) {
  if (at == null || !Number.isFinite(at)) return `not ${verb} yet`;
  return `${verb} ${humanDuration(Math.max(0, now - at))} ago`;
}
