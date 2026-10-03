// feat-ops-6, feat-stacks-5: the jobs the dashboard runs, as the page shows
// them. Pure view models over the server's JobView, the `action_progress`
// and `action_log` events and the `action_batch` event; the clock comes in
// as an argument, so `node --test` can drive every line.

import { formatDateTime, humanDuration } from "./format.js";

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
 *   restarts_dashboard: boolean, flow?: any}} Job
 *   `flow` (redesign-flows-6): the Update flow job's own rows and moves.
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
 * arch-self: the job's `subject` in the host's history, as the host writes
 * it (`{action}-{stack}`, e.g. `install-native-admin`). The route that
 * reads its outcome back takes this as `op`.
 * @param {Job} j
 */
export const outcomeSubject = (j) => `${j.action}-${j.stack}`;

/**
 * arch-self: jobs whose action restarts the dashboard, still held here as
 * queued or running. The dashboard's own memory of how they ended did not
 * survive the restart; these are the ones a reconnect must ask the host's
 * history about.
 * @param {Job[]} jobs
 * @returns {Job[]}
 */
export const jobsAwaitingOutcome = (jobs) =>
  jobs.filter((j) => j.restarts_dashboard && !finished(j.state));

/**
 * @typedef {{found: boolean, ok: boolean, steps: number, end: number}} Outcome
 */

/**
 * A held job (arch-self) patched with what `/data/actions/outcome` answered:
 * done/failed with the real final step count when the host's history has
 * it, otherwise "unknown" with a message saying where the real outcome is.
 * Pure, so the polling loop that calls it (act.js) stays a thin shell.
 * @param {Job[]} jobs
 * @param {number} jobId
 * @param {Outcome} out
 * @returns {Job[]}
 */
export function applyOutcome(jobs, jobId, out) {
  return jobs.map((j) => {
    if (j.job !== jobId) return j;
    if (out.found) {
      const steps = out.steps || null;
      return {
        ...j,
        state: out.ok ? "done" : "failed",
        finished_at: out.end,
        message:
          j.message ??
          "the dashboard restarted during this job; this outcome comes from the host's jobs history",
        progress: steps
          ? {
              op: j.progress?.op ?? outcomeSubject(j),
              step: j.progress?.step ?? "",
              n: steps,
              m: steps,
              finished: true,
              changed: j.progress?.changed ?? false,
              expected_step_s: null,
              expected_total_s: null,
              expected_remaining_s: null,
              elapsed_s: j.progress?.elapsed_s ?? 0,
              runs: j.progress?.runs ?? 0,
            }
          : j.progress,
      };
    }
    return {
      ...j,
      state: "unknown",
      message:
        "the dashboard restarted during this job; the jobs history on the host has the outcome",
    };
  });
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
 * indeterminate, busy). A step total, once the host sends one, is the
 * bar's one truth: step-based, never 100 before the end. Time history
 * (earlier runs' durations) feeds only `remaining()` — it used to also
 * feed the bar via `max(byTime, bySteps)`, which pinned a 263/310 run at
 * 99% because an earlier, shorter run made the time share read "almost
 * done" (Kenny, 2026-10-02, fix-189) while barely a third of the steps
 * were in. Without a step total, time is still the only thing the bar has
 * to go by, so that fallback stays.
 * @param {Job} j
 * @param {number | null} progressAt
 * @param {number} now
 * @returns {number | null}
 */
export function percent(j, progressAt, now) {
  if (finished(j.state)) return j.state === "done" ? 100 : null;
  const p = j.progress;
  if (!p) return j.state === "queued" ? 0 : null;
  if (p.m) {
    const bySteps = Math.round(((p.n - (p.finished ? 0 : 1)) / p.m) * 100);
    return Math.min(99, Math.max(0, bySteps));
  }
  const left = remaining(j, progressAt, now).s;
  const ran = elapsedS(j, now);
  const byTime =
    left != null && ran != null && ran + left > 0
      ? Math.round((ran / (ran + left)) * 100)
      : null;
  if (byTime == null) return null;
  return Math.min(99, Math.max(0, byTime));
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
 * @typedef {{key: string, label: string, value: string, numeric?: boolean,
 *   late?: boolean, badge?: Badge}} FactCell
 */

/**
 * The running-job panel's facts grid: one cell per fact, every value a
 * non-empty string so the grid never reflows as a job changes state
 * (Kenny, 2026-10-02: "maak hier ook een grid van, met vaste locatie").
 * `badge` on the State cell is drawn as the badge component, not plain
 * text; every other cell is text, right at home in a <dd>.
 * @param {ReturnType<typeof jobPanel>} v
 * @returns {FactCell[]}
 */
export function jobFacts(v) {
  return [
    { key: "state", label: "State", value: v.badge.label, badge: v.badge },
    { key: "step", label: "Step", value: v.step || "—", numeric: true },
    { key: "origin", label: "Origin", value: v.origin || "—" },
    {
      key: "elapsed",
      label: "Running for",
      value: v.elapsed || "—",
      numeric: true,
    },
    {
      key: "remaining",
      label: "Expected remaining",
      value: v.finished ? "finished" : v.remaining || "—",
      numeric: true,
      late: v.late,
    },
  ];
}

/**
 * redesign-final-c2 (FLOWS.md §1.2): the running-job drawer's step list
 * (ui.js `checkList` rows) — the steps before the current one ticked, the
 * current one running (failed when the job failed, the rest then
 * skipped), the ones after it waiting. A step is named by what the host
 * sent as it began (`names`, step number → name), "Step N" when this page
 * never saw it. A long job keeps the two steps around the current one and
 * folds the rest into a "Steps a–b" row on either side.
 * @param {Job} j
 * @param {Map<number, string>} names
 * @returns {import("./ui.js").CheckRow[]}
 */
export function jobSteps(j, names) {
  const p = j.progress;
  if (!p) {
    if (finished(j.state))
      return [
        {
          title: jobBadge(j.state).label,
          desc: j.message ?? "the job ended",
          state: j.state === "done" ? "ok" : "bad",
        },
      ];
    return [
      j.state === "queued"
        ? {
            title: "Waiting in the queue",
            desc: "starts when the one before it ends",
            state: "wait",
          }
        : {
            title: "Starting",
            desc: "the host is getting ready",
            state: "run",
          },
    ];
  }
  const total = Math.max(p.m ?? p.n, p.n);
  const ok = j.state === "done";
  const ended = finished(j.state);
  /** @param {number} i @returns {import("./ui.js").CheckRow} */
  const row = (i) => {
    const title = names.get(i) ?? (i === p.n ? p.step : `Step ${i}`);
    if (i < p.n || (ok && ended)) return { title, desc: "done", state: "ok" };
    if (i === p.n)
      return ended
        ? { title, desc: j.message ?? "failed here", state: "bad" }
        : { title, desc: `step ${p.n} of ${total}, running now`, state: "run" };
    return ended
      ? { title, desc: "not run", state: "skip" }
      : { title, desc: "to come", state: "wait" };
  };
  if (total <= 7) return Array.from({ length: total }, (_, k) => row(k + 1));
  const from = Math.max(1, p.n - 1);
  const to = Math.min(total, p.n + 2);
  /** @type {import("./ui.js").CheckRow[]} */
  const rows = [];
  if (from > 1)
    rows.push({ title: `Steps 1–${from - 1}`, desc: "done", state: "ok" });
  for (let i = from; i <= to; i++) rows.push(row(i));
  if (to < total)
    rows.push({
      title: `Steps ${to + 1}–${total}`,
      desc: ended ? "not run" : "to come",
      state: ended && !ok ? "skip" : ok ? "ok" : "wait",
    });
  return rows;
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
    // 24h, not the viewer's locale (Kenny, 2026-10-02).
    time: new Intl.DateTimeFormat(opts.locale, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
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
export const queuedText = (j, opts) => formatDateTime(j.queued_at, opts);
