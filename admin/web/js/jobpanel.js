// feat-ops-6: the running-job panel. One job, live: its steps, how long it
// has run, where the time went, what is expected to remain, why it failed,
// the host's lines as they come (the kp log component), and the end.
// one-progress-view-1: the panel is the shared progress view (runview.js,
// the layout of the Update flow page) fed from one job: jobPanel() and the
// step helpers in jobs.js work out WHAT to show, runview.js draws it in
// fixed columns where nothing moves while the job runs.

import { act, actionLabel, onAct } from "./act.js";
import { labeledCliCommand, openDialog } from "./actui.js";
import { fetchReport } from "./dom.js";
import { humanBytes } from "./backupsview.js";
import { elapsedS, finished, jobPanel, jobSteps } from "./jobs.js";
import { runSpec } from "./runspec.js";
import { buildRunView } from "./runview.js";
import { openBusyCard } from "/static/kp/js/busy-card.js";
import { statePlate } from "/static/kp/js/state-plate.js";

/**
 * One job, live: the approved detail of the Activity demo (section 5), laid
 * out as the shared progress view. Its header says who started it and how it
 * stands; three figures tick (steps done, time taken, the median of earlier
 * runs); the steps are a list that never folds or reorders; one bar splits
 * the time by step; a disc shrinks while an estimate says time is left; a
 * failed job says what happened, why and what to do, with the error as a
 * code block; the host's lines follow in the log.
 * @param {number} jobId
 * @param {{compact?: boolean, steps?: boolean, framed?: boolean}} [opts]
 *   compact: no heading (inside a dialog that has its own); steps: false
 *   leaves the step list out (the Restore wizard draws its own); framed:
 *   the panel draws its own card (a page; a dialog is the frame already)
 * @returns {{element: HTMLElement, stop: () => void}}
 */
export function mountJobPanel(jobId, opts = {}) {
  /** Each step's name as the host sent it when the step began. */
  /** @type {Map<number, string>} */
  const stepNames = new Map();
  /** How long each step took, by its name, as the progress events showed it. */
  /** @type {Map<string, number>} */
  const stepSeconds = new Map();
  let lastStep = { n: 0, name: "", at: 0 };
  /** The host's own steps of this job, once its history entry is read. @type {import("./activity.js").Step[] | null} */
  let historySteps = /** @type {import("./activity.js").Step[] | null} */ (
    null
  );
  let historyTried = false;
  const abort = new AbortController();
  const view = buildRunView({
    label: `Job ${jobId}`,
    jobId,
    compact: opts.compact,
    framed: opts.framed,
    steps: opts.steps,
    dataJob: String(jobId),
  });
  const { element, plate } = view;
  /** @type {import("/static/kp/js/busy-card.js").BusyCard | null} */
  let busy = null;
  /** How many paints in a row found the job ended and nothing changed. */
  let settled = 0;
  let lastSig = "";

  const paint = () => {
    const j = act.jobs.find((x) => x.job === jobId);
    if (!j) {
      // Not heard of yet: the central busy card over the whole panel. Once
      // the jobs are read and this one is still missing, the dashboard no
      // longer keeps it: the theme's empty plate says so, filling the panel.
      if (act.jobsRead) {
        busy?.remove();
        busy = null;
        if (!plate.firstChild) {
          plate.append(
            statePlate(document, {
              kind: "empty",
              title: `Job ${jobId} is no longer kept`,
              text: "The dashboard keeps the last 200 jobs; the Activity page's History still lists what the host did.",
            }),
          );
          plate.hidden = false;
        }
      } else
        busy ??= openBusyCard(element, {
          words: "Reading the job…",
          area: () => [element],
        });
      return;
    }
    plate.replaceChildren();
    plate.hidden = true;
    busy?.remove();
    busy = null;
    const now = Date.now() / 1000;
    const v = jobPanel(j, {
      label: actionLabel,
      progressAt: act.progressAt.get(jobId) ?? null,
      now,
    });
    const p = j.progress;
    const ended = finished(j.state);

    // The steps, and how long each took.
    if (p) {
      if (p.n > 0) stepNames.set(p.n, p.step);
      if (lastStep.n && lastStep.n !== p.n)
        stepSeconds.set(lastStep.name, Math.max(0, p.elapsed_s - lastStep.at));
      if (lastStep.n !== p.n && p.n > 0)
        lastStep = { n: p.n, name: p.step, at: p.elapsed_s };
      if (ended && lastStep.n === p.n)
        stepSeconds.set(
          lastStep.name,
          Math.max(0, (elapsedS(j, now) ?? p.elapsed_s) - lastStep.at),
        );
    }
    if (ended && !historyTried && j.reqs?.length) {
      historyTried = true;
      void readSteps(j);
    }
    /** @type {import("./ui.js").CheckRow[]} */
    let rows;
    /** @type {Map<string, number>} */
    let times = stepSeconds;
    const hist = historySteps;
    if (hist?.length) {
      const failed = j.state === "failed";
      rows = hist.map((s, i) => ({
        title: s.step,
        desc:
          failed && i === hist.length - 1
            ? "failed here"
            : s.changed
              ? "changed something"
              : "nothing to change",
        state: failed && i === hist.length - 1 ? "bad" : "ok",
      }));
      times = new Map(
        hist
          .filter((s) => s.end > 0 && s.end >= s.start)
          .map((s) => [s.step, s.end - s.start]),
      );
    } else rows = jobSteps(j, stepNames);

    view.paint(
      runSpec(j, {
        rows,
        times,
        historyLen: hist?.length || null,
        now,
        progressAt: act.progressAt.get(jobId) ?? null,
        label: actionLabel,
        human: (n) => humanBytes(n),
      }),
    );
    const failed = j.state === "failed";
    view.setCli(failed ? "" : (v.cli ?? ""), () =>
      labeledCliCommand("The same from a workstation:", v.cli ?? ""),
    );
    // A finished run stops refreshing: once two paints in a row found the
    // same final content, the timer stops (timebar-after-done-1).
    const sig = ended ? JSON.stringify([j.state, j.message, rows.length]) : "";
    settled = ended && sig === lastSig ? settled + 1 : 0;
    lastSig = sig;
    if (settled >= 2 && historyTried && (hist != null || !j.reqs?.length)) {
      clearInterval(timer);
      timerOn = false;
    }
  };

  /** The host's own steps for a finished job, from its history entry. @param {import("./jobs.js").Job} j */
  const readSteps = async (j) => {
    const r = await fetchReport(
      `/data/history?since=${Math.max(0, Math.floor(j.queued_at - 120))}&limit=200`,
      "the job's steps",
      abort.signal,
    ).catch(() => null);
    if (!r?.ok) return;
    const entry = (r.report?.entries ?? []).find(
      (/** @type {import("./activity.js").Entry} */ e) =>
        e.kind === "op" && e.req != null && j.reqs?.includes(e.req),
    );
    if (entry && entry.kind === "op" && entry.steps.length) {
      historySteps = entry.steps;
      paint();
    }
  };

  let timerOn = true;
  let timer = setInterval(paint, 1000);
  const startTimer = () => {
    timerOn = true;
    timer = setInterval(paint, 1000);
  };
  const offJobs = onAct("jobs", (id) => {
    if (id == null || id === jobId) {
      // A change after the timer stopped (a job that restarts) wakes it.
      const j = act.jobs.find((x) => x.job === jobId);
      if (!timerOn && j && !finished(j.state)) startTimer();
      paint();
    }
  });
  paint();
  return {
    element,
    stop: () => {
      offJobs();
      abort.abort();
      busy?.remove();
      view.stop();
      clearInterval(timer);
    },
  };
}

/**
 * One job, live, in a dialog: its facts, progress and its log. The log
 * scrolls inside the panel's fixed height; the dialog never grows as lines
 * arrive (invariant 47). Shared by Activity, its Running now view and the
 * Console (redesign-activity; moved here from activitykit.js on merge).
 * @param {number} job
 * @param {{onClose?: () => void}} [opts]
 */
export function openJobDialog(job, opts = {}) {
  const j = act.jobs.find((x) => x.job === job);
  const panel = mountJobPanel(job, { compact: true });
  const d = openDialog({
    title: j
      ? `${actionLabel(j.action)} · ${j.stack === "_host" ? "the whole host" : j.stack} · job ${job}`
      : `Job ${job}`,
    description:
      "This job, live: its steps, how long it has run and every line the host printed for it.",
    body: [panel.element],
    id: "job-dialog",
    wide: true,
  });
  d.dialog.classList.add("act-dialog--run");
  void d.closed.then(() => {
    panel.stop();
    opts.onClose?.();
  });
  return d;
}
