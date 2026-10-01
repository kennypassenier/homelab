// feat-overview-10 (backup calendar): pure view model, so the day grid and
// its "how many of the fleet backed up that night" verdict are testable
// without a DOM. Self-contained on purpose (its own module, its own page):
// a second helper is building the Backups page in the same milestone, and
// the owner asked for this calendar to read the host its own way so the
// two merge without touching each other's files.

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
 *   missing: string[], ratio: number, tone: "ok"|"warn"|"bad"|"muted"}} CalendarDay
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
