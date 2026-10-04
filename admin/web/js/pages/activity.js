// redesign-activity (3.71.0, the Activity demo Kenny approved on
// 2026-10-03; FLOWS.md §1 "Activity: Running now, History (with who),
// Planned, Host log"): everything the host and this dashboard did, in one
// place. Three views under one header, each its own address:
//
//   /activity                 Now and history: the KPI strip, the open
//                             incidents, Running now (each job live, its log
//                             scrolling inside its own box), the 14-day
//                             timeline and the History feed grouped by day,
//                             who started each operation on every row; every
//                             job of the dashboard folded at the end
//   /activity?view=planned    the Schedules page, hosted as a view
//   /activity?view=host-log   the host-log explorer (log.js)
//
// The old addresses land here (router.js): /jobs → ?view=running (this
// view, scrolled to Running now; `?job=N` opens that job), /log →
// ?view=host-log, /schedules → ?view=planned, /timeline → ?view=timeline
// (this view, scrolled to the timeline).

import { act, actionLabel, catalogReady, onAct } from "../act.js";
import { openAction } from "../actiondialog.js";
import { fetchReport, h } from "../dom.js";
import { declare, drivable, viaForm } from "../drivable.js";
import {
  SHOW,
  WINDOWS,
  byDay,
  clock,
  dayLabel,
  dayStart,
  dayWord,
  feedRows,
  filterFromParams,
  filterToParams,
  incidentFor,
  kpis,
  openFailures,
  rowMatches,
  showCounts,
  stepSegments,
  windowDays,
} from "../activityview.js";
import { formatDay, humanDuration } from "../format.js";
import { openIncident } from "../incident.js";
import {
  elapsedS,
  finished,
  originText,
  remaining,
  stepText,
} from "../jobs.js";
import { logLine } from "../jobs.js";
import { current, subscribe } from "../store.js";
import {
  countOf,
  attentionBand,
  emptyState,
  ensureStyle,
  keyRow,
  kpiStrip,
  pageHeader,
  section,
  segSwitch,
  skeletonBlock,
  skeletonLines,
  toolbar,
} from "../ui.js";
import { openJobDialog } from "../jobpanel.js";
import { setParams } from "../urlstate.js";
import { tabRow } from "../dom.js";
import { mountJobsTable } from "./jobs.js";
import { mountHostLog } from "./log.js";
import { mount as mountSchedules } from "./schedules.js";

const SVG = "http://www.w3.org/2000/svg";
/** History rows drawn per page (the demo drew about forty). */
const PAGE = 50;

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
const SHOW_CHIP = declare({
  id: "activity-show",
  page: "activity",
  opens: "view",
  row: "all|failed|nightly|claude",
  what: "show All of History, or only Failed, Nightly or By Claude",
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
  what: "open a timeline mark's operation in History",
});
const SEARCH = declare({
  id: "activity-search",
  page: "activity",
  opens: "view",
  what: "History's search box",
});
const EVERY_DAY = declare({
  id: "activity-show-every-day",
  page: "activity",
  opens: "view",
  what: "drop the days picked on the timeline",
  shows: "while days picked on the timeline narrow History",
});
const CLEAR = declare({
  id: "activity-clear-filters",
  page: "activity",
  opens: "view",
  what: "clear every History filter",
  shows: "when the filters match no row",
});
const MORE = declare({
  id: "activity-show-more",
  page: "activity",
  opens: "view",
  what: "draw the next 50 older History rows",
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
    row: "info|warn|error",
    what: "show lines of at least this level",
  }),
  reset: declare({
    id: "host-log-reset",
    page: "log",
    opens: "view",
    what: "show every source and level again",
    shows: "while a source or level filter narrows the lines",
    reach: [{ do: "click", control: "host-log-level", row: "error" }],
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
  /** @type {Set<string>} */
  const open = new Set();
  /** How many History rows are drawn; "Show more" adds a page. */
  let limit = PAGE;
  let firstPaint = true;
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
  const allJobs = h(
    "a",
    {
      href: "#jobs",
      class: "link",
      title: "Every job this dashboard ran, newest first",
    },
    "All jobs (the last 200)",
  );
  drivable(allJobs, ALL_JOBS);
  running.body.append(
    runList,
    h("div", { class: "ac-foot" }, runFoot, allJobs),
  );

  // ── Timeline ──────────────────────────────────────────────────────────
  const tlBox = h("div", {
    class: "timeline ac-tl",
    tabindex: "-1",
  });
  tlBox.append(skeletonBlock("94px", "Reading the history"));
  const timeline = section({
    title: "Timeline",
    desc: "Each operation placed in time, so a gap in the nightly backups or a cluster of failures stands out.",
    id: "timeline",
  });
  timeline.body.append(
    tlBox,
    h(
      "div",
      { class: "ac-foot" },
      h(
        "ul",
        { class: "timeline-legend" },
        ...[
          ["ok", "succeeded"],
          ["bad", "failed"],
          ["warn", "running"],
          ["info", "nightly round"],
        ].map(([t, l]) =>
          h("li", null, h("span", { class: `swatch ${t}` }), l),
        ),
      ),
      h(
        "span",
        { class: "ac-hint" },
        "Click a mark to open it below · drag across days to show only those · click a day to jump to it",
      ),
    ),
  );

  // ── History ───────────────────────────────────────────────────────────
  const count = h("span", { class: "ac-count", role: "status" });
  const rangeChip = h("span", { class: "ac-range", hidden: "" });
  // The demo's one-of Show switch (senior review, finding 7): All and
  // Failed with their exact counts, Nightly, By Claude.
  const chips = segSwitch({
    label: "Show",
    items: [
      { value: "all", label: "All", hint: "Every operation" },
      ...SHOW.map((x) => ({ value: x.value, label: x.label, hint: x.hint })),
    ],
    value: [...filter.show][0] ?? "all",
    onChange: (v) => {
      filter = { ...filter, show: new Set(v === "all" ? [] : [v]) };
      changed();
    },
    mark: (b, v) => void drivable(b, SHOW_CHIP, v),
  });
  /** Pick one of the switch's values as a click would. @param {string} v */
  const pickShow = (v) =>
    /** @type {HTMLElement | null} */ (
      chips.el.querySelector(`button[data-v="${v}"]`)
    )?.click();
  const tb = toolbar({
    search: {
      placeholder: "Search: stack, action, error",
      label: "Search history",
      value: filter.q,
      onInput: (q) => {
        filter = { ...filter, q };
        changed();
      },
    },
    groups: [chips.el],
    state: [count, rangeChip],
  });
  if (tb.search) drivable(tb.search, SEARCH);
  const feed = h("div", { class: "ac-feed", id: "history-feed" });
  const historyCard = section({
    title: "History",
    desc: "Every operation, newest first, grouped by day; open a row for its steps, its error and what to do.",
    id: "history",
  });
  historyCard.body.append(tb.el, feed);

  // ── every job (folded) ────────────────────────────────────────────────
  const jobsCard = section({
    title: "Every job",
    desc: "The last 200 jobs this dashboard sent to the host, whatever came of them; click one to follow it.",
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
    act.jobs
      .filter((j) => !finished(j.state))
      .sort((a, b) => a.queued_at - b.queued_at);

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
  /** @type {Map<number, {el: HTMLElement, update: () => void}>} */
  const blocks = new Map();
  /** @param {import("../jobs.js").Job} j0 */
  const jobBlock = (j0) => {
    const id = j0.job;
    const title = h("div", { class: "ac-job__title" });
    const facts = h("div", { class: "ac-job__facts" });
    const steps = h("div", {
      class: "ac-job__steps",
      role: "list",
      "aria-label": "Steps",
    });
    const log = h("pre", {
      class: "ac-job__log mono",
      "aria-live": "polite",
      "aria-label": "The newest lines of this job",
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
      h("div", { class: "ac-job__acts" }, openLog),
      steps,
      log,
    );
    let shownLines = 0;
    const update = () => {
      const j = act.jobs.find((x) => x.job === id);
      if (!j) return;
      const t = now();
      title.replaceChildren(
        `${actionLabel(j.action)} `,
        j.stack === "_host" ? "the host" : stackLink(j.stack),
        h("span", { class: "ac-muted" }, ` · job ${j.job}`),
      );
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
          ? [
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
      else
        steps.replaceChildren(
          h(
            "div",
            {
              class: "ac-step ac-step--whole",
              role: "listitem",
              "data-s": "run",
            },
            h("i"),
            h("span", null, p?.step ?? stepText(j)),
          ),
        );
      const ls = act.logs.get(id) ?? [];
      if (ls.length !== shownLines) {
        const near = log.scrollHeight - log.scrollTop - log.clientHeight < 24;
        log.textContent = ls
          .slice(-200)
          .map((l) => {
            const v = logLine(l);
            return `${v.time}  ${v.msg}`;
          })
          .join("\n");
        log.hidden = ls.length === 0;
        shownLines = ls.length;
        if (near || shownLines === ls.length) log.scrollTop = log.scrollHeight;
      }
    };
    update();
    return { el, update };
  };

  const paintRunning = () => {
    const run = runningJobs();
    const ids = new Set(run.map((j) => j.job));
    for (const [id, b] of blocks)
      if (!ids.has(id)) {
        b.el.remove();
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
      if (!runList.querySelector(".nx-empty"))
        runList.replaceChildren(
          act.jobsRead
            ? emptyState({
                title: "Nothing is running",
                text: "A job you start from any stack's page shows here live, step by step; the running pill in the bar follows it too.",
              })
            : skeletonLines(3, "Reading the jobs"),
        );
    } else runList.querySelector(".nx-empty, .nx-skeleton")?.remove();
    const queued = run.filter((j) => j.state === "queued").length;
    runFoot.textContent = `${run.length - queued} running · ${queued} queued`;
  };

  // ── timeline ──────────────────────────────────────────────────────────
  /** @type {{a: number, b: number, target: Element} | null} */
  let drag = null;
  /** @type {(px: number) => number} */
  let inv = () => 0;
  /**
   * A timeline mark Live view can click: an SVG element has no `click()`,
   * so the mark gets one that does what a person's click does.
   * @param {Element} g
   * @param {string} key
   */
  const clickable = (g, key) => {
    /** @type {any} */ (g).click = () => openRow(key);
    return g;
  };
  const paintTimeline = () => {
    if (!historyRead) return;
    const W = Math.max(240, Math.round(tlBox.clientWidth || 1200));
    const L = W < 600 ? 72 : 104;
    const lanes = /** @type {const} */ ([
      ["Operations", 30],
      ["Nightly", 22],
      ["Failures", 22],
    ]);
    const H = 4 + lanes.reduce((a, l) => a + l[1] + 6, 0) + 22;
    const T0 = t0();
    const T1 = now() + 3600;
    const xOf = (/** @type {number} */ t) =>
      L + ((t - T0) / (T1 - T0)) * (W - L - 8);
    inv = (px) => T0 + ((px - L) / (W - L - 8)) * (T1 - T0);
    const svg = svgEl("svg", {
      viewBox: `0 0 ${W} ${H}`,
      width: W,
      height: H,
      role: "img",
      "aria-label": `Timeline of the last ${days} days`,
    });
    const today = dayStart(now());
    svg.append(
      svgEl("rect", {
        class: "ac-tl__today",
        x: xOf(today),
        y: 0,
        width: Math.max(0, W - 8 - xOf(today)),
        height: H - 22,
      }),
    );
    /** @type {Record<string, [number, number]>} */
    const ly = {};
    let y = 4;
    for (const [name, hh] of lanes) {
      svg.append(
        svgEl("rect", {
          class: "lane",
          x: L,
          y,
          width: W - L - 8,
          height: hh,
          rx: 4,
        }),
        svgEl("text", { class: "lane-label", x: 0, y: y + hh / 2 + 4 }, name),
      );
      ly[name] = [y, hh];
      y += hh + 6;
    }
    for (let d = 0; d <= days; d++) {
      const t = T0 + d * 86400;
      const xx = xOf(t);
      svg.append(
        svgEl("line", { class: "tick", x1: xx, x2: xx, y1: 0, y2: H - 22 }),
      );
      const every = W > 900 ? 1 : W > 600 ? 2 : days > 7 ? 4 : 2;
      if (
        d < days &&
        (d % every === 0 || d === days - 1) &&
        !(d !== days - 1 && days - 1 - d < every)
      )
        svg.append(
          svgEl(
            "text",
            { class: "tick-label", x: xx + 4, y: H - 6 },
            d === days - 1 ? "today" : formatDay(t),
          ),
        );
    }
    const FILL = {
      ok: "var(--success-foreground)",
      warn: "var(--warning-foreground)",
      bad: "var(--destructive)",
      info: "var(--chart-1, var(--primary))",
    };
    for (const r of [...rows].reverse()) {
      if (r.start < T0) continue;
      const [yy, hh] = r.entry.kind === "phase" ? ly.Nightly : ly.Operations;
      const end = r.took == null ? now() : r.start + r.took;
      const w = Math.max(3, xOf(end) - xOf(r.start));
      const tone = r.entry.kind === "phase" ? "info" : r.tone;
      const g = svgEl("g", {
        class: "mark",
        tabindex: 0,
        role: "button",
        "data-kind": tone,
        "data-key": r.key,
        "aria-label": `${r.what}, ${dayLabel(dayStart(r.start), now())} ${clock(r.start)}, ${r.state}. Enter opens it below.`,
      });
      g.append(
        svgEl("title", {}, `${r.what} · ${clock(r.start)} · ${r.state}`),
        svgEl("rect", {
          x: xOf(r.start),
          y: yy + 4,
          width: w,
          height: hh - 8,
          rx: 2,
          style: `fill: ${FILL[tone]}`,
        }),
      );
      drivable(
        /** @type {HTMLElement} */ (/** @type {unknown} */ (g)),
        MARK,
        r.key,
      );
      svg.append(clickable(g, r.key));
      if (r.state === "failed") {
        const [fy, fh] = ly.Failures;
        const fx = xOf(r.start);
        const fg = svgEl("g", {
          class: "mark",
          tabindex: 0,
          role: "button",
          "data-kind": "bad",
          "data-key": r.key,
          "aria-label": `${r.what} failed. Enter opens it below.`,
        });
        fg.append(
          svgEl("path", {
            d: `M${fx},${fy + 3} l6,${fh / 2 - 3} l-6,${fh / 2 - 3} l-6,-${fh / 2 - 3}z`,
            style: `fill: ${FILL.bad}`,
          }),
        );
        drivable(
          /** @type {HTMLElement} */ (/** @type {unknown} */ (fg)),
          MARK,
          r.key,
        );
        svg.append(clickable(fg, r.key));
      }
    }
    const [ry, rh] = ly.Operations;
    for (const j of runningJobs())
      if (j.started_at != null)
        svg.append(
          svgEl("rect", {
            class: "ac-tl__run",
            x: xOf(j.started_at),
            y: ry + 4,
            width: 6,
            height: rh - 8,
            rx: 2,
            style: `fill: ${FILL.warn}`,
          }),
        );
    const brush = (/** @type {number} */ a, /** @type {number} */ b) =>
      svgEl("rect", {
        class: "ac-tl__brush",
        x: Math.min(a, b),
        y: 0,
        width: Math.max(2, Math.abs(b - a)),
        height: H - 22,
      });
    if (filter.range)
      svg.append(brush(xOf(filter.range.from), xOf(filter.range.to)));
    if (drag) svg.append(brush(drag.a, drag.b));
    tlBox.replaceChildren(svg);
  };
  const px = (/** @type {PointerEvent} */ e) =>
    e.clientX - tlBox.getBoundingClientRect().left;
  tlBox.addEventListener("pointerdown", (e) => {
    if (!historyRead || e.button !== 0) return;
    drag = { a: px(e), b: px(e), target: /** @type {Element} */ (e.target) };
    tlBox.setPointerCapture(e.pointerId);
  });
  tlBox.addEventListener("pointermove", (e) => {
    if (!drag) return;
    drag.b = px(e);
    paintTimeline();
  });
  // A touch scroll that starts on the timeline ends in pointercancel, not
  // pointerup: drop the drag, or the brush stays stuck (finding 13).
  tlBox.addEventListener("pointercancel", () => {
    if (!drag) return;
    drag = null;
    paintTimeline();
  });
  tlBox.addEventListener("pointerup", () => {
    const d = drag;
    drag = null;
    if (!d) return;
    if (Math.abs(d.b - d.a) > 6) {
      filter = {
        ...filter,
        range: { from: inv(Math.min(d.a, d.b)), to: inv(Math.max(d.a, d.b)) },
      };
      changed();
      return;
    }
    const mk = d.target.closest?.(".mark");
    const key = mk?.getAttribute("data-key");
    if (key) openRow(key);
    else {
      const day = dayStart(inv(d.a));
      // A day further down than the rows drawn so far: draw up to it.
      const last = rows.filter((r) => rowMatches(r, filter));
      const idx = last.findIndex((r) => dayStart(r.start) <= day);
      if (idx >= limit) {
        limit = idx + PAGE;
        paintFeed();
      }
      const el = document.getElementById(`day-${day}`);
      el?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
    paintTimeline();
  });
  tlBox.addEventListener("keydown", (e) => {
    const mk = /** @type {Element} */ (e.target).closest?.(".mark");
    if (mk && e.key === "Enter") {
      const key = mk.getAttribute("data-key");
      if (key) openRow(key);
    }
  });

  // ── History feed ──────────────────────────────────────────────────────
  /** @param {string} key */
  const openRow = (key) => {
    open.add(key);
    if (filter.show.size || filter.q) {
      filter = { ...filter, show: new Set(), q: "" };
      if (tb.search) tb.search.value = "";
      pickShow("all");
    }
    const idx = rows.findIndex((r) => r.key === key);
    if (idx >= limit) limit = idx + PAGE;
    changed();
    const el = feed.querySelector(`[data-key="${CSS.escape(key)}"]`);
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
    /** @type {HTMLElement | null} */ (el)?.focus({ preventScroll: true });
  };

  /** @param {import("../activityview.js").FeedRow} r */
  const detail = (r) => {
    const e = r.entry;
    /** @type {[string, number | null, boolean][]} */
    const steps =
      e.kind === "op" && e.steps.length
        ? e.steps.map((s) => [
            s.step,
            s.end > 0 && s.end >= s.start ? s.end - s.start : null,
            !(r.state === "failed" && s === e.steps[e.steps.length - 1]),
          ])
        : [[r.what, r.took, r.state !== "failed"]];
    const total = steps.reduce((a, s) => a + (s[1] ?? 0), 0) || 1;
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
    return h(
      "div",
      { class: "ac-detail" },
      h(
        "div",
        { class: "ac-gantt", "aria-label": "Steps" },
        ...steps.map(([n, d, ok]) =>
          h(
            "div",
            null,
            h("span", null, n),
            h(
              "span",
              null,
              h("i", {
                class: ok ? "" : "bad",
                style: `inline-size:${ok ? Math.max(2, ((d ?? 0) / total) * 100) : 100}%`,
              }),
            ),
            h("em", null, ok ? (d == null ? "—" : humanDuration(d)) : "failed"),
          ),
        ),
      ),
      r.error ? h("div", { class: "ac-err mono" }, r.error) : "",
      h("div", { class: "ac-acts" }, ...acts),
    );
  };

  const paintFeed = () => {
    if (!historyRead) {
      feed.replaceChildren(skeletonLines(8, "Reading the history"));
      count.textContent = "";
      return;
    }
    if (historyError) {
      const again = h(
        "button",
        { type: "button", class: "kp-button kp-button--sm" },
        "Try again",
      );
      drivable(again, RETRY);
      again.addEventListener("click", () => void loadHistory());
      feed.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          h(
            "span",
            { class: "kp-alert__body" },
            `The history could not be read: ${historyError}`,
          ),
          again,
        ),
      );
      return;
    }
    // The row that had focus gets it back after the repaint (finding 4):
    // the minute's reread and a job's end no longer drop it on <body>.
    const had = /** @type {HTMLElement | null} */ (
      document.activeElement instanceof Element &&
      feed.contains(document.activeElement)
        ? document.activeElement.closest("[data-key]")
        : null
    );
    const focusKey = had?.dataset.key ?? null;
    const refocus = () => {
      if (focusKey == null) return;
      /** @type {HTMLElement | null} */ (
        feed.querySelector(`.ac-row[data-key="${CSS.escape(focusKey)}"]`)
      )?.focus({ preventScroll: true });
    };
    const shown = rows.filter((r) => rowMatches(r, filter));
    // redesign-final (exact counts): the rows drawn, of all there are —
    // "Show more" draws the next page.
    count.textContent = `${Math.min(shown.length, limit)} of ${rows.length} shown`;
    countOf(count, feed, ".ac-row");
    chips.counts(showCounts(rows, filter));
    rangeChip.hidden = !filter.range;
    if (filter.range) {
      const clear = h(
        "button",
        { type: "button", class: "link-button" },
        "Show every day",
      );
      drivable(clear, EVERY_DAY);
      clear.addEventListener("click", () => {
        filter = { ...filter, range: null };
        changed();
      });
      rangeChip.replaceChildren(
        `From the timeline: ${dayWord(filter.range.from, now())} ${clock(filter.range.from)} → ${dayWord(filter.range.to, now())} ${clock(filter.range.to)} · `,
        clear,
      );
    }
    if (!shown.length) {
      const clear = h(
        "button",
        { type: "button", class: "kp-button kp-button--sm" },
        "Clear the filters",
      );
      drivable(clear, CLEAR);
      clear.addEventListener("click", () => {
        filter = { ...filter, q: "", range: null };
        if (tb.search) tb.search.value = "";
        pickShow("all");
        changed();
      });
      feed.replaceChildren(
        emptyState(
          rows.length
            ? {
                title: "Nothing matches",
                text: "No operation of this window passes the filter.",
                action: clear,
              }
            : {
                title: `Nothing ran in the last ${days === 1 ? "day" : `${days} days`}`,
                text: "Every operation the host runs, asked for or on its own, lands here with who started it.",
              },
        ),
      );
      return;
    }
    const out = [];
    if (stale) {
      const again = h(
        "button",
        { type: "button", class: "link-button" },
        "Read it again",
      );
      drivable(again, RETRY);
      again.addEventListener("click", () => void loadHistory());
      out.push(
        h(
          "p",
          { class: "ac-stale", role: "status" },
          `Not refreshed: ${stale}. These rows are from ${clock(readAt)}. `,
          again,
        ),
      );
    }
    for (const d of byDay(shown.slice(0, limit))) {
      out.push(
        h(
          "div",
          { class: "ac-dayhead", id: `day-${d.day}` },
          h("b", null, dayLabel(d.day, now())),
          h(
            "span",
            null,
            `${d.ops} ${d.ops === 1 ? "operation" : "operations"}`,
            d.failed
              ? h("span", { class: "ac-bad" }, ` · ${d.failed} failed`)
              : " · all ok",
          ),
        ),
      );
      for (const r of d.rows) {
        const isOpen = open.has(r.key);
        const row = drivable(
          h(
            "div",
            {
              class: `ac-row${r.entry.kind === "phase" ? " ac-row--phase" : ""}`,
              role: "button",
              tabindex: "0",
              "aria-expanded": String(isOpen),
              "data-key": r.key,
              "data-tone": r.state === "failed" ? "bad" : "",
              title: "Open for its steps, its error and what to do (Enter)",
            },
            h("span", { class: "ac-t" }, clock(r.start)),
            h(
              "span",
              { class: "ac-st" },
              h("span", {
                class: `ac-dot ac-dot--${r.tone}`,
                "aria-hidden": "true",
              }),
              r.state,
            ),
            h(
              "span",
              { class: "ac-w" },
              ...(r.stack && r.entry.kind === "op"
                ? [`${r.verb} `, stackLink(r.stack)]
                : [r.what]),
            ),
            h(
              "span",
              { class: "ac-by" },
              h(
                "span",
                { class: `ac-by-chip ac-by-chip--${r.actor.kind}` },
                r.actor.text,
              ),
            ),
            h(
              "span",
              { class: "ac-took" },
              r.took == null ? "—" : humanDuration(r.took),
            ),
            h("span", { class: "ac-chev", "aria-hidden": "true" }, "›"),
          ),
          ROW,
          r.key,
        );
        const toggle = () => {
          if (open.has(r.key)) open.delete(r.key);
          else open.add(r.key);
          paintFeed();
          /** @type {HTMLElement | null} */ (
            feed.querySelector(`[data-key="${CSS.escape(r.key)}"]`)
          )?.focus({ preventScroll: true });
        };
        row.addEventListener("click", (ev) => {
          if (!(/** @type {Element} */ (ev.target).closest("a, button")))
            toggle();
        });
        row.addEventListener("keydown", (ev) => {
          if (ev.target !== row) return;
          if (ev.key === "Enter" || ev.key === " ") {
            ev.preventDefault();
            toggle();
          }
        });
        out.push(row);
        if (isOpen) out.push(detail(r));
      }
    }
    if (shown.length > limit) {
      const more = h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          title: "Draw the next older operations of this window",
        },
        `Show ${Math.min(PAGE, shown.length - limit)} more`,
      );
      drivable(more, MORE);
      more.addEventListener("click", () => {
        limit += PAGE;
        paintFeed();
      });
      out.push(
        h(
          "div",
          { class: "ac-more" },
          h("span", null, `Showing 1–${limit} of ${shown.length}`),
          more,
        ),
      );
    }
    feed.replaceChildren(...out);
    refocus();
  };

  const changed = () => {
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, filterToParams(filter)),
    );
    // The days picked on the timeline show in the toolbar's state zone
    // (rangeChip, with Show every day), so the active-filter row stays
    // absent: one place per filter, never two.
    paintFeed();
    paintTimeline();
  };

  const paintAll = () => {
    rebuild();
    paintKpis();
    paintAttention();
    paintRunning();
    paintTimeline();
    paintFeed();
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
      else historyError = r.error.why;
    } else {
      historyError = null;
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
    if (rows.length !== before) paintFeed();
  });
  void catalogReady().then(() => paintAttention());
  const tick = setInterval(() => {
    for (const b of blocks.values()) b.update();
  }, 1000);
  const reread = setInterval(() => void loadHistory(), 60_000);
  const ro = new ResizeObserver(() => paintTimeline());
  ro.observe(tlBox);

  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (document.querySelector("dialog[open]")) return;
    const a = document.activeElement;
    if (/INPUT|TEXTAREA|SELECT/.test(a?.tagName ?? "")) return;
    if (e.key === "/") {
      e.preventDefault();
      tb.search?.focus();
    } else if (e.key === "f" || e.key === "F") {
      pickShow(filter.show.has("failed") ? "all" : "failed");
    } else if (e.key === "j" || e.key === "k") {
      const list = /** @type {HTMLElement[]} */ ([
        ...feed.querySelectorAll(".ac-row"),
      ]);
      const i = list.indexOf(/** @type {HTMLElement} */ (a));
      const n =
        list[
          Math.max(0, Math.min(list.length - 1, i + (e.key === "j" ? 1 : -1)))
        ];
      n?.focus();
      n?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Escape" && (filter.show.size || filter.range)) {
      filter = { ...filter, range: null };
      pickShow("all");
      changed();
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
    ro.disconnect();
    document.removeEventListener("keydown", onKey);
  };
}

/**
 * @param {string} tag
 * @param {Record<string, string | number>} attrs
 * @param {...(Node | string)} children
 */
function svgEl(tag, attrs, ...children) {
  const e = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  e.append(...children);
  return e;
}

/**
 * A host's reason as a sentence: ends with a full stop.
 * @param {string} s
 */
const sentence = (s) => (/[.!?]$/.test(s.trim()) ? s.trim() : `${s.trim()}.`);
