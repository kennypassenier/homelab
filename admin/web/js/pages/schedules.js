// Schedules (feat-stacks-8, arch-schedule; rebuilt for kp-themes 11.0.0 as the
// demo Kenny approved on 2026-10-19, feat-kp11demo-9): what the dashboard runs
// on its own and when. The next seven days as kp's week, the next run counting
// down with the three after it, a list with the schedule's form beside it
// (kp's master-detail: Kenny chose "beside the list", not a drawer), every
// schedule in a datatable with its last fourteen slots, the busy hours and the
// stacks without a backup schedule, and the history of what the schedules ran.
// Every time is the host's wall clock. A page module of its own, so Activity
// can host it as a tab: `mount(root)` draws into any container and returns its
// cleanup.

import {
  act,
  actionLabel,
  catalogReady,
  loadSchedules,
  onAct,
  send,
} from "../act.js";
import { schedulableActions } from "../actionforms.js";
import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h, patchKeyed, tableBlock } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import { openJobDialog } from "../jobpanel.js";
import {
  COVERAGE,
  LEGEND,
  TEMPLATES,
  WEEKDAYS,
  busyModel,
  cadenceWords,
  clashWhen,
  countText,
  coverageModel,
  dateTimeText,
  draftFor,
  historyModel,
  HISTORY_LANES,
  movedTime,
  nextUp,
  runsWithClash,
  scheduleTitle,
  shortWhen,
  slotPeriods,
  targetText,
  weekEvents,
  zoneParts,
} from "../schedules.js";
import { buildScheduleForm } from "../schedulesform.js";
import { current, subscribe } from "../store.js";
import {
  TOAST_MS,
  keyRow,
  pageHeader,
  section,
  toast,
  toastRegion,
} from "../ui.js";
import { ACTION_TILE, attachActionTiles } from "/static/kp/js/action-tiles.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import { openConfirmation } from "/static/kp/js/components.js";
import {
  COUNTDOWN_END_EVENT,
  attachCountdowns,
  countdownOf,
} from "/static/kp/js/countdown.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import {
  GANTT_OPEN_EVENT,
  attachGantts,
  setGanttKinds,
  setGanttLegend,
  setGanttPoints,
  setGanttRuns,
  setGanttState,
} from "/static/kp/js/gantt.js";
import {
  attachHeats,
  attachMatrices,
  setHeat,
  setMatrix,
} from "/static/kp/js/heat.js";
import {
  PANE_RETRY_EVENT,
  PANE_SELECT_EVENT,
  attachMasterDetails,
  masterDetail,
  setPaneState,
} from "/static/kp/js/master-detail.js";
import { attachRuns, setRuns, setRunsState } from "/static/kp/js/runs.js";
import {
  SELECTION_ACTION_EVENT,
  attachSelectionBars,
  setSelectionBusy,
  setSelectionResult,
} from "/static/kp/js/selection-bar.js";
import { undoToast } from "/static/kp/js/undo-toast.js";
import { attachUptimes, setUptime } from "/static/kp/js/uptime.js";
import {
  WEEK_OPEN_EVENT,
  attachWeeks,
  setWeekEvents,
  setWeekLegend,
  setWeekState,
} from "/static/kp/js/week.js";

/**
 * @typedef {import("../schedules.js").ScheduleView} ScheduleView
 * @typedef {import("../schedules.js").Schedule} Schedule
 * @typedef {import("../schedules.js").Template} Template
 */

/** How long a change can be undone: as long as its toast shows. */
const UNDO_MS = TOAST_MS;

// fix-239, invariant 39: Live view reaches every control on this page.
const NEW_SCHEDULE = declare({
  id: "new-schedule",
  page: "schedules",
  opens: "view",
  what: "open the form of a new schedule beside the list",
});
const SCHEDULE_TEMPLATE = declare({
  id: "schedule-template",
  page: "schedules",
  opens: "view",
  row: "<template id>",
  what: "open the form of a new schedule filled in from a template (empty page only)",
  shows: "on an empty Schedules page",
});
const TOGGLE_SCHEDULE = declare({
  id: "toggle-schedule",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "turn one schedule on or off (Undo for 6 s)",
});
const EDIT_SCHEDULE = declare({
  id: "edit-schedule",
  page: "schedules",
  opens: "view",
  row: "<schedule id>",
  what: "open one schedule's form beside the list",
  // kp-themes 11: the row's menu is three buttons in the table; its old
  // name opens the form, the button that was its first entry.
  was: [{ id: "schedule-menu" }],
});
const RUN_SCHEDULE = declare({
  id: "run-schedule",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "start one schedule's action now, as a job; its times stay",
});
const DELETE_SCHEDULE = declare({
  id: "delete-schedule",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "delete one schedule (Undo for 6 s; jobs it ran stay on Activity)",
});
const RUN_NEXT = declare({
  id: "run-next-now",
  page: "schedules",
  opens: "run",
  what: "start the next scheduled action now, as a job; its times stay",
});
const SKIP_NEXT = declare({
  id: "skip-next-run",
  page: "schedules",
  opens: "run",
  what: "skip only the next run; the schedule stays on",
});
const UNDO_CHANGE = declare({
  id: "undo-schedule-change",
  page: "schedules",
  opens: "run",
  what: "undo the last switch or delete while its toast shows (6 s)",
  shows: "for 6 s after a switch or a delete",
  reach: [{ do: "click", control: "delete-schedule", row: "*" }],
});
const FIND_SCHEDULE = declare({
  id: "find-schedule",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "click a run on the calendar to find its schedule in the table",
});
const SORT_SCHEDULES = declare({
  id: "sort-schedules",
  page: "schedules",
  opens: "run",
  row: "<column: on, what, when, next>",
  what: "sort the schedules by a column (again: the other way)",
});
const MOVE_CLASH = declare({
  id: "move-schedule-clash",
  page: "schedules",
  opens: "run",
  row: "<schedule id>",
  what: "move one of two schedules that start in the same minute 15 minutes",
  shows: "under the busy hours, when two schedules start together",
});

const ONLY_GAPS = declareField({
  id: "sched-only-gaps",
  page: "schedules",
  what: "show only the stacks without a backup schedule (coverage)",
});

/** The zone a host answers in before its list has come. */
const DEFAULT_ZONE = "Europe/Brussels";
/** The pane list's key for the form of a schedule that does not exist yet. */
const NEW_KEY = "+new";

/** The host's nightly round hour, read once per page visit. */
async function readNightly() {
  const r = await fetchJson("/data/host-settings", "the host settings");
  if (!r.ok) return undefined;
  /** @type {{key: string, value: unknown}[]} */
  const fields = r.body?.page?.fields ?? [];
  const v = fields.find((f) => f.key === "backup_hour")?.value;
  return typeof v === "number" && v >= 0 && v <= 23 ? v : null;
}

const now = () => Date.now() / 1000;
const hostTarget = () => act.catalog?.host_target ?? "_host";
/** @param {Schedule} s */
const titleOf = (s) =>
  scheduleTitle(actionLabel(s.action), s.stack, hostTarget());
/** @param {number} t @param {string} z "Tue 20/10/2026 03:30" */
const withWeekday = (t, z) =>
  `${WEEKDAYS[zoneParts(t, z).wd]} ${dateTimeText(t, z)}`;
const pad = (/** @type {number} */ n) => String(n).padStart(2, "0");

/**
 * One of the page's cards (ui.js `section`), its foot one line of text.
 * @param {{title?: string, desc?: string, body: import("../dom.js").Child,
 *   foot?: string, id?: string, label?: string}} spec
 */
const card = (spec) =>
  section({
    ...spec,
    plain: true,
    foot: spec.foot ? [spec.foot] : undefined,
  });

/**
 * @param {HTMLElement} root
 * @param {{level?: "h1" | "h2"}} [opts] `level`: "h2" when another page
 *   (Activity's "Planned" view) carries the page's h1
 * @returns {() => void}
 */
export function mount(root, opts = {}) {
  // `nx-ops`: the shared blocks in the ops-kit look the Schedules demo uses.
  root.classList.add("sch-root", "nx-ops");
  /** @type {number | null | undefined} undefined: not read (yet) */
  let nightly;
  let alive = true;
  /** Deletes waiting out their Undo: the rows are hidden meanwhile. */
  /** @type {Set<string>} */
  const removed = new Set();
  /** A toast on this page; its Undo is a control Live view can press. */
  /** @param {string} text */
  const say = (text) => toast(text, { host: root, replace: "all" });
  /** @param {ReturnType<typeof undoToast>} t */
  const driveUndo = (t) => {
    const b = [...t.toast.querySelectorAll("button")].find((x) =>
      /undo/i.test(x.textContent ?? ""),
    );
    if (b) drivable(b, UNDO_CHANGE);
  };

  const zone = () => act.schedules?.zone ?? DEFAULT_ZONE;
  const list = () =>
    (act.schedules?.schedules ?? []).filter((v) => !removed.has(v.schedule.id));
  /** @param {string} id */
  const viewOf = (id) => list().find((v) => v.schedule.id === id);
  const stackNames = () => {
    const fleet = current().fleet;
    return fleet ? fleet.stacks.map((x) => x.name) : null;
  };

  // ── the header ──
  const count = h("span", { id: "sched-count", class: "sch-num" });
  const zoneChip = h("span", { class: "sch-chip", id: "sched-zone" });
  const read = agoEl("read");
  const add = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "sched-new",
      title: "Pick an action, a target and when it runs (N)",
      onclick: () => void openForm(NEW_KEY),
    },
    // redesign-final-h5: no key badge on this one button; N is in its
    // description, as every other button's key is.
    "New schedule",
  );
  drivable(add, NEW_SCHEDULE);
  const header = pageHeader({
    level: opts.level,
    title: "Schedules",
    desc: "What the dashboard runs on its own, and when. A slot missed while the dashboard was down is skipped and you get a notice; it never runs late.",
    meta: [zoneChip, count, read],
    actions: [add],
  }).el;
  // redesign-final-h5: hosted in Activity's Planned view this is a section
  // of that page (an h2 at section size), never a second page title.
  if (opts.level === "h2") header.classList.add("sch-head--hosted");

  const paintZone = () => {
    const z = zone();
    let mine = "";
    try {
      mine = Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      mine = "";
    }
    zoneChip.replaceChildren(
      h("span", { class: "sch-dot sch-dot--info" }),
      mine === z
        ? `Host clock: ${z}, the same as yours`
        : `Host clock: ${z}; yours is ${mine || "unknown"}, times here are the host's`,
    );
  };

  // ── 1 · the week ──
  const weekEl = h("section", {
    class: "kp-week",
    "data-kp-week": "",
    "data-kp-week-nav": "",
    "data-kp-week-upcoming": "8",
    "data-kp-week-upcoming-label": "The next runs",
    "data-kp-week-upcoming-empty": "No run in the next 7 days.",
    "data-kp-time-zone": zone(),
    "aria-label": "Runs in the next 7 days",
  });
  const weekCard = card({
    title: "The next 7 days: every run on one calendar",
    desc: "Shows every run of the coming week as a pill at its time of day, with the host's own nightly round dashed for reference, and the very next run ringed.",
    body: h("div", { class: "sch-weekbox" }, weekEl),
    id: "sched-week",
  });

  // ── 2 · the next run ──
  const countdown = h("div", {
    class: "kp-countdown",
    "data-kp-countdown": "1:00:00",
    "data-kp-countdown-name": "Next run in",
    id: "sched-countdown",
  });
  const heroTitle = h("b", { id: "sched-hero-title" });
  const heroWhen = h("span", { id: "sched-hero-when" });
  const heroNote = h("p", {
    class: "sch-hero__note",
    id: "sched-hero-note",
    role: "status",
  });
  const runNow = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "sched-run-now",
      title: "Start this action now as a job; the schedule keeps its next slot",
    },
    "Run now",
  );
  drivable(runNow, RUN_NEXT);
  const skip = h(
    "button",
    {
      type: "button",
      class: "kp-button",
      id: "sched-skip",
      title: "Skip only this one run; the schedule stays on",
    },
    "Skip this run",
  );
  drivable(skip, SKIP_NEXT);
  const thenEl = h("div", {
    class: "kp-runs",
    "data-kp-runs": "",
    "data-kp-time-zone": zone(),
    id: "sched-then",
  });
  // The card's own state is the countdown and the schedule it names; the
  // three runs after it are the runs list's component, with its own busy
  // card and plates, so they stand beside that stage, never inside it.
  const hero = h(
    "section",
    {
      class: "kp-card sch-hero",
      "data-kp-state": "loading",
      id: "sched-hero",
      "aria-label": "The next run",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h(
        "div",
        { class: "kp-card__data" },
        h(
          "div",
          { class: "sch-hero__main" },
          countdown,
          h("p", { class: "sch-hero__what" }, heroTitle, heroWhen),
          heroNote,
          h("div", { class: "sch-hero__actions" }, runNow, skip),
        ),
      ),
    ),
  );
  const heroFrame = h(
    "div",
    { class: "kp-card sch-hero__grid" },
    hero,
    h("div", { class: "sch-hero__then" }, thenEl),
  );
  const heroCard = card({
    title: "The next run, counting down",
    desc: "Tells what starts next and how long that is, with the exact date beside the countdown and the three runs after it, flagging a run that starts in the same minute as another.",
    body: heroFrame,
  });

  // ── 3 · the form beside the list ──
  const paneList = h("ul", {
    class: "kp-pane__list",
    role: "listbox",
    "aria-label": "Schedules",
    id: "sched-pane-list",
  });
  const paneBody = h("div", { class: "kp-pane__body", id: "sched-pane-body" });
  const paneHeading = h("h3", { "data-kp-pane-heading": "", tabindex: "-1" });
  const panes = h(
    "div",
    {
      class: "kp-split kp-split--panes",
      "data-kp-master-detail": "",
      "data-kp-pane-widths": "22rem 1fr",
      "data-kp-pane-history": "off",
      id: "sched-panes",
    },
    h(
      "section",
      {
        class: "kp-split__pane",
        "data-kp-pane": "list",
        "aria-label": "Schedules",
      },
      paneList,
    ),
    h(
      "section",
      {
        class: "kp-split__pane",
        "data-kp-pane": "detail",
        "aria-label": "Schedule",
      },
      paneHeading,
      paneBody,
    ),
  );
  const editCard = card({
    title: "Change a schedule, or make a new one",
    desc: "Opens the schedule you pick from the list in a form beside it, and shows the next five runs while you type.",
    body: h("div", { class: "sch-surface" }, panes),
    id: "sched-edit",
  });

  // ── 4 · the table ──
  const tableId = "sched-table";
  const table = tableBlock({
    remember: "schedules",
    caption: "All schedules",
    captionHidden: true,
    search: "Search a stack or an action",
    state: "loading",
    nothing: "No schedules yet",
    pageSize: 10,
    pageSizes: "10,25,50",
    columns: [
      { label: "On", sort: "text", filter: "choice" },
      { label: "What", sort: "text", filter: "choice" },
      { label: "When", sort: "text" },
      { label: "Next run", sort: "number", cls: "kp-numeric" },
      { label: "Last 14", sort: null },
      { label: "Actions", sort: null },
    ],
  });
  table.wrap.id = tableId;
  const COLS = ["on", "what", "when", "next"];
  {
    const headRow = /** @type {HTMLTableRowElement} */ (
      table.wrap.querySelector("thead tr")
    );
    headRow.prepend(
      h(
        "th",
        { scope: "col", class: "select-col" },
        h("input", {
          class: "kp-field__check",
          type: "checkbox",
          "data-kp-select-all": "",
          "aria-label": "Select every schedule on this page",
        }),
      ),
    );
    const ths = [...headRow.querySelectorAll("th")];
    ths[ths.length - 1].replaceChildren(
      h("span", { class: "kp-sr-only" }, "Actions"),
    );
    // Live view sorts a column by pressing its header.
    ths
      .filter((th) => th.hasAttribute("data-kp-sort"))
      .forEach((th, i) => drivable(th, SORT_SCHEDULES, COLS[i]));
    // kp's "Search in" choice, as the demo has it.
    const bar = table.wrap.querySelector(".kp-datatable__bar");
    bar?.append(
      h("select", {
        class: "kp-field__input kp-datatable__select",
        "data-kp-datatable-scope": "",
        "aria-label": "Search in",
      }),
    );
  }
  const bulkOff = h(
    "button",
    {
      type: "button",
      class: "kp-button",
      "data-kp-selection-action": "off",
      title: "Turn the ticked schedules off; their slots are kept",
    },
    "Turn off",
  );
  const bulkOn = h(
    "button",
    {
      type: "button",
      class: "kp-button",
      "data-kp-selection-action": "on",
      title: "Turn the ticked schedules on",
    },
    "Turn on",
  );
  const selectionBar = h(
    "div",
    {
      class: "kp-selection",
      "data-kp-selection": "",
      "data-kp-selection-for": tableId,
      "data-kp-placement": "sticky-top",
      "aria-label": "Selected schedules",
    },
    h("div", { class: "kp-selection__actions" }, bulkOff, bulkOn),
  );
  const tiles = h("div", {
    class: "kp-actions sch-templates",
    role: "group",
    "aria-label": "Starting points",
  });
  {
    const none = table.wrap.querySelector("[data-kp-datatable-empty-none]");
    none?.append(
      h(
        "p",
        { class: "kp-empty__body" },
        "Nothing runs on its own apart from the host's nightly round. Start from one of these, or make your own with New schedule.",
      ),
      tiles,
    );
  }
  const tableCard = card({
    title: "All schedules: search, sort, bulk turn off, delete with undo",
    desc: "Lists every schedule with its next run and the last fourteen slots (ran, failed, missed), lets you turn several off at once, and gives a deleted schedule back for six seconds.",
    body: h("div", { class: "sch-tablebox" }, selectionBar, table.wrap),
    foot: "Times are the host's clock; a run that starts becomes a job on Activity.",
    id: "sched-all",
  });

  // ── 5 · the busy hours ──
  const heatEl = h("div", {
    class: "kp-heat sch-heat",
    "data-kp-heat": "",
    "aria-label": "Schedule starts by weekday and hour",
  });
  const clashList = h("ul", {
    class: "sch-clashes kp-formgrid",
    "aria-label": "Clashes",
  });
  const busyCard = card({
    title: "The busy hours: where two schedules start together",
    desc: "Maps the week by weekday and hour and marks the hours in which two schedules (or a schedule and the host's nightly round) start in the same minute, so you move one before they fight over the disk.",
    body: h(
      "div",
      { class: "sch-busybox" },
      h(
        "div",
        {
          class: "sch-heatwrap",
          tabindex: 0,
          role: "region",
          "aria-label": "Schedule starts by weekday and hour",
        },
        heatEl,
      ),
      clashList,
    ),
    id: "sched-busy",
  });

  // ── 6 · coverage ──
  const coverSummary = h("p", { class: "sch-summary", role: "status" });
  const matrixEl = h("div", {
    class: "kp-matrix sch-matrix",
    "data-kp-matrix": "",
    "data-kp-view": "grid",
    "aria-label": "Which stack has which schedule",
  });
  const onlyGaps = h("input", {
    class: "kp-field__check",
    type: "checkbox",
    id: ONLY_GAPS,
  });
  const coverCard = card({
    title: "Coverage: which stack has no backup schedule",
    desc: "Crosses every stack with every action and shows at a glance where a schedule exists and where none does, so a stack that is never backed up cannot hide in a list.",
    body: h(
      "div",
      { class: "sch-coverbox" },
      h(
        "div",
        { class: "sch-coverhead" },
        coverSummary,
        h(
          "label",
          { class: "sch-gapsonly", for: "sched-only-gaps" },
          onlyGaps,
          " Only gaps",
        ),
      ),
      matrixEl,
    ),
    id: "sched-coverage",
  });

  // ── 7 · history ──
  const ganttEl = h("section", {
    class: "kp-gantt",
    "data-kp-gantt": "",
    "data-kp-gantt-days": "",
    "data-kp-gantt-ranges": "24h 7d",
    "data-kp-gantt-range": "7d",
    "data-kp-gantt-ghosts": "1 1 1 1 1 1 1",
    "data-kp-time-zone": zone(),
    "aria-label": "Runs of the last 7 days",
  });
  const historyCard = card({
    title: "History: what ran, when and for how long",
    desc: "Draws every run of the last days as a bar in the lane of the schedule that started it, so a failed or overlong run stands out, and opens the job behind a bar.",
    body: h("div", { class: "sch-historybox" }, ganttEl),
    id: "sched-history",
  });

  const keys = keyRow([
    ["N", "new schedule"],
    ["Space", "turn the focused schedule on or off"],
    ["E", "edit it"],
    ["Esc", "close a menu or the list of times"],
    ["click a header", "sort"],
  ]);

  root.replaceChildren(
    header,
    weekCard.el,
    heroCard.el,
    editCard.el,
    tableCard.el,
    busyCard.el,
    coverCard.el,
    historyCard.el,
    keys,
  );

  // ── attach every kp part, now that it stands in the page ──
  /** @type {(() => void)[]} */
  const detach = [];
  detach.push(
    attachWeeks(weekCard.el, {
      decorate: (part, info) => {
        if (info.kind === "pill" && info.id && !info.id.startsWith("nightly@"))
          drivable(part, FIND_SCHEDULE, info.id.split("@")[0]);
      },
    }),
    attachCardStates(root),
    attachCountdowns(root),
    attachRuns(root),
    attachMasterDetails(editCard.el, { history: false }),
    attachDataTables(tableCard.el),
    attachSelectionBars(tableCard.el),
    attachActionTiles(tableCard.el),
    attachUptimes(tableCard.el),
    attachHeats(busyCard.el),
    attachMatrices(coverCard.el),
    attachGantts(historyCard.el),
  );
  setWeekLegend(weekEl, LEGEND);
  setGanttLegend(ganttEl, LEGEND);
  setGanttKinds(ganttEl, [
    { kind: "missed", label: "Missed", shape: "triangle", tone: "destructive" },
  ]);
  const handle = masterDetail(panes);
  setPaneState(panes, "empty", {
    title: "Pick a schedule",
    text: "Choose one in the list to change it, or start a new one.",
  });
  const dt = () => dataTable(table.wrap);

  // First frame: every section is loading in its final geometry.
  const loadingAll = () => {
    count.textContent = "";
    setWeekState(weekEl, "loading");
    setCardState(hero, "loading");
    setRunsState(thenEl, "loading");
    paneLoading = true;
    setPaneState(panes, "loading", { words: "Reading the schedules…" });
    table.loading({ words: "Reading the schedules" });
    setHeat(heatEl, {
      columns: hourColumns(),
      rows: [],
      state: "loading",
      skeleton: { rows: 7, columns: 24 },
    });
    setMatrix(matrixEl, {
      rows: [],
      columns: [],
      state: "loading",
      skeleton: 5,
    });
    setGanttState(ganttEl, "loading");
  };

  /** @param {import("../doctor.js").RouteError} e */
  const failedAll = (e) => {
    const why = `The schedules could not be read: ${e.why}`;
    count.textContent = "";
    setWeekState(weekEl, "error", why);
    setCardState(hero, "error", {
      title: "The next run could not be read",
      text: `${e.why}. The schedules are unchanged.`,
    });
    setRunsState(thenEl, "error", {
      title: "The next runs could not be read",
      text: `${e.why}. The schedules are unchanged.`,
    });
    paneError = true;
    setPaneState(panes, "error", {
      title: "The schedules could not be read",
      text: `${e.why}. Nothing was changed.`,
    });
    table.failed(e);
    setHeat(heatEl, {
      columns: hourColumns(),
      rows: [],
      state: "error",
      message: "The schedules could not be read.",
    });
    setMatrix(matrixEl, {
      rows: [],
      columns: [],
      state: "error",
      message: "The schedules could not be read.",
    });
    setGanttState(ganttEl, "error", "The history could not be read.");
  };

  const hourColumns = () => {
    const every = 3;
    return Array.from({ length: 24 }, (_, hh) => ({
      key: String(hh),
      label: hh % every === 0 ? pad(hh) : "",
    }));
  };

  const ctx = () => ({
    zone: zone(),
    nightly: typeof nightly === "number" ? nightly : null,
    name: (/** @type {Schedule} */ s) => titleOf(s),
  });

  // ── drawing the data ──
  /** @type {ScheduleView | null} */
  let heroNext = null;
  let heroTimer = 0;

  const drawWeek = () => {
    const all = list();
    if (!all.length) {
      setWeekEvents(weekEl, []);
      setWeekState(weekEl, "empty", "No schedule has a run in this week.");
      return;
    }
    setWeekEvents(
      weekEl,
      weekEvents(all, {
        now: now(),
        zone: zone(),
        nightly: typeof nightly === "number" ? nightly : null,
        label: actionLabel,
        hostTarget: hostTarget(),
      }),
    );
    setWeekState(weekEl, "ready");
  };

  const drawHero = () => {
    window.clearTimeout(heroTimer);
    const all = list();
    const next = nextUp(all);
    heroNext = next;
    heroNote.textContent = "";
    if (!next || next.next_run == null) {
      setCardState(hero, "empty", {
        title: "No run planned",
        text: "Every schedule is off or has no further run. Turn one on in the table.",
      });
      setRuns(thenEl, [], { reason: "No run is planned." });
      return;
    }
    setCardState(hero, "ready");
    const s = next.schedule;
    heroTitle.textContent = titleOf(s);
    heroWhen.textContent = `${withWeekday(next.next_run, zone())} · ${cadenceWords(s.when)}`;
    const clock = countdownOf(countdown);
    const left = Math.round(next.next_run - now());
    if (left > 0) {
      clock?.reset(left);
      clock?.start();
    } else clock?.reset(1);
    setRuns(
      thenEl,
      runsWithClash(
        all.filter((v) => v !== next),
        s.when,
        next.next_run,
        3,
        ctx(),
      ),
      { reason: "This was the last run of the schedule." },
    );
  };

  /** Where the pane list's rows are drawn from. */
  const drawPaneList = () => {
    const all = list();
    paneList.replaceChildren(
      h(
        "li",
        { role: "option", "data-kp-pane-key": NEW_KEY },
        h(
          "span",
          { class: "sch-pane__what" },
          h("b", null, "New schedule"),
          h("small", null, "Pick an action, a stack and when it runs"),
        ),
      ),
      ...all.map((v) =>
        h(
          "li",
          { role: "option", "data-kp-pane-key": v.schedule.id },
          h(
            "span",
            { class: "sch-pane__what" },
            h("b", null, titleOf(v.schedule)),
            h(
              "small",
              null,
              `${cadenceWords(v.schedule.when)}${v.schedule.enabled ? "" : " · off"}`,
            ),
          ),
        ),
      ),
    );
    if (formKey) handle?.select(formKey);
  };

  /** @param {ScheduleView} v */
  const argSummary = (v) => {
    const a = v.schedule.args ?? {};
    const x = a.commit ?? a.app ?? a.unit ?? a.tag;
    return typeof x === "string" ? x : "";
  };

  /** @param {ScheduleView} v */
  const rowEl = (v) => {
    const s = v.schedule;
    const t = titleOf(s);
    const act_ = actionLabel(s.action);
    const next = s.enabled && v.next_run != null ? v.next_run : null;
    const nextText = !s.enabled
      ? "off"
      : next == null
        ? "no further run"
        : dateTimeText(next, zone());
    const sw = h("input", {
      class: "kp-switch__input",
      type: "checkbox",
      role: "switch",
      "data-sch-switch": s.id,
      title: "On: runs at its times. Off: keeps the schedule, runs nothing.",
    });
    sw.checked = s.enabled;
    drivable(sw, TOGGLE_SCHEDULE, s.id);
    const arg = argSummary(v);
    const strip = h("div", {
      class: "kp-uptime sch-strip",
      "data-kp-uptime": "",
      "data-kp-uptime-count": "14",
      "data-kp-uptime-label": "",
      "data-sch-strip": s.id,
    });
    const btn = (
      /** @type {string} */ role,
      /** @type {string} */ text,
      /** @type {string} */ title,
      /** @type {string} */ extra,
    ) =>
      h(
        "button",
        {
          type: "button",
          class: `kp-button kp-button--sm${extra}`,
          "data-kp-action": role,
          "data-sch-act": role,
          "data-sch-id": s.id,
          title,
        },
        text,
      );
    const edit = drivable(
      btn("edit", "Edit…", "Change what, where or when", ""),
      EDIT_SCHEDULE,
      s.id,
    );
    const run = drivable(
      btn("run", "Run now", "Start it once as a job; its times stay", ""),
      RUN_SCHEDULE,
      s.id,
    );
    const del = drivable(
      btn(
        "delete",
        "Delete",
        "Remove it; jobs it ran stay on Activity; Undo for 6 s",
        " kp-button--destructive",
      ),
      DELETE_SCHEDULE,
      s.id,
    );
    const cell = (
      /** @type {string} */ label,
      /** @type {import("../dom.js").Child[]} */ ...kids
    ) => h("td", { "data-label": label }, ...kids);
    const tr = h(
      "tr",
      {
        "data-kp-row-key": s.id,
        "data-id": s.id,
        class: s.enabled ? null : "off",
        tabindex: 0,
      },
      h(
        "td",
        { class: "select-col", "data-label": "" },
        h("input", {
          class: "kp-field__check",
          type: "checkbox",
          "data-kp-select-row": "",
          "data-kp-label": t,
          value: s.id,
          "aria-label": `Select ${t}`,
        }),
      ),
      h(
        "td",
        {
          "data-label": "On",
          "data-kp-filter-value": s.enabled ? "on" : "off",
          "data-kp-sort-value": s.enabled ? 0 : 1,
        },
        h(
          "label",
          { class: "kp-switch" },
          sw,
          h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
          h("span", { class: "kp-sr-only" }, `${t} turned on`),
        ),
      ),
      h(
        "td",
        {
          "data-label": "What",
          "data-kp-filter-value": act_,
          "data-kp-sort-value": t.toLowerCase(),
        },
        h("b", null, act_),
        ` · ${targetText(s.stack, hostTarget())}${arg ? ` · ${arg}` : ""}`,
        s.note ? h("small", { class: "sch-note" }, s.note) : null,
      ),
      cell("When", cadenceWords(s.when)),
      h(
        "td",
        {
          class: "kp-numeric",
          "data-label": "Next run",
          "data-kp-sort-value": next ?? 9e15,
        },
        nextText,
      ),
      cell("Last 14", strip),
      h(
        "td",
        { "data-label": "" },
        h("div", { class: "kp-row-actions" }, edit, run, del),
      ),
    );
    return tr;
  };

  const drawTable = () => {
    const all = list();
    const keep = dt()?.selected() ?? [];
    const focused =
      /** @type {HTMLElement | null} */ (document.activeElement)
        ?.closest?.("tr[data-id]")
        ?.getAttribute("data-id") ?? null;
    const rows = all.map(rowEl);
    patchKeyed(table.tbody, rows);
    table.ready();
    attachUptimes(table.tbody);
    for (const v of all) {
      const el = /** @type {HTMLElement | null} */ (
        table.tbody.querySelector(
          `[data-sch-strip="${CSS.escape(v.schedule.id)}"]`,
        )
      );
      if (el)
        setUptime(el, {
          label: "",
          periods: slotPeriods(v, act.jobs, { now: now(), zone: zone() }),
          open: "info",
        });
    }
    const ids = new Set(all.map((v) => v.schedule.id));
    const still = keep.filter((k) => ids.has(k));
    if (still.length) dt()?.select(still);
    if (focused)
      /** @type {HTMLElement | null} */ (
        table.tbody.querySelector(`tr[data-id="${CSS.escape(focused)}"]`)
      )?.focus({ preventScroll: true });
  };

  const drawTemplates = () => {
    tiles.replaceChildren(
      ...TEMPLATES.map((tpl) =>
        drivable(
          h(
            "button",
            {
              type: "button",
              class: ACTION_TILE.slice(1),
              "data-template": tpl.id,
              onclick: () => void openForm(NEW_KEY, { template: tpl }),
            },
            h("strong", { class: "kp-action__title" }, tpl.title),
            h("span", { class: "kp-action__mark", "aria-hidden": "true" }),
            h("span", { class: "kp-action__text" }, tpl.says),
          ),
          SCHEDULE_TEMPLATE,
          tpl.id,
        ),
      ),
    );
  };

  const drawBusy = () => {
    const all = list();
    const { grid, clashes } = busyModel(all, { now: now(), ...ctx() });
    const columns = hourColumns();
    if (!all.length) {
      setHeat(heatEl, {
        columns,
        rows: [],
        state: "empty",
        message: "No schedules yet, so nothing starts.",
      });
    } else {
      const per = Array.from({ length: 24 }, (_, hh) =>
        grid.reduce((sum, day) => sum + day[hh].length, 0),
      );
      const most = Math.max(1, ...per);
      setHeat(heatEl, {
        label: "Schedule starts by weekday and hour",
        every: 3,
        columns,
        rows: grid.map((day, wd) => ({
          key: WEEKDAYS[wd],
          label: WEEKDAYS[wd],
          cells: day.map((cell, hh) => {
            const where = `${WEEKDAYS[wd]} ${pad(hh)}:00 to ${pad(hh)}:59`;
            const names = cell.map(
              (x) =>
                `${x.v ? titleOf(x.v.schedule) : "Host's nightly round"} ${timeOf(x.at)}`,
            );
            const same = clashes.some(
              (c) => c.days.includes(wd) && c.time.startsWith(pad(hh)),
            );
            if (!cell.length)
              return {
                tone: /** @type {const} */ ("none"),
                label: `${where}: nothing starts`,
              };
            if (same)
              return {
                tone: /** @type {const} */ ("bad"),
                label: `${where}: ${cell.length} starts, two in the same minute`,
                tip: names.join("\n"),
              };
            if (cell.length > 1)
              return {
                tone: /** @type {const} */ ("warn"),
                label: `${where}: ${cell.length} starts in the same hour`,
                tip: names.join("\n"),
              };
            return {
              tone: /** @type {const} */ ("ok"),
              label: `${where}: 1 start`,
              tip: names.join("\n"),
            };
          }),
        })),
        summary: {
          key: "all",
          label: "Starts",
          cells: per.map((n, hh) => ({
            tone: /** @type {const} */ ("ok"),
            share: n / most,
            label: `${pad(hh)}:00: ${n} ${n === 1 ? "start" : "starts"} in the week`,
          })),
        },
      });
    }
    clashList.setAttribute(
      "aria-label",
      `${clashes.length} ${clashes.length === 1 ? "clash" : "clashes"}`,
    );
    clashList.replaceChildren(
      ...(clashes.length
        ? clashes.map((c) => {
            const mv = c.move;
            const to = mv ? movedTime(mv.schedule.when.at) : null;
            const b = mv
              ? drivable(
                  h(
                    "button",
                    {
                      type: "button",
                      class: "kp-button kp-button--sm",
                      onclick: () => void moveSchedule(mv),
                    },
                    `Move ${titleOf(mv.schedule)} 15 minutes ${to?.later === false ? "earlier" : "later"}`,
                  ),
                  MOVE_CLASH,
                  mv.schedule.id,
                )
              : null;
            return h(
              "li",
              { class: "kp-formgrid__row" },
              h("span", { class: "kp-formgrid__label" }, clashWhen(c)),
              h(
                "span",
                { class: "kp-formgrid__control" },
                h("b", null, c.a),
                " and ",
                h("b", null, c.b),
              ),
              h("span", { class: "kp-formgrid__actions" }, b),
            );
          })
        : [
            h(
              "li",
              { class: "kp-formgrid__row" },
              h("span", { class: "kp-formgrid__label" }, "No clashes"),
              h(
                "span",
                { class: "kp-formgrid__control" },
                "Nothing starts in the same minute as anything else.",
              ),
              h("span", { class: "kp-formgrid__actions" }),
            ),
          ]),
    );
  };
  /** @param {number} at */
  const timeOf = (at) => dateTimeText(at, zone()).slice(11);

  const drawCoverage = () => {
    const all = list();
    const cov = coverageModel(all, stackNames() ?? [], hostTarget());
    const stacks = onlyGaps.checked ? cov.gaps : cov.rows;
    const n = cov.gaps.length;
    coverSummary.textContent = cov.rows.length
      ? `${n} ${n === 1 ? "stack has" : "stacks have"} no backup schedule${n ? `: ${cov.gaps.join(", ")}` : ""}.`
      : "";
    const columns = COVERAGE.map((c) => ({ key: c.key, label: c.label }));
    if (!cov.rows.length) {
      setMatrix(matrixEl, {
        rows: [],
        columns: [],
        state: "empty",
        message: "No stacks known to the host.",
      });
      return;
    }
    setMatrix(matrixEl, {
      corner: "Stack ↓ Action →",
      view: "grid",
      rows: stacks.map((s) => ({
        key: s,
        label: s,
        tone: /** @type {"muted" | "bad"} */ (
          cov.has(s, "backup") ? "muted" : "bad"
        ),
      })),
      columns,
      cell: (row, column) => {
        const s = cov.has(row, column);
        const colLabel =
          COVERAGE.find((c) => c.key === column)?.label ?? column;
        if (s)
          return {
            tone: "ok",
            text: shortWhen(s.when),
            label: `${row}: ${colLabel}, ${cadenceWords(s.when)}`,
          };
        if (column === "backup")
          return {
            tone: "bad",
            text: "No backup schedule",
            label: `${row} has no backup schedule`,
          };
        return {
          tone: "none",
          label: `${row}: no ${colLabel.toLowerCase()} schedule`,
        };
      },
    });
  };

  /** The jobs the schedules started: the history and the strips follow them. */
  const drawHistory = () => {
    const m = historyModel(list(), act.jobs, {
      now: now(),
      zone: zone(),
      label: actionLabel,
      hostTarget: hostTarget(),
    });
    if (!m.lanes.length) {
      setGanttRuns(ganttEl, [], []);
      setGanttState(ganttEl, "empty", "Nothing has run in this window.");
      historyCard.setFoot([]);
      return;
    }
    setGanttRuns(ganttEl, m.runs, m.lanes);
    setGanttPoints(ganttEl, m.points, m.lanes);
    setGanttState(ganttEl, "ready");
    historyCard.setFoot(
      m.hidden > 0
        ? [
            `Showing the ${HISTORY_LANES} schedules with the most runs in the last 7 days; ${m.hidden} more ${m.hidden === 1 ? "has" : "have"} runs too.`,
          ]
        : [],
    );
  };

  // ── what a person does ──
  /** The row of a schedule in the table, found, marked and focused. @param {string} id */
  const flash = (id) => {
    const tr = /** @type {HTMLElement | null} */ (
      table.tbody.querySelector(`tr[data-id="${CSS.escape(id)}"]`)
    );
    if (!tr) return;
    tr.scrollIntoView({ block: "center", behavior: "smooth" });
    tr.classList.add("sch-flash");
    tr.focus({ preventScroll: true });
    setTimeout(() => tr.classList.remove("sch-flash"), 1400);
  };

  /** @param {Schedule} s */
  const bodyOf = (s) => ({
    stack: s.stack,
    action: s.action,
    args: s.args,
    when: s.when,
    enabled: s.enabled,
    note: (s.note ?? "").trim(),
  });

  /** @param {Schedule} s @param {boolean} on @param {boolean} [quiet] */
  const setOn = async (s, on, quiet = false) => {
    paintOn(s.id, on);
    const r = await send(
      "PUT",
      `/data/schedules/${encodeURIComponent(s.id)}`,
      { ...bodyOf(s), enabled: on },
      "the schedule",
    );
    if (!r.ok) {
      paintOn(s.id, !on);
      say(`Not saved: ${r.error.why}`);
      return false;
    }
    const view = /** @type {ScheduleView} */ (r.body);
    if (!quiet) {
      const t = undoToast(
        on
          ? `${titleOf(s)} is on again: next run ${view.next_run == null ? "none" : dateTimeText(view.next_run, zone())}.`
          : `${titleOf(s)} is off. Its slots are kept; nothing runs until you turn it on.`,
        {
          region: toastRegion(root),
          ms: UNDO_MS,
          mode: "revert",
          clock: "ring",
          key: "Mod+Z",
          undo: () => void setOn({ ...s, enabled: on }, !on, true),
        },
      );
      driveUndo(t);
    }
    void loadSchedules();
    return true;
  };

  /** The row and its pill show `on` at once, before the host answers. */
  const paintOn = (/** @type {string} */ id, /** @type {boolean} */ on) => {
    const tr = table.tbody.querySelector(`tr[data-id="${CSS.escape(id)}"]`);
    tr?.classList.toggle("off", !on);
    const input = /** @type {HTMLInputElement | null} */ (
      tr?.querySelector('input[role="switch"]') ?? null
    );
    if (input) input.checked = on;
    const all = list().map((v) =>
      v.schedule.id === id
        ? { ...v, schedule: { ...v.schedule, enabled: on } }
        : v,
    );
    count.textContent = countText(all);
  };

  /** @param {Schedule} s */
  const runNowFor = async (s, note = false) => {
    const r = await send(
      "POST",
      `/data/actions/${encodeURIComponent(s.stack)}/${encodeURIComponent(s.action)}`,
      s.args ?? {},
      `${actionLabel(s.action)} ${s.stack}`,
    );
    if (!r.ok) {
      say(`Not started: ${r.error.why}`);
      return;
    }
    const text = `${titleOf(s)} started as job ${r.body?.job}. Follow it on Activity.`;
    if (note) heroNote.textContent = text;
    else say(text);
  };

  /** @param {Schedule} s @param {number} slot */
  const skipRun = async (s, slot) => {
    const r = await send(
      "POST",
      `/data/schedules/${encodeURIComponent(s.id)}/skip`,
      { slot },
      "skipping a run",
    );
    if (!r.ok) {
      say(`Not skipped: ${r.error.why}`);
      void loadSchedules();
      return;
    }
    const next = /** @type {ScheduleView} */ (r.body).next_run;
    heroNote.textContent = `Skipped ${withWeekday(slot, zone())}. Only this run: ${titleOf(s)} stays on${
      next == null
        ? "; there is no next one."
        : `; the next one is ${withWeekday(next, zone())}.`
    }`;
    void loadSchedules();
  };

  /** @param {ScheduleView} v moved 15 minutes, so two schedules stop starting together */
  const moveSchedule = async (v) => {
    const s = v.schedule;
    const to = movedTime(s.when.at);
    const r = await send(
      "PUT",
      `/data/schedules/${encodeURIComponent(s.id)}`,
      { ...bodyOf(s), when: { ...s.when, at: to.at } },
      "the schedule",
    );
    if (!r.ok) {
      say(`Not moved: ${r.error.why}`);
      return;
    }
    say(`${titleOf(s)} now starts at ${to.at}.`);
    void loadSchedules();
  };

  /** @param {Schedule} s */
  const remove = (s) => {
    removed.add(s.id);
    render();
    const t = undoToast(`${titleOf(s)} deleted`, {
      region: toastRegion(root),
      ms: UNDO_MS,
      clock: "ring",
      key: "Mod+Z",
      leave: "commit",
      commit: async () => {
        const r = await send(
          "DELETE",
          `/data/schedules/${encodeURIComponent(s.id)}`,
          undefined,
          "the schedule",
        );
        removed.delete(s.id);
        if (!r.ok) throw new Error(r.error.why);
        void loadSchedules();
      },
      undo: () => {
        removed.delete(s.id);
        render();
      },
      restore: () => {
        removed.delete(s.id);
        render();
      },
    });
    driveUndo(t);
    if (formKey === s.id) closeForm();
  };

  // ── the form in the detail pane ──
  let paneError = false;
  let paneLoading = false;
  /** @type {string | null} */
  let formKey = null;
  /** @type {ReturnType<typeof buildScheduleForm> | null} */
  let form = null;

  const closeForm = () => {
    form?.destroy();
    form = null;
    formKey = null;
    paneBody.replaceChildren();
    paneHeading.textContent = "";
    handle?.select(null);
    handle?.close();
    setPaneState(panes, "empty", {
      title: "Pick a schedule",
      text: "Choose one in the list to change it, or start a new one.",
    });
  };

  /** @param {HTMLElement} anchor @param {() => void} then */
  const askLose = (anchor, then) => {
    const n = form?.dirty() ?? 0;
    if (!n) return then();
    openConfirmation(anchor, {
      phrase: `Leave this form and lose ${n} ${n === 1 ? "change" : "changes"}?`,
      onAccept: then,
      initialFocus: "cancel",
    });
  };

  /**
   * @param {string} key a schedule id, or NEW_KEY
   * @param {{template?: Template, stack?: string, action?: string}} [preset]
   */
  const openForm = async (key, preset = {}) => {
    const catalog = await catalogReady();
    if (!catalog || !alive) return;
    const go = () => {
      const existing = key === NEW_KEY ? null : (viewOf(key)?.schedule ?? null);
      if (key !== NEW_KEY && !existing) return;
      const d = draftFor(existing, preset.template ?? null, {
        stacks: stackNames() ?? [],
        hostTarget: hostTarget(),
        now: now(),
        zone: zone(),
        nightly: typeof nightly === "number" ? nightly : null,
      });
      if (preset.action) d.action = preset.action;
      if (preset.stack) d.stack = preset.stack;
      form?.destroy();
      formKey = key;
      paneHeading.textContent = existing ? titleOf(existing) : "New schedule";
      paneBody.replaceChildren();
      setPaneState(panes, "ready");
      form = buildScheduleForm(paneBody, {
        title: existing
          ? `Edit schedule: ${titleOf(existing)}`
          : "New schedule",
        draft: d,
        id: existing?.id ?? null,
        actions: schedulableActions(catalog),
        stacks: stackNames,
        others: list,
        hostTarget: hostTarget(),
        zone: zone(),
        nightly: typeof nightly === "number" ? nightly : null,
        label: actionLabel,
        now,
        appsOf: (stack) =>
          current()
            .fleet?.stacks.find((x) => x.name === stack)
            ?.apps?.map((a) => a.name) ?? [],
        save: async (b) => {
          const r = existing
            ? await send(
                "PUT",
                `/data/schedules/${encodeURIComponent(existing.id)}`,
                b.body,
                "the schedule",
              )
            : await send("POST", "/data/schedules", b.body, "the schedule");
          if (!r.ok) return { ok: false, why: r.error.why };
          const id = /** @type {ScheduleView} */ (r.body)?.schedule?.id;
          await loadSchedules();
          if (!existing && id) void openForm(id);
          say(existing ? "Schedule saved." : "Schedule added.");
          return { ok: true };
        },
        onClose: closeForm,
        onSaved: () => {},
      });
      handle?.select(key);
      handle?.open(key);
      form.focus();
      editCard.el.scrollIntoView({ block: "nearest", behavior: "smooth" });
    };
    if (key === formKey && !preset.template && !preset.action && !preset.stack)
      return;
    askLose(paneList, go);
  };

  panes.addEventListener(PANE_SELECT_EVENT, (event) => {
    const key = /** @type {CustomEvent} */ (event).detail?.key;
    if (!key || key === formKey) return;
    // The list moved at once; a form with changes asks first and the list
    // goes back to the form that stays.
    if (form?.dirty()) {
      const was = formKey;
      handle?.select(was);
    }
    void openForm(key);
  });
  panes.addEventListener(PANE_RETRY_EVENT, () => first());

  // The edit buttons, run buttons and delete buttons of the rows.
  tableCard.el.addEventListener("click", (event) => {
    const b = /** @type {HTMLElement | null} */ (
      /** @type {Element} */ (event.target).closest("[data-sch-act]")
    );
    if (!b) return;
    const v = viewOf(b.getAttribute("data-sch-id") ?? "");
    if (!v) return;
    const role = b.getAttribute("data-sch-act");
    if (role === "edit") void openForm(v.schedule.id);
    else if (role === "run") void runNowFor(v.schedule);
    else if (role === "delete") remove(v.schedule);
  });
  tableCard.el.addEventListener("change", (event) => {
    const c = /** @type {HTMLInputElement} */ (event.target);
    const id = c.getAttribute?.("data-sch-switch");
    if (!id) return;
    const v = viewOf(id);
    if (v) void setOn(v.schedule, c.checked);
  });

  // Bulk turn on and off, the host may refuse one.
  tableCard.el.addEventListener(SELECTION_ACTION_EVENT, (event) => {
    const d = /** @type {CustomEvent} */ (event).detail;
    const items = /** @type {{key: string, label: string}[]} */ (
      d?.items ?? []
    );
    if (!items.length) return;
    const on = d.action === "on";
    event.preventDefault();
    void (async () => {
      setSelectionBusy(
        selectionBar,
        `${on ? "Turning on" : "Turning off"} ${items.length}…`,
      );
      /** @type {{key: string, reason: string}[]} */
      const refused = [];
      for (const it of items) {
        const v = viewOf(it.key);
        if (!v) continue;
        const r = await send(
          "PUT",
          `/data/schedules/${encodeURIComponent(it.key)}`,
          { ...bodyOf(v.schedule), enabled: on },
          "the schedule",
        );
        if (!r.ok) refused.push({ key: it.key, reason: r.error.why });
      }
      setSelectionBusy(selectionBar, null);
      setSelectionResult(selectionBar, {
        done: items.length - refused.length,
        refused,
      });
      void loadSchedules();
    })();
  });

  onlyGaps.addEventListener("change", drawCoverage);
  matrixEl.addEventListener("kp-matrix-pick", (event) => {
    const d = /** @type {CustomEvent} */ (event).detail;
    const col = COVERAGE.find((c) => c.key === d.colKey);
    if (
      !col ||
      coverageModel(list(), stackNames() ?? [], hostTarget()).has(
        d.rowKey,
        d.colKey,
      )
    )
      return;
    void openForm(NEW_KEY, { stack: d.rowKey, action: col.actions[0] });
  });
  ganttEl.addEventListener(GANTT_OPEN_EVENT, (event) => {
    const d = /** @type {CustomEvent} */ (event).detail;
    const job = Number(String(d?.run?.id ?? "").replace(/^j/, ""));
    if (Number.isInteger(job) && job > 0) openJobDialog(job);
  });
  weekEl.addEventListener(WEEK_OPEN_EVENT, (event) => {
    const id = String(/** @type {CustomEvent} */ (event).detail?.id ?? "");
    if (id.includes("@") && !id.startsWith("nightly@")) flash(id.split("@")[0]);
  });

  runNow.addEventListener("click", () => {
    if (heroNext) void runNowFor(heroNext.schedule, true);
  });
  skip.addEventListener("click", () => {
    if (heroNext?.next_run != null)
      void skipRun(heroNext.schedule, heroNext.next_run);
  });
  countdown.addEventListener(COUNTDOWN_END_EVENT, () => {
    heroNote.textContent =
      "Started just now; it runs on Activity. Moving on to the next schedule.";
    heroTimer = window.setTimeout(() => void loadSchedules(), 2500);
  });
  hero.addEventListener("kp-card-retry", () => first());
  weekEl.addEventListener("kp-week-retry", () => first());
  thenEl.addEventListener("kp-runs-retry", () => first());
  table.wrap.addEventListener("kp-datatable-retry", () => first());
  heatEl.addEventListener("kp-heat-retry", () => first());
  matrixEl.addEventListener("kp-uses-retry", () => first());
  ganttEl.addEventListener("kp-gantt-retry", () => first());

  // ── keys: N new, Space toggles the focused row, E edits it ──
  let lastG = 0;
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return;
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (
      t?.closest?.("input, select, textarea, [contenteditable]") ||
      document.querySelector("dialog[open]")
    )
      return;
    if (e.key === "g") {
      lastG = Date.now();
      return;
    }
    // "g n" is the shortcut to Notifications, not a new schedule.
    if (Date.now() - lastG < 1500) return;
    if (e.key === "n" || e.key === "N") {
      e.preventDefault();
      void openForm(NEW_KEY);
      return;
    }
    const tr = /** @type {HTMLElement | null} */ (
      t?.closest?.("tr[data-id]") ?? null
    );
    if (!tr || !root.contains(tr)) return;
    const v = viewOf(tr.dataset.id ?? "");
    if (!v) return;
    if (e.key === " ") {
      e.preventDefault();
      void setOn(v.schedule, !v.schedule.enabled);
    } else if (e.key === "e" || e.key === "E") {
      e.preventDefault();
      void openForm(v.schedule.id);
    }
  };
  document.addEventListener("keydown", onKey);

  // ── the whole page, from what the dashboard holds ──
  let lastSig = "";
  const render = () => {
    if (!alive || !act.schedules) return;
    paintZone();
    const all = list();
    count.textContent = all.length ? countText(all) : "";
    weekEl.setAttribute("data-kp-time-zone", zone());
    drawWeek();
    drawHero();
    drawPaneList();
    if (!formKey && (paneError || paneLoading)) {
      paneError = false;
      paneLoading = false;
      closeForm();
    }
    drawTable();
    drawTemplates();
    drawBusy();
    drawCoverage();
    drawHistory();
    lastSig = JSON.stringify([all, nightly, act.jobs.length]);
  };
  const renderIfChanged = () => {
    const sig = JSON.stringify([list(), nightly, act.jobs.length]);
    if (sig !== lastSig) render();
  };

  const first = () => {
    if (!act.schedules) loadingAll();
    void loadSchedules().then((e) => {
      if (e && !act.schedules) failedAll(e);
    });
  };

  root.prepend(header);
  paintZone();
  // "read … ago" counts from the moment the list last arrived.
  if (act.schedules) setAgo(read, now());
  const offAct = onAct("schedules", () => {
    setAgo(read, now());
    render();
  });
  const offJobs = onAct("jobs", renderIfChanged);
  const offStore = subscribe(() => {
    form?.stacksChanged();
    if (act.schedules) drawCoverage();
  });
  void readNightly().then((v) => {
    nightly = v;
    if (act.schedules) render();
  });
  loadingAll();
  first();
  render();
  return () => {
    alive = false;
    offAct();
    offJobs();
    offStore();
    window.clearTimeout(heroTimer);
    document.removeEventListener("keydown", onKey);
    form?.destroy();
    for (const d of detach) d();
    root.classList.remove("sch-root", "nx-ops");
  };
}
