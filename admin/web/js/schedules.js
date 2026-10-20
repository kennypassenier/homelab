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
 *   last_missed?: {slot: number, why: "down" | "busy" | "refused"},
 *   paused_until?: number | null}} Schedule
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
 * A cadence in words, as a sentence part: "every day at 12:00", "on
 * weekdays at 18:00", "on Mon, Thu at 03:30", "once on 25/10/2026 at 20:00".
 * @param {When} w
 */
export function cadenceWords(w) {
  if (w.every === "day") return `every day at ${w.at}`;
  if (w.every === "once") return `once on ${isoDayText(w.date)} at ${w.at}`;
  const d = [...new Set(w.days)].filter((x) => x >= 0 && x <= 6).sort();
  const days =
    d.length === 7
      ? "every day"
      : d.join() === "0,1,2,3,4"
        ? "on weekdays"
        : d.join() === "5,6"
          ? "at the weekend"
          : `on ${d.map((x) => SHORT_DAYS[x]).join(", ")}`;
  return `${days} at ${w.at}`;
}

/**
 * A cadence short enough for a cell or a lane: "Daily 03:30", "Weekdays
 * 18:00", "Weekend 09:00", "Mon, Thu 03:30", "25/10 20:00".
 * @param {When} w
 */
export function shortWhen(w) {
  if (w.every === "day") return `Daily ${w.at}`;
  if (w.every === "once") return `${isoDayText(w.date).slice(0, 5)} ${w.at}`;
  const d = [...new Set(w.days)].filter((x) => x >= 0 && x <= 6).sort();
  const days =
    d.length === 7
      ? "Daily"
      : d.join() === "0,1,2,3,4"
        ? "Weekdays"
        : d.join() === "5,6"
          ? "Weekend"
          : d.map((x) => SHORT_DAYS[x]).join(", ");
  return `${days} ${w.at}`;
}

/** The host target's name in a sentence. */
export const targetText = (
  /** @type {string} */ stack,
  hostTarget = "_host",
) => (stack === hostTarget ? "the host" : stack);

/**
 * "Back up · films", "Patch the fleet", "ZFS replicate · the whole host": the
 * action with what it acts on, unless its label already names that.
 * @param {string} label
 * @param {string} stack
 * @param {string} [hostTarget]
 */
export function scheduleTitle(label, stack, hostTarget = "_host") {
  if (stack !== hostTarget) return `${label} · ${stack}`;
  return /\b(the|a|host|fleet|devices|repository)\b/i.test(label)
    ? label
    : `${label} · the whole host`;
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
  {
    id: "restore-test-weekly",
    title: "Test a restore every Monday",
    says: "Restores the newest backup into a scratch directory at 06:00, so a backup is proven, not assumed.",
    action: "verify-restore",
    host: false,
    every: "week",
    days: [0],
    at: "06:00",
    tomorrow: false,
  },
];

/**
 * The form's first values: the schedule being edited, else a template,
 * else the demo's own start (Back up, every day, an hour after the host's
 * nightly round, or 03:00 when it has none).
 * @param {Schedule | null} existing
 * @param {Template | null} template
 * @param {{stacks: string[], hostTarget: string, now: number, zone: string,
 *   nightly?: number | null}} ctx
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
  const after = typeof ctx.nightly === "number" ? (ctx.nightly + 1) % 24 : 3;
  return {
    action: "backup",
    stack: first,
    every: "day",
    days: [1, 4],
    at: `${String(after).padStart(2, "0")}:00`,
    date: "",
    note: "",
    enabled: true,
    args: {},
  };
}

/**
 * The words of the "how often" choice, as the sentence reads them.
 * @type {Record<string, string>}
 */
export const EVERY_WORDS = {
  day: "every day",
  week: "every week",
  once: "once",
};

/**
 * The form to the body POST or PUT /data/schedules wants, or what is wrong
 * with it.
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

/**
 * A date typed as dd/mm/yyyy (the page's one format) or given as an ISO
 * date, as the ISO date the host wants; "" when it is neither.
 * @param {string} text
 */
export function isoDate(text) {
  const t = text.trim();
  if (/^\d{4}-\d{2}-\d{2}$/.test(t)) return t;
  const m = /^(\d{1,2})\/(\d{1,2})\/(\d{4})$/.exec(t);
  if (!m) return "";
  const [d, mo, y] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const c = new Date(Date.UTC(y, mo - 1, d));
  if (c.getUTCFullYear() !== y || c.getUTCMonth() !== mo - 1) return "";
  if (c.getUTCDate() !== d) return "";
  return `${y}-${String(mo).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
}

/** @type {Record<string, string>} */
const MISSED_WHY = {
  down: "the dashboard was down",
  busy: "the previous run was still going",
  refused: "the host refused it",
};

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

// ── the kinds of action: the colour a run is drawn in ───────────────────
// kp's week and timeline colour a run by `series` 1 to 5. The five families
// below are what the legend names; an action none of them lists (guards,
// resize, a template build…) is drawn in the fifth.

/** @typedef {1 | 2 | 3 | 4 | 5} Series */
export const FAMILIES = /** @type {const} */ ([
  {
    series: 1,
    label: "Back up",
    actions: [
      "backup",
      "backup-native",
      "backup-host-meta",
      "backup-devices",
      "zfs-replicate",
    ],
  },
  {
    series: 2,
    label: "Update",
    actions: [
      "update",
      "update-native",
      "release-update-native",
      "update-host",
    ],
  },
  { series: 3, label: "Patch the fleet", actions: ["patch"] },
  {
    series: 4,
    label: "Deploy",
    actions: ["deploy", "deploy-commit", "apply"],
  },
  { series: 5, label: "Restore test and others", actions: [] },
]);

/** What the legend under the week and the timeline names. */
export const LEGEND = FAMILIES.map(({ series, label }) => ({ series, label }));

/**
 * The colour (series) of an action.
 * @param {string} action
 * @returns {Series}
 */
export function seriesOf(action) {
  return (
    FAMILIES.find((f) =>
      /** @type {readonly string[]} */ (f.actions).includes(action),
    )?.series ?? 5
  );
}

// ── the host's clock, day by day ────────────────────────────────────────

/**
 * Local midnight of the day `k` days from the one `unix` falls on.
 * @param {number} unix
 * @param {string} zone
 * @param {number} [k]
 */
export function dayStart(unix, zone, k = 0) {
  const p = zoneParts(unix, zone);
  const c = addDays(p.y, p.m, p.d, k);
  return zonedInstant(c.y, c.m, c.d, 0, 0, zone);
}

/** Who a clash is with, when it is the host's own round. */
export const NIGHTLY_WORDS = "the host's nightly round";

/**
 * The host's nightly round on every day in [from, to).
 * @param {number} from
 * @param {number} to
 * @param {number | null | undefined} hour null: the host has no round
 * @param {string} zone
 * @returns {number[]}
 */
export function nightlyRounds(from, to, hour, zone) {
  /** @type {number[]} */
  const out = [];
  if (typeof hour !== "number") return out;
  for (let k = 0; ; k += 1) {
    const day = dayStart(from, zone, k);
    if (day >= to) break;
    const p = zoneParts(day, zone);
    const t = zonedInstant(p.y, p.m, p.d, hour, 0, zone);
    if (t >= from && t < to) out.push(t);
  }
  return out;
}

/**
 * Every start of the schedules that are on in [from, to), by its minute,
 * and the minutes of the nightly round.
 * @param {ScheduleView[]} list
 * @param {number} from
 * @param {number} to
 * @param {string} zone
 * @param {number | null | undefined} nightly
 */
function startsIn(list, from, to, zone, nightly) {
  /** @type {Map<number, ScheduleView[]>} */
  const starts = new Map();
  for (const v of list) {
    if (!v.schedule.enabled) continue;
    for (const t of runsBetween(v.schedule.when, from, to, zone)) {
      const at = starts.get(t);
      if (at) at.push(v);
      else starts.set(t, [v]);
    }
  }
  return { starts, round: new Set(nightlyRounds(from, to, nightly, zone)) };
}

/**
 * Another schedule (or the host's round) that starts in the very minute of
 * `t`, by its name.
 * @param {ReturnType<typeof startsIn>} idx
 * @param {ScheduleView | null} v the schedule asking (it never clashes with itself)
 * @param {number} t
 * @param {(s: Schedule) => string} name
 * @returns {string | null}
 */
function clashAt(idx, v, t, name) {
  if (idx.round.has(t)) return NIGHTLY_WORDS;
  const other = idx.starts.get(t)?.find((o) => o !== v);
  return other ? name(other.schedule) : null;
}

/**
 * The next `n` runs of `when` after a moment, each with what it starts
 * together with (the form's preview and the next-run card's "Then").
 * @param {ScheduleView[]} others the schedules it can clash with
 * @param {When} when
 * @param {number} after
 * @param {number} n
 * @param {{zone: string, nightly?: number | null,
 *   name: (s: Schedule) => string}} o
 * @returns {{at: number, conflict?: string}[]} `at` in ms, as kp's runs list takes it
 */
export function runsWithClash(others, when, after, n, o) {
  const runs = nextRuns(when, after, n, o.zone);
  if (!runs.length) return [];
  const idx = startsIn(
    others,
    runs[0],
    runs[runs.length - 1] + 1,
    o.zone,
    o.nightly,
  );
  return runs.map((t) => {
    const c = clashAt(idx, null, t, o.name);
    return c
      ? { at: t * 1000, conflict: `Same minute as ${c}` }
      : { at: t * 1000 };
  });
}

// ── the week ────────────────────────────────────────────────────────────

/**
 * What kp's week takes: every run of every schedule from two weeks back to
 * nine weeks ahead (the buttons move the range), a turned-off schedule
 * dimmed, the next run ringed, a run skipped on purpose struck, and the
 * host's nightly round each day as the dashed reference.
 * @param {ScheduleView[]} list
 * @param {{now: number, zone: string, nightly?: number | null,
 *   label: (action: string) => string, hostTarget?: string}} o
 * @returns {{id: string, start: number, label: string, series: Series,
 *   state?: "next" | "off" | "skipped" | "reference", detail: string}[]}
 */
export function weekEvents(list, o) {
  const { now, zone } = o;
  const hostTarget = o.hostTarget ?? "_host";
  const from = dayStart(now, zone, -14);
  const to = dayStart(now, zone, 63);
  const name = (/** @type {Schedule} */ s) =>
    scheduleTitle(o.label(s.action), s.stack, hostTarget);
  const idx = startsIn(list, from, to, zone, o.nightly);
  const next = nextUp(list);
  /** @type {ReturnType<typeof weekEvents>} */
  const out = [];
  for (const v of list) {
    const s = v.schedule;
    for (const t of runsBetween(s.when, from, to, zone)) {
      const skipped =
        s.enabled && t > now && v.next_run != null && t < v.next_run;
      const clash = s.enabled ? clashAt(idx, v, t, name) : null;
      /** @type {"next" | "off" | "skipped" | undefined} */
      const state = !s.enabled
        ? "off"
        : v === next && t === v.next_run
          ? "next"
          : skipped
            ? "skipped"
            : undefined;
      out.push({
        id: `${s.id}@${t}`,
        start: t * 1000,
        label: s.stack === hostTarget ? "whole host" : s.stack,
        series: seriesOf(s.action),
        ...(state ? { state } : {}),
        detail: `${o.label(s.action)} · ${SHORT_DAYS[zoneParts(t, zone).wd]} ${dateTimeText(t, zone)} · ${cadenceWords(s.when)}${
          !s.enabled
            ? " · turned off"
            : skipped
              ? s.paused_until && t < s.paused_until
                ? " · paused"
                : " · skipped"
              : ""
        }${clash ? ` · same minute as ${clash}` : ""}`,
      });
    }
  }
  for (const t of nightlyRounds(from, to, o.nightly, zone))
    out.push({
      id: `nightly@${t}`,
      start: t * 1000,
      label: "Host nightly round",
      series: 5,
      state: "reference",
      detail: `${SHORT_DAYS[zoneParts(t, zone).wd]} ${dateTimeText(t, zone)} · backups and updates of the host itself, for reference`,
    });
  return out;
}

// ── the busy hours ──────────────────────────────────────────────────────

/**
 * @typedef {{at: number, v: ScheduleView | null}} Start v null: the host's round
 * @typedef {{days: number[], at: number, time: string, a: string, b: string,
 *   move: ScheduleView | null}} Clash one pair at one time of day, on the
 *   weekdays (0 = Monday) it happens
 */

/**
 * The starts of the current week (Monday first) by weekday and hour, and
 * the pairs that start in the same minute, one line for the same pair at
 * the same time of day on several days.
 * @param {ScheduleView[]} list
 * @param {{now: number, zone: string, nightly?: number | null,
 *   name: (s: Schedule) => string}} o
 * @returns {{grid: Start[][][], clashes: Clash[]}}
 */
export function busyModel(list, o) {
  const monday = dayStart(o.now, o.zone, -zoneParts(o.now, o.zone).wd);
  const end = dayStart(monday, o.zone, 7);
  /** @type {Start[][][]} */
  const grid = Array.from({ length: 7 }, () =>
    Array.from({ length: 24 }, () => []),
  );
  for (const v of list) {
    if (!v.schedule.enabled) continue;
    for (const at of runsBetween(v.schedule.when, monday, end, o.zone)) {
      const p = zoneParts(at, o.zone);
      grid[p.wd][p.hh].push({ at, v });
    }
  }
  for (const at of nightlyRounds(monday, end, o.nightly, o.zone)) {
    const p = zoneParts(at, o.zone);
    grid[p.wd][p.hh].push({ at, v: null });
  }
  const who = (/** @type {Start} */ x) =>
    x.v ? o.name(x.v.schedule) : NIGHTLY_WORDS;
  /** @type {Map<string, Clash>} */
  const groups = new Map();
  for (let wd = 0; wd < 7; wd += 1)
    for (let hh = 0; hh < 24; hh += 1) {
      const cell = grid[wd][hh];
      for (let i = 0; i < cell.length; i += 1)
        for (let j = i + 1; j < cell.length; j += 1) {
          if (cell[i].at !== cell[j].at) continue;
          const time = timeText(cell[i].at, o.zone);
          const a = who(cell[i]);
          const b = who(cell[j]);
          const key = `${a}|${b}|${time}`;
          const g = groups.get(key);
          if (g) g.days.push(wd);
          else
            groups.set(key, {
              days: [wd],
              at: cell[i].at,
              time,
              a,
              b,
              move: cell[i].v ?? cell[j].v,
            });
        }
    }
  return { grid, clashes: [...groups.values()] };
}

/**
 * "Every day at 03:30" or "Mon, Thu at 03:30": when a clash happens.
 * @param {Clash} c
 */
export function clashWhen(c) {
  const d = [...c.days].sort();
  return d.length === 7
    ? `Every day at ${c.time}`
    : `${d.map((x) => SHORT_DAYS[x]).join(", ")} at ${c.time}`;
}

/**
 * The time a schedule moves to when a clash is resolved: 15 minutes later
 * (earlier when that would cross midnight, so the weekday does not change).
 * @param {string} at HH:MM
 * @returns {{at: string, later: boolean}}
 */
export function movedTime(at) {
  const { h, m } = hhmm(at);
  const t = h * 60 + m;
  const later = t + 15 < 1440;
  const next = later ? t + 15 : t - 15;
  const pad = (/** @type {number} */ n) => String(n).padStart(2, "0");
  return { at: `${pad(Math.floor(next / 60))}:${pad(next % 60)}`, later };
}

// ── coverage ────────────────────────────────────────────────────────────

/** The columns of the coverage matrix and the actions each one counts. */
export const COVERAGE = /** @type {const} */ ([
  { key: "backup", label: "Back up", actions: ["backup", "backup-native"] },
  { key: "update", label: "Update", actions: ["update", "update-native"] },
  { key: "restore-test", label: "Restore test", actions: ["verify-restore"] },
]);

/**
 * Which stack has which schedule: the stacks of the fleet (and any stack a
 * schedule names), and for each column the schedule that is on, if any.
 * @param {ScheduleView[]} list
 * @param {string[]} stacks
 * @param {string} [hostTarget]
 */
export function coverageModel(list, stacks, hostTarget = "_host") {
  const rows = [
    ...new Set([
      ...stacks,
      ...list.map((v) => v.schedule.stack).filter((s) => s !== hostTarget),
    ]),
  ];
  /** @param {string} stack @param {string} column @returns {Schedule | undefined} */
  const has = (stack, column) => {
    const c = COVERAGE.find((x) => x.key === column);
    return list.find(
      (v) =>
        v.schedule.enabled &&
        v.schedule.stack === stack &&
        /** @type {readonly string[]} */ (c?.actions ?? []).includes(
          v.schedule.action,
        ),
    )?.schedule;
  };
  return { rows, has, gaps: rows.filter((r) => !has(r, "backup")) };
}

// ── the last fourteen slots ─────────────────────────────────────────────

/**
 * The last `count` slots of a schedule as kp's uptime strip takes them,
 * oldest first: ran (a job it started), failed, missed (the host's record
 * of the newest slot that passed without a run), or no record (older than
 * the jobs the dashboard keeps, or skipped on purpose, which is not
 * recorded). A schedule with fewer slots than `count` pads the start.
 * @param {ScheduleView} v
 * @param {import("./jobs.js").Job[]} jobs
 * @param {{now: number, zone: string, count?: number}} o
 * @returns {{key: string, label: string, state: "up" | "degraded" | "down" | "none",
 *   tip?: string, details?: string[], href?: string}[]}
 */
export function slotPeriods(v, jobs, o) {
  const count = o.count ?? 14;
  const s = v.schedule;
  const slots = runsBetween(s.when, o.now - 400 * 86_400, o.now, o.zone).slice(
    -count,
  );
  /** @type {Map<number, import("./jobs.js").Job>} */
  const byslot = new Map();
  for (const j of jobs)
    if (j.origin.from === "schedule" && j.origin.schedule === s.id)
      byslot.set(j.origin.slot, j);
  /** @type {ReturnType<typeof slotPeriods>} */
  const out = [];
  for (let i = 0; i < count - slots.length; i += 1)
    out.push({ key: `none-${i}`, label: "No slot yet", state: "none" });
  for (const slot of slots) {
    const label = dateTimeText(slot, o.zone);
    const job = byslot.get(slot);
    if (job) {
      const failed = ["failed", "refused"].includes(job.state);
      const odd = ["unknown", "deferred"].includes(job.state);
      out.push({
        key: String(slot),
        label,
        state: failed ? "down" : odd ? "degraded" : "up",
        tip:
          failed || odd
            ? `${label}: job ${job.job} ${job.state}`
            : `Ran ${label} as job ${job.job}`,
        details: [`Job ${job.job} ${job.state}`],
        href: `/jobs?job=${job.job}`,
      });
    } else if (s.last_missed?.slot === slot) {
      out.push({
        key: String(slot),
        label,
        state: "down",
        tip: `Missed ${label}: ${MISSED_WHY[s.last_missed.why] ?? s.last_missed.why}`,
      });
    } else
      out.push({
        key: String(slot),
        label,
        state: "none",
        tip: `${label}: no record of this slot`,
      });
  }
  return out;
}

// ── history ─────────────────────────────────────────────────────────────

/** How many schedules the history draws a lane for. */
export const HISTORY_LANES = 7;

/**
 * The runs the schedules started in the last `days` days as kp's timeline
 * takes them, one lane per schedule (the busiest `HISTORY_LANES`), and the
 * slots the host recorded as missed as marks in the lane.
 * @param {ScheduleView[]} list
 * @param {import("./jobs.js").Job[]} jobs
 * @param {{now: number, zone: string, days?: number,
 *   label: (action: string) => string, hostTarget?: string}} o
 */
export function historyModel(list, jobs, o) {
  const since = o.now - (o.days ?? 7) * 86_400;
  const hostTarget = o.hostTarget ?? "_host";
  const byId = new Map(list.map((v) => [v.schedule.id, v]));
  const title = (/** @type {string} */ action, /** @type {string} */ stack) =>
    scheduleTitle(o.label(action), stack, hostTarget);
  /** @type {Map<string, number>} */
  const busy = new Map();
  /** @type {{id: string, lane: string, label: string, start: number,
   *   end?: number, state: "queued" | "running" | "ok" | "failed",
   *   series: Series, detail: string}[]} */
  const runs = [];
  for (const j of jobs) {
    if (j.origin.from !== "schedule") continue;
    const v = byId.get(j.origin.schedule);
    const at = j.started_at ?? j.queued_at;
    if (!v || at < since) continue;
    busy.set(v.schedule.id, (busy.get(v.schedule.id) ?? 0) + 1);
    const state =
      j.state === "done"
        ? "ok"
        : j.state === "running"
          ? "running"
          : j.state === "queued" || j.state === "deferred"
            ? "queued"
            : "failed";
    runs.push({
      id: `j${j.job}`,
      lane: v.schedule.id,
      label: title(j.action, j.stack),
      start: at * 1000,
      ...(j.finished_at != null && state !== "running" && state !== "queued"
        ? { end: j.finished_at * 1000 }
        : {}),
      state,
      series: seriesOf(j.action),
      detail:
        j.state === "unknown"
          ? `Job ${j.job}: the outcome is not known, the dashboard restarted during the run`
          : j.state === "failed" || j.state === "refused"
            ? `Job ${j.job} ${j.state}${j.message ? `: ${j.message}` : ""}`
            : `Job ${j.job} started by the schedule`,
    });
  }
  /** @type {{id: string, lane: string, at: number, kind: string,
   *   label: string, detail: string}[]} */
  const points = [];
  for (const v of list) {
    const m = v.schedule.last_missed;
    if (!m || m.slot < since) continue;
    busy.set(v.schedule.id, (busy.get(v.schedule.id) ?? 0) + 1);
    points.push({
      id: `m${v.schedule.id}@${m.slot}`,
      lane: v.schedule.id,
      at: m.slot * 1000,
      kind: "missed",
      label: title(v.schedule.action, v.schedule.stack),
      detail: `Missed ${dateTimeText(m.slot, o.zone)}: ${MISSED_WHY[m.why] ?? m.why}`,
    });
  }
  const shown = [...busy.entries()]
    .sort(
      (a, b) =>
        b[1] - a[1] ||
        (byId.get(a[0])?.next_run ?? Infinity) -
          (byId.get(b[0])?.next_run ?? Infinity),
    )
    .slice(0, HISTORY_LANES)
    .map(([id]) => id);
  const lanes = shown.flatMap((id) => {
    const v = byId.get(id);
    return v
      ? [
          {
            id,
            label: title(v.schedule.action, v.schedule.stack),
            sub: shortWhen(v.schedule.when),
          },
        ]
      : [];
  });
  return {
    lanes,
    runs: runs.filter((r) => shown.includes(r.lane)),
    points: points.filter((p) => shown.includes(p.lane)),
    hidden: busy.size - shown.length,
  };
}

/**
 * The moment a schedule's pause ends (unix seconds), or null when it is not
 * paused now (feat-kp11demo-9.5).
 * @param {Schedule} s
 * @param {number} now
 */
export const pausedUntil = (s, now) =>
  s.enabled && s.paused_until != null && s.paused_until > now
    ? s.paused_until
    : null;
