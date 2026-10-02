// feat-overview-10 (backup calendar): pure view model, so the day grid and
// its "how many of the fleet backed up that night" verdict are testable
// without a DOM. Self-contained on purpose (its own module, its own page):
// a second helper is building the Backups page in the same milestone, and
// the owner asked for this calendar to read the host its own way so the
// two merge without touching each other's files.
//
// fix-214 (Kenny, 2026-10-02): a month view (a date picker), not a fixed
// 35-day strip that only ever grows. A day's detail panel (`dayDetail`) can
// only show what the host's `Rpc::BackupCalendar` answer already carries
// per stack per night — a bare unix timestamp (`host/src/main.rs`'s
// `nights` is `cached.snapshots.iter().map(|s| s.time)`, built from
// `core::ops::backup::SnapRun { id, short_id, time, run }`). The host
// COULD be asked to also forward `short_id` (restic's own id for each
// snapshot) and the per-snapshot size (today only on `GetBackups`'s
// `repo_size_bytes`, which reads the LATEST snapshot's total, not each
// night's own), which would let the day panel name which exact restic
// snapshot a night holds and how big it was — not added here, since this
// round is UI-only and that is host work.

/**
 * One calendar day's local date key (`YYYY-MM-DD`, the viewer's own
 * timezone — a backup calendar reads by the night Kenny lives in, not UTC).
 * @param {number} unix seconds
 * @param {TimeZone} [tz]
 * @returns {string}
 */
function dayKey(unix, tz) {
  const d = new Date(unix * 1000);
  const parts = new Intl.DateTimeFormat("en-CA", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    timeZone: tz,
  }).formatToParts(d);
  const get = (/** @type {string} */ t) =>
    parts.find((p) => p.type === t)?.value ?? "";
  return `${get("year")}-${get("month")}-${get("day")}`;
}

/** @typedef {string | undefined} TimeZone test-pinned; undefined = viewer's own */

/**
 * @typedef {{date: string, backed_up: string[], expected: string[],
 *   missing: string[], ratio: number,
 *   tone: "ok"|"warn"|"bad"|"muted"|"future"|"before"}} CalendarDay
 *   `future`: later than today — nothing to verdict yet. `before`: earlier
 *   than the oldest snapshot the whole fleet has ever produced (fix-214) —
 *   this calendar had no history at all yet, which reads differently from a
 *   month the fleet simply slept through.
 */

/**
 * Every day from `days` ago through today, each with which of `expected`
 * stacks has at least one snapshot that day.
 * @param {Record<string, number[]>} stacks stack -> unix seconds per snapshot
 * @param {string[]} expected every stack that should appear (so a stack
 *   with zero snapshots in the window still shows as missing, not absent)
 * @param {number} days how many days back, inclusive of today
 * @param {number} now unix seconds
 * @param {TimeZone} [tz]
 * @returns {CalendarDay[]} oldest first
 */
export function calendarDays(stacks, expected, days, now, tz) {
  /** @type {Map<string, Set<string>>} */
  const byDay = new Map();
  for (const [stack, times] of Object.entries(stacks)) {
    for (const t of times) {
      const key = dayKey(t, tz);
      if (!byDay.has(key)) byDay.set(key, new Set());
      byDay.get(key)?.add(stack);
    }
  }
  const out = [];
  for (let i = days - 1; i >= 0; i--) {
    const unix = now - i * 86400;
    const date = dayKey(unix, tz);
    const backed = [...(byDay.get(date) ?? [])].sort();
    const missing = expected.filter((s) => !backed.includes(s));
    const ratio = expected.length ? backed.length / expected.length : 1;
    /** @type {CalendarDay["tone"]} */
    let tone;
    if (!expected.length) tone = "muted";
    else if (ratio >= 1) tone = "ok";
    else if (ratio > 0) tone = "warn";
    else tone = "bad";
    out.push({ date, backed_up: backed, expected, missing, ratio, tone });
  }
  return out;
}

/**
 * fix-214 (Kenny, Dutch: "die kan toch net als een datepicker per maand de
 * dingen tonen, met controls om door de maanden te scrollen?"): the month
 * equivalent of `calendarDays`'s sliding window — every day of ONE calendar
 * month, each with its own verdict, so the page can show a real month
 * (a date picker) rather than a fixed 35-day strip that only ever grows.
 * @param {Record<string, number[]>} stacks stack -> unix seconds per snapshot
 * @param {string[]} expected every stack that should appear
 * @param {number} year
 * @param {number} month 1-12 (plain calendar numbers, never JS Date's 0-based)
 * @param {string} today YYYY-MM-DD, the viewer's own "today" — a day later
 *   than this is the future: nothing to verdict yet.
 * @param {number | null} earliestKnown unix seconds of the oldest snapshot
 *   across the WHOLE fleet (every stack, not just this month's `expected`),
 *   or `null` when nothing has ever been read. A day before it existed
 *   before this calendar had any history at all, which reads differently
 *   from a night the fleet was simply asked to back up and didn't.
 * @param {TimeZone} [tz]
 * @returns {CalendarDay[]} oldest first, one entry per day of the month
 */
export function monthDays(
  stacks,
  expected,
  year,
  month,
  today,
  earliestKnown,
  tz,
) {
  /** @type {Map<string, Set<string>>} */
  const byDay = new Map();
  for (const [stack, times] of Object.entries(stacks)) {
    for (const t of times) {
      const key = dayKey(t, tz);
      if (!byDay.has(key)) byDay.set(key, new Set());
      byDay.get(key)?.add(stack);
    }
  }
  const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
  const earliestDay = earliestKnown != null ? dayKey(earliestKnown, tz) : null;
  const out = [];
  for (let d = 1; d <= daysInMonth; d++) {
    const date = `${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
    const backed = [...(byDay.get(date) ?? [])].sort();
    const missing = expected.filter((s) => !backed.includes(s));
    const ratio = expected.length ? backed.length / expected.length : 1;
    /** @type {CalendarDay["tone"]} */
    let tone;
    if (date > today) tone = "future";
    else if (earliestDay != null && date < earliestDay) tone = "before";
    else if (!expected.length) tone = "muted";
    else if (ratio >= 1) tone = "ok";
    else if (ratio > 0) tone = "warn";
    else tone = "bad";
    out.push({ date, backed_up: backed, expected, missing, ratio, tone });
  }
  return out;
}

/**
 * fix-214: the oldest snapshot across the WHOLE fleet (every stack this
 * page has read "ok", regardless of which month is on screen) — the one
 * fact that tells a month from before the fleet existed apart from a month
 * the fleet simply slept through.
 * @param {Record<string, number[]>} stacks
 * @returns {number | null}
 */
export function earliestSnapshot(stacks) {
  let min = null;
  for (const times of Object.values(stacks))
    for (const t of times) if (min == null || t < min) min = t;
  return min;
}

/**
 * fix-214: one calendar month's identity (year, 1-12 month), shifted by
 * `delta` months — the pure half of the prev/next/PageUp/PageDown
 * navigation, so moving from December back a month lands on last
 * December, not month 0.
 * @param {{year: number, month: number}} ym
 * @param {number} delta
 * @returns {{year: number, month: number}}
 */
export function shiftMonth(ym, delta) {
  const total = ym.year * 12 + (ym.month - 1) + delta;
  return {
    year: Math.floor(total / 12),
    month: (((total % 12) + 12) % 12) + 1,
  };
}

/**
 * fix-214: everything the calendar can honestly say about one night, per
 * stack — built from the SAME `results` the grid itself reads (fix-216's
 * `StackResult` shape: `ok`/`no_backup`/`failed`/`pending`), so the detail
 * panel never needs a fetch of its own and never claims more than the host
 * actually answered (no per-snapshot size or file count: the host's
 * `BackupCalendar` answer carries only each night's unix time — see this
 * module's own header comment for what else it could be asked to send).
 * @param {Record<string, import("./pages/backupcalendar.js").StackResult>} results
 * @param {string} date YYYY-MM-DD
 * @param {TimeZone} [tz]
 * @returns {{stack: string,
 *   state: "backed_up"|"missing"|"no_backup"|"not_read",
 *   times: number[]}[]} sorted by stack name
 */
export function dayDetail(results, date, tz) {
  return Object.keys(results)
    .sort()
    .map((stack) => {
      const r = results[stack];
      if (r.status === "no_backup")
        return { stack, state: /** @type {const} */ ("no_backup"), times: [] };
      if (r.status === "ok") {
        const times = r.times.filter((t) => dayKey(t, tz) === date);
        return times.length
          ? { stack, state: /** @type {const} */ ("backed_up"), times }
          : { stack, state: /** @type {const} */ ("missing"), times };
      }
      return { stack, state: /** @type {const} */ ("not_read"), times: [] };
    });
}

/**
 * `calendarDays` grouped into whole weeks (Monday first), the leading and
 * trailing days of the partial weeks at each end padded with `null` so
 * every row has 7 cells — a plain grid, no week-of-year arithmetic a
 * caller has to get right twice.
 * @param {CalendarDay[]} daysOldestFirst
 * @returns {(CalendarDay | null)[][]}
 */
export function calendarWeeks(daysOldestFirst) {
  if (!daysOldestFirst.length) return [];
  /** @param {string} iso */
  const weekday = (iso) => {
    // Monday = 0 … Sunday = 6, from the ISO date alone (UTC midnight is
    // fine: only the weekday is read, never the time).
    const js = new Date(`${iso}T00:00:00Z`).getUTCDay();
    return (js + 6) % 7;
  };
  /** @type {(CalendarDay | null)[][]} */
  const weeks = [];
  /** @type {(CalendarDay | null)[]} */
  let row = new Array(weekday(daysOldestFirst[0].date)).fill(null);
  for (const d of daysOldestFirst) {
    row.push(d);
    if (row.length === 7) {
      weeks.push(row);
      row = [];
    }
  }
  if (row.length) {
    while (row.length < 7) row.push(null);
    weeks.push(row);
  }
  return weeks;
}

// ── fix-177: per-stack progressive loading ──────────────────────────────
// One request per stack (the page already knows the fleet's stack names,
// `current().fleet.stacks`), run concurrently, so one slow or hung
// repository times out alone instead of an all-stacks call's budget
// swallowing every stack that already answered. The fold and the progress
// count (fix-179: the Backups page needs the same shape) live in
// `perstack.js`, a shared pure module; this file keeps only what is
// calendar-specific.

import { stackReadProgress, withStackResult } from "./perstack.js";

export { withStackResult };

/**
 * `ok`: read, with its snapshot times (possibly none yet, still a stack
 * the calendar should expect a night from). `empty`: read, but the host
 * has nothing to say about it (isn't a stack it knows) — not an error,
 * just a stack the calendar leaves out entirely, same as the old
 * all-stacks answer silently never naming it. `no_backup` (fix-202): the
 * host named this stack explicitly as keeping no data at all (every mount
 * is `no_backup`/`no_data`, no native service) — known and terminal at
 * once, never "not read yet", shown as its own line rather than folded
 * into "no backup that night" (which would wrongly repeat every night for
 * a reason that has nothing to do with backups actually failing).
 * `failed`: the read itself did not finish, or the host could name no
 * manifest for it at all (network, timeout, the host down, no manifest on
 * record) — named under the progress bar, never folded into "no backup
 * that night".
 * @typedef {{status: "pending"} | {status: "ok", times: number[]} |
 *   {status: "empty"} | {status: "no_backup"} |
 *   {status: "failed", reason: string}} StackResult
 */

/**
 * `perstack.js`'s `stackReadProgress`, under the calendar page's own name
 * (kept so the page and its tests need no change).
 * @param {Record<string, StackResult>} results
 * @returns {{total: number, loaded: number, pct: number, done: boolean,
 *   failed: {stack: string, reason: string}[]}}
 */
export function calendarProgress(results) {
  return stackReadProgress(results);
}

/**
 * fix-202: the stacks the host has already named as keeping no data at all
 * — read, terminal, never pending, but also never part of a night's ratio.
 * Sorted so the list painted from it does not reorder itself as more
 * stacks settle.
 * @param {Record<string, StackResult>} results
 * @returns {string[]}
 */
export function calendarNoBackup(results) {
  return Object.entries(results)
    .filter(([, r]) => r.status === "no_backup")
    .map(([name]) => name)
    .sort();
}

/**
 * The stacks whose data is trustworthy enough to count toward a night's
 * ratio: read successfully, even with zero snapshots ever (a stack `calendarDays`
 * is told to expect, at a flat "bad" every night until it backs up once). A
 * stack that failed to read is left out of `expected` entirely — "not
 * read" is its own state (named in `calendarProgress().failed`), never
 * folded into "no backup that night", which every other night would wrongly
 * repeat for a reason that has nothing to do with backups.
 * @param {Record<string, StackResult>} results
 * @returns {{stacks: Record<string, number[]>, expected: string[]}}
 */
export function calendarInputs(results) {
  /** @type {Record<string, number[]>} */
  const stacks = {};
  for (const [name, r] of Object.entries(results)) {
    if (r.status === "ok") stacks[name] = r.times;
  }
  return { stacks, expected: Object.keys(stacks).sort() };
}
