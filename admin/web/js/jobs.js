// feat-ops-6, feat-stacks-5: the jobs the dashboard runs, as the page shows
// them. Pure view models over the server's JobView, the `action_progress`
// and `action_log` events and the `action_batch` event; the clock comes in
// as an argument, so `node --test` can drive every line.

import { formatClock, formatDateTime, humanDuration } from "./format.js";

/**
 * @typedef {{op: string, step: string, n: number, m: number | null,
 *   finished: boolean, changed: boolean, expected_step_s: number | null,
 *   expected_total_s: number | null, expected_remaining_s: number | null,
 *   elapsed_s: number, runs: number, expected_total_min_s?: number | null,
 *   expected_total_max_s?: number | null}} Progress
 *   `expected_total_s` is the median of the earlier runs; the fastest and
 *   slowest (feat-timing-1) are optional until the host sends them.
 * @typedef {{from: "manual"} | {from: "batch", batch: number} |
 *   {from: "schedule", schedule: string, slot: number} |
 *   {from: "claude", by: string}} Origin
 * @typedef {"queued" | "running" | "done" | "failed" | "deferred" |
 *   "refused" | "unknown"} JobState
 * @typedef {{label: string, done: number, total: number | null,
 *   rate_bps: number | null}} JobBytes
 *   ask-kp11be-5: bytes moved in the transfer the job is in (restic).
 * @typedef {{job: number, origin: Origin, stack: string, action: string,
 *   args: Record<string, unknown>, state: JobState, queued_at: number,
 *   started_at: number | null, finished_at: number | null, reqs: number[],
 *   message: string | null, cli: string | null, progress: Progress | null,
 *   bytes?: JobBytes | null, restarts_dashboard: boolean, flow?: any}} Job
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
 * The queue's order (fix-live-1): the job that runs first on top. The
 * queue time is whole seconds, so ties fall back to the job id, ascending.
 * @param {Pick<Job, "job" | "queued_at">} a
 * @param {Pick<Job, "job" | "queued_at">} b
 */
export const queueOrder = (a, b) => a.queued_at - b.queued_at || a.job - b.job;

/**
 * How far along a job is, for ranking two views of it: a finished view
 * beats an unfinished one, then the later finish, then the further step.
 * @param {Job} j
 */
function rankJob(j) {
  return [
    finished(j.state) ? 1 : 0,
    j.finished_at ?? 0,
    j.state === "running" ? 1 : 0,
    j.progress?.n ?? 0,
  ];
}

/**
 * Whether view `a` is at least as far along as view `b` of the same job.
 * @param {Job} a
 * @param {Job} b
 */
export function atLeastAsFar(a, b) {
  const x = rankJob(a);
  const y = rankJob(b);
  for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return true;
}

/**
 * The server's job list and the views the page held before it was read
 * (fix-live-1): the server's list wins, because a stale local copy used to
 * overwrite it. A held view replaces the server's only when it is
 * provably further along (finished while the server says running, or a
 * later step) or when it arrived after the read began (`fresh`), and a job
 * the server does not list at all is kept only when it arrived after the
 * read began.
 * @param {Job[]} server
 * @param {Job[]} held
 * @param {Set<number>} fresh ids of jobs the live channel updated after the read began
 * @returns {Job[]}
 */
export function mergeJobLists(server, held, fresh) {
  /** @type {Job[]} */
  let jobs = [];
  // progress-stays-put-1: the server's copy of a running job carries no step
  // progress, so the held copy's progress rides on it. Without this the
  // list flipped between "step 15 of 86" and "Starting" at every re-read.
  for (const j of server)
    jobs = upsertJob(
      jobs,
      keepProgress(
        j,
        held.find((x) => x.job === j.job),
      ),
    );
  for (const j of held) {
    const s = jobs.find((x) => x.job === j.job);
    if (!s) {
      if (fresh.has(j.job)) jobs = upsertJob(jobs, j);
    } else if (fresh.has(j.job) ? atLeastAsFar(j, s) : false)
      jobs = upsertJob(jobs, j);
  }
  return jobs;
}

/**
 * A job's new view with the progress (and byte counter) of the older view
 * kept when the new one has none or is behind it. Progress only moves
 * forward: an older snapshot never pulls a run back to an earlier step.
 * @param {Job} view
 * @param {Job | undefined} old
 * @returns {Job}
 */
export function keepProgress(view, old) {
  if (!old) return view;
  /** @type {Job} */
  let out = view;
  const behind =
    !view.progress ||
    (old.progress != null &&
      (view.progress.n < old.progress.n ||
        (view.progress.n === old.progress.n &&
          !view.progress.finished &&
          old.progress.finished)));
  if (old.progress && behind) out = { ...out, progress: old.progress };
  if (!view.bytes && old.bytes && !finished(view.state))
    out = { ...out, bytes: old.bytes };
  return out;
}

/**
 * A job's new view in the list, newest first, the oldest dropped past
 * `KEEP_JOBS`. A progress event the page kept is not lost when the job
 * view without it arrives.
 * @param {Job[]} jobs
 * @param {Job} view
 * @returns {Job[]}
 */
export function upsertJob(jobs, view) {
  const merged = keepProgress(
    view,
    jobs.find((j) => j.job === view.job),
  );
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
 * An `action_bytes` event laid on its job (ask-kp11be-5).
 * @param {Job[]} jobs
 * @param {{job: number, bytes: JobBytes}} ev
 * @returns {Job[]}
 */
export function applyBytes(jobs, ev) {
  return jobs.map((j) => (j.job === ev.job ? { ...j, bytes: ev.bytes } : j));
}

/**
 * The byte counter of a running transfer as the job panel's second bar
 * reads it: how far in percent (null while the total is unknown, the bar
 * is then busy), and the words under it. `human` writes a byte count
 * (`humanBytes`), passed in so this stays free of other modules.
 * @param {JobBytes | null | undefined} b
 * @param {(n: number) => string | null} human
 * @returns {{percent: number | null, text: string} | null}
 */
export function bytesView(b, human) {
  if (!b) return null;
  const done = human(b.done) ?? "0 B";
  const rate =
    b.rate_bps != null && b.rate_bps > 0 ? `, ${human(b.rate_bps)}/s` : "";
  if (!b.total || b.total <= 0)
    return {
      percent: null,
      text: `${b.label}: ${done} so far${rate}; restic is still counting what there is to move`,
    };
  const percent = Math.min(100, Math.floor((b.done / b.total) * 100));
  return {
    percent,
    text: `${b.label}: ${done} of ${human(b.total)} (${percent} %)${rate}`,
  };
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
  if (p.n === 0) return p.m ? `starting, ${p.m} steps planned` : "starting";
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
 * @returns {{text: string, late: boolean, s: number | null, over: number}}
 *   `over`: seconds past the estimate (fix-timing-1: it keeps counting up).
 */
export function remaining(j, progressAt, now) {
  const p = j.progress;
  if (finished(j.state)) return { text: "", late: false, s: null, over: 0 };
  if (!p) return { text: "", late: false, s: null, over: 0 };
  if (p.expected_remaining_s == null)
    return {
      text:
        p.runs === 0
          ? "no earlier run to go by"
          : "no estimate for these steps",
      late: false,
      s: null,
      over: 0,
    };
  const left = p.expected_remaining_s - Math.max(0, now - (progressAt ?? now));
  if (left <= 0) {
    const over = Math.max(0, Math.round(-left));
    return { text: `${humanDuration(over)} overdue`, late: true, s: 0, over };
  }
  return {
    text: `about ${humanDuration(left)} left`,
    late: false,
    s: left,
    over: 0,
  };
}

/**
 * What the earlier runs say about the whole job (feat-timing-1): the
 * median, and the fastest and slowest when the host sends them.
 * @param {Progress | null} p
 * @returns {{median: number, min: number | null, max: number | null,
 *   runs: number, text: string} | null}
 */
export function estimateRange(p) {
  if (!p || p.runs <= 0 || p.expected_total_s == null) return null;
  const min = p.expected_total_min_s ?? null;
  const max = p.expected_total_max_s ?? null;
  const parts = [`median ${humanDuration(p.expected_total_s)}`];
  if (min != null && max != null)
    parts.push(`${humanDuration(min)} to ${humanDuration(max)}`);
  return {
    median: p.expected_total_s,
    min,
    max,
    runs: p.runs,
    text: `${parts.join(", ")} over ${p.runs} earlier ${p.runs === 1 ? "run" : "runs"}`,
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
    basis: estimateRange(p)?.text
      ? `expected from earlier runs: ${estimateRange(p)?.text}`
      : p && p.runs > 0
        ? `expected from ${p.runs} earlier ${p.runs === 1 ? "run" : "runs"}`
        : "",
  };
}

/**
 * The steps a run has in all, as far as the host has said: its announced
 * total, never less than the step it is at. `grows` is true while the host
 * has announced no total (the count can still go up, and says so in words).
 * @param {Progress | null | undefined} p
 * @returns {{total: number | null, grows: boolean}}
 */
export function stepTotal(p) {
  if (!p) return { total: null, grows: true };
  if (p.m) return { total: Math.max(p.m, p.n), grows: false };
  return { total: Math.max(p.n, 1), grows: true };
}

/**
 * progress-stays-put-1, steps-total-known-1: the figures over the step
 * list, one source for the three of them. `done` counts the steps that
 * finished; `total` is shown from the first frame; a total that can still
 * grow reads "at least N" with `note` saying why, never a blank.
 * @param {Job} j
 * @param {number | null} [historyLen] the host's own step count for a finished job
 * @returns {{done: number, total: number | null, of: string, note: string}}
 */
export function stepFigures(j, historyLen = null) {
  const p = j.progress;
  const ended = finished(j.state);
  if (historyLen) {
    const done = j.state === "done" ? historyLen : Math.max(0, historyLen - 1);
    return { done, total: historyLen, of: String(historyLen), note: "" };
  }
  const t = stepTotal(p);
  const done = !p
    ? 0
    : ended
      ? j.state === "done"
        ? (t.total ?? p.n)
        : Math.max(0, p.n - 1)
      : Math.max(0, p.n - (p.finished ? 0 : 1));
  if (t.total == null)
    return {
      done,
      total: null,
      of: "not counted yet",
      note: "The host counts the steps when it starts.",
    };
  if (t.grows && !ended)
    return {
      done,
      total: t.total,
      of: `at least ${t.total}`,
      note: "The total grows as the host announces more steps.",
    };
  return { done, total: t.total, of: String(t.total), note: "" };
}

/**
 * The running job's step list (ui.js `checkList` rows): EVERY step of the
 * plan, from the first frame to the end, in one fixed order. The steps
 * before the current one are ticked, the current one runs (failed when the
 * job failed, the rest then skipped), the ones after it wait. A step is
 * named by what the host sent as it began (`names`, step number → name),
 * "Step N" when it has not begun or this page never saw it. A phase
 * before the first step ("the host is getting ready") is a state of the
 * list's first row, never a card that replaces the list; the list is never
 * folded, so no row moves when another changes (Kenny, 2026-10-22/23:
 * "verschijnen mag, verschuiven niet").
 * @param {Job} j
 * @param {Map<number, string>} names
 * @returns {import("./ui.js").CheckRow[]}
 */
export function jobSteps(j, names) {
  const p = j.progress;
  const ended = finished(j.state);
  const ok = j.state === "done";
  if (!p) {
    if (ended)
      return [
        {
          title: jobBadge(j.state).label,
          desc: j.message ?? "the job ended",
          state: ok ? "ok" : "bad",
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
            desc: "the host is getting ready and will list the steps",
            state: "run",
          },
    ];
  }
  const { total: t, grows } = stepTotal(p);
  const total = t ?? p.n;
  /** @param {number} i @returns {import("./ui.js").CheckRow} */
  const row = (i) => {
    const title = names.get(i) ?? (i === p.n ? p.step : `Step ${i}`);
    if (i < p.n || (ok && ended) || (i === p.n && p.finished && !ended))
      return { title, desc: "done", state: "ok" };
    if (i === p.n)
      return ended
        ? { title, desc: j.message ?? "failed here", state: "bad" }
        : { title, desc: `step ${p.n} of ${total}, running now`, state: "run" };
    // Nothing has begun yet (n is 0): the first step is the one starting.
    if (!ended && p.n === 0 && i === 1)
      return {
        title,
        desc: "starting: the host is getting ready",
        state: "run",
      };
    return ended
      ? { title, desc: "not run", state: "skip" }
      : { title, desc: "to come", state: "wait" };
  };
  const rows = Array.from({ length: total }, (_, k) => row(k + 1));
  if (grows && !ended)
    rows.push({
      title: "More steps may follow",
      desc: "the host has not said how many steps there are",
      state: "wait",
    });
  return rows;
}

/**
 * A finished job's median is compared with the time it took only when both
 * measure the same thing (timebar-after-done-1): a median of one small
 * operation inside a bigger job ("2 s" against a run of 1 min 4 s) says
 * nothing about the run, so there the median is named for what it is and no
 * "slower" badge is drawn.
 * @param {ReturnType<typeof estimateRange>} est
 * @param {number | null} ran seconds the job took
 * @param {JobState} state
 * @param {string} [op] the operation the median belongs to
 * @returns {{text: string, delta: {tone: "warning" | "success", text: string} | null,
 *   comparable: boolean}}
 */
export function medianView(est, ran, state, op = "") {
  if (!est) return { text: "no median yet", delta: null, comparable: false };
  const comparable =
    ran == null || ran <= 0 || (ran <= est.median * 4 && ran * 4 >= est.median);
  const text = `${humanDuration(est.median)}${
    !comparable && op ? ` (of ${op}, a part of this job)` : ""
  }`;
  if (!comparable || ran == null || state !== "done")
    return { text, delta: null, comparable };
  const diff = ran - est.median;
  if (Math.abs(diff) < 1) return { text, delta: null, comparable };
  return {
    text,
    delta: {
      tone: diff > 0 ? "warning" : "success",
      text: `${humanDuration(Math.abs(diff))} ${diff > 0 ? "slower" : "faster"}`,
    },
    comparable,
  };
}

/**
 * "Where the time went" (timebar-after-done-1, progress-stays-put-1): one
 * bar split by step, agreeing with the step list. A step the page did not
 * see begin has no time here, and the words say so instead of "No step has
 * finished yet" while steps show done; the gap between the bar's sum and
 * the time the job took is named.
 * @param {{title: string, state: string}[]} rows the step list
 * @param {Map<string, number>} times seconds by step title
 * @param {number | null} medianS
 * @param {number | null} ran seconds the job took
 * @param {boolean} ended
 * @returns {{kind: "empty", message: string} |
 *   {kind: "bar", parts: {key: string, label: string, value: number,
 *     bad: boolean}[], sum: number, whole: number, note: string}}
 */
export function timeView(rows, times, medianS, ran, ended) {
  const parts = [];
  for (const r of rows) {
    const s = times.get(r.title);
    if (s != null && s > 0)
      parts.push({
        key: r.title,
        label: r.title,
        value: s,
        bad: r.state === "bad",
      });
  }
  if (!parts.length) {
    const anyDone = rows.some((r) => r.state === "ok");
    return {
      kind: "empty",
      message: ended
        ? "The host did not record how long each step took."
        : anyDone
          ? "The steps are timed as they begin; the first times appear with the next step."
          : "No step has finished yet.",
    };
  }
  const sum = parts.reduce((a, x) => a + x.value, 0);
  const whole = Math.max(medianS ?? 0, sum);
  const gap = ran != null ? Math.round(ran - sum) : 0;
  return {
    kind: "bar",
    parts,
    sum,
    whole,
    note:
      gap >= 3
        ? `${humanDuration(sum)} of the ${humanDuration(ran ?? 0)} taken is timed per step; the rest ran before this view saw the steps begin.`
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
    // 24h, not the viewer's locale (Kenny, 2026-10-02).
    time: formatClock(l.ts, { ...opts, seconds: true }),
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

/** A job that waited less than this many seconds shows no waiting band. */
export const WAIT_BAND_MIN_S = 8;

/** How long a job waited before it started (a queued job: so far). @param {Job} j @param {number} now */
export const waitedS = (j, now) =>
  Math.max(0, (j.started_at ?? now) - j.queued_at);

/**
 * The jobs as the Jobs timeline draws them (kp gantt, epoch ms): a lane per
 * stack that had a job in the last 24 hours (or has one open), a bar from
 * the start to the end of each job, a queued job as a ghost bar from the
 * moment it was queued, and the time a job waited as a thin band before its
 * bar.
 * @param {Job[]} jobs
 * @param {(action: string) => string} label
 * @param {number} now unix seconds
 * @param {number} from unix seconds: waiting bands that begin before this
 *   would start left of the plot and are left out
 */
export function jobTimeline(jobs, label, now, from) {
  const recent = jobs.filter(
    (j) => !finished(j.state) || (j.finished_at ?? j.queued_at) >= now - 86400,
  );
  const name = (/** @type {Job} */ j) =>
    j.stack === "_host" ? "host" : j.stack;
  const lanes = [...new Set(recent.map(name))]
    .sort()
    .map((id) => ({ id, label: id }));
  /** @param {Job} j @returns {"queued" | "running" | "ok" | "failed"} */
  const stateOf = (j) =>
    j.state === "queued" || j.state === "running"
      ? j.state
      : j.state === "done" || j.state === "deferred"
        ? "ok"
        : "failed";
  const runs = recent.map((j) => {
    const waited = waitedS(j, now);
    return {
      id: `job-${j.job}`,
      lane: name(j),
      label: `${label(j.action)} · ${name(j)}`,
      start:
        (j.state === "queued" ? j.queued_at : (j.started_at ?? j.queued_at)) *
        1000,
      ...(finished(j.state) && j.finished_at != null
        ? { end: j.finished_at * 1000 }
        : {}),
      state: stateOf(j),
      detail: `Job #${j.job}, ${originText(j.origin)}. ${
        j.state === "queued"
          ? `Waiting since ${formatDateTime(j.queued_at)}.`
          : `Waited ${humanDuration(waited)} before it started.`
      }${j.message ? ` ${j.message}` : ""}`,
    };
  });
  const zones = recent
    .filter(
      (j) =>
        j.started_at != null &&
        waitedS(j, now) >= WAIT_BAND_MIN_S &&
        j.queued_at >= from,
    )
    .map((j) => ({
      id: `wait-${j.job}`,
      lane: name(j),
      from: j.queued_at * 1000,
      to: /** @type {number} */ (j.started_at) * 1000,
      label: `waited ${humanDuration(waitedS(j, now))}`,
      tone: /** @type {const} */ ("muted"),
    }));
  return { lanes, runs, zones };
}

/**
 * The three figures under the Jobs timeline: the jobs in the view, how many
 * wait or run now, and the longest wait in the view.
 * @param {Job[]} jobs
 * @param {number} now unix seconds
 * @param {{from: number, to: number}} view unix seconds
 */
export function jobFigures(jobs, now, view) {
  const inView = jobs.filter(
    (j) => (j.finished_at ?? Infinity) >= view.from && j.queued_at <= view.to,
  );
  return {
    inView: inView.length,
    live: jobs.filter((j) => !finished(j.state)).length,
    longestWaitS: inView.reduce((m, j) => Math.max(m, waitedS(j, now)), 0),
  };
}
