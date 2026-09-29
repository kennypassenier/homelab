// feat-ops-6, feat-stacks-5: the jobs the dashboard runs, as the page shows
// them. Pure view models over the server's JobView, the `action_progress`
// and `action_log` events and the `action_batch` event; the clock comes in
// as an argument, so `node --test` can drive every line.

import { formatTime, humanDuration } from "./format.js";

/**
 * @typedef {{op: string, step: string, n: number, m: number | null,
 *   finished: boolean, changed: boolean, expected_step_s: number | null,
 *   expected_total_s: number | null, expected_remaining_s: number | null,
 *   elapsed_s: number, runs: number}} Progress
 * @typedef {{from: "manual"} | {from: "batch", batch: number} |
 *   {from: "schedule", schedule: string, slot: number} |
 *   {from: "claude", by: string}} Origin
 * @typedef {"queued" | "running" | "done" | "failed" | "deferred" |
 *   "refused" | "unknown"} JobState
 * @typedef {{job: number, origin: Origin, stack: string, action: string,
 *   args: Record<string, unknown>, state: JobState, queued_at: number,
 *   started_at: number | null, finished_at: number | null, reqs: number[],
 *   message: string | null, cli: string | null, progress: Progress | null,
 *   restarts_dashboard: boolean}} Job
 * @typedef {{job: number, req: number, level: string, source: string,
 *   msg: string, ts: number}} LogLine
 * @typedef {{job: number, stack: string, state: JobState,
 *   message: string | null}} BatchJob
 * @typedef {{batch: number, jobs: BatchJob[], done: number, ok: number,
 *   failed: number, deferred: number}} Batch
 * @typedef {{label: string, tone: "ok" | "warn" | "bad" | "info"}} Badge
 */

/** How many jobs the page keeps, as the server does. */
export const KEEP_JOBS = 200;
/** How many log lines the page keeps per job. */
export const KEEP_LINES = 500;

/** The state column's sort order: what needs a look first. */
export const STATE_ORDER =
  "failed,refused,unknown,running,queued,deferred,done";

/** @type {Record<JobState, Badge>} */
const STATES = {
  queued: { label: "queued", tone: "info" },
  running: { label: "running", tone: "info" },
  done: { label: "done", tone: "ok" },
  failed: { label: "failed", tone: "bad" },
  deferred: { label: "deferred", tone: "warn" },
  refused: { label: "refused", tone: "bad" },
  unknown: { label: "unknown", tone: "warn" },
};

/** @param {JobState} s @returns {Badge} */
export const jobBadge = (s) => STATES[s] ?? { label: String(s), tone: "warn" };

/** @param {JobState} s */
export const finished = (s) => s !== "queued" && s !== "running";

/**
 * A job's new view in the list, newest first, the oldest dropped past
 * `KEEP_JOBS`. A progress event the page kept is not lost when the job
 * view without it arrives.
 * @param {Job[]} jobs
 * @param {Job} view
 * @returns {Job[]}
 */
export function upsertJob(jobs, view) {
  const old = jobs.find((j) => j.job === view.job);
  const merged =
    old && !view.progress && old.progress
      ? { ...view, progress: old.progress }
      : view;
  const rest = jobs.filter((j) => j.job !== view.job);
  return [merged, ...rest].sort((a, b) => b.job - a.job).slice(0, KEEP_JOBS);
}

/**
 * An `action_progress` event laid on its job.
 * @param {Job[]} jobs
 * @param {{job: number, progress: Progress}} ev
 * @returns {Job[]}
 */
export function applyProgress(jobs, ev) {
  return jobs.map((j) =>
    j.job === ev.job
      ? {
          ...j,
          progress: ev.progress,
          state: j.state === "queued" ? "running" : j.state,
        }
      : j,
  );
}

/**
 * An `action_log` line added to its job's lines, the oldest dropped.
 * @param {Map<number, LogLine[]>} logs
 * @param {LogLine} line
 * @param {number} [cap]
 */
export function addLog(logs, line, cap = KEEP_LINES) {
  const list = logs.get(line.job) ?? [];
  list.push(line);
  if (list.length > cap) list.splice(0, list.length - cap);
  logs.set(line.job, list);
  return list;
}

/**
 * Where a job came from, in words.
 * @param {Origin} o
 */
export function originText(o) {
  if (o.from === "batch") return `batch ${o.batch}`;
  if (o.from === "schedule") return "schedule";
  if (o.from === "claude") return `Claude (${o.by})`;
  return "by hand";
}

/**
 * "Step 3/35", or "Step 3" when no run before ever finished.
 * @param {Job} j
 */
export function stepText(j) {
  const p = j.progress;
  if (!p) {
    if (j.state === "queued") return "waiting in the queue";
    if (j.state === "running") return "starting";
    return "—";
  }
  return p.m ? `step ${p.n}/${p.m}` : `step ${p.n}`;
}

/**
 * How long a job has run: up to now while it runs, start to end after.
 * @param {Job} j
 * @param {number} now unix seconds
 * @returns {number | null} seconds
 */
export function elapsedS(j, now) {
  if (j.started_at == null) return null;
  const end = j.finished_at ?? now;
  return Math.max(0, end - j.started_at);
}

/**
 * What is left by the medians of earlier runs, counted down from the moment
 * the last progress event arrived.
 * @param {Job} j
 * @param {number | null} progressAt unix seconds the last progress arrived
 * @param {number} now
 * @returns {{text: string, late: boolean, s: number | null}}
 */
export function remaining(j, progressAt, now) {
  const p = j.progress;
  if (finished(j.state)) return { text: "", late: false, s: null };
  if (!p) return { text: "", late: false, s: null };
  if (p.expected_remaining_s == null)
    return {
      text:
        p.runs === 0
          ? "no earlier run to go by"
          : "no estimate for these steps",
      late: false,
      s: null,
    };
  const left = p.expected_remaining_s - Math.max(0, now - (progressAt ?? now));
  if (left <= 0)
    return { text: "taking longer than earlier runs", late: true, s: 0 };
  return {
    text: `about ${humanDuration(left)} left`,
    late: false,
    s: left,
  };
}

/**
 * How far along, 0 to 100, or null when nothing says (the bar is then
 * indeterminate, busy). Time-based when earlier runs give a total, but
 * never behind the steps already finished (a slow run's time share stayed
 * near 0 while its steps went by, Kenny 2026-09-29); step-based without a
 * time; never 100 before the end.
 * @param {Job} j
 * @param {number | null} progressAt
 * @param {number} now
 * @returns {number | null}
 */
export function percent(j, progressAt, now) {
  if (finished(j.state)) return j.state === "done" ? 100 : null;
  const p = j.progress;
  if (!p) return j.state === "queued" ? 0 : null;
  const left = remaining(j, progressAt, now).s;
  const ran = elapsedS(j, now);
  const byTime =
    left != null && ran != null && ran + left > 0
      ? Math.round((ran / (ran + left)) * 100)
      : null;
  const bySteps = p.m
    ? Math.round(((p.n - (p.finished ? 0 : 1)) / p.m) * 100)
    : null;
  if (byTime == null && bySteps == null) return null;
  return Math.min(99, Math.max(byTime ?? 0, bySteps ?? 0));
}

/**
 * The words under a finished job: what came of it, and what to do.
 * @param {Job} j
 * @returns {{tone: "success" | "destructive" | "warning" | "info", title: string, text: string} | null}
 */
export function outcome(j) {
  const msg = j.message ?? "";
  switch (j.state) {
    case "done":
      return {
        tone: "success",
        title: "Done",
        text: msg || "The host finished the job.",
      };
    case "failed":
      return {
        tone: "destructive",
        title: "Failed",
        text:
          msg || "The host reported a failure; the log above has the details.",
      };
    case "deferred":
      return {
        tone: "info",
        title: "Deferred",
        text: msg || "The host stood aside on purpose; nothing changed.",
      };
    case "refused":
      return {
        tone: "destructive",
        title: "Refused",
        text: msg || "The dashboard refused before anything reached the host.",
      };
    case "unknown":
      return {
        tone: "warning",
        title: "Outcome unknown",
        text: `${msg ? `${msg}. ` : ""}The line dropped before the answer; the Activity page's history tells what happened.`,
      };
    default:
      return null;
  }
}

/**
 * Everything the running-job panel shows (feat-ops-6).
 * @param {Job} j
 * @param {{label: (action: string) => string, progressAt: number | null,
 *   now: number}} ctx
 */
export function jobPanel(j, ctx) {
  const ran = elapsedS(j, ctx.now);
  const left = remaining(j, ctx.progressAt, ctx.now);
  const p = j.progress;
  return {
    title: `${ctx.label(j.action)} · ${j.stack === "_host" ? "the whole host" : j.stack}`,
    badge: jobBadge(j.state),
    step: stepText(j),
    stepName: p?.step ?? "",
    elapsed: ran == null ? "not started" : humanDuration(ran),
    remaining: left.text,
    late: left.late,
    percent: percent(j, ctx.progressAt, ctx.now),
    finished: finished(j.state),
    outcome: outcome(j),
    cli: j.cli,
    origin: originText(j.origin),
    restarts: j.restarts_dashboard,
    basis:
      p && p.runs > 0
        ? `expected from ${p.runs} earlier ${p.runs === 1 ? "run" : "runs"}`
        : "",
  };
}

/**
 * The jobs table's rows.
 * @param {Job[]} jobs
 * @param {(action: string) => string} label
 * @param {number} now
 */
export function jobRows(jobs, label, now) {
  return jobs.map((j) => {
    const ran = elapsedS(j, now);
    return {
      job: j.job,
      queued: j.queued_at,
      stack: j.stack === "_host" ? "host" : j.stack,
      action: label(j.action),
      origin: originText(j.origin),
      badge: jobBadge(j.state),
      took: ran,
      tookText: ran == null ? "—" : humanDuration(ran),
      step: stepText(j),
      message: j.message ?? "",
    };
  });
}

/**
 * A batch's progress (feat-stacks-5).
 * @param {Batch} b
 */
export function batchView(b) {
  const total = b.jobs.length;
  const parts = [`${b.done} of ${total} finished`];
  if (b.ok) parts.push(`${b.ok} done`);
  if (b.failed) parts.push(`${b.failed} failed`);
  if (b.deferred) parts.push(`${b.deferred} deferred`);
  return {
    total,
    percent: total ? Math.round((b.done / total) * 100) : 0,
    text: parts.join(" · "),
    finished: total > 0 && b.done >= total,
    rows: b.jobs.map((j) => ({
      job: j.job,
      stack: j.stack,
      badge: jobBadge(j.state),
      message: j.message ?? "",
    })),
  };
}

/**
 * A log line as the kp log component draws it.
 * @param {LogLine} l
 * @param {{locale?: string, timeZone?: string}} [opts]
 */
export function logLine(l, opts = {}) {
  const level = String(l.level || "info").toLowerCase();
  const severity =
    level.startsWith("err") || level === "fail"
      ? "error"
      : level.startsWith("warn")
        ? "warning"
        : level === "debug"
          ? "debug"
          : "info";
  return {
    time: new Intl.DateTimeFormat(opts.locale, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      timeZone: opts.timeZone,
    }).format(new Date(l.ts * 1000)),
    source: l.source || "host",
    level,
    severity,
    msg: l.msg,
  };
}

/**
 * The moment a job was queued, as a person reads it.
 * @param {Job} j
 * @param {{locale?: string, timeZone?: string}} [opts]
 */
export const queuedText = (j, opts) => formatTime(j.queued_at, opts);
