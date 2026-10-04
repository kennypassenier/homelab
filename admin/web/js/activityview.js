// redesign-activity (3.71.0, the Activity demo Kenny approved on
// 2026-10-03): the pure half of the Activity page. The host's history
// (`/data/history`: operations and nightly phases), its incident bundles and
// the dashboard's own jobs, turned into what the page shows: who started
// each operation, the day groups of the History feed, its filters, the KPI
// strip and the open incidents of the attention band. No DOM; the clock
// comes in as an argument, so `node --test` drives every line.

import { isSecretAudit } from "./activity.js";
import { formatClock, formatDay, humanDuration } from "./format.js";
import { nameKey } from "./namekey.js";

/**
 * @typedef {import("./activity.js").Entry} Entry
 * @typedef {import("./activity.js").OpEntry} OpEntry
 * @typedef {import("./jobs.js").Job} Job
 * @typedef {"claude" | "nightly" | "schedule" | "person" | "host"} ActorKind
 * @typedef {{text: string, kind: ActorKind}} Actor
 * @typedef {{show: Set<string>, q: string,
 *   range: {from: number, to: number} | null}} FeedFilter
 *   `show`: "failed", "nightly", "claude" (none: everything; several: any
 *   of them)
 */

/** The windows the header offers, in days. */
export const WINDOWS = /** @type {const} */ ([1, 7, 14, 30]);

/** The History feed's "Show" chips, in their fixed order. */
export const SHOW = /** @type {const} */ ([
  { value: "failed", label: "Failed", hint: "Only what failed (F)" },
  { value: "nightly", label: "Nightly", hint: "Only the nightly round" },
  {
    value: "claude",
    label: "By Claude",
    hint: "What Claude did through Live view",
  },
]);

/** Verbs a person reads, by the host's operation label. */
const VERBS = /** @type {Record<string, string>} */ ({
  deploy: "Deploy",
  backup: "Back up",
  "scheduled-backup": "Back up",
  "backup-native": "Back up",
  "scheduled-backup-native": "Back up",
  restore: "Restore",
  update: "Update",
  "scheduled-update": "Update",
  "update-native": "Update",
  "scheduled-update-native": "Update",
  "release-update": "Update the host",
  "scheduled-release-update": "Update the host",
  resize: "Resize",
  destroy: "Destroy",
  disable: "Park",
  enable: "Unpark",
  guards: "Add log guards",
  "apply-guards": "Add log guards",
  "patch-fleet": "Patch the fleet",
  "reveal-secret": "Reveal a secret",
  "copy-secret": "Copy a secret",
});

/**
 * The verb of an operation label: "scheduled-backup" → "Back up", an
 * unknown "self-test" → "Self test".
 * @param {string} label
 */
export function verbOf(label) {
  if (VERBS[label]) return VERBS[label];
  const w = label.replace(/[-_]+/g, " ").trim();
  return w ? w[0].toUpperCase() + w.slice(1) : "Operation";
}

/**
 * The stack an operation was about, from its subject ("backup-notes",
 * "deploy media"): the longest fleet name it ends with; null when it is
 * about the host or no stack the fleet knows.
 * @param {string | null | undefined} subject
 * @param {string[]} stacks
 * @returns {string | null}
 */
export function stackOf(subject, stacks) {
  if (!subject) return null;
  let best = null;
  for (const n of stacks)
    if (
      (subject === n ||
        subject.endsWith(`-${n}`) ||
        subject.endsWith(` ${n}`)) &&
      (!best || n.length > best.length)
    )
      best = n;
  return best;
}

/**
 * Who a session's token belongs to, as a person reads it (senior review,
 * finding 8): the dashboard's own token is Kenny; a workstation's token
 * (a lower-case machine name, fix-120's per-machine tokens) is Kenny on
 * that machine's CLI or TUI; a name the host already gives ("Claude (Live
 * view)", a schedule) stays as it is.
 * @param {string} by
 */
export function ownerOf(by) {
  if (by === "admin") return "Kenny";
  if (/^[a-z0-9][a-z0-9._-]*$/.test(by)) return `Kenny · CLI on ${by}`;
  return by;
}

/**
 * Who started an entry, as the History's "By" chip names it: the nightly
 * round, a schedule, Claude through Live view, a person or a workstation,
 * or the host on its own. A job the dashboard ran knows its origin; the
 * host's own line knows the token that asked (`by`).
 * @param {Entry} e
 * @param {Map<number, import("./jobs.js").Origin>} [origins] a request id's
 *   dashboard job origin
 * @returns {Actor}
 */
export function actorOf(e, origins) {
  if (e.kind === "phase") return { text: "the host", kind: "nightly" };
  const o = e.req != null ? origins?.get(e.req) : undefined;
  if (o?.from === "claude")
    return { text: "Claude (Live view)", kind: "claude" };
  if (e.by && /claude/i.test(e.by)) return { text: e.by, kind: "claude" };
  if (o?.from === "schedule")
    return { text: `schedule “${o.schedule}”`, kind: "schedule" };
  if (e.by && /^schedule\b/i.test(e.by))
    return { text: e.by, kind: "schedule" };
  if (e.req == null && /^scheduled-/.test(e.label))
    return { text: "nightly round", kind: "nightly" };
  if (e.by) return { text: ownerOf(e.by), kind: "person" };
  if (e.req != null) return { text: "asked", kind: "person" };
  return { text: "the host", kind: "host" };
}

/**
 * @typedef {{key: string, entry: Entry, start: number, took: number | null,
 *   verb: string, stack: string | null, what: string,
 *   tone: "ok" | "bad" | "warn" | "info", state: string, actor: Actor,
 *   error: string | null, search: string}} FeedRow
 */

/**
 * One History row from one entry.
 * @param {Entry} e
 * @param {{stacks: string[], origins?: Map<number, any>}} ctx
 * @returns {FeedRow}
 */
export function feedRow(e, ctx) {
  const actor = actorOf(e, ctx.origins);
  const took = e.end > 0 && e.end >= e.start ? e.end - e.start : null;
  if (e.kind === "phase") {
    const what = `Nightly round · ${e.count} ${e.count === 1 ? "stack" : "stacks"}`;
    return {
      key: `p${e.start}`,
      entry: e,
      start: e.start,
      took,
      verb: "Nightly round",
      stack: null,
      what,
      tone: "info",
      state: "round",
      actor,
      error: null,
      search: `${what} nightly ${e.name}`.toLowerCase(),
    };
  }
  const audit = isSecretAudit(e);
  const verb = verbOf(e.label);
  const stack = audit ? null : stackOf(e.subject, ctx.stacks);
  const what = audit
    ? `${actor.text} ${e.subject ?? e.label}`
    : stack
      ? `${verb} ${stack}`
      : (e.subject ?? verb);
  const running = e.end === 0 || e.end < e.start;
  const tone = e.deferred ? "warn" : running ? "warn" : e.ok ? "ok" : "bad";
  const state = e.deferred
    ? "deferred"
    : running
      ? "running"
      : e.ok
        ? "ok"
        : "failed";
  return {
    key: `o${e.start}-${e.label}-${e.subject ?? ""}-${e.req ?? ""}`,
    entry: e,
    start: e.start,
    took,
    verb,
    stack,
    what,
    tone,
    state,
    actor,
    error: plainError(e.deferred ?? e.error ?? null),
    search:
      `${what} ${e.label} ${e.subject ?? ""} ${e.error ?? ""} ${actor.text}`.toLowerCase(),
  };
}

/**
 * Every row, newest first.
 * @param {Entry[]} entries
 * @param {{stacks: string[], origins?: Map<number, any>}} ctx
 */
export const feedRows = (entries, ctx) =>
  entries
    .filter((e) => e && (e.kind === "op" || e.kind === "phase"))
    .map((e) => feedRow(e, ctx))
    .sort((a, b) => b.start - a.start);

/**
 * Whether a row passes the feed's filter: a time range from the timeline,
 * any of the "Show" chips, and the search text.
 * @param {FeedRow} r
 * @param {FeedFilter} f
 */
export function rowMatches(r, f) {
  if (f.range && (r.start < f.range.from || r.start > f.range.to)) return false;
  if (f.show.size) {
    const any =
      (f.show.has("failed") && r.state === "failed") ||
      (f.show.has("nightly") && r.actor.kind === "nightly") ||
      (f.show.has("claude") && r.actor.kind === "claude");
    if (!any) return false;
  }
  const q = f.q.trim().toLowerCase();
  if (q && !q.split(/\s+/).every((w) => r.search.includes(w))) return false;
  return true;
}

/**
 * The Show switch's exact counts (the demo's "All 129 · Failed 3"): the
 * rows the days and the search let through, and how many of them failed.
 * @param {FeedRow[]} rows
 * @param {FeedFilter} f
 */
export function showCounts(rows, f) {
  const base = rows.filter((r) => rowMatches(r, { ...f, show: new Set() }));
  return {
    all: base.length,
    failed: base.filter((r) => r.state === "failed").length,
  };
}

/**
 * The start of the local day a moment falls in, unix seconds.
 * @param {number} t
 */
export function dayStart(t) {
  const d = new Date(t * 1000);
  d.setHours(0, 0, 0, 0);
  return Math.floor(d.getTime() / 1000);
}

/**
 * An operation's error as a person reads it (redesign-final M8): the host
 * appends " :: incident bundle <name>" to a failure; the row has its own
 * Open the incident, so the file name goes.
 * @param {string | null} e
 * @returns {string | null}
 */
export const plainError = (e) =>
  e == null ? null : e.replace(/\s*::\s*incident bundle \S+/g, "").trim() || e;

/**
 * "Today · 03/10/2026", "Yesterday · 02/10/2026", "01/10/2026".
 * @param {number} day a dayStart
 * @param {number} now
 * @param {{locale?: string, timeZone?: string}} [opts]
 */
export function dayLabel(day, now, opts = {}) {
  const words = formatDay(day, opts);
  const today = dayStart(now);
  if (day === today) return `Today · ${words}`;
  if (day === dayStart(today - 43200)) return `Yesterday · ${words}`;
  return words;
}

/**
 * The feed's rows grouped by day, newest day first, each with its count of
 * operations and failures for the sticky day header.
 * @param {FeedRow[]} rows newest first
 * @returns {{day: number, rows: FeedRow[], ops: number, failed: number}[]}
 */
export function byDay(rows) {
  /** @type {Map<number, FeedRow[]>} */
  const m = new Map();
  for (const r of rows) {
    const k = dayStart(r.start);
    const list = m.get(k);
    if (list) list.push(r);
    else m.set(k, [r]);
  }
  return [...m].map(([day, rs]) => ({
    day,
    rows: rs,
    ops: rs.filter((r) => r.entry.kind === "op").length,
    failed: rs.filter((r) => r.state === "failed").length,
  }));
}

/**
 * The failed operations nothing has fixed since: no later operation of the
 * same label on the same subject succeeded. Newest first.
 * @param {FeedRow[]} rows newest first
 * @returns {FeedRow[]}
 */
export function openFailures(rows) {
  const out = [];
  for (let i = 0; i < rows.length; i++) {
    const r = rows[i];
    if (r.state !== "failed" || r.entry.kind !== "op") continue;
    const e = r.entry;
    const fixed = rows
      .slice(0, i)
      .some(
        (x) =>
          x.entry.kind === "op" &&
          x.state === "ok" &&
          x.entry.label === e.label &&
          (x.entry.subject ?? "") === (e.subject ?? ""),
      );
    if (!fixed) out.push(r);
  }
  return out;
}

/**
 * The incident bundle a failed operation left, by name
 * (`<unix seconds>-<operation>`): the one about the same subject that was
 * written closest to the failure (within an hour).
 * @param {FeedRow} r
 * @param {string[]} names
 * @returns {string | null}
 */
export function incidentFor(r, names) {
  if (r.entry.kind !== "op") return null;
  const e = r.entry;
  const subj = nameKey(e.subject ?? e.label);
  let best = null;
  let gap = Infinity;
  for (const n of names) {
    const m = /^(\d+)-(.+)$/.exec(n);
    if (!m) continue;
    const at = Number(m[1]);
    const d = Math.abs(at - (e.end || e.start));
    // Names compare by their key (namekey.js): "update kp-soft" is
    // "update-kp-soft".
    const op = nameKey(m[2]);
    const about =
      op === subj || (r.stack != null && op.endsWith(`-${nameKey(r.stack)}`));
    if (about && d <= 3600 && d < gap) {
      best = n;
      gap = d;
    }
  }
  return best;
}

/**
 * How many entries per day matched, oldest day first, for a sparkline.
 * @param {FeedRow[]} rows
 * @param {number} days
 * @param {number} now
 * @param {(r: FeedRow) => boolean} pred
 */
export function perDay(rows, days, now, pred) {
  const t0 = dayStart(now) - (days - 1) * 86400;
  const out = Array.from({ length: days }, () => 0);
  for (const r of rows) {
    if (!pred(r)) continue;
    const i = Math.floor((dayStart(r.start) - t0) / 86400);
    if (i >= 0 && i < days) out[i] += 1;
  }
  return out;
}

/**
 * @param {number} t
 * @param {{locale?: string, timeZone?: string}} [opts]
 */
export const clock = (t, opts = {}) => formatClock(t, opts);

/**
 * The KPI strip: operations, the share that succeeded, open incidents, the
 * last nightly round and what runs now. Exact numbers (invariant 60).
 * @param {{rows: FeedRow[], days: number, now: number, open: FeedRow[],
 *   running: Job[], runningText: string,
 *   opts?: {locale?: string, timeZone?: string}}} x
 */
export function kpis(x) {
  const ops = x.rows.filter(
    (r) => r.entry.kind === "op" && r.state !== "running",
  );
  const failed = ops.filter((r) => r.state === "failed");
  const pct = ops.length
    ? (((ops.length - failed.length) / ops.length) * 100).toFixed(1)
    : "—";
  const phases = x.rows.filter((r) => r.entry.kind === "phase");
  const last = phases[0] ?? null;
  /** @type {string} */
  let nightCtx = "no nightly round in this window";
  /** @type {"ok" | "warn" | "bad" | null} */
  let nightTone = null;
  if (last && last.entry.kind === "phase") {
    const p = last.entry;
    const inRound = x.rows.filter(
      (r) =>
        r.entry.kind === "op" &&
        r.actor.kind === "nightly" &&
        r.start >= p.start &&
        r.start <= (p.end || p.start + 6 * 3600),
    );
    const ok = inRound.filter((r) => r.state === "ok").length;
    const parts = [
      `last: ${last.took == null ? "running" : humanDuration(last.took)}`,
    ];
    if (inRound.length) parts.push(`${ok} of ${p.count} backed up`);
    nightCtx = parts.join(" · ");
    nightTone = inRound.length && ok < p.count ? "warn" : null;
  }
  const days = x.days;
  return [
    {
      key: "ops",
      label: "Operations",
      value: String(ops.length),
      ctx: `${Math.round(ops.length / days)} a day on average`,
      spark: perDay(x.rows, days, x.now, (r) => r.entry.kind === "op"),
    },
    {
      key: "ok",
      label: "Succeeded",
      value: pct,
      unit: ops.length ? "%" : "",
      ctx: `${failed.length} failed in ${days} ${days === 1 ? "day" : "days"}`,
      spark: perDay(
        x.rows,
        days,
        x.now,
        (r) => r.entry.kind === "op" && r.state === "ok",
      ),
    },
    {
      key: "incidents",
      label: "Open incidents",
      value: String(x.open.length),
      ctx: x.open.length
        ? `${x.open[0].what}, ${dayWord(x.open[0].start, x.now, x.opts)} ${clock(x.open[0].start, x.opts)}`
        : "nothing failed that is still failing",
      tone: x.open.length ? "bad" : null,
    },
    {
      key: "nightly",
      label: "Nightly round",
      value: last ? clock(last.start, x.opts) : "—",
      ctx: nightCtx,
      tone: nightTone,
      spark: phases
        .slice()
        .reverse()
        .map((r) => r.took ?? 0),
    },
    {
      key: "running",
      label: "Running now",
      value: String(x.running.length),
      ctx: x.runningText || "nothing runs right now",
    },
  ];
}

/**
 * "today", "yesterday" or the date.
 * @param {number} t
 * @param {number} now
 * @param {{locale?: string, timeZone?: string}} [opts]
 */
export function dayWord(t, now, opts = {}) {
  const d = dayStart(t);
  if (d === dayStart(now)) return "today";
  if (d === dayStart(dayStart(now) - 43200)) return "yesterday";
  return formatDay(t, opts);
}

/**
 * The steps strip of a running job: one segment per step when the host
 * says how many there are (at most 12), else none (the bar shows a
 * percentage instead).
 * @param {import("./jobs.js").Progress | null} p
 * @returns {{n: number, state: "done" | "run" | "todo"}[]}
 */
export function stepSegments(p) {
  if (!p || !p.m || p.m > 12) return [];
  const cur = p.finished ? p.n + 1 : p.n;
  return Array.from({ length: p.m }, (_, i) => ({
    n: i + 1,
    state: i + 1 < cur ? "done" : i + 1 === cur ? "run" : "todo",
  }));
}

/**
 * The feed's filter from the address (`?show=failed,claude&q=notes&from=…&to=…`).
 * @param {URLSearchParams} p
 * @returns {FeedFilter}
 */
export function filterFromParams(p) {
  // One of All / Failed / Nightly / By Claude (the demo's one-of switch):
  // an older address naming several keeps the first known one.
  const first = (p.get("show") ?? "")
    .split(",")
    .find((v) => SHOW.some((s) => s.value === v));
  const show = new Set(first ? [first] : []);
  const from = Number(p.get("from"));
  const to = Number(p.get("to"));
  return {
    show,
    q: p.get("q") ?? "",
    range:
      Number.isFinite(from) && Number.isFinite(to) && from > 0 && to > from
        ? { from, to }
        : null,
  };
}

/**
 * The address's query for a filter (null drops a key).
 * @param {FeedFilter} f
 * @returns {Record<string, string | null>}
 */
export const filterToParams = (f) => ({
  show: f.show.size
    ? SHOW.filter((s) => f.show.has(s.value))
        .map((s) => s.value)
        .join(",")
    : null,
  q: f.q.trim() || null,
  from: f.range ? String(Math.floor(f.range.from)) : null,
  to: f.range ? String(Math.ceil(f.range.to)) : null,
});

/**
 * The window from `?days=`, 14 by default.
 * @param {URLSearchParams} p
 */
export function windowDays(p) {
  const d = Number(p.get("days"));
  return /** @type {readonly number[]} */ (WINDOWS).includes(d) ? d : 14;
}
