// feat-ops-6: the running-job panel. One job, live: "step 3/35", how long
// it has run, what is expected to remain, a kp progress bar, the host's
// lines as they come (the kp log component), and the end. Drawn from
// jobPanel() in jobs.js; this module only puts it on the page.

import { act, actionLabel, onAct } from "./act.js";
import { badge, labeledCopyLine, openDialog } from "./actui.js";
import { ensureStyle, fetchJson, h, progressBar, setProgress } from "./dom.js";
import { declare, drivable } from "./drivable.js";
import { jobFacts, jobPanel, jobSteps, logLine } from "./jobs.js";
import {
  LOG_LEVELS,
  levelShown,
  logText,
  readLogLevels,
  saveLogLevels,
  wholeLogLines,
} from "./logs.js";
import { countOf, checkList } from "./ui.js";
import { attachLogs } from "/static/kp/js/log.js";

const LOG_COPY = declare({
  id: "job-log-copy",
  page: "activity",
  opens: "run",
  row: "<job>",
  what: "copy a job's whole log to the clipboard",
  shows: "in a job's log",
});
const LOG_LEVEL = declare({
  id: "job-log-level",
  page: "activity",
  opens: "view",
  row: "<job>:<debug|info|warning|critical>",
  what: "show or hide a job log's lines of one level",
  shows: "in a job's log",
});
const LOG_WHOLE = declare({
  id: "job-log-whole",
  page: "activity",
  opens: "view",
  row: "<job>",
  what: "show a job's whole log instead of its newest lines",
  shows: "in a job's log",
});

/**
 * One log line as the kp log component draws it.
 * @param {import("./jobs.js").LogLine} l
 */
export function logLineEl(l) {
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
}

/**
 * The whole log of a job from the dashboard: every line it kept (the page
 * itself holds only the newest 500), or null when the dashboard cannot say
 * (an older one without the route, a job it no longer keeps).
 * @param {number} jobId
 * @param {AbortSignal} [signal]
 * @returns {Promise<{lines: import("./jobs.js").LogLine[], max: number | null} | null>}
 */
export async function fetchWholeLog(jobId, signal) {
  const r = await fetchJson(
    `/data/actions/jobs/${jobId}/log`,
    "the job's whole log",
    signal,
  );
  if (!r.ok) return null;
  const lines = wholeLogLines(r.body);
  return lines ? { lines, max: r.body?.max ?? null } : null;
}

/**
 * feat-logs-1: the tools every job log carries: a level filter (debug,
 * info, warning, critical; several at once, remembered for every log), Copy
 * (the whole log, not only what the page holds) and, unless `whole` is
 * off, "Show the whole log".
 * @param {number} jobId
 * @param {{onChange: () => void, whole?: boolean}} opts
 * @returns {{el: HTMLElement, levels: () => Set<string>,
 *   wholeLines: () => import("./jobs.js").LogLine[] | null,
 *   addLive: (l: import("./jobs.js").LogLine) => boolean,
 *   setHidden: (n: number) => void, stop: () => void}}
 */
export function mountLogTools(jobId, opts) {
  ensureStyle("/css/pages/activity.css");
  /** @type {Set<string>} */
  let levels = readLogLevels();
  /** The job's whole log while it is shown. @type {import("./jobs.js").LogLine[] | null} */
  let whole = null;
  /** @type {Set<string>} */
  let wholeKeys = new Set();
  let base = "";
  let hidden = 0;
  const abort = new AbortController();
  const note = h("span", {
    class: "measured log-tools__note",
    role: "status",
  });
  const paintNote = () => {
    note.textContent = [
      base,
      hidden
        ? `${hidden} ${hidden === 1 ? "line" : "lines"} hidden by the level filter`
        : "",
    ]
      .filter(Boolean)
      .join(" · ");
  };
  const group = h("div", {
    class: "log-tools__levels",
    role: "group",
    "aria-label": "Show lines of these levels",
  });
  const buttons = LOG_LEVELS.map((lv) => {
    const b = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost log-tools__level",
          title: `Show or hide the ${lv.label.toLowerCase()} lines; several levels can be on`,
        },
        lv.label,
      ),
      LOG_LEVEL,
      `${jobId}:${lv.value}`,
    );
    b.addEventListener("click", () => {
      if (levels.has(lv.value)) levels.delete(lv.value);
      else levels.add(lv.value);
      saveLogLevels(levels);
      paintLevels();
      opts.onChange();
    });
    return { value: lv.value, b };
  });
  group.append(...buttons.map((x) => x.b));
  const paintLevels = () => {
    for (const { value, b } of buttons)
      b.setAttribute("aria-pressed", String(levels.has(value)));
  };
  paintLevels();

  const copy = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm kp-button--secondary",
        title: "Copy the job's whole log to the clipboard",
      },
      "Copy",
    ),
    LOG_COPY,
    String(jobId),
  );
  copy.addEventListener("click", () => {
    void (async () => {
      copy.disabled = true;
      /** @type {import("./jobs.js").LogLine[]} */
      let lines = whole ?? [];
      if (!whole) {
        const all = await fetchWholeLog(jobId, abort.signal);
        if (all) lines = all.lines;
        else {
          lines = act.logs.get(jobId) ?? [];
          base = `The dashboard could not give the whole log, so Copy took the ${lines.length} ${lines.length === 1 ? "line" : "lines"} this page holds`;
          paintNote();
        }
      }
      try {
        await navigator.clipboard.writeText(logText(lines));
        copy.textContent = "Copied";
      } catch {
        copy.textContent = "Press Ctrl C";
      }
      copy.disabled = false;
      setTimeout(() => {
        if (copy.isConnected) copy.textContent = "Copy";
      }, 1500);
    })();
  });

  /** @type {HTMLButtonElement | null} */
  let wholeBtn = null;
  if (opts.whole !== false) {
    wholeBtn = /** @type {HTMLButtonElement} */ (
      drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--sm kp-button--ghost",
            title:
              "The page keeps the newest lines; this reads every line the dashboard kept for the job",
          },
          "Show the whole log",
        ),
        LOG_WHOLE,
        String(jobId),
      )
    );
    const btn = wholeBtn;
    btn.addEventListener("click", () => {
      void (async () => {
        if (whole) {
          whole = null;
          wholeKeys = new Set();
          base = "";
          btn.textContent = "Show the whole log";
          paintNote();
          opts.onChange();
          return;
        }
        btn.disabled = true;
        const all = await fetchWholeLog(jobId, abort.signal);
        btn.disabled = false;
        if (!all) {
          base = `The dashboard could not give the whole log; this shows the ${(act.logs.get(jobId) ?? []).length} lines the page holds`;
          paintNote();
          return;
        }
        whole = [...all.lines];
        wholeKeys = new Set(whole.map((l) => `${l.ts}|${l.msg}`));
        base = `The whole log: ${whole.length} ${whole.length === 1 ? "line" : "lines"}${all.max && whole.length >= all.max ? ` (the dashboard keeps at most ${all.max})` : ""}`;
        btn.textContent = "Show the newest lines only";
        paintNote();
        opts.onChange();
      })();
    });
  }

  const el = h(
    "div",
    { class: "log-tools" },
    group,
    h(
      "div",
      { class: "log-tools__acts" },
      copy,
      ...(wholeBtn ? [wholeBtn] : []),
    ),
    note,
  );
  return {
    el,
    levels: () => levels,
    wholeLines: () => whole,
    /** A live line while the whole log is shown: kept once. */
    addLive: (l) => {
      if (!whole) return false;
      const k = `${l.ts}|${l.msg}`;
      if (wholeKeys.has(k)) return true;
      wholeKeys.add(k);
      whole.push(l);
      return true;
    },
    setHidden: (n) => {
      hidden = n;
      paintNote();
    },
    stop: () => abort.abort(),
  };
}

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
  const bar = progressBar("Progress", null);
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
    // A log streams: a line is appended, never news of its own. Each line
    // unfolding the theme's way moved the follow-the-tail scroll and the
    // dialog's height with it on every line (fix-261's dialog that must
    // never resize while lines arrive); the box's own size glide stays.
    "data-kp-arrive": "none",
  });
  const logCount = h("span", { class: "measured" });
  const tools = mountLogTools(jobId, { onChange: () => redraw() });
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
      {
        class: "job-log-wrap kp-accordion__item kp-accordion__item--bare",
        open: "",
      },
      h("summary", null, "Log ", logCount),
      tools.el,
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
    setProgress(bar, v.percent);
    if (v.percent == null) pctText.textContent = v.finished ? "" : "…";
    else pctText.textContent = `${v.percent}%`;
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

  // fix-371-1 (Kenny, 2026-10-04): the box itself (app.css `.job-log`) is
  // always at least ten lines tall, empty or loading too; an empty job
  // reads one placeholder row inside it rather than the box collapsing
  // away. `emptyRow` is never a `.kp-log__line`, so it is not counted.
  const emptyText = "No lines yet. Lines appear here as the host sends them.";
  const emptyRow = () => h("p", { class: "job-log-empty measured" }, emptyText);
  const countLines = () => {
    const n = log.querySelectorAll(":scope > .kp-log__line").length;
    logCount.textContent = n ? `(${n} ${n === 1 ? "line" : "lines"})` : "";
    const ph = log.querySelector(".job-log-empty");
    if (n === 0) {
      if (!ph) log.append(emptyRow());
    } else ph?.remove();
  };
  /** The lines the level filter lets through, drawn in the box. */
  const redraw = () => {
    const all = tools.wholeLines() ?? act.logs.get(jobId) ?? [];
    const shown = all.filter((l) => levelShown(l.level, tools.levels()));
    tools.setHidden(all.length - shown.length);
    log.replaceChildren(...shown.map(logLineEl));
    attachLogs(log);
    countLines();
    log.scrollTop = log.scrollHeight;
  };
  for (const l of act.logs.get(jobId) ?? [])
    if (levelShown(l.level, tools.levels())) log.append(logLineEl(l));
  tools.setHidden(
    (act.logs.get(jobId) ?? []).filter(
      (l) => !levelShown(l.level, tools.levels()),
    ).length,
  );
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
      tools.addLive(l);
      if (!levelShown(l.level, tools.levels())) {
        tools.setHidden(
          (tools.wholeLines() ?? act.logs.get(jobId) ?? []).filter(
            (x) => !levelShown(x.level, tools.levels()),
          ).length,
        );
        return;
      }
      const near = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
      log.querySelector(".job-log-empty")?.remove();
      const el = logLineEl(l);
      log.append(el);
      while (!tools.wholeLines() && log.childElementCount > 500)
        log.firstElementChild?.remove();
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
      tools.stop();
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
