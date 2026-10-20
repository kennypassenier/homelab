// feat-ops-6: the running-job panel. One job, live: its steps, how long it
// has run, where the time went, what is expected to remain, why it failed,
// the host's lines as they come (the kp log component), and the end. The
// detail is the approved Activity demo's section 5 (kp11 adoption). Drawn
// from jobPanel() in jobs.js; this module only puts it on the page.

import { act, actionLabel, onAct } from "./act.js";
import { badge, labeledCliCommand, openDialog } from "./actui.js";
import { fetchReport, h, progressBar, setProgress } from "./dom.js";
import { formatDateTime, humanDuration } from "./format.js";
import { humanBytes } from "./backupsview.js";
import {
  bytesView,
  elapsedS,
  estimateRange,
  finished,
  jobPanel,
  jobSteps,
  remaining,
} from "./jobs.js";
import { jobLogViewer } from "./joblog.js";
import { openBusyCard } from "/static/kp/js/busy-card.js";
import { setCode } from "/static/kp/js/code.js";
import { attachProblems, setProblems } from "/static/kp/js/problems.js";
import { setStackbar } from "/static/kp/js/stackbar.js";
import { statePlate } from "/static/kp/js/state-plate.js";
import {
  attachSteplists,
  buildSteplist,
  setStep,
} from "/static/kp/js/steplist.js";
import { attachTicks } from "/static/kp/js/tick.js";
import { attachTimerDiscs, timerDiscOf } from "/static/kp/js/timer-disc.js";

/**
 * One job, live: the approved detail of the Activity demo (section 5). Its
 * header says who started it and how it stands; three figures tick (steps
 * done, time taken, the median of earlier runs); the steps are a list; one
 * bar splits the time by step; a disc shrinks while an estimate says time is
 * left; a failed job says what happened, why and what to do, with the error
 * as a code block; the host's lines follow in the log.
 * @param {number} jobId
 * @param {{compact?: boolean, steps?: boolean}} [opts] compact: no heading
 *   (inside a dialog that has its own); steps: false leaves the step list
 *   out (the Restore wizard draws its own)
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
  const title = h("h3", { class: "job-title" });
  const sub = h("p", { class: "measured job-sub" });
  const badgeHost = h("span", { class: "job-head__badge" });
  const restarts = h(
    "div",
    { class: "kp-alert kp-alert--warning", role: "status", hidden: "" },
    "This job restarts the dashboard: the page loses its link for a moment and reads the job's end after the restart.",
  );
  const doneN = h("b", { "data-kp-tick": "" }, "0");
  const totalN = h("span", null, "—");
  const tookDd = h("dd", { class: "job-fact" }, "—");
  const medianText = h("span", null, "—");
  const delta = h("span", { class: "kp-badge", hidden: "" });
  const figures = h(
    "dl",
    { class: "kp-kv job-figures", "aria-label": "The job in figures" },
    h(
      "div",
      { class: "kp-kv__row" },
      h("dt", null, "Steps done"),
      h("dd", { class: "job-fact" }, doneN, " of ", totalN),
    ),
    h("div", { class: "kp-kv__row" }, h("dt", null, "Time taken"), tookDd),
    h(
      "div",
      { class: "kp-kv__row" },
      h("dt", null, "Median for this operation"),
      h("dd", { class: "job-fact" }, medianText, " ", delta),
    ),
  );
  // How far along: by steps when the host counts them, else by the time
  // earlier runs took; with neither (a first run) the bar is indeterminate
  // and the line under it says why, never a 0 % that looks stuck.
  const progress = progressBar("How far the job is", null, "job-progress");
  const progressNote = h("p", { class: "measured job-progress__note" });
  // ask-kp11be-5: a restic backup or restore also says how much data has
  // moved: a second bar with the bytes done of the total and the rate.
  const bytesBar = progressBar("How much data has moved", null, "job-bytes");
  const bytesNote = h("p", { class: "measured job-progress__note" });
  const progressBox = h(
    "div",
    { class: "job-progress-box", hidden: "" },
    progress,
    progressNote,
    bytesBar,
    bytesNote,
  );
  const bar = h("figure", { class: "kp-stackbar job-bar" });
  const stepsHost = h("div", {
    class: "job-steps",
    hidden: opts.steps === false ? "" : null,
  });
  const discEl = h("div", {
    class: "kp-timer-disc job-disc",
    "data-kp-timer-disc": "1m",
    "data-kp-timer-disc-name": "Time left on the job",
    "data-kp-timer-disc-controls": "none",
  });
  const discText = h("p", {
    class: "measured job-disc__text",
    "aria-live": "off",
  });
  const discHost = h(
    "div",
    { class: "job-disc-host", hidden: "" },
    discEl,
    discText,
  );
  const problems = h("div", {
    class: "kp-problems job-problems",
    "data-kp-problems": "",
    "data-kp-problems-mode": "each",
  });
  const code = h("div", { class: "kp-code job-code", hidden: "" });
  const basis = h("p", { class: "measured job-basis" });
  const end = h("div", { class: "job-outcome" });
  const cli = h("div", { class: "job-cli" });
  const jobLog = jobLogViewer(jobId, {
    height: opts.compact ? "12rem" : "16rem",
  });
  const element = h(
    "section",
    {
      class: `kp-card job-panel${opts.compact ? " job-panel--compact" : ""}`,
      "aria-label": `Job ${jobId}`,
      "data-job": String(jobId),
    },
    h(
      "header",
      { class: "job-head" },
      h("div", null, ...(opts.compact ? [] : [title]), sub),
      badgeHost,
    ),
    restarts,
    figures,
    progressBox,
    bar,
    h("div", { class: "job-cols" }, stepsHost, discHost),
    problems,
    code,
    basis,
    end,
    cli,
    h(
      "details",
      {
        class: "job-log-wrap kp-accordion__item kp-accordion__item--bare",
        open: "",
      },
      h("summary", null, "Log"),
      jobLog.element,
    ),
  );
  const plate = h("div", { class: "job-plate", hidden: "" });
  element.append(plate);
  const detachKp = [attachTicks(element), attachProblems(element)];
  /** @type {import("/static/kp/js/busy-card.js").BusyCard | null} */
  let busy = null;
  /** Whether the disc was wired yet: it needs the median for its scale. */
  let discWired = false;

  /** @type {string} */
  let stepsShape = "";
  /** @param {import("./ui.js").CheckRow[]} rows @param {Map<string, number>} times */
  const paintSteps = (rows, times) => {
    if (opts.steps === false) return;
    const stateOf =
      /** @type {Record<string, import("/static/kp/js/steplist.js").StepState>} */ ({
        ok: "done",
        bad: "failed",
        run: "running",
        wait: "pending",
        skip: "skipped",
      });
    const items = rows.map((r, i) => {
      const s = times.get(r.title);
      return {
        id: String(i),
        title: r.title,
        note: r.desc,
        state: stateOf[r.state] ?? "pending",
        ...(s != null && r.state !== "wait" && r.state !== "skip"
          ? { time: humanDuration(s) }
          : {}),
      };
    });
    const shape = JSON.stringify(items.map((x) => [x.title, x.note]));
    if (shape !== stepsShape || !stepsHost.firstElementChild) {
      stepsShape = shape;
      const list = buildSteplist(items, { label: "Steps of the job" });
      stepsHost.replaceChildren(list);
      attachSteplists(stepsHost);
      return;
    }
    const list = /** @type {Element} */ (stepsHost.firstElementChild);
    for (const it of items)
      setStep(list, it.id, {
        state: it.state,
        note: it.note,
        time: it.time ?? "",
      });
  };

  let cliShown = "";
  let lastOutcome = "";
  let lastBadge = "";
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
    title.textContent = v.title;
    sub.textContent = `Started ${v.origin === "by hand" ? "by hand" : `from ${v.origin}`}${
      j.queued_at ? ` on ${formatDateTime(j.queued_at)}` : ""
    }, job ${j.job}`;
    if (v.badge.label !== lastBadge) {
      lastBadge = v.badge.label;
      badgeHost.replaceChildren(badge(v.badge));
    }
    restarts.hidden = !v.restarts;

    // The steps, and how long each took.
    if (p) {
      stepNames.set(p.n, p.step);
      if (lastStep.n && lastStep.n !== p.n)
        stepSeconds.set(lastStep.name, Math.max(0, p.elapsed_s - lastStep.at));
      if (lastStep.n !== p.n)
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
    paintSteps(rows, times);

    // The figures.
    const total = p ? Math.max(p.m ?? p.n, p.n) : 0;
    const done = historySteps?.length
      ? j.state === "done"
        ? historySteps.length
        : historySteps.length - 1
      : !p
        ? 0
        : ended
          ? j.state === "done"
            ? total
            : Math.max(0, p.n - 1)
          : Math.max(0, p.n - (p.finished ? 0 : 1));
    const nTotal = historySteps?.length ?? total;
    doneN.textContent = String(done);
    totalN.textContent = nTotal ? String(nTotal) : "—";
    const ran = elapsedS(j, now);
    tookDd.textContent = ran == null ? "not started" : humanDuration(ran);
    const est = estimateRange(p);
    medianText.textContent = est ? humanDuration(est.median) : "no median yet";
    const diff =
      est && ran != null && j.state === "done" ? ran - est.median : null;
    delta.hidden = diff == null || Math.abs(diff) < 1;
    if (diff != null && Math.abs(diff) >= 1) {
      delta.className = `kp-badge kp-badge--${diff > 0 ? "warning" : "success"}`;
      delta.textContent = `${humanDuration(Math.abs(diff))} ${diff > 0 ? "slower" : "faster"}`;
    }

    progressBox.hidden = ended;
    setProgress(progress, v.percent);
    progressNote.textContent =
      v.percent == null
        ? "Working. There is no earlier run to estimate from yet, so the bar only shows that it is busy."
        : `${v.percent} % done, by ${p?.m ? "steps" : "the time earlier runs took"}.`;

    // The data moved so far, while a transfer reports it.
    const moved = bytesView(j.bytes, (n) => humanBytes(n));
    bytesBar.hidden = bytesNote.hidden = moved == null;
    if (moved) {
      setProgress(bytesBar, moved.percent);
      bytesNote.textContent = moved.text;
    }

    // Where the time went: one bar, split by step.
    /** @type {{key: string, label: string, value: number, tone?: "destructive"}[]} */
    const parts = [];
    for (const r of rows) {
      const s = times.get(r.title);
      if (s != null && s > 0)
        parts.push({
          key: r.title,
          label: r.title,
          value: s,
          ...(r.state === "bad"
            ? { tone: /** @type {const} */ ("destructive") }
            : {}),
        });
    }
    if (!parts.length)
      setStackbar(bar, {
        title: "Where the time went",
        state: "empty",
        message: ended
          ? "The host did not record how long each step took."
          : "No step has finished yet.",
      });
    else {
      const sum = parts.reduce((a, x) => a + x.value, 0);
      const whole = Math.max(est?.median ?? 0, sum);
      setStackbar(bar, {
        title: "Where the time went",
        total: whole,
        format: (s) => humanDuration(s),
        restLabel: ended ? "Under the median" : "Still to come (estimate)",
        parts,
        ...(est
          ? {
              marks: [
                {
                  at: Math.min(1, est.median / whole),
                  label: `median ${humanDuration(est.median)}`,
                },
              ],
            }
          : {}),
      });
    }

    // The disc shrinks while an estimate says time is left.
    const left = remaining(j, act.progressAt.get(jobId) ?? null, now);
    const running = !ended && left.s != null && est != null;
    discHost.hidden = !running && !ended;
    if (running && est) {
      if (!discWired) {
        discWired = true;
        discEl.setAttribute(
          "data-kp-timer-disc-scale",
          String(Math.max(1, Math.ceil(est.median / 60))),
        );
        attachTimerDiscs(discHost);
      }
      const clk = timerDiscOf(discEl);
      if (clk && left.s != null) {
        if (
          clk.state() !== "running" ||
          Math.abs(clk.remaining() / 1000 - left.s) > 3
        ) {
          clk.reset(Math.max(1, left.s));
          clk.start();
        }
      }
      discEl.hidden = false;
      discText.textContent = left.late
        ? `${left.text}: this run is past what earlier runs took`
        : `About ${humanDuration(left.s)} left (an estimate from the median, not a promise)`;
    } else {
      timerDiscOf(discEl)?.pause();
      discEl.hidden = true;
      discText.textContent = !ended
        ? ""
        : j.state === "failed"
          ? `Stopped after ${humanDuration(ran)}. No time left to estimate.`
          : est && ran != null
            ? `Finished in ${humanDuration(ran)}, ${humanDuration(Math.abs(ran - est.median))} ${ran > est.median ? "slower" : "faster"} than the median.`
            : `Finished in ${humanDuration(ran)}.`;
    }

    // What happened, and what to do.
    const failed = j.state === "failed";
    const msg = (j.message ?? "").trim();
    setProblems(
      problems,
      failed
        ? [
            {
              key: "failed",
              what: `${actionLabel(j.action)} stopped${p?.step ? ` at "${p.step}"` : ""}`,
              why: msg
                ? msg.split(/(?<=[.!?])\s/)[0]
                : "The host reported a failure but gave no reason.",
              remedy:
                "Read the log below, fix what it names, then run it again.",
              ...(j.cli ? { command: j.cli } : {}),
              severity: "critical",
            },
          ]
        : [],
    );
    code.hidden = !(failed && msg);
    if (failed && msg)
      setCode(code, { text: msg, title: "What the host said", maxLines: 6 });
    basis.textContent = v.basis;
    basis.hidden = !v.basis || v.finished;
    const o = v.outcome;
    const shownOutcome =
      o && j.state !== "failed" && o.tone !== "success" ? o : null;
    const sig = shownOutcome
      ? `${shownOutcome.title}|${shownOutcome.text}`
      : "";
    if (sig !== lastOutcome) {
      lastOutcome = sig;
      end.replaceChildren(
        ...(shownOutcome
          ? [
              h(
                "div",
                {
                  class: `kp-alert kp-alert--${shownOutcome.tone}`,
                  role: "status",
                  "data-kp-semantic": "",
                },
                h(
                  "span",
                  { class: "kp-alert__body" },
                  h(
                    "span",
                    { class: "kp-alert__label" },
                    `${shownOutcome.title}: `,
                  ),
                  shownOutcome.text,
                ),
              ),
            ]
          : []),
      );
    }
    const wantCli = failed ? "" : (v.cli ?? "");
    if (wantCli !== cliShown) {
      cliShown = wantCli;
      cli.replaceChildren(
        ...(wantCli
          ? [labeledCliCommand("The same from a workstation:", wantCli)]
          : []),
      );
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

  const offJobs = onAct("jobs", (id) => {
    if (id == null || id === jobId) paint();
  });
  const timer = setInterval(paint, 1000);
  paint();
  return {
    element,
    stop: () => {
      offJobs();
      jobLog.stop();
      abort.abort();
      busy?.remove();
      timerDiscOf(discEl)?.stop();
      for (const off of detachKp) off();
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
  void d.closed.then(() => {
    panel.stop();
    opts.onClose?.();
  });
  return d;
}
