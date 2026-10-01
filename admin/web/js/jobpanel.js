// feat-ops-6: the running-job panel. One job, live: "step 3/35", how long
// it has run, what is expected to remain, a kp progress bar, the host's
// lines as they come (the kp log component), and the end. Drawn from
// jobPanel() in jobs.js; this module only puts it on the page.

import { act, actionLabel, onAct } from "./act.js";
import { badge, labeledCopyLine } from "./actui.js";
import { h } from "./dom.js";
import { jobPanel, logLine } from "./jobs.js";
import { attachLogs } from "/static/kp/js/log.js";

/**
 * @param {number} jobId
 * @param {{compact?: boolean}} [opts] compact: no heading (inside a dialog
 *   that has its own)
 * @returns {{element: HTMLElement, stop: () => void}}
 */
export function mountJobPanel(jobId, opts = {}) {
  const title = h("h3", { class: "job-title" });
  const stateDD = h("dd", { class: "job-state" });
  const stepDD = h("dd", { class: "job-step" });
  const originDD = h("dd", { class: "job-origin" });
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
  const elapsed = h("dd", { class: "job-elapsed" });
  const left = h("dd", { class: "job-remaining" });
  // Kenny, 2026-10-01: state, step, origin, elapsed and remaining used to
  // scatter across a title row and two separate paragraphs; one kv-grid
  // keeps every fact lined up in place instead.
  const facts = h(
    "dl",
    { class: "facts job-facts" },
    h("dt", null, "State"),
    stateDD,
    h("dt", null, "Step"),
    stepDD,
    h("dt", null, "Origin"),
    originDD,
    h("dt", null, "Running for"),
    elapsed,
    h("dt", null, "Expected remaining"),
    left,
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
      class: "kp-card job-panel",
      "aria-label": `Job ${jobId}`,
      "data-job": String(jobId),
    },
    ...(opts.compact ? [] : [h("div", { class: "title-row" }, title)]),
    restarts,
    facts,
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
      originDD.textContent = `Job ${jobId}: waiting for the dashboard to report it…`;
      return;
    }
    const v = jobPanel(j, {
      label: actionLabel,
      progressAt: act.progressAt.get(jobId) ?? null,
      now: Date.now() / 1000,
    });
    title.textContent = v.title;
    stateDD.replaceChildren(badge(v.badge));
    originDD.textContent = `Job ${j.job} · ${v.origin}`;
    restarts.hidden = !v.restarts;
    stepDD.textContent = v.step;
    if (v.percent == null) {
      bar.removeAttribute("value");
      pctText.textContent = v.finished ? "" : "…";
    } else {
      bar.value = v.percent;
      pctText.textContent = `${v.percent}%`;
    }
    stepName.textContent = v.stepName;
    stepName.hidden = !v.stepName;
    elapsed.textContent = v.elapsed;
    left.textContent = v.finished ? "finished" : v.remaining || "—";
    left.dataset.late = String(v.late);
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
