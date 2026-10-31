// feat-kpadopt-2 (kp-themes 11.0.0, the approved app-homelab-backups demo):
// what the Backups page hands to the package's components, worked out
// without a DOM — the coverage grid's spec, the month's calendar days, the
// night's status rows and the week of one stack, the repositories' 7-night
// strips and the stack bar. Pure, so the node tests pin the same shapes the
// browser draws; the page (pages/backups.js) only feeds them in.

import {
  addDays,
  cellState,
  drillState,
  drillWords,
  fleetNight,
  humanBytes,
  nightKey,
  nightsNow,
  stackNight,
} from "./backupsview.js";
import { formatClock, formatDay } from "./format.js";

/** A night (a civil date) as every page writes a day: dd/mm/yyyy. @param {string} n */
export const nightDate = (n) =>
  formatDay(Date.parse(`${n}T12:00:00Z`) / 1000, { timeZone: "UTC" });

/** @param {string} n @returns {number} 0 Sunday … 6 Saturday */
// date-key: the weekday decides which axis labels stay quiet; it is never shown.
const dayOfWeek = (n) => new Date(`${n}T12:00:00Z`).getUTCDay();

/** @param {number} n */
const pad = (n) => String(n).padStart(2, "0");

/** @param {string} n @param {string} s @param {string} what */
const aria = (n, s, what) => `${s}, ${nightDate(n)}: ${what}`;

/**
 * @typedef {import("./backupsview.js").StackRead} StackRead
 * @typedef {import("/static/kp/dist/kp-themes.min.js").HeatSpec} HeatSpec
 * @typedef {import("/static/kp/dist/kp-themes.min.js").HeatCell} HeatCell
 * @typedef {{last: string, current: string}} NightsNow
 */

/**
 * One stack on one night, as a heat cell: the words are the cell's own, the
 * tip the hover card's (the reason a night was missed comes from the host).
 * @param {string} stack
 * @param {StackRead} read
 * @param {string} night
 * @param {NightsNow} now
 * @returns {HeatCell}
 */
export function nightCell(stack, read, night, now) {
  const st = cellState(read, night, now);
  const when = nightDate(night);
  switch (st) {
    case "load":
      return { tone: "loading", label: "reading" };
    case "unread": {
      const why = read.status === "failed" ? read.reason : "not read";
      return {
        tone: "bad",
        label: "not read",
        tip: `${stack} · ${when}: the host could not read it (${why}).`,
        aria: aria(night, stack, `not read, ${why}`),
      };
    }
    case "none":
      return {
        tone: "none",
        label: "keeps no data by design",
        tip: `${stack} keeps no data by design: its files live somewhere else.`,
      };
    case "before":
      return {
        tone: "before",
        label: "before any history",
        tip: `${stack} · ${when}: before this stack had any backup history.`,
      };
    case "wait":
      return {
        tone: "future",
        label: "not run yet",
        tip: `${stack} · ${when}: tonight's round has not run yet.`,
      };
    case "miss": {
      const why = read.status === "ok" && read.reason ? `, ${read.reason}` : "";
      return {
        tone: "bad",
        label: "missed",
        tip: `${stack} · ${when}: no snapshot${why}.`,
      };
    }
    default: {
      const n = stackNight(read, night, now);
      const ids = n.ids.map((x) => `${x.owner} ${x.short_id}`).join(" · ");
      return {
        tone: "ok",
        label: "backed up",
        tip: `${stack} · ${when}: snapshot at ${n.times.map((t) => formatClock(t)).join(" and ")}.${ids ? ` ${ids}.` : ""}`,
      };
    }
  }
}

/**
 * The whole-fleet cell of one night.
 * @param {Record<string, StackRead>} reads
 * @param {string} night
 * @param {NightsNow} now
 * @param {boolean} loading
 * @returns {HeatCell}
 */
export function fleetCell(reads, night, now, loading) {
  if (loading) return { tone: "loading", label: "reading" };
  const f = fleetNight(reads, night, now);
  if (f.before) return { tone: "before", label: "before any backup history" };
  if (f.wait) return { tone: "future", label: "not run yet" };
  const tone = f.ok === f.expected ? "ok" : f.ok === 0 ? "bad" : "warn";
  return {
    tone,
    share: f.expected ? f.ok / f.expected : 0,
    label: `${f.ok} of ${f.expected} stacks backed up`,
    tip: `${nightDate(night)}: ${f.ok} of ${f.expected} stacks backed up.${f.waiting.length ? ` Waiting: ${f.waiting.join(", ")}.` : ""} Press to pin this night.`,
  };
}

/**
 * The columns of a night axis: weekends are minor in the quiet choice, so
 * the weeks show as gaps; the package names one column in `every` and the
 * last.
 * @param {string[]} nights oldest first
 * @param {"quiet" | "plain"} weekend
 * @param {number} every
 */
export function nightColumns(nights, weekend, every) {
  return nights.map((n, i) => {
    const last = i === nights.length - 1;
    const named =
      (i % every === 0 || last) &&
      !(weekend === "quiet" && !last && [0, 6].includes(dayOfWeek(n)));
    return {
      key: n,
      label: named ? String(Number(n.slice(8))) : "",
      minor: !named,
    };
  });
}

/**
 * The grid of stacks by nights.
 * @param {{names: string[], reads: Record<string, StackRead>, nights: string[],
 *   now: NightsNow, weekend: "quiet" | "plain", every: number,
 *   markOf: (stack: string) => string, fleetReady: boolean, label: string,
 *   state?: "ready" | "loading" | "empty" | "error", message?: string,
 *   shown?: Set<string>}} x
 * @returns {HeatSpec}
 */
export function nightsSpec(x) {
  const columns = nightColumns(x.nights, x.weekend, x.every);
  const pending = Object.values(x.reads).some((r) => r.status === "pending");
  const names = x.names.filter((s) => !x.shown?.size || x.shown.has(s));
  return {
    columns,
    summary: {
      key: "fleet",
      label: "Whole fleet",
      cells: x.nights.map((n) =>
        fleetCell(x.reads, n, x.now, !x.fleetReady || pending),
      ),
    },
    rows: names.map((s) => {
      const r = x.reads[s] ?? { status: "pending" };
      return {
        key: s,
        label: s,
        mark: x.markOf(s),
        muted: r.status === "ok" && r.noBackup,
        pressed: x.shown?.has(s),
        cells: x.nights.map((n) => nightCell(s, r, n, x.now)),
      };
    }),
    label: x.label,
    toggle: true,
    every: x.every,
    state: x.state ?? "ready",
    message: x.message,
    skeleton: { rows: Math.max(1, x.names.length), columns: x.nights.length },
  };
}

/**
 * The sentence under a pinned night: who missed it and why.
 * @param {Record<string, StackRead>} reads
 * @param {string} night
 * @param {NightsNow} now
 */
export function nightSentence(reads, night, now) {
  const f = fleetNight(reads, night, now);
  const when = nightDate(night);
  if (night > now.last && f.ok === 0) return `${when} has not run yet.`;
  if (f.before) return `${when}: before any backup history.`;
  if (f.expected === 0) return `${when}: no stack keeps data to back up.`;
  const missed = Object.entries(reads)
    .filter(([, r]) => cellState(r, night, now) === "miss")
    .map(([s, r]) =>
      r.status === "ok" && r.reason ? `${s} (${r.reason})` : s,
    );
  if (!missed.length)
    return `${when} pinned: ${f.ok} of ${f.expected} stacks backed up, every stack that keeps data.`;
  return `${when} pinned: ${f.ok} of ${f.expected} stacks backed up. Missed: ${missed.join(", ")}.`;
}

/**
 * Every night of a month, `YYYY-MM-DD`.
 * @param {number} year
 * @param {number} month 1-12
 */
export function monthNights(year, month) {
  const n = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return Array.from(
    { length: n },
    (_, i) => `${year}-${pad(month)}-${pad(i + 1)}`,
  );
}

/**
 * The calendar's days: the whole history the reads hold, and the days after
 * the last finished night as not run yet.
 * @param {Record<string, StackRead>} reads
 * @param {NightsNow} now
 * @param {{from: string, to: string}} range
 * @returns {Record<string, import("/static/kp/dist/kp-themes.min.js").CalendarDay>}
 */
export function calendarDays(reads, now, range) {
  const pending = Object.values(reads).some((r) => r.status === "pending");
  /** @type {Record<string, import("/static/kp/dist/kp-themes.min.js").CalendarDay>} */
  const days = {};
  for (let d = range.from; d <= range.to; d = addDays(d, 1)) {
    if (pending) {
      days[d] = { tone: "loading", label: "reading" };
      continue;
    }
    const f = fleetNight(reads, d, now);
    if (f.before) {
      days[d] = { tone: "before", label: "before any backup history" };
    } else if (f.wait || (d > now.last && f.ok === 0)) {
      days[d] = { tone: "future", label: "not run yet" };
    } else {
      const missed = f.expected - f.ok;
      days[d] = {
        tone: missed === 0 ? "ok" : f.ok === 0 ? "bad" : "warn",
        ...(missed === 0 ? {} : { count: `−${missed}` }),
        label:
          missed === 0
            ? `all ${f.expected} stacks backed up`
            : `${f.ok} of ${f.expected} stacks backed up`,
      };
    }
  }
  return days;
}

/**
 * The status list rows of one night: a stack per row, a miss with its
 * repair, a late start with its word.
 * @param {string[]} names
 * @param {Record<string, StackRead>} reads
 * @param {string} night
 * @param {NightsNow} now
 * @returns {import("/static/kp/dist/kp-themes.min.js").StatusRow[]}
 */
export function nightRows(names, reads, night, now) {
  return names.map((s) => {
    const r = reads[s] ?? { status: "pending" };
    const st = cellState(r, night, now);
    if (st === "load")
      return { id: s, label: s, detail: "reading…", state: "loading" };
    if (st === "unread")
      return {
        id: s,
        label: s,
        detail: `not read: ${r.status === "failed" ? r.reason : "no answer"}`,
        state: "failed",
        fix: `Read ${s} again`,
      };
    if (st === "none")
      return {
        id: s,
        label: s,
        detail: "keeps no data by design",
        state: "empty",
      };
    if (st === "before")
      return {
        id: s,
        label: s,
        detail: "before this stack had any history",
        state: "empty",
      };
    if (st === "wait")
      return {
        id: s,
        label: s,
        detail: "tonight's round has not run yet",
        state: "empty",
      };
    if (st === "miss")
      return {
        id: s,
        label: s,
        detail: `missed${r.status === "ok" && r.reason ? `: ${r.reason}` : ""}`,
        state: "failed",
        fix: `Back up ${s} now`,
      };
    const n = stackNight(r, night, now);
    const late = n.times.length > 0 && lateStart(n.times[0]);
    return {
      id: s,
      label: s,
      detail: n.times.map((t) => formatClock(t)).join(", "),
      state: late ? "warn" : "ok",
      ...(late ? { fix: "Started late" } : {}),
    };
  });
}

/** A round that begins after 03:35 is late (the nightly round starts at 03:00). @param {number} unix */
export const lateStart = (unix) => {
  const [h, m] = formatClock(unix).split(":").map(Number);
  return h * 60 + m >= 3 * 60 + 35 && h < 12;
};

/**
 * The summary line over the night's rows: exact counts.
 * @param {{ok: number, warn: number, failed: number, empty: number, loading: number, total: number}} c
 * @param {string} night
 */
export function nightSummary(c, night) {
  if (c.loading > 0) return `${c.total - c.loading} of ${c.total} stacks read`;
  const live = c.total - c.empty;
  return `${c.ok + c.warn} of ${live} backed up on ${nightDate(night)}`;
}

/**
 * The seven nights of a repository as uptime periods.
 * @param {StackRead} read
 * @param {import("./backupsview.js").Repo} repo
 * @param {NightsNow} now
 * @returns {import("/static/kp/dist/kp-themes.min.js").UptimePeriod[]}
 */
export function repoPeriods(read, repo, now) {
  const have = new Map();
  for (const x of repo.snapshots ?? []) have.set(nightKey(x.time), x);
  return Array.from({ length: 7 }, (_, i) => {
    const night = addDays(now.last, i - 6);
    const label = nightDate(night);
    const snap = have.get(night);
    if (snap)
      return {
        key: night,
        label,
        state: "up",
        tip: `${label}: snapshot ${formatClock(snap.time)}, ${snap.short_id}`,
      };
    const st = cellState(read, night, now);
    if (st === "miss")
      return {
        key: night,
        label,
        state: "down",
        tip: `${label}: missed${read.status === "ok" && read.reason ? `, ${read.reason}` : ""}`,
      };
    return { key: night, label, state: "none", tip: `${label}: no history` };
  });
}

/**
 * Parts of the stack bar: the repositories' sizes per stack, biggest first.
 * `value` is bytes; a repository without a measured size counts as none.
 * @param {{stack: string, size_bytes?: number | null}[]} repos
 */
export function stackSizes(repos) {
  /** @type {Map<string, {stack: string, bytes: number, count: number}>} */
  const by = new Map();
  for (const r of repos) {
    const e = by.get(r.stack) ?? { stack: r.stack, bytes: 0, count: 0 };
    e.bytes += r.size_bytes ?? 0;
    e.count++;
    by.set(r.stack, e);
  }
  return [...by.values()].sort((a, b) => b.bytes - a.bytes);
}

/** @param {number} bytes */
export const sizeText = (bytes) => humanBytes(bytes) ?? "—";

export { drillState, drillWords, nightsNow };
