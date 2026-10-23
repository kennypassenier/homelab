// one-process-batch-1 (Kenny, 2026-10-23: "van begin tot einde moet dat 1
// proces zijn, met een op voorhand bekend aantal stappen"): a batch is ONE
// process on the page. Its step plan is the concatenation of every member's
// plan (the server's `action_batch.plan`, made before the first job starts),
// its total is the sum, and its progress counts across the members. The
// view never switches to a member job: this module turns the batch and its
// member jobs into one synthetic job (and its step rows) that the shared
// progress view (runview.js, through runspec.js) draws like any other.
//
// Pure: the caller passes the batch, the jobs, the clock and a memory of the
// step names seen; nothing here touches the page.

import { finished } from "./jobs.js";

/**
 * @typedef {{job: number, stack: string, steps: string[], exact: boolean,
 *   estimated_because?: string[]}} MemberPlan
 * @typedef {import("./jobs.js").Job} Job
 * @typedef {import("./jobs.js").Batch & {plan?: MemberPlan[] | null,
 *   steps_total?: number | null, steps_exact?: boolean | null}} PlannedBatch
 * @typedef {{title: string, desc: string, state: "ok" | "bad" | "run" |
 *   "wait" | "skip"}} Row
 */

/** The member states that ran nothing and ended: their steps are skipped. */
const NEVER_RAN = new Set(["refused", "deferred"]);

/**
 * How many steps member `i` has in the batch: what the host announced for
 * it once it has (the truth), else its planned count.
 * @param {MemberPlan | undefined} plan
 * @param {Job | undefined} j
 */
export const memberLength = (plan, j) =>
  j?.progress?.m || plan?.steps.length || 0;

/**
 * Remember the name of the step each member is at, by member and step
 * number, so a step the plan only estimated is named as the host sent it.
 * @param {Map<string, string>} seen
 * @param {(Job | undefined)[]} jobs
 */
export function rememberSteps(seen, jobs) {
  for (const j of jobs)
    if (j?.progress && j.progress.n > 0 && j.progress.step)
      seen.set(`${j.job}:${j.progress.n}`, j.progress.step);
}

/**
 * Where a member stands in its own steps: how many of its steps are over
 * (`over`), and the state of each step `k` (0-based) of its `len`.
 * @param {Job | undefined} j
 * @param {number} len
 * @returns {{over: number, state: (k: number) => Row["state"],
 *   desc: (k: number) => string}}
 */
export function memberStanding(j, len) {
  const s = j?.state ?? "queued";
  const p = j?.progress;
  if (s === "done") return { over: len, state: () => "ok", desc: () => "done" };
  if (NEVER_RAN.has(s) && !p)
    return { over: len, state: () => "skip", desc: () => "not run" };
  if (s === "queued" || !p)
    return s === "running"
      ? {
          over: 0,
          state: (k) => (k === 0 ? "run" : "wait"),
          desc: (k) => (k === 0 ? "starting" : "to come"),
        }
      : { over: 0, state: () => "wait", desc: () => "to come" };
  if (s === "running") {
    const at = p.n;
    return {
      over: Math.max(0, at - (p.finished ? 0 : 1)),
      state: (k) =>
        k + 1 < at || (k + 1 === at && p.finished)
          ? "ok"
          : k + 1 === at
            ? "run"
            : at === 0 && k === 0
              ? "run"
              : "wait",
      desc: (k) =>
        k + 1 < at || (k + 1 === at && p.finished)
          ? "done"
          : k + 1 === at || (at === 0 && k === 0)
            ? "running now"
            : "to come",
    };
  }
  // failed / unknown / refused after some progress: it stopped at step n.
  const at = p.n;
  return {
    over: Math.max(0, at - 1),
    state: (k) => (k + 1 < at ? "ok" : k + 1 === at ? "bad" : "skip"),
    desc: (k) =>
      k + 1 < at ? "done" : k + 1 === at ? "failed here" : "not run",
  };
}

/**
 * The batch's state as one process.
 * @param {string[]} states the members' states
 * @returns {Job["state"]}
 */
export function batchState(states) {
  if (!states.length) return "queued";
  if (states.every((s) => s === "queued")) return "queued";
  if (!states.every((s) => finished(/** @type {any} */ (s)))) return "running";
  if (states.some((s) => s === "failed" || s === "refused" || s === "unknown"))
    return "failed";
  if (states.every((s) => s === "deferred")) return "deferred";
  return "done";
}

/**
 * The whole batch as one process: a synthetic job for the shared progress
 * view, the rows of its single step list ("kitchen · provision container"),
 * and whether its total is an estimate.
 * @param {PlannedBatch} b
 * @param {Job[]} jobs every job the page knows
 * @param {number} now unix seconds
 * @param {Map<string, string>} [seen] see {@link rememberSteps}
 * @returns {{job: Job, rows: Row[], total: number | null, approx: boolean,
 *   current: number | null, because: string[]}}
 */
export function batchProcess(b, jobs, now, seen = new Map()) {
  const members = b.jobs.map((m) => ({
    m,
    j: jobs.find((x) => x.job === m.job),
    plan: b.plan?.find((p) => p.job === m.job),
  }));
  const live = members.map((x) => x.j);
  rememberSteps(seen, live);
  const planned = b.plan != null;
  /** @type {Row[]} */
  const rows = [];
  let total = 0;
  let overAll = 0;
  /** @type {number | null} */
  let current = null;
  let runAt = 0;
  let runStep = "";
  let runFinished = false;
  for (const { m, j, plan } of members) {
    const len = memberLength(plan, j);
    const st = memberStanding(j, len);
    if (j?.state === "running" && current == null) {
      current = j.job;
      runAt = total + (j.progress?.n ?? 0);
      runStep = `${m.stack} · ${j.progress?.step ?? ""}`.replace(/ · $/, "");
      runFinished = !!j.progress?.finished;
    }
    for (let k = 0; k < len; k++) {
      // An exact plan names the step before it is reached (one-process-
      // batch-2); an estimate yields to the name the host sent.
      const exactName =
        plan?.exact && plan.steps.length === len ? plan.steps[k] : undefined;
      const name =
        exactName ??
        seen.get(`${m.job}:${k + 1}`) ??
        plan?.steps[k] ??
        `step ${k + 1}`;
      rows.push({
        title: `${m.stack} · ${name}`,
        desc: st.desc(k),
        state: st.state(k),
      });
    }
    total += len;
    overAll += st.over;
  }
  const states = members.map((x) => x.j?.state ?? x.m.state ?? "queued");
  const state = batchState(states);
  const ended = finished(state);
  const started = live.map((j) => j?.started_at).filter((t) => t != null);
  const startedAt = started.length
    ? Math.min(.../** @type {number[]} */ (started))
    : null;
  const first = live.find((j) => j) ?? null;
  // The step the process is at: the running member's, else the number of
  // steps over (a failed batch points at the step that failed).
  const n =
    current != null
      ? runAt
      : state === "failed"
        ? Math.min(total, overAll + 1)
        : overAll;
  const finishedAts = live.map((j) => j?.finished_at).filter((t) => t != null);
  const expTotals = live.map((j) => j?.progress?.expected_total_s);
  const expRemaining = members.map(({ j }) =>
    !j || j.state === "queued"
      ? null
      : finished(j.state)
        ? 0
        : (j.progress?.expected_remaining_s ?? null),
  );
  const allKnown = (/** @type {(number|null|undefined)[]} */ xs) =>
    xs.length > 0 && xs.every((x) => x != null);
  const sum = (/** @type {(number|null|undefined)[]} */ xs) =>
    xs.reduce((/** @type {number} */ a, x) => a + (x ?? 0), 0);
  /** @type {Job["progress"]} */
  const progress = planned
    ? {
        op: `batch-${b.batch}`,
        step: runStep,
        n,
        m: total,
        finished: current != null ? runFinished : true,
        changed: false,
        expected_step_s: null,
        expected_total_s: allKnown(expTotals) ? sum(expTotals) : null,
        expected_remaining_s:
          current != null && allKnown(expRemaining) ? sum(expRemaining) : null,
        elapsed_s: startedAt == null ? 0 : Math.max(0, now - startedAt),
        runs: Math.min(0, ...live.map((j) => j?.progress?.runs ?? 0)),
      }
    : null;
  const bad = members.filter(
    (x) => x.j && ["failed", "refused", "unknown"].includes(x.j.state),
  );
  const running = current == null ? null : live.find((j) => j?.job === current);
  const stacks = members.map((x) => x.m.stack);
  /** @type {Job} */
  const job = {
    job: b.batch,
    origin: { from: "manual" },
    stack: stacks.length === 1 ? stacks[0] : `${stacks.length} stacks`,
    action: first?.action ?? "",
    args: {},
    state,
    queued_at: Math.min(...live.map((j) => j?.queued_at ?? now)),
    started_at: startedAt,
    finished_at:
      ended && finishedAts.length
        ? Math.max(.../** @type {number[]} */ (finishedAts))
        : null,
    reqs: [],
    message: bad.length
      ? bad
          .map((x) =>
            `${x.m.stack}: ${x.j?.message ?? x.j?.state ?? ""}`.trim(),
          )
          .join("\n")
      : null,
    cli: null,
    progress,
    bytes: running?.bytes ?? null,
    restarts_dashboard: live.some((j) => j?.restarts_dashboard),
  };
  return {
    job,
    rows,
    total: planned ? total : null,
    approx: !planned || b.steps_exact === false,
    current,
    // Why some member's count is an estimate: "kitchen: patch: the host's
    // managed stacks were not read".
    because: members.flatMap(({ m, plan }) =>
      plan && !plan.exact
        ? (plan.estimated_because ?? []).map((w) => `${m.stack}: ${w}`)
        : [],
    ),
  };
}

/**
 * The "of N" the figures show for a batch: the sum, with "about" while some
 * member's count is only an estimate from earlier runs.
 * @param {number | null} total
 * @param {boolean} approx
 */
export function batchOf(total, approx) {
  if (total == null) return "not counted yet";
  return approx ? `about ${total}` : String(total);
}
