// redesign-activity (3.71.0, the Activity demo Kenny approved on
// 2026-10-03; FLOWS.md §1 "Activity: Running now, History (with who),
// Planned, Host log"): everything the host and this dashboard did, in one
// place. Three views under one header, each its own address:
//
//   /activity                 Now and history: the KPI strip, the open
//                             incidents, Running now (each job live, its log
//                             scrolling inside its own box), the timeline
//                             (kp gantt), the stack-by-day grid (kp heat)
//                             and the History (kp data table, grouped by
//                             day, who started each operation on every row);
//                             every job of the dashboard, with its own
//                             timeline, folded at the end
//   /activity?view=planned    the Schedules page, hosted as a view
//   /activity?view=host-log   the host-log explorer (log.js)
//
// The old addresses land here (router.js): /jobs → ?view=running (this
// view, scrolled to Running now; `?job=N` opens that job), /log →
// ?view=host-log, /schedules → ?view=planned, /timeline → ?view=timeline
// (this view, scrolled to the timeline).

import {
  act,
  actionLabel,
  catalogReady,
  loadJobs,
  onAct,
  send,
} from "../act.js";
import { openAction, reopenBatch } from "../actiondialog.js";
import { notify } from "../actui.js";
import {
  fetchReport,
  h,
  progressBar,
  tableBlock,
  bindTableUrl,
} from "../dom.js";
import { declare, drivable, viaForm } from "../drivable.js";
import {
  TIMELINE_LANES,
  WINDOWS,
  clock,
  dayStart,
  dayWord,
  filterFromParams,
  filterToParams,
  feedRows,
  heatModel,
  incidentFor,
  inRange,
  kpis,
  openFailures,
  showFilters,
  stepSegments,
  timelineModel,
  windowDays,
} from "../activityview.js";
import { formatDay, humanDuration } from "../format.js";
import {
  HISTORY_COLUMNS,
  historyBody,
  historyDetail,
} from "../historytable.js";
import { openIncident } from "../incident.js";
import {
  elapsedS,
  estimateRange,
  finished,
  originText,
  queueOrder,
  remaining,
  stepText,
} from "../jobs.js";
import { narrowBar } from "../narrowbar.js";
import { cover } from "../statecover.js";
import { current, subscribe } from "../store.js";
import {
  attentionBand,
  ensureStyle,
  keyRow,
  kpiStrip,
  pageHeader,
  section,
  segSwitch,
} from "../ui.js";
import { openJobDialog } from "../jobpanel.js";
import { stage } from "../stage.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import { jobLogViewer } from "../joblog.js";
import { setParams } from "../urlstate.js";
import { tabRow } from "../dom.js";
import { mountJobsTable } from "./jobs.js";
import { mountHostLog } from "./log.js";
import { mount as mountSchedules } from "./schedules.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import {
  attachGantts,
  setGanttKinds,
  setGanttLegend,
  setGanttPoints,
  setGanttRuns,
  setGanttSelection,
  setGanttState,
} from "/static/kp/js/gantt.js";
import {
  attachHeats,
  setHeat,
  setHeatLegend,
  setHeatPick,
  setHeatRows,
} from "/static/kp/js/heat.js";
import { attachSteplists } from "/static/kp/js/steplist.js";
import { attachTicks, tickTo } from "/static/kp/js/tick.js";
import { attachToolbars } from "/static/kp/js/toolbar.js";
import { leave } from "/static/kp/js/motion.js";

// Live view (invariant 39): every control on this page that opens a dialog
// or runs something. "Run it again" reaches its dialog as a catalog form.
const OPEN_JOB = declare({
  id: "activity-open-job",
  page: "activity",
  opens: "dialog",
  row: "<job>",
  what: "open a running job's live panel: its steps and its log",
  shows: "while a job runs (Running now)",
});
const CANCEL_JOB = declare({
  id: "activity-cancel-job",
  page: "activity",
  opens: "run",
  row: "<job>",
  what: "cancel a queued job before it starts",
  shows: "on a job that waits in the queue",
});
const CANCEL_QUEUED = declare({
  id: "activity-cancel-all-queued",
  page: "activity",
  opens: "run",
  what: "cancel every job that still waits in the queue",
  shows: "while jobs wait in the queue",
});
const OPEN_BATCH = declare({
  id: "activity-open-batch",
  page: "activity",
  opens: "dialog",
  row: "<batch>",
  what: "open the batch a job belongs to",
  shows: "on a job that came from a batch",
});
const OPEN_INCIDENT = declare({
  id: "activity-open-incident",
  page: "activity",
  opens: "dialog",
  row: "<incident bundle>",
  what: "open a failed operation's incident bundle",
});
const SHOW_LOG = declare({
  id: "activity-show-log",
  page: "activity",
  opens: "dialog",
  row: "<job>",
  what: "open the log of the job an operation in History ran as",
  shows: "in an opened History row of an operation this dashboard ran as a job",
  reach: [{ do: "click", control: "activity-row", row: "*" }],
});
const WINDOW = declare({
  id: "activity-window",
  page: "activity",
  opens: "view",
  row: "1|7|14|30",
  what: "show the last 1, 7, 14 or 30 days",
});
const ROW = declare({
  id: "activity-row",
  page: "activity",
  opens: "view",
  row: "<row key>",
  what: "open or close one History row: its steps, its error and what to do",
});
// 3.71.0 coordinator rule: every clickable element carries a declared id.
const VIEW_TAB = declare({
  id: "activity-view",
  page: "activity",
  opens: "view",
  row: "now|planned|host-log",
  what: "open one of Activity's views: Now and history, Planned, Host log",
});
const KPI_TILE = declare({
  id: "activity-kpi",
  page: "activity",
  opens: "view",
  row: "ok|incidents|nightly|running",
  what: "open the History or Running now a KPI tile counts",
});
const RUN_AGAIN = declare({
  id: "activity-run-again",
  page: "activity",
  opens: "dialog",
  row: "<row key>",
  what: "open the action dialog that starts a failed operation again",
});
// redesign-integrate-8: a failed update's Run it again opens the one
// Update flow (redesign-flows-1), an address, not a dialog: its own control,
// so Live view does not wait for a dialog that never comes.
const UPDATE_AGAIN = declare({
  id: "activity-update-again",
  page: "activity",
  opens: "view",
  row: "<row key>",
  what: "a failed update's Run it again: open the Update flow for its stack",
});
const STACK_LINK = declare({
  id: "activity-stack-link",
  page: "activity",
  opens: "view",
  row: "<stack>",
  what: "go to the stack an operation or a job was about",
});
const ALL_JOBS = declare({
  id: "activity-all-jobs",
  page: "activity",
  opens: "view",
  what: "unfold Every job: the last 200 jobs of this dashboard",
});
const EVERY_JOB = declare({
  id: "activity-every-job",
  page: "activity",
  opens: "view",
  what: "fold or unfold Every job",
});
const JOB_ROW = declare({
  id: "activity-job-row",
  page: "activity",
  opens: "dialog",
  row: "<job>",
  what: "open one job of Every job, live, in a dialog",
  shows: "in the unfolded Every job, once a job ran",
  reach: [{ do: "click", control: "activity-every-job" }],
});
const CHARTS = declare({
  id: "activity-charts",
  page: "activity",
  opens: "view",
  row: "<row key>",
  what: "open the charts around an operation's moment",
  shows: "in an opened History row",
  reach: [{ do: "click", control: "activity-row", row: "*" }],
});
const MARK = declare({
  id: "activity-timeline-mark",
  page: "activity",
  opens: "view",
  row: "<row key>",
  what: "open a timeline bar's operation in History",
  // A failed operation is a bar and a mark: both open the same row.
  twins: true,
});
const ADD_FILTER = declare({
  id: "activity-add-filter",
  page: "activity",
  opens: "view",
  what: "add a History filter: stack, operation, by or outcome",
  was: ["activity-show", "activity-show-more"],
});
const DAY = declare({
  id: "activity-day",
  page: "activity",
  opens: "view",
  row: "<day>",
  what: "fold or unfold one day of History",
});
const TAG_REMOVE = declare({
  id: "activity-filter-remove",
  page: "activity",
  opens: "view",
  row: "<range|day|stack:name>",
  what: "drop one pick made on the timeline or the grid",
  shows: "while a range, a day or a stack narrows History",
  was: ["activity-show-every-day"],
});
const TAGS_CLEAR = declare({
  id: "activity-filters-clear",
  page: "activity",
  opens: "view",
  what: "drop every pick made on the timeline and the grid",
  shows: "while a range, a day or a stack narrows History",
});
const HEAT_DAY = declare({
  id: "activity-heat-day",
  page: "activity",
  opens: "view",
  row: "<day>",
  what: "pick a day in the stack-by-day grid: History narrows to it",
  twins: true,
});
const HEAT_STACK = declare({
  id: "activity-heat-stack",
  page: "activity",
  opens: "view",
  row: "<stack>",
  what: "pick a stack in the stack-by-day grid: History narrows to it",
});
const SEARCH = declare({
  id: "activity-search",
  page: "activity",
  opens: "view",
  what: "History's search box",
});
const CLEAR = declare({
  id: "activity-clear-filters",
  page: "activity",
  opens: "view",
  what: "clear the History's search and filters",
  shows: "when the search and filters match no row",
});
const RETRY = declare({
  id: "activity-retry",
  page: "activity",
  opens: "run",
  what: "read the history again after a failed read",
  shows: "after the history could not be read",
});
const LOG_DRIVE = {
  follow: declare({
    id: "host-log-follow",
    page: "log",
    opens: "view",
    what: "pause or resume following the newest host lines",
  }),
  source: declare({
    id: "host-log-source",
    page: "log",
    opens: "view",
    row: "<source>",
    what: "turn one source's lines on or off",
    selects: true,
  }),
  only: declare({
    id: "host-log-only",
    page: "log",
    opens: "view",
    row: "<source>",
    what: "show only one source's lines",
  }),
  level: declare({
    id: "host-log-level",
    page: "log",
    opens: "view",
    row: "debug|info|warning|critical",
    what: "show or hide the host log's lines of one level",
    selects: true,
  }),
  reset: declare({
    id: "host-log-reset",
    page: "log",
    opens: "view",
    what: "show every source and level again",
    shows: "while a source or level filter narrows the lines",
    reach: [{ do: "click", control: "host-log-level", row: "debug" }],
  }),
  line: declare({
    id: "host-log-line",
    page: "log",
    opens: "view",
    row: "<line number>",
    what: "open or close one line's details",
  }),
  openJob: declare({
    id: "host-log-open-job",
    page: "log",
    opens: "dialog",
    row: "<job>",
    what: "open the job a host line belongs to",
    shows: "in an opened line this dashboard's job printed",
    reach: [{ do: "click", control: "host-log-line", row: "*" }],
  }),
  copy: declare({
    id: "host-log-copy-line",
    page: "log",
    opens: "run",
    what: "copy an opened line to the clipboard",
    shows: "in an opened line",
    reach: [{ do: "click", control: "host-log-line", row: "*" }],
  }),
  everything: declare({
    id: "host-log-everything",
    page: "log",
    opens: "view",
    what: "show every source again",
  }),
  onlyLine: declare({
    id: "host-log-only-line-source",
    page: "log",
    opens: "view",
    what: "show only the opened line's source",
    shows: "in an opened line",
    reach: [{ do: "click", control: "host-log-line", row: "*" }],
  }),
  newLines: declare({
    id: "host-log-back-to-tail",
    page: "log",
    opens: "view",
    what: "jump back to the newest lines and follow again",
    shows: "while the tail is paused and new lines came",
    reach: [{ do: "click", control: "host-log-follow" }],
  }),
  search: declare({
    id: "host-log-search",
    page: "log",
    opens: "view",
    what: "the Lines containing box",
  }),
  retry: declare({
    id: "host-log-retry",
    page: "log",
    opens: "run",
    what: "read the host's lines again after a failed read",
    shows: "after the lines could not be read",
  }),
};
const LOG_DOWNLOAD = declare({
  id: "host-log-download",
  page: "log",
  opens: "run",
  what: "save the lines the filter shows as host-log.txt",
});

/** The views, as tabs: each its own address. */
const VIEWS = /** @type {const} */ ([
  { view: "", label: "Now and history" },
  { view: "planned", label: "Planned" },
  { view: "host-log", label: "Host log" },
]);

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  ensureStyle("/css/pages/activity.css");
  root.classList.add("ac-page", "nx-ops");
  const params = new URLSearchParams(location.search);
  const asked = params.get("view") ?? "";
  const view = asked === "planned" || asked === "host-log" ? asked : "";
  const days = windowDays(params);

  const windowSeg = segSwitch({
    label: "Window",
    items: WINDOWS.map((d) => ({
      value: String(d),
      label: `${d} ${d === 1 ? "day" : "days"}`,
      hint: `Show the last ${d === 1 ? "day" : `${d} days`}`,
    })),
    value: String(days),
    onChange: (v) =>
      ctx.navigate(
        `/activity${setParams(location.search, { days: v === "14" ? null : v, from: null, to: null })}`,
      ),
    mark: (b, v) => void drivable(b, WINDOW, v),
  });
  const header = pageHeader({
    title: "Activity",
    desc:
      view === "planned"
        ? "Everything the host and this dashboard did, and what they will do: these are the schedules, every run on the calendar."
        : view === "host-log"
          ? "Everything the host and this dashboard did: here every line the host prints, live, whoever started the work."
          : `Everything the host and this dashboard did: what runs right now, what failed, and every operation of the last ${days === 1 ? "day" : `${days} days`}.`,
    live: view === "" ? "updated" : false,
    actions: view === "" ? [windowSeg.el] : [],
  });
  const tabs = tabRow(
    "Activity views",
    VIEWS.map((v) => ({
      href: `/activity${v.view ? `?view=${v.view}` : ""}`,
      label: v.label,
      current: v.view === view,
    })),
  );
  tabs.classList.add("ac-tabs");
  tabs
    .querySelectorAll("a")
    .forEach((a, i) =>
      drivable(
        /** @type {HTMLElement} */ (a),
        VIEW_TAB,
        VIEWS[i].view || "now",
      ),
    );
  const body = h("div", { class: "ac-body" });
  root.replaceChildren(header.el, tabs, body);

  if (view === "planned") {
    // The Schedules page, hosted as this view (its own module draws it,
    // its Live view controls stay declared on "schedules").
    const stop = mountSchedules(body, { level: "h2" });
    return () => {
      stop();
      root.classList.remove("ac-page", "nx-ops");
    };
  }
  if (view === "host-log") {
    // Pause and Download sit together in the card's head, the key hints
    // once in the toolbar, as Console does (senior review, finding 9).
    const pause = drivable(
      h("button", {
        type: "button",
        class: "kp-button kp-button--sm kp-button--secondary",
      }),
      LOG_DRIVE.follow,
    );
    const download = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--secondary",
          title: "Save the lines the filter shows as a .txt file",
        },
        "Download",
      ),
      LOG_DOWNLOAD,
    );
    const card = section({
      title: "Host log",
      // Finding 10: true at both widths (beside the lines on a wide
      // screen, above them on a phone).
      desc: "Every line the host prints for every operation, newest last; turn sources and levels on or off with the filters. Secrets are masked before a line leaves the host.",
      id: "host-log-card",
      tools: [pause, download],
    });
    body.append(card.el);
    const paintPause = (/** @type {boolean} */ on) => {
      pause.textContent = on ? "Pause" : "Resume";
      pause.title = on
        ? "Stop following; new lines wait below a marker (Space)"
        : "Follow the newest lines again (Space)";
      pause.setAttribute("aria-pressed", String(!on));
    };
    const ex = mountHostLog(card.body, {
      drive: LOG_DRIVE,
      followInBar: false,
      label: "Every line the host prints",
      onFollow: paintPause,
    });
    paintPause(ex.following());
    pause.addEventListener("click", () => ex.setFollow(!ex.following()));
    download.addEventListener("click", () => ex.download());
    body.append(
      keyRow([
        ["/", "search"],
        ["Ctrl K", "go to…"],
      ]),
    );
    return () => {
      ex.cleanup();
      root.classList.remove("ac-page", "nx-ops");
    };
  }
  const stop = mountNow(body, { days, header, ctx });
  return () => {
    stop();
    root.classList.remove("ac-page", "nx-ops");
  };
}

/**
 * The Now and history view.
 * @param {HTMLElement} body
 * @param {{days: number, header: ReturnType<typeof pageHeader>,
 *   ctx: {navigate: (href: string) => void}}} x
 * @returns {() => void}
 */
function mountNow(body, x) {
  const { days } = x;
  const params = new URLSearchParams(location.search);
  let filter = filterFromParams(params);
  /** @type {import("../activityview.js").FeedRow[]} */
  let rows = [];
  /** @type {import("../activity.js").Entry[]} */
  let entries = [];
  /** @type {string[]} */
  let incidents = [];
  let historyRead = false;
  /** @type {string | null} */
  let historyError = null;
  /** A reread that failed after a good read: the rows stay, with a note
   * (senior review, finding 4). @type {string | null} */
  let stale = null;
  let readAt = 0;
  /** The host's own words for a failed first read (kp's failed slot). @type {import("../doctor.js").RouteError | null} */
  let historyFail = null;
  let firstPaint = true;
  /** Whether the address's old `?show=` / `?q=` reached the table yet. */
  let urlApplied = false;
  const now = () => Date.now() / 1000;
  const t0 = () => dayStart(now()) - (days - 1) * 86400;
  const stackNames = () => (current().fleet?.stacks ?? []).map((s) => s.name);

  // ── KPI strip ─────────────────────────────────────────────────────────
  const strip = kpiStrip(
    [
      { key: "ops", label: "Operations" },
      { key: "ok", label: "Succeeded", href: "/activity?show=failed" },
      {
        key: "incidents",
        label: "Open incidents",
        href: "/activity?show=failed",
      },
      {
        key: "nightly",
        label: "Nightly round",
        href: "/activity?show=nightly",
      },
      { key: "running", label: "Running now", href: "/activity?view=running" },
    ],
    { loading: true },
  );
  strip.el.setAttribute("aria-label", `The last ${days} days`);
  for (const [key, t] of strip.tiles)
    if (t.el instanceof HTMLAnchorElement) drivable(t.el, KPI_TILE, key);
  const band = attentionBand();
  band.el.id = "attention";

  // ── Running now ───────────────────────────────────────────────────────
  const running = section({
    title: "Running now",
    desc: "Jobs this dashboard sent to the host that have not finished; each follows live.",
    id: "running",
  });
  const runList = h("div", { class: "ac-jobs" });
  const runFoot = h("span");
  const cancelAll = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm kp-button--ghost",
        title: "Take every job that still waits out of the queue",
        hidden: "",
      },
      "Cancel all queued",
    ),
    CANCEL_QUEUED,
  );
  cancelAll.addEventListener("click", () => {
    const waiting = runningJobs().filter((j) => j.state === "queued");
    cancelAll.disabled = true;
    void cancelJobs(waiting).then((refused) => {
      cancelAll.disabled = false;
      if (!refused && !cancelGone)
        notify(
          `Cancelled ${waiting.length} queued ${waiting.length === 1 ? "job" : "jobs"}.`,
          "success",
        );
    });
  });
  const allJobs = h(
    "a",
    {
      // This page's own path with the section's anchor (never a bare #).
      href: `${location.pathname}${location.search}#jobs`,
      class: "link",
      title: "Every job this dashboard ran, newest first",
    },
    "All jobs (the last 200)",
  );
  drivable(allJobs, ALL_JOBS);
  const runStage = stage(runList);
  setCardState(runStage, "loading", { words: "Reading the jobs" });
  running.body.append(
    runStage,
    h(
      "div",
      { class: "ac-foot" },
      h("span", { class: "ac-foot__queue" }, runFoot, cancelAll),
      allJobs,
    ),
  );

  // ── Timeline (kp gantt) ────────────────────────────────────────────────
  // The days the gantt offers: 24 h, 7 d, 14 d, 30 d up to the window.
  const rangeWords = [
    "24h",
    ...[7, 14, 30].filter((d) => d <= days).map((d) => `${d}d`),
  ];
  const narrow = narrowBar({
    label: "Filters set from the timeline and the grid",
    markRemove: (b, id) => void drivable(b, TAG_REMOVE, id),
    markClear: (b) => void drivable(b, TAGS_CLEAR),
  });
  const tlEl = h("section", {
    class: "kp-gantt",
    "data-kp-gantt": "",
    "data-kp-gantt-days": "",
    "data-kp-gantt-ranges": rangeWords.join(" "),
    "data-kp-gantt-range": rangeWords[rangeWords.length - 1],
    "data-kp-gantt-ghosts": "2 3",
    id: "timeline-chart",
    "aria-label": `Operations of the last ${days === 1 ? "day" : `${days} days`}`,
  });
  const tlCover = h("div", { class: "ac-cover" }, tlEl);
  const tlOps = h("b", { "data-kp-tick": "" }, "0");
  const tlFailed = h("b", { "data-kp-tick": "" }, "0");
  const fact = (/** @type {string} */ k, /** @type {Node} */ v) =>
    h("div", { class: "kp-kv__row" }, h("dt", null, k), h("dd", null, v));
  const timeline = section({
    title: `The timeline: what ran in the last ${days === 1 ? "day" : `${days} days`}`,
    desc: "Draws every operation as a bar on a time axis, so you see when things ran, how long they took and which failed; drag across it to pick a range that narrows the History, press a bar to open that operation.",
    id: "timeline",
  });
  timeline.body.append(
    narrow.el,
    tlCover,
    h(
      "dl",
      { class: "kp-kv ac-facts", "aria-label": "What the timeline holds" },
      fact("Operations in view", tlOps),
      fact("Failed in view", tlFailed),
    ),
  );

  // ── Stacks by day (kp heat) ───────────────────────────────────────────
  const heatEl = h("div", {
    class: "kp-heat",
    "data-kp-heat": "",
    "data-kp-heat-toggle": "",
    id: "heat-grid",
    "aria-label": "Operations per stack per day",
  });
  const heatCover = h("div", { class: "ac-cover ac-heat" }, heatEl);
  const heatCard = section({
    title: "Which stack is flaky: stacks by day",
    desc: "Shows every stack against every day, coloured by whether it ran cleanly, so a stack that keeps failing stands out at a glance; press a day or a stack name to narrow the History below.",
    id: "heat",
  });
  heatCard.body.append(heatCover);
  // kp's heat legend under the grid.
  setHeatLegend(heatEl, [
    { tone: "ok", label: "everything ran fine" },
    { tone: "bad", label: "something failed" },
    { tone: "none", label: "not touched that day" },
  ]);

  // ── History (kp data table) ───────────────────────────────────────────
  const stale$ = h("p", { class: "ac-stale", role: "status", hidden: "" });
  const t = tableBlock({
    remember: "activity-history",
    caption:
      "Operations on the stacks, newest first, a heading per day. Each row opens to its steps.",
    captionHidden: true,
    search: "Search the history",
    state: "loading",
    pageSize: 25,
    pageSizes: "10,25,50",
    nothing: `Nothing ran in the last ${days === 1 ? "day" : `${days} days`}. Every operation the host runs, asked for or on its own, is listed here as soon as it starts, with who started it.`,
    columns: HISTORY_COLUMNS.map((c) => ({ ...c })),
  });
  t.tbody.id = "history-table";
  t.wrap.setAttribute("data-kp-expandable", "");
  t.wrap.setAttribute("data-kp-expand-single", "");
  t.wrap.setAttribute("data-kp-filter-mode", "add");
  t.wrap.classList.add("ac-history");
  const historyCard = section({
    title: "History: every operation, grouped by day",
    desc: "Lists every operation in the order it happened, a heading per day, so you can find one by stack, kind or outcome and open it to see its steps.",
    id: "history",
  });
  historyCard.body.append(stale$, t.wrap);

  // ── every job (folded) ────────────────────────────────────────────────
  const jobsCard = section({
    title: "Every job",
    desc: "Draws every job on a lane of its stack, with the time it waited in the queue before it started, and lists the last 200 jobs this dashboard sent to the host; press a bar or a row to follow one.",
    id: "jobs",
    collapsible: true,
    open: false,
    mount: (b) => mountJobsTable(b, { drive: JOB_ROW }),
  });
  const fold = jobsCard.el.querySelector("summary");
  if (fold) drivable(/** @type {HTMLElement} */ (fold), EVERY_JOB);
  allJobs.addEventListener("click", (e) => {
    e.preventDefault();
    jobsCard.open();
    jobsCard.el.scrollIntoView({ block: "start", behavior: "smooth" });
  });

  body.append(
    strip.el,
    band.el,
    running.el,
    timeline.el,
    heatCard.el,
    historyCard.el,
    jobsCard.el,
    keyRow([
      ["/", "search"],
      ["J / K", "next / previous row"],
      ["Enter", "open a row"],
      ["F", "failed only"],
      ["Ctrl K", "go to…"],
    ]),
  );

  // ── painting ──────────────────────────────────────────────────────────
  const origins = () => {
    /** @type {Map<number, any>} */
    const m = new Map();
    for (const j of act.jobs) for (const r of j.reqs ?? []) m.set(r, j.origin);
    return m;
  };
  const rebuild = () => {
    rows = feedRows(entries, { stacks: stackNames(), origins: origins() });
  };
  const runningJobs = () =>
    act.jobs.filter((j) => !finished(j.state)).sort(queueOrder);

  /** fix-batch: a 404 from a cancel route means this dashboard has none. */
  let cancelGone = false;
  /**
   * Cancel queued jobs: one request per batch (its still-queued jobs) and
   * one per job that came by hand or from a schedule.
   * @param {import("../jobs.js").Job[]} jobs
   * @returns {Promise<number>} how many requests were refused
   */
  const cancelJobs = async (jobs) => {
    /** @type {Set<number>} */
    const batches = new Set();
    /** @type {string[]} */
    const urls = [];
    for (const j of jobs) {
      if (j.origin.from === "batch") {
        if (batches.has(j.origin.batch)) continue;
        batches.add(j.origin.batch);
        urls.push(`/data/actions/batch/${j.origin.batch}/cancel`);
      } else urls.push(`/data/actions/jobs/${j.job}/cancel`);
    }
    let refused = 0;
    for (const u of urls) {
      const r = await send("POST", u, undefined, "cancel the queued job");
      if (r.ok) continue;
      if (r.status === 404) cancelGone = true;
      else {
        refused += 1;
        notify(`${r.error.what}: ${r.error.why}.`, "warning");
      }
    }
    await loadJobs();
    return refused;
  };

  /** A stack's name as a link to its hub. @param {string} name */
  const stackLink = (name) =>
    drivable(
      h(
        "a",
        { class: "link", href: `/stacks/${encodeURIComponent(name)}` },
        name,
      ),
      STACK_LINK,
      name,
    );

  /** @param {import("../jobs.js").Job} j */
  const jobWords = (j) =>
    `${actionLabel(j.action)} ${j.stack === "_host" ? "the host" : j.stack} · ${stepText(j)}`;

  const paintKpis = () => {
    if (!historyRead) return;
    const run = runningJobs();
    const fails = openFailures(rows);
    for (const k of kpis({
      rows,
      days,
      now: now(),
      open: fails,
      running: run,
      runningText: run[0] ? jobWords(run[0]) : "",
    })) {
      const t = strip.tiles.get(k.key);
      if (!t) continue;
      delete t.el.dataset.loading;
      t.set(/** @type {any} */ (k));
    }
  };

  /** A catalog action that runs `r` again, or null. @param {import("../activityview.js").FeedRow} r */
  const againAction = (r) => {
    if (r.entry.kind !== "op") return null;
    const label = r.entry.label.replace(/^scheduled-/, "");
    const a = act.catalog?.actions.find((x) => x.action === label);
    if (!a) return null;
    return {
      action: a.action,
      stack: r.stack ?? act.catalog?.host_target ?? "_host",
    };
  };

  /** @param {import("../activityview.js").FeedRow} r */
  const againButton = (r, primary = true) => {
    const a = againAction(r);
    if (!a) return null;
    const b = h(
      "button",
      {
        type: "button",
        class: `kp-button kp-button--sm${primary ? " kp-button--primary" : ""}`,
        "data-action": a.action,
        title:
          "Start the same operation again: its dialog says what will happen first",
      },
      "Run it again",
    );
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      void openAction(a.stack, a.action);
    });
    return viaForm(
      drivable(b, a.action === "update" ? UPDATE_AGAIN : RUN_AGAIN, r.key),
      a.action,
    );
  };
  /** @param {string} name */
  const incidentButton = (name) => {
    const b = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          title:
            "The incident bundle: the log, the plan and the host's state at the time",
          "data-incident": name,
        },
        "Open the incident",
      ),
      OPEN_INCIDENT,
      name,
    );
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      void openIncident(name);
    });
    return b;
  };

  const paintAttention = () => {
    if (!historyRead) return;
    const fails = openFailures(rows);
    const shown = fails.slice(0, 3);
    band.set([
      ...shown.map((r) => {
        const bundle = incidentFor(r, incidents);
        const acts = [
          ...(bundle ? [incidentButton(bundle)] : []),
          ...[againButton(r)].filter((b) => b != null),
        ];
        return {
          key: r.key,
          tone: /** @type {const} */ ("bad"),
          title: `${r.what} failed ${dayWord(r.start, now())} at ${clock(r.start)}`,
          // redesign-final M8: the bundle by its button, never its file name.
          text: `${sentence(r.error ?? "The host reported a failure")}${bundle ? " Its incident bundle holds the log: Open the incident." : ""}`,
          action: acts.length ? h("div", { class: "ac-acts" }, ...acts) : null,
        };
      }),
      ...(fails.length > shown.length
        ? [
            {
              key: "more",
              tone: /** @type {const} */ ("warn"),
              title: `${fails.length - shown.length} more ${fails.length - shown.length === 1 ? "operation is" : "operations are"} still failing`,
              text: "History below, with Failed on, lists every one.",
              action: null,
            },
          ]
        : []),
    ]);
  };

  // Running now: one block per job, updated in place.
  /** @type {Map<number, {el: HTMLElement, update: () => void, stop?: () => void}>} */
  const blocks = new Map();
  /** "batch N" as a link that opens the batch's panel. @param {number} n */
  const batchLink = (n) => {
    const a = drivable(
      h(
        "a",
        {
          class: "link",
          href: `/activity?batch=${n}`,
          title: "Open this batch: its progress and every job in it",
        },
        `batch ${n}`,
      ),
      OPEN_BATCH,
      String(n),
    );
    a.addEventListener("click", (e) => {
      e.preventDefault();
      void reopenBatch(n);
    });
    return h("span", { class: "ac-muted" }, " · ", a);
  };
  /** @param {import("../jobs.js").Job} j0 */
  const jobBlock = (j0) => {
    const id = j0.job;
    // Both are redrawn every second the job moves: the facts do not arrive
    // the theme's way each time (kp's arrival squeezes a new span, and the
    // line wrapped and grew the card while it played).
    const title = h("div", {
      class: "ac-job__title",
      "data-kp-arrive": "none",
    });
    const facts = h("div", {
      class: "ac-job__facts",
      "data-kp-arrive": "none",
    });
    // The step bar of a job with no step count: kp's busy bar, drawn by the theme.
    const wholeWords = h("span");
    const whole = h(
      "div",
      { class: "ac-step ac-step--whole", role: "listitem", "data-s": "run" },
      progressBar("Working", null),
      wholeWords,
    );
    const steps = h("div", {
      class: "ac-job__steps",
      role: "list",
      "aria-label": "Steps",
    });
    // feat-kpadopt-2: kp's log viewer, the job panel's own builder.
    const jobLog = jobLogViewer(id, {
      whole: false,
      tools: "search,wrap,time,level,copy",
      label: "The newest lines of this job",
      height: "11rem",
    });
    jobLog.element.classList.add("ac-job__log");
    const cancel = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost",
          title: "Take this job out of the queue before it starts",
          hidden: "",
        },
        "Cancel",
      ),
      CANCEL_JOB,
      String(id),
    );
    cancel.addEventListener("click", () => {
      cancel.disabled = true;
      void cancelJobs(act.jobs.filter((x) => x.job === id)).then(() => {
        cancel.disabled = false;
      });
    });
    const openLog = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          title: "This job in a dialog: its steps and every line it printed",
        },
        "Open the log",
      ),
      OPEN_JOB,
      String(id),
    );
    openLog.addEventListener("click", () => openJobDialog(id));
    const el = h(
      "article",
      { class: "ac-job", "data-job": String(id) },
      h("div", { class: "ac-job__main" }, title, facts),
      h("div", { class: "ac-job__acts" }, cancel, openLog),
      steps,
      jobLog.element,
    );
    const update = () => {
      const j = act.jobs.find((x) => x.job === id);
      if (!j) return;
      const t = now();
      title.replaceChildren(
        `${actionLabel(j.action)} `,
        j.stack === "_host" ? "the host" : stackLink(j.stack),
        h("span", { class: "ac-muted" }, ` · job ${j.job}`),
        ...(j.origin.from === "batch" ? [batchLink(j.origin.batch)] : []),
      );
      cancel.hidden = j.state !== "queued" || cancelGone;
      const p = j.progress;
      const ran = elapsedS(j, t);
      const left = remaining(j, act.progressAt.get(id) ?? null, t);
      facts.replaceChildren(
        h(
          "span",
          null,
          j.state === "queued"
            ? "waiting in the queue"
            : p
              ? h(
                  "span",
                  null,
                  "step ",
                  h("b", null, p.m ? `${p.n} of ${p.m}` : String(p.n)),
                )
              : "starting",
        ),
        ...(ran != null
          ? [h("span", null, "running ", h("b", null, humanDuration(ran)))]
          : []),
        ...(left.s != null
          ? left.late
            ? [
                h(
                  "span",
                  { class: "ac-late", title: "Past what earlier runs took" },
                  h("b", null, left.text),
                ),
              ]
            : [
                h(
                  "span",
                  null,
                  "about ",
                  h("b", null, humanDuration(left.s)),
                  " left",
                ),
              ]
          : left.text
            ? [h("span", null, left.text)]
            : []),
        ...(estimateRange(p)
          ? [h("span", null, estimateRange(p)?.text ?? "")]
          : []),
        originText(j.origin).startsWith("by ")
          ? h("span", null, h("b", null, originText(j.origin)))
          : h("span", null, "by ", h("b", null, originText(j.origin))),
      );
      const segs = stepSegments(p);
      if (segs.length)
        steps.replaceChildren(
          ...segs.map((s) =>
            h(
              "div",
              { class: "ac-step", role: "listitem", "data-s": s.state },
              h("i"),
              h(
                "span",
                null,
                s.state === "run" && p?.step
                  ? `${s.n} · ${p.step}`
                  : String(s.n),
              ),
            ),
          ),
        );
      else {
        // One busy bar, kept between updates: a bar built anew on every
        // line would restart the theme's busy picture each time.
        wholeWords.textContent = p?.step ?? stepText(j);
        if (steps.firstElementChild !== whole || steps.childElementCount > 1)
          steps.replaceChildren(whole);
      }
    };
    update();
    return {
      el,
      update,
      stop: () => jobLog.stop(),
    };
  };

  const paintRunning = () => {
    const run = runningJobs();
    const ids = new Set(run.map((j) => j.job));
    // A job that ended leaves the theme's way (kp-themes' leave()).
    /** @type {Promise<void>[]} */
    const gone = [];
    for (const [id, b] of blocks)
      if (!ids.has(id)) {
        b.stop?.();
        gone.push(leave(b.el));
        blocks.delete(id);
      }
    for (const j of run) {
      let b = blocks.get(j.job);
      if (!b) {
        b = jobBlock(j);
        blocks.set(j.job, b);
      } else b.update();
      runList.append(b.el);
    }
    if (!run.length) {
      const idle = () => {
        if (runningJobs().length) return;
        if (act.jobsRead)
          setCardState(runStage, "empty", {
            title: "Nothing is running",
            text: "A job you start from any stack's page shows here live, step by step; the running pill in the bar follows it too.",
          });
        else setCardState(runStage, "loading", { words: "Reading the jobs" });
      };
      // The empty state waits for the last job's exit.
      if (gone.length) void Promise.all(gone).then(idle);
      else idle();
    } else setCardState(runStage, "ready");
    const queued = run.filter((j) => j.state === "queued").length;
    runFoot.textContent = `${run.length - queued} running · ${queued} queued`;
    cancelAll.hidden = queued === 0 || cancelGone;
  };

  // ── kp parts the page carries ─────────────────────────────────────────
  const detachKp = [
    attachCardStates(running.body),
    attachToolbars(timeline.body),
    attachTicks(timeline.body),
    attachGantts(timeline.body, {
      // Every bar opens its operation: one control, a row per operation.
      decorate: (part, info) => {
        if (info.kind === "bar" && info.id) drivable(part, MARK, info.id);
      },
    }),
    attachHeats(heatCard.body),
    attachDataTables(historyCard.body, {
      expandSingle: true,
      filterMode: "add",
      decorate: (part, info) => {
        if (info.kind === "row-toggle" && info.value)
          drivable(part, ROW, info.value);
        else if (info.kind === "row-group-toggle")
          drivable(part, DAY, info.value ?? "");
      },
    }),
  ];
  setGanttState(tlEl, "loading");
  // The narrowing the timeline, the grid and the table share ──────────
  /** The day picked in the grid: a dayStart, as text. @type {string | null} */
  let pickDay = null;
  /** The stacks picked in the grid. @type {Set<string>} */
  const pickStacks = new Set();
  const count = (
    /** @type {number} */ n,
    /** @type {string} */ one,
    /** @type {string} */ many,
  ) => `${n} ${n === 1 ? one : many}`;
  const narrowed = () =>
    filter.range != null || pickDay != null || pickStacks.size > 0;
  /** The rows the picks leave, before the table's own search and filters. */
  const visibleRows = () =>
    rows.filter(
      (r) =>
        inRange(r, filter.range) &&
        (pickDay == null || String(dayStart(r.start)) === pickDay) &&
        (!pickStacks.size || (r.stack != null && pickStacks.has(r.stack))),
    );
  const syncNarrow = () => {
    const tags = [
      ...(filter.range
        ? [
            {
              id: "range",
              label: `From the timeline: ${stamp(filter.range.from)} to ${stamp(filter.range.to)}`,
            },
          ]
        : []),
      ...(pickDay != null
        ? [{ id: "day", label: `Day: ${formatDay(Number(pickDay))}` }]
        : []),
      ...[...pickStacks].map((s) => ({
        id: `stack:${s}`,
        label: `Stack: ${s}`,
      })),
    ];
    narrow.set(tags);
    // While the history is read the components show their own loading.
    narrow.state.textContent = !historyRead
      ? ""
      : narrowed()
        ? `${count(visibleRows().length, "operation", "operations")} of ${rows.length} are listed in the History`
        : `${count(rows.length, "operation", "operations")} in the last ${days === 1 ? "day" : `${days} days`}. Drag across the timeline, or press a day or a stack, to narrow the History.`;
  };
  /** @param {number} unix */
  const stamp = (unix) => `${formatDay(unix)} ${clock(unix)}`;

  // ── timeline ──────────────────────────────────────────────────────────
  /** The rounds the timeline draws as one bar: its id → the rows. @type {Map<string, import("../activityview.js").FeedRow[]>} */
  let tlRounds = new Map();
  const retryRead = () => void loadHistory();
  const paintTimeline = () => {
    cover(tlCover, null);
    if (!historyRead) {
      setGanttState(tlEl, "loading");
      return;
    }
    if (historyError) {
      setGanttRuns(tlEl, [], TIMELINE_LANES);
      setGanttPoints(tlEl, [], TIMELINE_LANES);
      setGanttState(tlEl, "error", "The history could not be read");
      cover(tlCover, "error", {
        title: "The history could not be read",
        text: historyError,
        retry: retryRead,
      });
      return;
    }
    const m = timelineModel(rows, { from: t0(), now: now() });
    tlRounds = m.rounds;
    if (!m.runs.length && !m.points.length) {
      setGanttRuns(tlEl, [], TIMELINE_LANES);
      setGanttPoints(tlEl, [], TIMELINE_LANES);
      setGanttState(tlEl, "empty", "Nothing ran in this window");
      cover(tlCover, "empty", {
        title: `Nothing has run in the last ${days === 1 ? "day" : `${days} days`}`,
        text: "Every update, apply, restart and backup is drawn here as soon as it starts.",
      });
    } else {
      setGanttRuns(tlEl, m.runs, TIMELINE_LANES);
      setGanttPoints(tlEl, m.points, TIMELINE_LANES);
      setGanttKinds(tlEl, [
        {
          kind: "failed",
          label: "A stack failed",
          shape: "diamond",
          tone: "destructive",
        },
      ]);
      setGanttLegend(tlEl, [
        { series: 1, label: "Nightly round" },
        { series: 2, label: "Scheduled operations" },
        { series: 3, label: "Operator action" },
      ]);
    }
    paintTimelineFacts(false);
  };
  const paintTimelineFacts = (/** @type {boolean} */ animate) => {
    const list = rows.filter(
      (r) =>
        r.entry.kind === "op" && r.start >= t0() && inRange(r, filter.range),
    );
    const failed = list.filter((r) => r.state === "failed").length;
    for (const [el, n] of /** @type {const} */ ([
      [tlOps, list.length],
      [tlFailed, failed],
    ])) {
      if (animate) tickTo(el, String(n));
      else el.textContent = String(n);
    }
  };
  tlEl.addEventListener("kp-gantt-select", (e) => {
    const { from, to, source } = /** @type {CustomEvent} */ (e).detail;
    filter = {
      ...filter,
      range:
        from != null && to != null
          ? { from: from / 1000, to: to / 1000 }
          : null,
    };
    syncNarrow();
    paintTimelineFacts(true);
    paintFeed();
    changed();
    if (source === "day" && filter.range)
      historyCard.el.scrollIntoView({ behavior: "smooth", block: "start" });
  });
  tlEl.addEventListener("kp-gantt-open", (e) => {
    const id = String(/** @type {CustomEvent} */ (e).detail.id);
    const round = tlRounds.get(id);
    const row = round
      ? (round.find((r) => r.state === "failed") ?? round[0])
      : rows.find((r) => r.key === id);
    if (row) openRow(row.key);
  });

  // ── stacks by day ─────────────────────────────────────────────────────
  const paintHeat = () => {
    cover(heatCover, null);
    const names = [
      ...new Set([
        ...stackNames(),
        ...rows.map((r) => r.stack).filter((s) => s != null),
      ]),
    ];
    // The frame keeps the height of every stack plus the fleet row.
    heatCover.style.setProperty(
      "--ac-heat-rows",
      String(Math.max(names.length, 4) + 1),
    );
    if (!historyRead) {
      setHeat(heatEl, {
        state: "loading",
        skeleton: { rows: Math.max(names.length, 4), columns: days },
        label: "Operations per stack per day, loading",
      });
      return;
    }
    if (historyError) {
      setHeat(heatEl, {
        state: "error",
        message: "The history could not be read.",
        label: "Operations per stack per day",
      });
      cover(heatCover, "error", {
        title: "The history could not be read",
        text: historyError,
        retry: retryRead,
      });
      return;
    }
    if (!rows.some((r) => r.entry.kind === "op")) {
      setHeat(heatEl, {
        state: "empty",
        message: "Nothing has run yet, so there is nothing to compare.",
        label: "Operations per stack per day",
      });
      cover(heatCover, "empty", {
        title: "Nothing has run yet",
        text: "Once stacks have been updated or backed up, each day shows here.",
      });
      return;
    }
    const m = heatModel(rows, { stacks: names, days, now: now() });
    const every =
      window.innerWidth < 700 ? (days > 14 ? 7 : 3) : days > 14 ? 3 : 1;
    // The axis names every `every`-th day counting back from today, so the
    // last label is always today; an unnamed day has no label at all.
    const columns = m.columns.map((c, i) => ({
      key: c.key,
      label:
        (m.columns.length - 1 - i) % every !== 0
          ? ""
          : String(Number(formatDay(c.day + 3600).slice(0, 2))),
      minor: (m.columns.length - 1 - i) % every !== 0,
    }));
    const said = (/** @type {string} */ stack, /** @type {number} */ day) => {
      const c = m.cell(stack, String(day));
      return c
        ? `${count(c.n, "operation", "operations")}${c.f ? `, ${c.f} failed (${[...c.kinds].join(", ")})` : ", all fine"}`
        : "not touched";
    };
    /** @type {import("/static/kp/js/heat.js").HeatSpec} */
    const spec = {
      columns,
      rows: names.map((s) => ({
        key: s,
        label: s,
        cells: m.columns.map((c) => {
          const cell = m.cell(s, c.key);
          const words = said(s, c.day);
          const date = formatDay(c.day + 3600);
          return {
            tone: cell ? (cell.f ? "bad" : "ok") : "none",
            label: words,
            aria: `${s}, ${date}: ${words}`,
            ...(cell ? { tip: `${s}, ${date}: ${words}.` } : {}),
          };
        }),
      })),
      summary: {
        key: "fleet",
        label: "Fleet",
        cells: m.columns.map((c) => {
          const date = formatDay(c.day + 3600);
          const f = m.fleet(c.key);
          if (!f.ran)
            return {
              tone: "none",
              label: "nothing ran",
              aria: `Fleet, ${date}: nothing ran`,
            };
          const words = f.bad
            ? `${f.bad} of ${f.ran} stacks that ran had a failure`
            : `all ${f.ran} stacks that ran were fine`;
          return {
            tone: f.bad ? "bad" : "ok",
            share: f.bad ? Math.max(f.bad / f.ran, 0.18) : 1,
            label: words,
            aria: `Fleet, ${date}: ${words}`,
          };
        }),
      },
      every: 1,
      toggle: true,
      picked: pickDay,
      label: `Operations per stack per day, last ${days === 1 ? "day" : `${days} days`}`,
    };
    setHeat(heatEl, spec);
    setHeatRows(heatEl, [...pickStacks]);
    for (const b of /** @type {NodeListOf<HTMLElement>} */ (
      heatEl.querySelectorAll(".kp-heat__label-button")
    ))
      drivable(b, HEAT_STACK, b.dataset.key ?? "");
    for (const b of /** @type {NodeListOf<HTMLElement>} */ (
      heatEl.querySelectorAll('.kp-heat__cell[data-row-key="fleet"]')
    ))
      drivable(b, HEAT_DAY, b.dataset.colKey ?? "");
  };
  heatEl.addEventListener("kp-heat-pick", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    pickDay = d.picked ? d.colKey : null;
    syncNarrow();
    paintFeed();
  });
  heatEl.addEventListener("kp-heat-row", (e) => {
    pickStacks.clear();
    for (const k of /** @type {CustomEvent} */ (e).detail.keys)
      pickStacks.add(k);
    syncNarrow();
    paintFeed();
  });
  narrow.el.addEventListener("kp-toolbar-remove", (e) => {
    const id = String(/** @type {CustomEvent} */ (e).detail?.id ?? "");
    if (id === "range") {
      filter = { ...filter, range: null };
      setGanttSelection(tlEl, null);
    } else if (id === "day") {
      pickDay = null;
      setHeatPick(heatEl, null);
    } else if (id.startsWith("stack:")) {
      pickStacks.delete(id.slice(6));
      setHeatRows(heatEl, [...pickStacks]);
    }
    afterNarrow();
  });
  narrow.el.addEventListener("kp-toolbar-clear", () => {
    filter = { ...filter, range: null };
    pickDay = null;
    pickStacks.clear();
    setGanttSelection(tlEl, null);
    setHeatPick(heatEl, null);
    setHeatRows(heatEl, []);
    afterNarrow();
  });
  const afterNarrow = () => {
    syncNarrow();
    paintTimelineFacts(true);
    paintFeed();
    changed();
  };

  // ── History feed (kp data table) ──────────────────────────────────────
  const dt = dataTable(t.wrap);
  /** A column's place in the table's header (kp adds a column for the row toggle). @param {string} name */
  const columnOf = (name) =>
    [...t.wrap.querySelectorAll("thead th")].findIndex(
      (th) => (th.textContent ?? "").trim() === name,
    );
  /** Wire the table's own controls for Live view, once it is attached. */
  const markTable = () => {
    const add = t.wrap.querySelector("[data-kp-datatable-add-filter]");
    if (add instanceof HTMLElement && !add.dataset.drive)
      drivable(add, ADD_FILTER);
    const search = t.wrap.querySelector("[data-kp-datatable-search]");
    if (search instanceof HTMLElement) drivable(search, SEARCH);
    const clear = t.wrap.querySelector("[data-kp-datatable-clear]");
    if (clear instanceof HTMLElement) drivable(clear, CLEAR);
  };
  /** @param {string} key */
  const rowOf = (key) =>
    t.tbody.querySelector(`tr[data-kp-row-key="${CSS.escape(key)}"]`);
  /** @param {string} key */
  const fillDetail = (key) => {
    const r = rows.find((x) => x.key === key);
    const cell = rowOf(key)?.nextElementSibling?.querySelector("td");
    if (!r || !cell) return;
    const d = detail(r);
    cell.replaceChildren(d);
    attachSteplists(d);
  };
  t.wrap.addEventListener("kp-datatable-expand", (e) => {
    const { key, open } = /** @type {CustomEvent} */ (e).detail;
    if (open) fillDetail(key);
  });
  t.wrap.addEventListener("kp-datatable-retry", retryRead);

  /** @param {string} key */
  const openRow = (key) => {
    const r = rows.find((x) => x.key === key);
    if (!r || !dt) return;
    // A row the picks or the table's filters hide is brought back first.
    if (!visibleRows().includes(r)) {
      filter = { ...filter, range: null };
      pickDay = null;
      pickStacks.clear();
      setGanttSelection(tlEl, null);
      setHeatPick(heatEl, null);
      setHeatRows(heatEl, []);
      syncNarrow();
      changed();
    }
    dt.clearFilters();
    dt.query("");
    paintFeed();
    const group = String(dayStart(r.start));
    dt.fold(dt.view().folded.filter((k) => k !== group));
    const at = dt.view().keys.indexOf(key);
    if (at >= 0) dt.page(Math.floor(at / dt.view().pageSize));
    dt.expand([key]);
    const row = rowOf(key);
    row?.scrollIntoView({ block: "center", behavior: "smooth" });
    /** @type {HTMLElement | null | undefined} */ (
      row?.querySelector(".kp-datatable__expand")
    )?.focus({ preventScroll: true });
  };

  /** @param {import("../activityview.js").FeedRow} r */
  const detail = (r) => {
    const e = r.entry;
    const job =
      e.kind === "op" && e.req != null
        ? act.jobs.find((j) => j.reqs?.includes(e.req ?? -1))
        : null;
    const bundle = r.state === "failed" ? incidentFor(r, incidents) : null;
    const age = now() - r.start;
    const range =
      age < 3600
        ? "1h"
        : age < 6 * 3600
          ? "6h"
          : age < 86400
            ? "24h"
            : age < 7 * 86400
              ? "7d"
              : "30d";
    const acts = [
      ...(r.state === "failed" ? [againButton(r)] : []),
      ...(bundle ? [incidentButton(bundle)] : []),
      ...(job
        ? [
            (() => {
              const b = drivable(
                h(
                  "button",
                  {
                    type: "button",
                    class: "kp-button kp-button--sm",
                    title: "Every line this operation's job printed",
                  },
                  "Show the log",
                ),
                SHOW_LOG,
                String(job.job),
              );
              b.addEventListener("click", (ev) => {
                ev.stopPropagation();
                openJobDialog(job.job);
              });
              return b;
            })(),
          ]
        : []),
      drivable(
        h(
          "a",
          {
            class: "kp-button kp-button--sm",
            href: `/charts${setParams("", { stack: r.stack, range })}`,
            title:
              "The charts of this moment's window, for this stack or the host",
          },
          "Charts at this time",
        ),
        CHARTS,
        r.key,
      ),
    ].filter((b) => b != null);
    return historyDetail(r, { actions: /** @type {HTMLElement[]} */ (acts) });
  };

  /** What was painted last, so a reread that changed nothing repaints nothing. */
  let sig = "";
  const paintFeed = () => {
    syncNarrow();
    if (!historyRead) {
      t.loading({ words: "Reading the history…" });
      return;
    }
    if (historyError) {
      t.failed(
        historyFail ?? { what: "the history", why: historyError, fix: "" },
      );
      return;
    }
    stale$.hidden = !stale;
    if (stale) {
      const again = drivable(
        h("button", { type: "button", class: "link-button" }, "Read it again"),
        RETRY,
      );
      again.addEventListener("click", retryRead);
      stale$.replaceChildren(
        `Not refreshed: ${stale}. These rows are from ${clock(readAt)}. `,
        again,
      );
    }
    const shown = visibleRows();
    const next = JSON.stringify([
      dayStart(now()),
      shown.map((r) => [r.key, r.state, r.took]),
    ]);
    const changedRows = next !== sig;
    if (changedRows) {
      sig = next;
      // The row that had focus gets it back after the repaint.
      const had =
        document.activeElement instanceof Element &&
        t.tbody.contains(document.activeElement)
          ? document.activeElement.closest("tr[data-kp-row-key]")
          : null;
      const focusKey = had?.getAttribute("data-kp-row-key") ?? null;
      const onToggle = !!document.activeElement?.closest(
        ".kp-datatable__expand",
      );
      t.tbody.replaceChildren(...historyBody(shown, { now: now(), stackLink }));
      t.ready();
      for (const k of dt?.view().expanded ?? []) fillDetail(k);
      if (focusKey != null) {
        const row = rowOf(focusKey);
        /** @type {HTMLElement | null | undefined} */ (
          onToggle ? row?.querySelector(".kp-datatable__expand") : row
        )?.focus({ preventScroll: true });
      }
    } else t.ready({ refresh: false });
    markTable();
  };

  const changed = () => {
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, filterToParams(filter)),
    );
  };

  /**
   * An older address's `?show=failed|nightly|claude` and `?q=` reach the
   * table as its own filters and search, once the rows are in.
   */
  const applyAddress = () => {
    urlApplied = true;
    const unbind = bindTableUrl(dt, "history");
    unbinds.push(unbind);
    const f = showFilters(rows, filter.show);
    if (dt) {
      if (f.outcome.length) dt.filter(columnOf("Outcome"), f.outcome);
      if (f.by.length) dt.filter(columnOf("By"), f.by);
      if (filter.q) dt.query(filter.q);
    }
    if (filter.show.size || filter.q) {
      filter = { ...filter, show: new Set(), q: "" };
      changed();
    }
    if (filter.range) {
      setGanttSelection(tlEl, {
        from: filter.range.from * 1000,
        to: filter.range.to * 1000,
      });
      syncNarrow();
      paintFeed();
    }
  };
  /** @type {(() => void)[]} */
  const unbinds = [];

  const paintAll = () => {
    rebuild();
    paintKpis();
    paintAttention();
    paintRunning();
    paintTimeline();
    paintHeat();
    paintFeed();
    if (historyRead && !historyError && !urlApplied) applyAddress();
    if (firstPaint && historyRead) {
      firstPaint = false;
      const p = new URLSearchParams(location.search);
      const v = p.get("view");
      const job = Number(p.get("job"));
      if (v === "running") running.el.scrollIntoView({ block: "start" });
      if (v === "timeline") timeline.el.scrollIntoView({ block: "start" });
      if (
        Number.isInteger(job) &&
        job > 0 &&
        act.jobs.some((j) => j.job === job)
      )
        openJobDialog(job, {
          onClose: () =>
            history.replaceState(
              history.state,
              "",
              location.pathname + setParams(location.search, { job: null }),
            ),
        });
    }
  };

  // ── reading ───────────────────────────────────────────────────────────
  const abort = new AbortController();
  const loadHistory = async () => {
    const since = t0();
    const r = await fetchReport(
      `/data/history?since=${since}&limit=5000`,
      "the history",
      abort.signal,
    );
    historyRead = true;
    if (!r.ok) {
      // Keep what the last good read showed; only a first read that fails
      // replaces the feed with the error.
      if (readAt) stale = r.error.why;
      else {
        historyError = r.error.why;
        historyFail = r.error;
      }
    } else {
      historyError = null;
      historyFail = null;
      stale = null;
      readAt = now();
      entries = r.report?.entries ?? [];
      x.header.live?.set(readAt);
    }
    paintAll();
  };
  const loadIncidents = async () => {
    const r = await fetchReport(
      "/data/incidents",
      "the incidents",
      abort.signal,
    );
    if (r.ok) incidents = r.report?.incidents ?? [];
    paintAll();
  };

  /** @type {Set<number>} */
  let wasRunning = new Set(runningJobs().map((j) => j.job));
  let queued = false;
  const onJobs = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      const now2 = new Set(runningJobs().map((j) => j.job));
      const ended = [...wasRunning].some((id) => !now2.has(id));
      wasRunning = now2;
      paintRunning();
      paintKpis();
      if (ended) void loadHistory();
    });
  };
  const offJobs = onAct("jobs", onJobs);
  const offLog = onAct("log", (/** @type {{job: number}} */ l) => {
    blocks.get(l.job)?.update();
  });
  const offCatalog = onAct("catalog", () => {
    paintAttention();
    paintFeed();
  });
  const unsub = subscribe(() => {
    const before = rows.length;
    rebuild();
    if (rows.length !== before) {
      paintHeat();
      paintFeed();
    }
  });
  void catalogReady().then(() => paintAttention());
  const tick = setInterval(() => {
    for (const b of blocks.values()) b.update();
  }, 1000);
  const reread = setInterval(() => void loadHistory(), 60_000);
  const heatQuery = window.matchMedia("(max-width: 700px)");
  heatQuery.addEventListener("change", paintHeat);

  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (document.querySelector("dialog[open]")) return;
    const a = document.activeElement;
    if (/INPUT|TEXTAREA|SELECT/.test(a?.tagName ?? "")) return;
    if (e.key === "/") {
      e.preventDefault();
      /** @type {HTMLElement | null} */ (
        t.wrap.querySelector("[data-kp-datatable-search]")
      )?.focus();
    } else if (e.key === "f" || e.key === "F") {
      const at = columnOf("Outcome");
      const on = dt?.view().filters[at];
      dt?.filter(
        at,
        Array.isArray(on) && on.includes("Failed") ? null : ["Failed"],
      );
    } else if (e.key === "j" || e.key === "k") {
      const list = /** @type {HTMLElement[]} */ ([
        ...t.tbody.querySelectorAll(
          "tr[data-kp-row-key] .kp-datatable__expand",
        ),
      ]);
      const i = list.indexOf(/** @type {HTMLElement} */ (a));
      const n =
        list[
          Math.max(0, Math.min(list.length - 1, i + (e.key === "j" ? 1 : -1)))
        ];
      n?.focus();
      n?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Escape" && narrowed()) {
      narrow.el.dispatchEvent(new CustomEvent("kp-toolbar-clear"));
    }
  };
  document.addEventListener("keydown", onKey);

  paintAll();
  void loadHistory();
  void loadIncidents();
  return () => {
    abort.abort();
    offJobs();
    offLog();
    offCatalog();
    unsub();
    clearInterval(tick);
    clearInterval(reread);
    heatQuery.removeEventListener("change", paintHeat);
    for (const off of unbinds) off();
    for (const off of detachKp) off();
    document.removeEventListener("keydown", onKey);
  };
}

/**
 * A host's reason as a sentence: ends with a full stop.
 * @param {string} s
 */
const sentence = (s) => (/[.!?]$/.test(s.trim()) ? s.trim() : `${s.trim()}.`);
