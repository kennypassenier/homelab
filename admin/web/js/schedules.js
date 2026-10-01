// feat-stacks-8 (arch-schedule): the schedules page's pure half. A
// schedule's `when` in words and back from the form, and the table's rows.
// The times a schedule names are the host's wall clock (Europe/Brussels);
// the next run is shown in the viewer's own time.

import { formatDateTime } from "./format.js";

/**
 * @typedef {{every: "day", at: string} |
 *   {every: "week", days: number[], at: string} |
 *   {every: "once", date: string, at: string}} When
 * @typedef {{id: string, stack: string, action: string,
 *   args: Record<string, unknown>, when: When, enabled: boolean, note: string,
 *   created_at: number, handled_until: number,
 *   last_run?: {slot: number, job: number}}} Schedule
 * @typedef {{schedule: Schedule, next_run: number | null,
 *   next_run_local: string | null,
 *   last_job: import("./jobs.js").Job | null}} ScheduleView
 * @typedef {{zone: string, schedules: ScheduleView[]}} ScheduleList
 * @typedef {{every: string, at: string, days: number[], date: string}} WhenValues
 */

/** Monday first, as the server counts (0 = Monday). */
export const DAYS = /** @type {const} */ ([
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
]);

/** @param {number[]} days */
function daysText(days) {
  const d = [...new Set(days)].filter((x) => x >= 0 && x <= 6).sort();
  if (d.length === 7) return "every day";
  if (d.join() === "0,1,2,3,4") return "on weekdays";
  if (d.join() === "5,6") return "at the weekend";
  return `on ${d.map((x) => DAYS[x].slice(0, 3)).join(", ")}`;
}

/**
 * "Every day at 03:00", "On Mon, Thu at 04:30", "Once on 1 Oct 2026 at 02:00".
 * @param {When} w
 * @param {string} [zone] the host's zone, named when given
 * @param {{locale?: string}} [opts]
 */
export function whenText(w, zone, opts = {}) {
  const tz = zone ? ` (${zone})` : "";
  if (w.every === "day") return `Every day at ${w.at}${tz}`;
  if (w.every === "week") {
    const d = daysText(w.days);
    return `${d[0].toUpperCase()}${d.slice(1)} at ${w.at}${tz}`;
  }
  // dd/mm/yyyy, not the viewer's locale (Kenny, 2026-10-02) — built
  // straight from the ISO date's own digits, so it needs no Intl call (and
  // no locale) at all.
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(w.date);
  const date = m ? `${m[3]}/${m[2]}/${m[1]}` : w.date;
  return `Once on ${date} at ${w.at}${tz}`;
}

/**
 * The form's values for a schedule's `when` (a new one starts at 03:00
 * every day).
 * @param {When | null} w
 * @returns {WhenValues}
 */
export function whenValues(w) {
  if (!w) return { every: "day", at: "03:00", days: [], date: "" };
  if (w.every === "week")
    return { every: "week", at: w.at, days: [...w.days], date: "" };
  if (w.every === "once")
    return { every: "once", at: w.at, days: [], date: w.date };
  return { every: "day", at: w.at, days: [], date: "" };
}

/**
 * The form back to a `when`, or what is wrong with it.
 * @param {WhenValues} v
 * @returns {{ok: true, when: When} | {ok: false, field: string, why: string}}
 */
export function whenFromValues(v) {
  if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(v.at))
    return {
      ok: false,
      field: "at",
      why: "Write the time as HH:MM, 00:00 to 23:59.",
    };
  if (v.every === "day") return { ok: true, when: { every: "day", at: v.at } };
  if (v.every === "week") {
    const days = [...new Set(v.days)].filter((d) => d >= 0 && d <= 6).sort();
    if (!days.length)
      return { ok: false, field: "days", why: "Pick at least one day." };
    return { ok: true, when: { every: "week", days, at: v.at } };
  }
  if (v.every === "once") {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(v.date))
      return { ok: false, field: "date", why: "Pick the date." };
    return { ok: true, when: { every: "once", date: v.date, at: v.at } };
  }
  return { ok: false, field: "every", why: "Pick how often." };
}

/**
 * The schedules table's rows.
 * @param {ScheduleView[]} list
 * @param {(action: string) => string} label
 * @param {{locale?: string, timeZone?: string}} [opts]
 */
export function scheduleRows(list, label, opts = {}) {
  return list.map((v) => {
    const s = v.schedule;
    const last = v.last_job;
    return {
      id: s.id,
      stack: s.stack === "_host" ? "host" : s.stack,
      action: label(s.action),
      when: whenText(s.when, undefined, opts),
      next: v.next_run,
      nextText: !s.enabled
        ? "off"
        : v.next_run == null
          ? "no further run"
          : formatDateTime(v.next_run, opts),
      last: last
        ? `${last.state} (job ${last.job})`
        : s.last_run
          ? `job ${s.last_run.job}`
          : "never",
      enabled: s.enabled,
      note: s.note,
    };
  });
}

/**
 * The body for POST or PUT /data/schedules.
 * @param {{stack: string, action: string, args: Record<string, unknown>,
 *   when: When, enabled: boolean, note: string}} v
 */
export function scheduleBody(v) {
  return {
    stack: v.stack,
    action: v.action,
    args: v.args,
    when: v.when,
    enabled: v.enabled,
    note: v.note.trim(),
  };
}

/**
 * The same schedule with only `enabled` changed, as the server wants it back.
 * @param {Schedule} s
 * @param {boolean} enabled
 */
export const toggledBody = (s, enabled) =>
  scheduleBody({ ...s, enabled, note: s.note ?? "" });
