// one-progress-view-1: what the shared progress view (runview.js) shows for
// one job, worked out in one place so the job dialog, Deploy, Deploy all
// changes, the Jobs page and the Update flow all say the same thing about a
// run. Pure: the caller passes the job, the steps it works out (the host's
// own step list, or the Update flow's rows) and the clock; nothing here
// touches the page.

import { formatDateTime, humanDuration } from "./format.js";
import {
  bytesView,
  elapsedS,
  estimateRange,
  finished,
  jobPanel,
  medianView,
  remaining,
  stepFigures,
  timeView,
} from "./jobs.js";

/** @type {Record<string, import("/static/kp/js/steplist.js").StepState>} */
const STEP_STATE = {
  ok: "done",
  bad: "failed",
  run: "running",
  wait: "pending",
  skip: "skipped",
};

/**
 * @typedef {{id?: string, title: string, desc: string, state: string,
 *   note?: string, time?: string}} StepRow
 * @typedef {{rows: StepRow[], times: Map<string, number>,
 *   historyLen?: number | null, now: number, progressAt: number | null,
 *   label: (action: string) => string, human: (n: number) => string | null,
 *   fixedFigures?: boolean}} RunCtx
 */

/**
 * The whole description of what the progress view draws for `j`.
 * @param {import("./jobs.js").Job} j
 * @param {RunCtx} ctx
 * @returns {import("./runview.js").RunSpec}
 */
export function runSpec(j, ctx) {
  const { rows, times, now } = ctx;
  const v = jobPanel(j, {
    label: ctx.label,
    progressAt: ctx.progressAt,
    now,
  });
  const p = j.progress;
  const ended = finished(j.state);
  const failed = j.state === "failed";
  const fig = stepFigures(j, ctx.historyLen ?? null);
  const ran = elapsedS(j, now);
  const est = estimateRange(p);
  const med = medianView(est, ran, j.state, p?.op ?? "");
  const left = remaining(j, ctx.progressAt, now);
  const running = !ended && left.s != null && est != null;
  const msg = (j.message ?? "").trim();
  const o = v.outcome;
  const shownOutcome =
    o && j.state !== "failed" && o.tone !== "success" ? o : null;
  const tv = timeView(rows, times, est?.median ?? null, ran, ended);
  // The bar never disappears: a finished run shows where it ended.
  const pct = ended
    ? j.state === "done"
      ? 100
      : fig.total
        ? Math.round((fig.done / fig.total) * 100)
        : 0
    : v.percent;
  const moved = bytesView(j.bytes, (n) => ctx.human(n));
  return {
    head: {
      title: v.title,
      sub: `Started ${v.origin === "by hand" ? "by hand" : `from ${v.origin}`}${
        j.queued_at ? ` on ${formatDateTime(j.queued_at)}` : ""
      }, job ${j.job}`,
      badge: v.badge,
      restarts: v.restarts,
    },
    figures: {
      done: fig.done,
      of: fig.of,
      note: fig.note,
      took: ran == null ? "not started" : humanDuration(ran),
      median: med.text,
      delta: med.delta,
    },
    bar: {
      percent: pct,
      note: ended
        ? j.state === "done"
          ? "Finished."
          : `Stopped at ${pct} %.`
        : v.percent == null
          ? "Working; no earlier run to estimate from yet."
          : `${v.percent} % done, by ${p?.m ? "steps" : "the time earlier runs took"}.`,
      bytes: ended ? null : moved,
      line: ended
        ? null
        : {
            step:
              p && fig.total
                ? Math.min(fig.total, Math.max(1, p.n))
                : undefined,
            steps: fig.total || undefined,
            done: j.bytes ? j.bytes.done : undefined,
            total: j.bytes?.total ? j.bytes.total : undefined,
            etaSeconds:
              left.s != undefined && !left.late
                ? Math.round(left.s)
                : undefined,
          },
    },
    steps: rows.map((r, i) => {
      const s = times.get(r.title);
      return {
        id: r.id ?? String(i),
        title: r.title,
        note: r.note ? `${r.desc} · ${r.note}` : r.desc,
        state: STEP_STATE[r.state] ?? "pending",
        ...(r.time
          ? { time: r.time }
          : s != null && r.state !== "wait" && r.state !== "skip"
            ? { time: humanDuration(s) }
            : {}),
      };
    }),
    time:
      tv.kind === "empty"
        ? tv
        : {
            kind: "bar",
            parts: tv.parts,
            whole: tv.whole,
            medianS: est?.median ?? null,
            restLabel: ended ? "Under the median" : "Still to come (estimate)",
            note: tv.note,
          },
    disc: {
      running,
      scaleMin: est ? Math.max(1, Math.ceil(est.median / 60)) : 1,
      leftS: left.s,
      text: running
        ? left.late
          ? `${left.text}: this run is past what earlier runs took`
          : `About ${humanDuration(left.s)} left (an estimate from the median, not a promise)`
        : !ended
          ? ""
          : failed
            ? `Stopped after ${humanDuration(ran)}. No time left to estimate.`
            : est && ran != null && med.comparable
              ? `Finished in ${humanDuration(ran)}, ${humanDuration(Math.abs(ran - est.median))} ${ran > est.median ? "slower" : "faster"} than the median.`
              : `Finished in ${humanDuration(ran)}.`,
    },
    result: {
      problems: failed
        ? [
            {
              key: "failed",
              what: `${ctx.label(j.action)} stopped${p?.step ? ` at "${p.step}"` : ""}`,
              why: msg
                ? msg.split(/(?<=[.!?])\s/)[0]
                : "The host reported a failure but gave no reason.",
              remedy:
                "Read the log beside it, fix what it names, then run it again.",
              ...(j.cli ? { command: j.cli } : {}),
              severity: "critical",
            },
          ]
        : [],
      code: failed && msg ? { text: msg, title: "What the host said" } : null,
      outcome: shownOutcome,
      cli: "",
      basis: v.basis && !v.finished ? v.basis : "",
    },
  };
}
