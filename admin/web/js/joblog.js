// feat-kpadopt-2 (kp-themes 11.1.1, WP-2): the log of one job, drawn by kp's
// log viewer (`logviewer.js`: find, wrap, time column, level floor, Copy,
// the ring buffer, the loading / empty / failed plates, follow-the-tail with
// the new-lines pill). One builder for the job panel (every job dialog), the
// Update page's run card and Activity's running jobs; the host's lines come
// from the act store (`act.logs`, `onAct("log")`).

import { act, onAct } from "./act.js";
import { declare, drivable } from "./drivable.js";
import { fetchJson, h } from "./dom.js";
import { wholeLogLines } from "./logs.js";
import {
  appendLogLines,
  attachLogViewers,
  clearLogViewer,
  setLogViewer,
} from "/static/kp/dist/kp-themes.min.js";

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
  row: "<job>",
  what: "choose the lowest level a job log shows (debug, info, warning or error)",
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

/** Lines the viewer keeps while it follows a running job. */
const KEEP_LIVE = 500;

/** Ids by line, so a line the viewer was handed twice is kept once. @type {WeakMap<object, string>} */
const ids = new WeakMap();
let serial = 0;

/**
 * One host line as the viewer keeps it.
 * @param {import("./jobs.js").LogLine} l
 */
const kpLine = (l) => {
  let id = ids.get(l);
  if (!id) {
    id = `j${l.job}-${++serial}`;
    ids.set(l, id);
  }
  return {
    id,
    ts: l.ts * 1000,
    source: l.source || "host",
    level: l.level || "info",
    message: l.msg,
  };
};

/**
 * The whole log of a job from the dashboard: every line it kept.
 * @param {number} jobId
 * @param {AbortSignal} [signal]
 * @returns {Promise<{lines: import("./jobs.js").LogLine[], max: number | null} | null>}
 */
async function fetchWholeLog(jobId, signal) {
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
 * A job's log in kp's log viewer, live.
 * @param {number} jobId
 * @param {{whole?: boolean, label?: string, height?: string,
 *   hint?: string, tools?: string}} [opts] whole: false leaves the "Show the
 *   whole log" switch out; height: the viewer's fixed height (a CSS length)
 * @returns {{element: HTMLElement, viewer: HTMLElement,
 *   bind: (jobId: number) => void, stop: () => void}}
 */
export function jobLogViewer(jobId, opts = {}) {
  let job = jobId;
  const abort = new AbortController();
  const viewer = h("div", {
    class: "kp-logviewer jl-viewer",
    "data-kp-logviewer": "",
    "data-kp-log-max": String(KEEP_LIVE),
    "data-kp-log-tools": opts.tools ?? "search,follow,wrap,time,level,copy",
    "data-kp-log-filename": `job-${jobId}-log`,
    "data-kp-log-hint":
      opts.hint ?? "No lines yet. Lines appear here as the host sends them.",
    "aria-label": opts.label ?? "The host's lines for this job",
    // A log streams: a line is appended, never news of its own.
    "data-kp-arrive": "none",
    style: opts.height ? `--kp-logviewer-height: ${opts.height}` : null,
  });
  const note = h("span", {
    class: "measured jl-note",
    role: "status",
  });
  /** @type {HTMLButtonElement | null} */
  let wholeBtn = null;
  /** The job's whole log while it is shown. @type {import("./jobs.js").LogLine[] | null} */
  let whole = null;
  const element = h("div", { class: "jl" }, viewer);
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
    element.append(h("div", { class: "jl-foot" }, wholeBtn, note));
  }

  const paintLive = () => {
    whole = null;
    note.textContent = "";
    if (wholeBtn) wholeBtn.textContent = "Show the whole log";
    clearLogViewer(viewer);
    setLogViewer(viewer, {
      max: KEEP_LIVE,
      lines: (act.logs.get(job) ?? []).map(kpLine),
      state: "ready",
    });
  };

  const detach = attachLogViewers(element);
  paintLive();

  // The Live view marks on what kp draws.
  const level = viewer.querySelector(".kp-logviewer__level");
  if (level instanceof HTMLElement) drivable(level, LOG_LEVEL, String(jobId));
  const copy = viewer.querySelector("[data-kp-logviewer-copy]");
  if (copy instanceof HTMLElement) drivable(copy, LOG_COPY, String(jobId));

  if (wholeBtn) {
    const btn = wholeBtn;
    btn.addEventListener("click", () => {
      void (async () => {
        if (whole) {
          paintLive();
          return;
        }
        btn.disabled = true;
        setLogViewer(viewer, { state: "loading" });
        const all = await fetchWholeLog(job, abort.signal);
        btn.disabled = false;
        if (abort.signal.aborted) return;
        if (!all) {
          setLogViewer(viewer, { state: "ready" });
          note.textContent = `The dashboard could not give the whole log; this shows the ${(act.logs.get(job) ?? []).length} lines the page holds`;
          return;
        }
        whole = [...all.lines];
        clearLogViewer(viewer);
        setLogViewer(viewer, {
          max: Math.max(KEEP_LIVE, whole.length),
          lines: whole.map(kpLine),
          state: "ready",
        });
        note.textContent = `The whole log: ${whole.length} ${whole.length === 1 ? "line" : "lines"}${all.max && whole.length >= all.max ? ` (the dashboard keeps at most ${all.max})` : ""}`;
        btn.textContent = "Show the newest lines only";
      })();
    });
  }

  const offLog = onAct(
    "log",
    (/** @type {import("./jobs.js").LogLine} */ l) => {
      if (l.job !== job) return;
      whole?.push(l);
      appendLogLines(viewer, [kpLine(l)]);
    },
  );

  return {
    element,
    viewer,
    /** Follow another job (the Update page's card reuses one viewer). @param {number} next */
    bind: (next) => {
      if (next === job) return;
      job = next;
      paintLive();
    },
    stop: () => {
      offLog();
      abort.abort();
      detach();
    },
  };
}
