// feat-stacks-8 (arch-schedule): the schedules page's pure half. A
// schedule's `when` in words and back from the form, and the table's rows.
// The times a schedule names are the host's wall clock (Europe/Brussels);
// the next run is shown in the viewer's own time.

import { formatClock, formatDateTime, formatDay } from "./format.js";

/**
 * @typedef {{every: "day", at: string} |
 *   {every: "week", days: number[], at: string} |
 *   {every: "once", date: string, at: string}} When
 * @typedef {{id: string, stack: string, action: string,
 *   args: Record<string, unknown>, when: When, enabled: boolean, note: string,
 *   created_at: number, handled_until: number,
 *   last_run?: {slot: number, job: number},
 *   last_missed?: {slot: number, why: "down" | "busy" | "refused"}}} Schedule
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

// ── redesign-schedules (release 3.71.0) ─────────────────────────────────
// The redesigned page (approved demo, Kenny 2026-10-03) shows every time in
// the host's own wall clock, the zone the server plans in, so a schedule's
// "10:00" reads 10:00 everywhere on the page. These helpers work that clock
// out with Intl for any IANA zone, the way core::schedule does for
// Europe/Brussels: in the autumn hour a time counts at its first
// occurrence, in the spring gap at the first minute after it.

// calendar-words: the weekday toggles' names, not a moment's format.
const SHORT_DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** @type {Map<string, Intl.DateTimeFormat>} */
const FORMATS = new Map();

/** @param {string} zone */
function zoneFormat(zone) {
  let f = FORMATS.get(zone);
  if (!f) {
    // date-key: the zone's wall-clock parts for arithmetic, never shown.
    f = new Intl.DateTimeFormat("en-GB", {
      timeZone: zone,
      hourCycle: "h23",
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "numeric",
      weekday: "short",
    });
    FORMATS.set(zone, f);
  }
  return f;
}

/**
 * @typedef {{y: number, m: number, d: number, hh: number, mm: number,
 *   wd: number}} ZoneParts wd: 0 = Monday … 6 = Sunday, as the server counts
 */

/**
 * The wall clock of `zone` at a unix second.
 * @param {number} unix
 * @param {string} zone
 * @returns {ZoneParts}
 */
export function zoneParts(unix, zone) {
  /** @type {Record<string, string>} */
  const p = {};
  for (const x of zoneFormat(zone).formatToParts(new Date(unix * 1000)))
    p[x.type] = x.value;
  return {
    y: Number(p.year),
    m: Number(p.month),
    d: Number(p.day),
    hh: Number(p.hour) % 24,
    mm: Number(p.minute),
    wd: SHORT_DAYS.indexOf(p.weekday),
  };
}

/** Seconds `zone` is ahead of UTC at a unix second (whole minutes). */
function offsetAt(/** @type {number} */ unix, /** @type {string} */ zone) {
  const t = Math.floor(unix / 60) * 60;
  const p = zoneParts(t, zone);
  return Date.UTC(p.y, p.m - 1, p.d, p.hh, p.mm) / 1000 - t;
}

/**
 * The instant a wall-clock minute of `zone` names: the first of two in the
 * repeated autumn hour, the first minute after the gap in the spring.
 * @param {number} y
 * @param {number} m 1-12
 * @param {number} d
 * @param {number} hh
 * @param {number} mm
 * @param {string} zone
 */
export function zonedInstant(y, m, d, hh, mm, zone) {
  const local = Date.UTC(y, m - 1, d, hh, mm) / 1000;
  const before = offsetAt(local - 86_400, zone);
  const after = offsetAt(local + 86_400, zone);
  const fits = (/** @type {number} */ t) => {
    const p = zoneParts(t, zone);
    return p.y === y && p.m === m && p.d === d && p.hh === hh && p.mm === mm;
  };
  const hits = [local - before, local - after].filter(fits);
  if (hits.length) return Math.min(...hits);
  // In the gap: the first minute that exists after it.
  let lo = local - after;
  let hi = local - before;
  while (hi - lo > 60) {
    const mid = lo + Math.floor((hi - lo) / 120) * 60;
    if (offsetAt(mid, zone) === after) hi = mid;
    else lo = mid;
  }
  return hi;
}

/** The civil date `k` days after y-m-d, with its weekday (0 = Monday). */
function addDays(
  /** @type {number} */ y,
  /** @type {number} */ m,
  /** @type {number} */ d,
  /** @type {number} */ k,
) {
  const t = new Date(Date.UTC(y, m - 1, d + k));
  return {
    y: t.getUTCFullYear(),
    m: t.getUTCMonth() + 1,
    d: t.getUTCDate(),
    wd: (t.getUTCDay() + 6) % 7,
  };
}

/** @param {string} at */
const hhmm = (at) => {
  const [h, m] = at.split(":").map(Number);
  return { h: h || 0, m: m || 0 };
};

/**
 * Every run of `when` in [from, to), oldest first.
 * @param {When} when
 * @param {number} from
 * @param {number} to
 * @param {string} zone
 * @returns {number[]}
 */
export function runsBetween(when, from, to, zone) {
  const { h, m } = hhmm(when.at);
  if (when.every === "once") {
    const x = /^(\d{4})-(\d{2})-(\d{2})$/.exec(when.date);
    if (!x) return [];
    const t = zonedInstant(+x[1], +x[2], +x[3], h, m, zone);
    return t >= from && t < to ? [t] : [];
  }
  const a = zoneParts(from, zone);
  const days = Math.ceil((to - from) / 86_400) + 2;
  /** @type {number[]} */
  const out = [];
  for (let k = -1; k <= days; k += 1) {
    const c = addDays(a.y, a.m, a.d, k);
    if (when.every === "week" && !when.days.includes(c.wd)) continue;
    const t = zonedInstant(c.y, c.m, c.d, h, m, zone);
    if (t >= from && t < to && !out.includes(t)) out.push(t);
  }
  return out.sort((x, y) => x - y);
}

/**
 * The next `n` runs of `when` strictly after `after`.
 * @param {When} when
 * @param {number} after
 * @param {number} n
 * @param {string} zone
 */
export function nextRuns(when, after, n, zone) {
  /** @type {number[]} */
  const out = [];
  let from = after + 1;
  for (let i = 0; i < 60 && out.length < n; i += 1) {
    out.push(...runsBetween(when, from, from + 7 * 86_400, zone));
    if (when.every === "once") break;
    from += 7 * 86_400;
  }
  return out.slice(0, n);
}

/** "03/10/2026" in the host's clock (redesign-final X4: the one format). */
export const dayText = (/** @type {number} */ t, /** @type {string} */ z) =>
  formatDay(t, { timeZone: z });

/** "10:00" in the host's clock. */
export const timeText = (/** @type {number} */ t, /** @type {string} */ z) =>
  formatClock(t, { timeZone: z });

/** "03/10/2026 10:00" in the host's clock. */
export const dateTimeText = (
  /** @type {number} */ t,
  /** @type {string} */ z,
) => formatDateTime(t, { timeZone: z });

/** An ISO date's own day, "05/10/2026", with no zone involved. */
function isoDayText(/** @type {string} */ date) {
  const x = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (!x) return date;
  return `${x[3]}/${x[2]}/${x[1]}`;
}

/** The short weekday names, Monday first, for the drawer's toggles. */
export const WEEKDAYS = SHORT_DAYS;

/**
 * The table's "When": "Every day at 10:00", "Mon, Thu at 22:00",
 * "Once, Mon 5 Oct at 08:00".
 * @param {When} w
 */
export function cadenceText(w) {
  if (w.every === "day") return `Every day at ${w.at}`;
  if (w.every === "week") {
    const d = [...new Set(w.days)].filter((x) => x >= 0 && x <= 6).sort();
    if (d.length === 7) return `Every day at ${w.at}`;
    return `${d.map((x) => SHORT_DAYS[x]).join(", ")} at ${w.at}`;
  }
  return `Once, ${isoDayText(w.date)} at ${w.at}`;
}

/**
 * "in 30 min", "in 2 h 14 min", "in 3 d".
 * @param {number} t
 * @param {number} now
 */
export function untilText(t, now) {
  const s = Math.max(0, t - now);
  // Whole minutes first, so the hour and the minutes never read "60".
  const min = Math.round(s / 60);
  if (min < 60) return `in ${min} min`;
  if (min < 1440) return `in ${Math.floor(min / 60)} h ${min % 60} min`;
  return `in ${Math.round(s / 86_400)} d`;
}

/** The host target's name in a sentence. */
export const targetText = (
  /** @type {string} */ stack,
  hostTarget = "_host",
) => (stack === hostTarget ? "the host" : stack);

/**
 * "Back up films", "Patch the fleet", "ZFS replicate the host": the action
 * with what it acts on, unless its label already names that.
 * @param {string} label
 * @param {string} stack
 * @param {string} [hostTarget]
 */
export function scheduleTitle(label, stack, hostTarget = "_host") {
  if (stack !== hostTarget) return `${label} ${stack}`;
  return /\b(the|a|host|fleet|devices|repository)\b/i.test(label)
    ? label
    : `${label} the host`;
}

/**
 * "6 schedules · 5 on".
 * @param {ScheduleView[]} list
 */
export function countText(list) {
  const on = list.filter((v) => v.schedule.enabled).length;
  return `${list.length} ${list.length === 1 ? "schedule" : "schedules"} · ${on} on`;
}

/**
 * Whether `at` lies within an hour of the host's nightly round (either
 * side, across midnight too). No round, no warning.
 * @param {string} at
 * @param {number | null} hour
 */
export function nearNightly(at, hour) {
  if (hour == null || !/^\d\d:\d\d$/.test(at)) return false;
  const { h, m } = hhmm(at);
  const diff = Math.abs(h * 60 + m - hour * 60);
  return Math.min(diff, 1440 - diff) < 60;
}

/**
 * What the drawer edits.
 * @typedef {{action: string, stack: string, every: string, days: number[],
 *   at: string, date: string, note: string, enabled: boolean,
 *   args: Record<string, unknown>}} Draft
 * A template the empty page offers: its card's words and its draft.
 * @typedef {{id: string, title: string, says: string, action: string,
 *   host: boolean, every: string, days: number[], at: string,
 *   tomorrow: boolean}} Template
 */

/** @type {Template[]} */
export const TEMPLATES = [
  {
    id: "backup-daily",
    title: "Back up a busy stack during the day",
    says: "Every day at 12:00, on top of the nightly round.",
    action: "backup",
    host: false,
    every: "day",
    days: [],
    at: "12:00",
    tomorrow: false,
  },
  {
    id: "patch-weekly",
    title: "Patch the fleet weekly",
    says: "apt dist-upgrade every container, Sunday at 22:00.",
    action: "patch",
    host: true,
    every: "week",
    days: [6],
    at: "22:00",
    tomorrow: false,
  },
  {
    id: "deploy-later",
    title: "Deploy later",
    says: "Once, at a time you pick: a release outside working hours.",
    action: "deploy",
    host: false,
    every: "once",
    days: [],
    at: "20:00",
    tomorrow: true,
  },
];

/**
 * The drawer's first values: the schedule being edited, else a template,
 * else the demo's own start (Back up, chosen weekdays Tue and Fri, 02:30).
 * @param {Schedule | null} existing
 * @param {Template | null} template
 * @param {{stacks: string[], hostTarget: string, now: number, zone: string}} ctx
 * @returns {Draft}
 */
export function draftFor(existing, template, ctx) {
  if (existing) {
    const w = existing.when;
    return {
      action: existing.action,
      stack: existing.stack,
      every: w.every,
      days: w.every === "week" ? [...w.days] : [],
      at: w.at,
      date: w.every === "once" ? w.date : "",
      note: existing.note ?? "",
      enabled: existing.enabled,
      args: { ...existing.args },
    };
  }
  const first = ctx.stacks[0] ?? ctx.hostTarget;
  if (template) {
    const t = zoneParts(ctx.now, ctx.zone);
    const next = addDays(t.y, t.m, t.d, 1);
    const pad = (/** @type {number} */ n) => String(n).padStart(2, "0");
    return {
      action: template.action,
      stack: template.host ? ctx.hostTarget : first,
      every: template.every,
      days: [...template.days],
      at: template.at,
      date: template.tomorrow ? `${next.y}-${pad(next.m)}-${pad(next.d)}` : "",
      note: "",
      enabled: true,
      args: {},
    };
  }
  return {
    action: "backup",
    stack: first,
    every: "week",
    days: [1, 4],
    at: "02:30",
    date: "",
    note: "",
    enabled: true,
    args: {},
  };
}

/**
 * The words of the "how often" choice, in the demo's order.
 * @type {Record<string, string>}
 */
export const EVERY_WORDS = {
  week: "on chosen weekdays",
  day: "every day",
  once: "once",
};

/**
 * The drawer read as one sentence: "Run Back up on films on chosen
 * weekdays at 02:30".
 * @param {Draft} d
 * @param {(action: string) => string} label
 * @param {string} hostTarget
 */
export function sentenceText(d, label, hostTarget) {
  const on = d.stack === hostTarget ? "the whole host" : d.stack;
  const how =
    d.every === "once"
      ? `once on ${isoDayText(d.date)}`
      : (EVERY_WORDS[d.every] ?? d.every);
  return `Run ${label(d.action)} on ${on} ${how} at ${d.at}`;
}

/**
 * The drawer to the body POST or PUT /data/schedules wants, or what is
 * wrong with it.
 * @param {Draft} d
 * @returns {{ok: true, body: ReturnType<typeof scheduleBody>} |
 *   {ok: false, field: string, why: string}}
 */
export function draftBody(d) {
  const w = whenFromValues({
    every: d.every,
    at: d.at,
    days: d.days,
    date: d.date,
  });
  if (!w.ok) return w;
  return {
    ok: true,
    body: scheduleBody({
      stack: d.stack,
      action: d.action,
      args: d.args,
      when: w.when,
      enabled: d.enabled,
      note: d.note,
    }),
  };
}

/** @type {Record<string, string>} */
const MISSED_WHY = {
  down: "dashboard down",
  busy: "previous run still going",
  refused: "refused",
};

/** @type {Record<string, "ok" | "bad" | "warn" | "info">} */
const JOB_TONE = {
  done: "ok",
  failed: "bad",
  refused: "bad",
  running: "info",
  queued: "info",
  deferred: "warn",
  unknown: "warn",
};

/**
 * The "Last run" cell: its status dot, its words and the job to link.
 * @param {ScheduleView} v
 * @param {string} zone
 * @returns {{tone: "ok" | "bad" | "warn" | "info" | "none", text: string,
 *   job: number | null}}
 */
export function lastRunView(v, zone) {
  const s = v.schedule;
  const ran = s.last_run;
  const miss = s.last_missed;
  if (miss && (!ran || miss.slot > ran.slot))
    return {
      tone: "warn",
      text: `missed ${dayText(miss.slot, zone)} (${MISSED_WHY[miss.why] ?? miss.why})`,
      job: null,
    };
  const job = v.last_job;
  if (job)
    return {
      tone: JOB_TONE[job.state] ?? "warn",
      text: job.state === "done" ? "ok" : job.state,
      job: job.job,
    };
  if (ran) return { tone: "none", text: "ran", job: ran.job };
  return { tone: "none", text: "not run yet", job: null };
}

/**
 * The schedule that runs first, of those that are on.
 * @param {ScheduleView[]} list
 * @returns {ScheduleView | null}
 */
export function nextUp(list) {
  /** @type {ScheduleView | null} */
  let best = null;
  for (const v of list) {
    if (!v.schedule.enabled || v.next_run == null) continue;
    if (!best || best.next_run == null || v.next_run < best.next_run) best = v;
  }
  return best;
}

/**
 * One block on the calendar.
 * @typedef {{id: string | null, t: number, at: string, hour: number,
 *   text: string, host: boolean, off: boolean, next: boolean,
 *   skipped: boolean}} Pill
 * @typedef {{head: string, today: boolean, start: number, pills: Pill[]}} Day
 */

/**
 * The next seven days from today in the host's clock: every run of every
 * schedule (one that is off greyed, one skipped on purpose marked), the
 * host's nightly round on each day, and where now is on today's column.
 * @param {ScheduleView[]} list
 * @param {number} now
 * @param {string} zone
 * @param {number | null} nightly the round's hour, null when it has none
 * @param {(action: string) => string} label
 * @param {string} [hostTarget]
 * @returns {{days: Day[], nowHour: number}}
 */
export function weekPlan(
  list,
  now,
  zone,
  nightly,
  label,
  hostTarget = "_host",
) {
  const p = zoneParts(now, zone);
  const next = nextUp(list);
  /** @type {Day[]} */
  const days = [];
  for (let k = 0; k < 7; k += 1) {
    const c = addDays(p.y, p.m, p.d, k);
    const n = addDays(p.y, p.m, p.d, k + 1);
    const start = zonedInstant(c.y, c.m, c.d, 0, 0, zone);
    const end = zonedInstant(n.y, n.m, n.d, 0, 0, zone);
    /** @type {Pill[]} */
    const pills = [];
    if (nightly != null)
      pills.push({
        id: null,
        t: zonedInstant(c.y, c.m, c.d, nightly, 0, zone),
        at: `${String(nightly).padStart(2, "0")}:00`,
        hour: nightly,
        text: "nightly round",
        host: true,
        off: false,
        next: false,
        skipped: false,
      });
    for (const v of list) {
      const s = v.schedule;
      for (const t of runsBetween(s.when, start, end, zone)) {
        const q = zoneParts(t, zone);
        pills.push({
          id: s.id,
          t,
          at: timeText(t, zone),
          hour: q.hh + q.mm / 60,
          text: scheduleTitle(label(s.action), s.stack, hostTarget),
          host: false,
          off: !s.enabled,
          next: v === next && t === v.next_run,
          skipped: s.enabled && t > now && v.next_run != null && t < v.next_run,
        });
      }
    }
    pills.sort((a, b) => a.t - b.t);
    days.push({
      head: k === 0 ? "Today" : `${SHORT_DAYS[c.wd]} ${c.d}`,
      today: k === 0,
      start,
      pills,
    });
  }
  return { days, nowHour: p.hh + p.mm / 60 };
}

/**
 * The phone's agenda: the next `n` runs of the schedules that are on,
 * within the coming week.
 * @param {ScheduleView[]} list
 * @param {number} now
 * @param {string} zone
 * @param {(action: string) => string} label
 * @param {number} [n]
 * @param {string} [hostTarget]
 * @returns {{id: string, t: number, when: string, text: string}[]}
 */
export function agenda(list, now, zone, label, n = 8, hostTarget = "_host") {
  /** @type {{id: string, t: number, when: string, text: string}[]} */
  const rows = [];
  for (const v of list) {
    const s = v.schedule;
    if (!s.enabled) continue;
    for (const t of runsBetween(s.when, now + 1, now + 7 * 86_400, zone)) {
      if (v.next_run != null && t < v.next_run) continue;
      rows.push({
        id: s.id,
        t,
        when: dateTimeText(t, zone),
        text: scheduleTitle(label(s.action), s.stack, hostTarget),
      });
    }
  }
  return rows.sort((a, b) => a.t - b.t).slice(0, n);
}

/**
 * Tops for blocks wanted at `tops` (pixels), pushed down in time order so
 * no two are closer than `gap`; same order as given.
 * @param {number[]} tops
 * @param {number} gap
 */
export function stackTops(tops, gap) {
  const order = tops.map((t, i) => ({ t, i })).sort((a, b) => a.t - b.t);
  const out = [...tops];
  let last = -Infinity;
  for (const o of order) {
    const t = Math.max(o.t, last + gap);
    out[o.i] = t;
    last = t;
  }
  return out;
}
