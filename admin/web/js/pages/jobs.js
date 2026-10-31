// Every job (feat-ops-6; redesign-activity 3.71.0): every job the
// dashboard ran or runs, newest first, as a kp datatable inside Activity's
// folded "Every job" section. A row opens that job live in a dialog (its
// log scrolls inside the dialog, invariant 47).

import { act, actionLabel, loadJobs, onAct } from "../act.js";
import { agoEl, setAgo } from "../ago.js";
import { badgeCell, bindTableUrl, h, tableBlock, td } from "../dom.js";
import {
  formatDateTime,
  formatDay,
  formatClock,
  humanDuration,
} from "../format.js";
import { STATE_ORDER, jobFigures, jobRows, jobTimeline } from "../jobs.js";
import { sortKeys } from "../sortkeys.js";
import { declare, drivable } from "../drivable.js";
import { openJobDialog } from "../jobpanel.js";
import { narrowBar } from "../narrowbar.js";
import { cover } from "../statecover.js";
import {
  attachDataTables,
  compare,
  dataTable,
  attachGantts,
  setGanttRuns,
  setGanttSelection,
  setGanttState,
  setGanttZones,
  attachTicks,
  tickTo,
  attachToolbars,
} from "/static/kp/dist/kp-themes.min.js";

const JOB_BAR = declare({
  id: "activity-job-bar",
  page: "activity",
  opens: "dialog",
  row: "<job>",
  what: "open a job from the Jobs timeline, live, in a dialog",
  shows: "in the unfolded Every job, once a job ran",
  reach: [{ do: "click", control: "activity-every-job" }],
  twins: true,
});
const JOB_TAG_REMOVE = declare({
  id: "activity-jobs-filter-remove",
  page: "activity",
  opens: "view",
  row: "range",
  what: "drop the range picked on the Jobs timeline",
  shows: "while a range narrows Every job",
  reach: [{ do: "click", control: "activity-every-job" }],
});
const JOB_TAGS_CLEAR = declare({
  id: "activity-jobs-filters-clear",
  page: "activity",
  opens: "view",
  what: "drop every pick made on the Jobs timeline",
  shows: "while a range narrows Every job",
  reach: [{ do: "click", control: "activity-every-job" }],
});

/**
 * @param {HTMLElement} root
 * @param {{drive: string}} opts `drive`: the Live view control a row is
 *   (declared by the page that hosts the table, drivable.js)
 * @returns {() => void}
 */
export function mountJobsTable(root, opts) {
  const keys = sortKeys();
  const ago = agoEl("updated");
  const t = tableBlock({
    remember: "jobs",
    caption: "Every job, newest first (the last 200)",
    search: "Search jobs",
    state: "loading",
    nothing: "No jobs yet: an action from any stack's page starts one.",
    columns: [
      { label: "Job", sort: "number" },
      { label: "Queued", sort: "time" },
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "Action", sort: "text", filter: "choice" },
      { label: "From", sort: "text", filter: "choice" },
      { label: "State", sort: "text", order: STATE_ORDER, filter: "choice" },
      { label: "Took", sort: "duration" },
      { label: "Step", sort: "text" },
      { label: "Message", sort: "text", cls: "wide" },
    ],
  });
  t.tbody.id = "jobs-table";

  // ── the Jobs timeline: a lane per stack, the wait before each bar ─────
  const narrow = narrowBar({
    label: "Filters set from the Jobs timeline",
    markRemove: (b, id) => void drivable(b, JOB_TAG_REMOVE, id),
    markClear: (b) => void drivable(b, JOB_TAGS_CLEAR),
  });
  const gantt = h("section", {
    class: "kp-gantt ac-jobs-gantt",
    "data-kp-gantt": "",
    "data-kp-gantt-ranges": "1h 6h 24h",
    "data-kp-gantt-range": "6h",
    "data-kp-gantt-ghosts": "1 2 2 1 1 1",
    "aria-label": "Jobs of the last 6 hours, a lane per stack",
  });
  const ganttCover = h("div", { class: "ac-cover" }, gantt);
  const nJobs = h("b", { "data-kp-tick": "" }, "0");
  const nLive = h("b", { "data-kp-tick": "" }, "0");
  const longest = h("dd", null, "none");
  const facts = h(
    "dl",
    { class: "kp-kv ac-facts", "aria-label": "What the Jobs timeline holds" },
    h(
      "div",
      { class: "kp-kv__row" },
      h("dt", null, "Jobs in view"),
      h("dd", null, nJobs),
    ),
    h(
      "div",
      { class: "kp-kv__row" },
      h("dt", null, "Waiting or running now"),
      h("dd", null, nLive),
    ),
    h(
      "div",
      { class: "kp-kv__row" },
      h("dt", null, "Longest wait in view"),
      longest,
    ),
  );
  root.replaceChildren(
    narrow.el,
    ganttCover,
    facts,
    t.wrap,
    h("p", { class: "measured" }, ago),
  );
  const detachKp = [
    attachToolbars(root),
    attachTicks(root),
    attachGantts(root, {
      decorate: (part, info) => {
        if (info.kind === "bar" && info.id?.startsWith("job-"))
          drivable(part, JOB_BAR, info.id.slice(4));
      },
    }),
  ];
  setGanttState(gantt, "loading");
  /** The range dragged on the timeline (unix seconds), or null. @type {{from: number, to: number} | null} */
  let selection = null;
  /** The window the timeline shows (unix seconds). */
  let win = { from: Date.now() / 1000 - 6 * 3600, to: Date.now() / 1000 };
  const view = () => selection ?? win;
  const stamp = (/** @type {number} */ unix) =>
    `${formatDay(unix)} ${formatClock(unix)}`;
  const syncBar = () => {
    narrow.set(
      selection
        ? [
            {
              id: "range",
              label: `From the timeline: ${stamp(selection.from)} to ${stamp(selection.to)}`,
            },
          ]
        : [],
    );
    const shown = inSelection().length;
    narrow.state.textContent = selection
      ? `${shown} of ${act.jobs.length} jobs are listed`
      : `${act.jobs.length} ${act.jobs.length === 1 ? "job" : "jobs"} kept (the last 200). Drag across the timeline to narrow the list.`;
  };
  const inSelection = () => {
    const sel = selection;
    return sel
      ? act.jobs.filter(
          (j) =>
            (j.finished_at ?? Infinity) >= sel.from && j.queued_at <= sel.to,
        )
      : act.jobs;
  };
  const paintTimeline = (animate = true) => {
    cover(ganttCover, null);
    if (!act.jobsRead) {
      if (act.failed.jobs) {
        setGanttRuns(gantt, [], []);
        setGanttState(gantt, "error", "The jobs could not be read");
        cover(ganttCover, "error", {
          title: "The jobs could not be read",
          text: act.failed.jobs.why,
          retry: () => void loadJobs(),
        });
      } else setGanttState(gantt, "loading");
      return;
    }
    const now = Date.now() / 1000;
    const m = jobTimeline(act.jobs, actionLabel, now, win.from);
    if (!m.runs.length) {
      setGanttRuns(gantt, [], []);
      setGanttZones(gantt, []);
      setGanttState(gantt, "empty", "No job in the last 24 hours");
      cover(ganttCover, "empty", {
        title: "No job in the last 24 hours",
        text: "A job is listed here as soon as it is queued.",
      });
    } else {
      setGanttRuns(gantt, m.runs, m.lanes);
      setGanttZones(gantt, m.zones);
    }
    const f = jobFigures(act.jobs, now, view());
    for (const [el, n] of /** @type {const} */ ([
      [nJobs, f.inView],
      [nLive, f.live],
    ])) {
      if (animate) tickTo(el, String(n));
      else el.textContent = String(n);
    }
    longest.textContent = f.inView ? humanDuration(f.longestWaitS) : "none";
  };
  gantt.addEventListener("kp-gantt-select", (e) => {
    const { from, to } = /** @type {CustomEvent} */ (e).detail;
    selection =
      from != null && to != null ? { from: from / 1000, to: to / 1000 } : null;
    syncBar();
    paintTimeline();
    shown = "";
    render();
  });
  gantt.addEventListener("kp-gantt-window", (e) => {
    const { from, to } = /** @type {CustomEvent} */ (e).detail;
    win = { from: from / 1000, to: to / 1000 };
    paintTimeline();
  });
  gantt.addEventListener("kp-gantt-open", (e) => {
    const id = Number(
      String(/** @type {CustomEvent} */ (e).detail.id).slice(4),
    );
    if (Number.isInteger(id) && id > 0) openJobDialog(id);
  });
  const lift = () => {
    selection = null;
    setGanttSelection(gantt, null);
    syncBar();
    paintTimeline();
    shown = "";
    render();
  };
  narrow.el.addEventListener("kp-toolbar-remove", lift);
  narrow.el.addEventListener("kp-toolbar-clear", lift);

  const detach = attachDataTables(t.wrap, { compare: keys.compare(compare) });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "jobs");

  t.tbody.addEventListener("click", (e) => {
    const target = /** @type {Element} */ (e.target);
    if (target.closest("a")) return;
    const id = Number(
      /** @type {HTMLElement | null} */ (target.closest("tr"))?.dataset.job,
    );
    if (Number.isInteger(id) && id > 0) openJobDialog(id);
  });

  let shown = "";
  if (!act.jobsRead)
    t.loading({ words: "Reading the jobs from the dashboard…" });
  const render = () => {
    if (!act.jobsRead) {
      if (act.failed.jobs) {
        t.failed(act.failed.jobs);
        paintTimeline(false);
      }
      return;
    }
    const now = Date.now() / 1000;
    const rows = jobRows(inSelection(), actionLabel, now);
    // Redraw only when something a row shows changed (a running job's
    // duration ticks with the second anyway).
    const sig = JSON.stringify(
      rows.map((r) => [
        r.job,
        r.badge.label,
        r.step,
        r.message,
        r.took == null,
      ]),
    );
    const running = rows.some((r) => r.badge.label === "running");
    if (sig === shown && !running) return;
    shown = sig;
    syncBar();
    paintTimeline(false);
    t.tbody.replaceChildren(
      ...rows.map((r) =>
        drivable(
          h(
            "tr",
            {
              class: "link-row",
              "data-job": String(r.job),
              "data-kp-row-key": String(r.job),
              title: "Open this job: its steps and its log",
            },
            td(String(r.job), "num"),
            td(keys.note("time", formatDateTime(r.queued), r.queued)),
            td(r.stack),
            td(r.action),
            td(r.origin),
            badgeCell(r.badge),
            td(
              r.took == null ? "—" : keys.note("duration", r.tookText, r.took),
              "num",
            ),
            td(r.step),
            td(r.message),
          ),
          opts.drive,
          String(r.job),
        ),
      ),
    );
    t.ready();
    setAgo(ago, now);
  };
  const offJobs = onAct("jobs", render);
  const retry = () => {
    t.loading({ words: "Reading the jobs from the dashboard…" });
    void loadJobs();
  };
  root.addEventListener("kp-datatable-retry", retry);
  const timer = setInterval(render, 5000);
  render();
  return () => {
    for (const off of detachKp) off();
    offJobs();
    root.removeEventListener("kp-datatable-retry", retry);
    clearInterval(timer);
    unbind();
    detach();
  };
}
