// feat-ops-6: the running-job panel. One job, live: "step 3/35", how long
// it has run, what is expected to remain, a kp progress bar, the host's
// lines as they come (the kp log component), and the end. Drawn from
// jobPanel() in jobs.js; this module only puts it on the page.

import { act, actionLabel, onAct } from "./act.js";
import { badge, labeledCopyLine, openDialog } from "./actui.js";
import { h } from "./dom.js";
import { jobFacts, jobPanel, jobSteps, logLine } from "./jobs.js";
import { countOf, checkList } from "./ui.js";
import { attachLogs } from "/static/kp/js/log.js";

/**
 * @param {number} jobId
 * @param {{compact?: boolean, steps?: boolean}} [opts] compact: no heading
 *   (inside a dialog that has its own); steps: the job's step list
 *   (redesign-final-c2, the running pill's drawer, FLOWS.md §1.2)
 * @returns {{element: HTMLElement, stop: () => void}}
 */
export function mountJobPanel(jobId, opts = {}) {
  /** Each step's name as the host sent it when the step began. */
  /** @type {Map<number, string>} */
  const stepNames = new Map();
  const steps = h("div", {
    class: "job-steps",
    hidden: opts.steps ? null : "",
  });
  let stepsSig = "";
  const title = h("h3", { class: "job-title" });
  const restarts = h(
    "div",
    { class: "kp-alert kp-alert--warning", role: "status", hidden: "" },
    "This job restarts the dashboard: the page loses its link for a moment and reads the job's end after the restart.",
  );
  const bar = h("progress", {
    class: "kp-progress",
    max: "100",
    "aria-label": "Progress",
  });
  const pctText = h("span", { class: "kp-progress__value" });
  const stepName = h("p", { class: "job-step-name mono" });
  // feat-jobpanel-1 (Kenny, 2026-10-02: "maak hier ook een grid van, met
  // vaste locatie"): State, Step, Origin, Running for and Expected
  // remaining used to be one column of label/value pairs, however wide the
  // dialog — most of a 1100px dialog sat empty beside them. `.job-facts`
  // (app.css) repeats the pairs 2-3 across on a wide screen and falls back
  // to `.facts`' own one-column phone rule. `jobFacts()` (jobs.js) is the
  // pure view model that fills each `<dd>` once the job is known; every
  // cell starts on "—" here so none of them is ever briefly empty.
  /** @type {Map<string, HTMLElement>} */
  const factDD = new Map(
    ["state", "step", "origin", "elapsed", "remaining"].map((key) => [
      key,
      h("dd", { class: `job-fact job-fact--${key}` }, "—"),
    ]),
  );
  const facts = h(
    "dl",
    { class: "facts job-facts" },
    h("dt", null, "State"),
    /** @type {HTMLElement} */ (factDD.get("state")),
    h("dt", null, "Step"),
    /** @type {HTMLElement} */ (factDD.get("step")),
    h("dt", null, "Origin"),
    /** @type {HTMLElement} */ (factDD.get("origin")),
    h("dt", null, "Running for"),
    /** @type {HTMLElement} */ (factDD.get("elapsed")),
    h("dt", null, "Expected remaining"),
    /** @type {HTMLElement} */ (factDD.get("remaining")),
  );
  const basis = h("p", { class: "measured job-basis" });
  const end = h("div", { class: "job-outcome" });
  const cli = h("div", { class: "job-cli" });
  const log = h("div", {
    class: "kp-log job-log",
    role: "log",
    "aria-live": "polite",
    "aria-label": "The host's lines for this job",
  });
  const logCount = h("span", { class: "measured" });
  const empty = h(
    "p",
    { class: "measured job-log-empty" },
    "No lines yet. Lines appear here as the host sends them.",
  );
  const element = h(
    "section",
    {
      class: `kp-card job-panel${opts.compact ? " job-panel--compact" : ""}`,
      "aria-label": `Job ${jobId}`,
      "data-job": String(jobId),
    },
    ...(opts.compact ? [] : [h("div", { class: "title-row" }, title)]),
    restarts,
    facts,
    steps,
    h(
      "div",
      { class: "kp-progress-group job-progress" },
      h(
        "div",
        { class: "kp-progress__wrap" },
        h("span", { class: "kp-progress__label" }, "Progress"),
        bar,
        pctText,
      ),
    ),
    stepName,
    basis,
    end,
    cli,
    h(
      "details",
      { class: "job-log-wrap", open: "" },
      h("summary", null, "Log ", logCount),
      empty,
      log,
    ),
  );

  let cliShown = "";
  let lastOutcome = "";
  const paint = () => {
    const j = act.jobs.find((x) => x.job === jobId);
    if (!j) {
      /** @type {HTMLElement} */ (factDD.get("origin")).textContent =
        `Job ${jobId}: waiting for the dashboard to report it…`;
      return;
    }
    const v = jobPanel(j, {
      label: actionLabel,
      progressAt: act.progressAt.get(jobId) ?? null,
      now: Date.now() / 1000,
    });
    title.textContent = v.title;
    restarts.hidden = !v.restarts;
    if (opts.steps) {
      if (j.progress) stepNames.set(j.progress.n, j.progress.step);
      const rows = jobSteps(j, stepNames);
      const sig = JSON.stringify(rows);
      if (sig !== stepsSig) {
        stepsSig = sig;
        steps.replaceChildren(checkList(rows));
      }
    }
    for (const c of jobFacts(v)) {
      if (c.key === "origin") continue; // set below, with the job number
      const dd = factDD.get(c.key);
      if (!dd) continue;
      dd.replaceChildren(...(c.badge ? [badge(c.badge)] : [c.value]));
      if (c.late != null) dd.dataset.late = String(c.late);
    }
    /** @type {HTMLElement} */ (factDD.get("origin")).textContent =
      `Job ${j.job} · ${v.origin}`;
    if (v.percent == null) {
      bar.removeAttribute("value");
      pctText.textContent = v.finished ? "" : "…";
    } else {
      bar.value = v.percent;
      pctText.textContent = `${v.percent}%`;
    }
    stepName.textContent = v.stepName;
    stepName.hidden = !v.stepName;
    basis.textContent = v.basis;
    basis.hidden = !v.basis || v.finished;
    const o = v.outcome;
    const sig = o ? `${o.title}|${o.text}` : "";
    if (sig !== lastOutcome) {
      lastOutcome = sig;
      end.replaceChildren(
        ...(o
          ? [
              h(
                "div",
                {
                  class: `kp-alert kp-alert--${o.tone}`,
                  role: o.tone === "destructive" ? "alert" : "status",
                  "data-kp-semantic": "",
                },
                h(
                  "span",
                  { class: "kp-alert__body" },
                  h("span", { class: "kp-alert__label" }, `${o.title}: `),
                  o.text,
                ),
              ),
            ]
          : []),
      );
    }
    if ((v.cli ?? "") !== cliShown) {
      cliShown = v.cli ?? "";
      cli.replaceChildren(
        ...(v.cli
          ? [labeledCopyLine("The same from a workstation:", v.cli)]
          : []),
      );
    }
  };

  /** @param {import("./jobs.js").LogLine} l */
  const lineEl = (l) => {
    const x = logLine(l);
    return h(
      "p",
      { class: "kp-log__line", "data-kp-severity": x.severity },
      h("time", { class: "kp-log__time" }, x.time),
      h(
        "span",
        { class: "kp-log__source", "data-kp-source": x.source },
        x.source,
      ),
      h("span", { class: "kp-log__level" }, x.level),
      h("span", { class: "kp-log__message" }, x.msg),
    );
  };
  const countLines = () => {
    const n = log.childElementCount;
    logCount.textContent = n ? `(${n} ${n === 1 ? "line" : "lines"})` : "";
    empty.hidden = n > 0;
    log.hidden = n === 0;
  };
  for (const l of act.logs.get(jobId) ?? []) log.append(lineEl(l));
  attachLogs(log);
  // redesign-final (exact counts): "(N lines)" is the log's lines.
  countOf(logCount, log, ":scope > .kp-log__line");
  countLines();

  const offJobs = onAct("jobs", (id) => {
    if (id == null || id === jobId) paint();
  });
  const offLog = onAct(
    "log",
    (/** @type {import("./jobs.js").LogLine} */ l) => {
      if (l.job !== jobId) return;
      const near = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
      const el = lineEl(l);
      log.append(el);
      while (log.childElementCount > 500) log.firstElementChild?.remove();
      attachLogs(el);
      countLines();
      if (near) log.scrollTop = log.scrollHeight;
    },
  );
  const timer = setInterval(paint, 1000);
  paint();
  return {
    element,
    stop: () => {
      offJobs();
      offLog();
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
