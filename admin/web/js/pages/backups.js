// feat-backup-1/2/3, redesign-backups (Backups redesign 3.71, approved by Kenny
// 2026-10-03); rebuilt on kp-themes 11.0.0 from the demo Kenny approved on
// 2026-10-19 (app-homelab-backups, feat-kp11demo-8): the Backups page.
//
// Top to bottom: the header (Refresh, Back up now…, Restore… as the one
// primary action); five KPI tiles (last night, newest snapshot,
// repositories, restore drills — a tile that filters the table — and what
// retired stacks still keep); the attention band; "Backups per night": the
// coverage grid of every stack against the last 7, 30 or 90 nights, or one
// month as a calendar with the same grid beside it (the Calendar page is the
// Month option of this switch now); "One night": who backed up on a night,
// with a repair next to each miss, and the week of one stack; the
// repositories as one grouped, expandable, sortable table with a bar of what
// each stack takes; and "Kept from retired stacks", folded, last.
//
// Data: `/data/backups/<stack>` (repositories, sizes, drill verdicts, every
// snapshot — the host's snapshot cache, fix-180) and
// `/data/backup-calendar?stack=<stack>` (which stacks keep no data by
// design, and each stack's snapshot nights — the whole history, so the 90
// night range and any month need no further read), one stack at a time so a
// slow repository holds up only its own row (fix-177, fix-224);
// `/data/retired`. Restore, Verify restore (fix-237) and the drills are the
// existing action dialogs; "Show a file…" (fix-241) is the existing
// read-only dialog.

import { bindTableUrl, ensureStyle, fetchJson, h, slowRead } from "../dom.js";
import { agoEl, setAgo } from "../ago.js";
import { formatClock, formatDateTime, humanDuration } from "../format.js";
import { openAction, openBatch } from "../actiondialog.js";
import { openDialog } from "../actui.js";
import { snapshotPickerRows } from "../snapshotpicker.js";
import { restoreHref } from "../restoreflow.js";
import { snapshotFileUrl, snapshotFileView } from "../snapshotfile.js";
import {
  declare,
  declareField,
  drivable,
  fieldId,
  viaForm,
} from "../drivable.js";
import { current, listen, subscribe } from "../store.js";
import { setParams } from "../urlstate.js";
import { retiredKit } from "./retired.js";
import { changesRetired } from "../retiredview.js";
import {
  allRepos,
  drillState,
  kpis,
  nightRange,
  nightsNow,
  offsetFor,
  searchFromView,
  viewFromSearch,
  addDays,
  cellState,
  nightKey,
} from "../backupsview.js";
import {
  calendarDays,
  monthNights,
  nightDate,
  nightRows,
  nightSentence,
  nightSummary,
  nightsSpec,
  sizeText,
  stackSizes,
} from "../backupgrid.js";
import {
  COL,
  feedCells,
  groupRow,
  reposShell,
  repoRow,
  selectionBar,
  snapshotList,
} from "../backuprepos.js";
import { gridRow, formGrid } from "../gridrow.js";
import {
  countOf,
  attentionBand,
  button,
  kbd,
  kpiStrip,
  pageHeader,
  section,
} from "../ui.js";
import {
  attachDataTables,
  dataTable,
  VIEW_EVENT,
} from "/static/kp/js/datatable.js";
import { attachRowSelect } from "/static/kp/js/row-select.js";
import { attachUptimes, setUptime } from "/static/kp/js/uptime.js";
import { attachBarCells, setBarCell } from "/static/kp/js/barcell.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import { attachActionColumns } from "/static/kp/js/actions.js";
import { attachHoverCards } from "/static/kp/js/hovercard.js";
import {
  HEAT_PAGE_EVENT,
  HEAT_PICK_EVENT,
  HEAT_ROW_EVENT,
  attachHeats,
  setHeat,
  setHeatPick,
  setHeatRows,
} from "/static/kp/js/heat.js";
import { attachFootprints } from "/static/kp/js/footprint.js";
import {
  SEGMENTED_EVENT,
  attachSegmented,
  setSegmentedValues,
} from "/static/kp/js/segmented.js";
import { RANGE_EVENT, attachRanges } from "/static/kp/js/range.js";
import {
  CALENDAR_MONTH_EVENT,
  CALENDAR_PICK_EVENT,
  attachCalendars,
  calendarMonth,
  calendarSelect,
  setCalendarDays,
  setCalendarLegend,
  setCalendarState,
} from "/static/kp/js/calendar.js";
import {
  CARD_RETRY_EVENT,
  attachCardStates,
  setCardState,
} from "/static/kp/js/card-state.js";
import {
  STACKBAR_PICK_EVENT,
  STACKBAR_RETRY_EVENT,
  attachStackbars,
  setStackbar,
} from "/static/kp/js/stackbar.js";
import {
  FACET_EVENT,
  attachFacets,
  getFacetValues,
  setFacetItems,
  setFacetValues,
} from "/static/kp/js/facets.js";
import {
  SELECTION_ACTION_EVENT,
  attachSelectionBars,
} from "/static/kp/js/selection-bar.js";
import {
  STATUS_RETRY_EVENT,
  attachStatusLists,
  buildStatusList,
  countStatus,
} from "/static/kp/js/statusrow.js";
import {
  WEEK_RANGE_EVENT,
  attachWeeks,
  setWeekEvents,
  setWeekLegend,
  setWeekStart,
  setWeekState,
} from "/static/kp/js/week.js";
import { drawnSelect } from "/static/kp/js/combobox.js";
import { registerShortcuts } from "/static/kp/js/shortcuts.js";
import { GROUP_BACKUPS, unlessDialog } from "../shortcuts.js";

// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const BK_FILTER = declareField({
  id: "bk-filter",
  page: "backups",
  what: "search the repositories by stack or app",
});
const NIGHT_PICK = declareField({
  id: "bk-night-pick",
  page: "backups",
  what: "the night the list of stacks shows (the last 14 nights)",
});
const WEEK_STACK = declareField({
  id: "bk-week-stack",
  page: "backups",
  what: "the stack whose week the night section draws",
});
const SNAPFILE = declareField({
  id: "snapfile",
  page: "backups",
  what: "the Show a file dialog: its snapshot (-snap) and path (-path)",
  row: "<stack>-<owner>-snap|path",
});

/**
 * @typedef {import("../backupsview.js").StackRead} StackRead
 * @typedef {import("../backupsview.js").Repo} Repo
 */

// ── Live view (fix-239, invariant 39): every control of this page is
// reachable with `homelab ui click <control> [row]`. The dialogs that are
// catalog actions (Restore, Restore (native), Verify restore, Wipe) and the
// batch (Back up now) are reached through their forms instead.
const REFRESH = declare({
  id: "backups-refresh",
  page: "backups",
  opens: "view",
  what: "read every stack's repositories from the host again",
});
const RESTORE_PICK = declare({
  id: "backups-restore",
  page: "backups",
  opens: "view",
  what: "open the Restore flow: pick an app and a night, then restore it",
});
const DRILL_NOW = declare({
  id: "backups-drill-now",
  page: "backups",
  opens: "dialog",
  what: "list every repository never restore-drilled, each with Verify restore",
});
const UNDRILLED = declare({
  id: "backups-undrilled-filter",
  page: "backups",
  opens: "view",
  what: "show only the repositories never restore-drilled, or all again",
});
const VIEW = declare({
  id: "backups-view",
  page: "backups",
  opens: "view",
  row: "<nights|month>",
  what: "switch the coverage between the last nights and one month",
});
const RANGE = declare({
  id: "backups-range",
  page: "backups",
  opens: "view",
  row: "<7d|30d|90d>",
  what: "how many nights the coverage grid shows",
  shows: "in the Nights view",
});
const NIGHT = declare({
  id: "backup-night",
  page: "backups",
  opens: "view",
  row: "<stack>/<YYYY-MM-DD>",
  what: "pin one stack's night of the coverage grid",
});
const FLEET_NIGHT = declare({
  id: "backup-fleet-night",
  page: "backups",
  opens: "view",
  row: "<YYYY-MM-DD>",
  what: "pin one night of the coverage grid from the whole-fleet row",
});
const MONTH_DAY = declare({
  id: "backup-month-day",
  page: "backups",
  opens: "view",
  row: "<YYYY-MM-DD>",
  what: "pin one day of the month calendar",
  shows: "in the Month view",
  reach: [{ do: "click", control: "backups-view", row: "month" }],
});
const MONTH_PREV = declare({
  id: "backup-month-prev",
  page: "backups",
  opens: "view",
  what: "show the month before",
  shows: "in the Month view",
  reach: [{ do: "click", control: "backups-view", row: "month" }],
});
const MONTH_NEXT = declare({
  id: "backup-month-next",
  page: "backups",
  opens: "view",
  what: "show the month after",
  shows: "in the Month view",
  reach: [{ do: "click", control: "backups-view", row: "month" }],
});
const MONTH_TODAY = declare({
  id: "backup-month-today",
  page: "backups",
  opens: "view",
  what: "show this month again",
  shows: "in the Month view",
  reach: [{ do: "click", control: "backups-view", row: "month" }],
});
const UNPIN = declare({
  id: "backup-night-unpin",
  page: "backups",
  opens: "view",
  what: "unpin the night and show last night again",
  shows: "while a night is pinned",
  reach: [{ do: "click", control: "backup-fleet-night", row: "*" }],
});
const EARLIER = declare({
  id: "backup-nights-earlier",
  page: "backups",
  opens: "view",
  what: "show the nights before the ones on screen",
  shows: "in the Nights view",
});
const LATER = declare({
  id: "backup-nights-later",
  page: "backups",
  opens: "view",
  what: "show the nights after the ones on screen",
  shows: "once the grid shows earlier nights",
  reach: [{ do: "click", control: "backup-nights-earlier" }],
});
const TODAY = declare({
  id: "backup-nights-today",
  page: "backups",
  opens: "view",
  what: "go back to the latest nights and unpin",
  shows: "in the Nights view",
});
const STACK_FILTER = declare({
  id: "backup-stack-filter",
  page: "backups",
  opens: "view",
  row: "<stack>",
  what: "turn one stack on or off in the coverage grid",
});
const CLEAR = declare({
  id: "backup-clear-filter",
  page: "backups",
  opens: "view",
  row: "<filter>",
  what: "turn one active filter off (stack:<name>, text, drills, all)",
  shows: "while a filter is on",
  reach: [{ do: "click", control: "backups-undrilled-filter" }],
});
const FOLD = declare({
  id: "backup-repo-group",
  page: "backups",
  opens: "view",
  row: "<stack>",
  what: "fold or unfold one stack's repositories",
});
const OPEN_REPO = declare({
  id: "backup-repo",
  page: "backups",
  opens: "view",
  row: "<stack>/<app>",
  what: "show or hide every snapshot of one repository",
});
const SORT = declare({
  id: "backup-repo-sort",
  page: "backups",
  opens: "view",
  row: "<app|age|snaps|size|drill>",
  what: "sort the repositories by a column (again: descending, then none)",
});
const RETRY = declare({
  id: "backup-stack-retry",
  page: "backups",
  opens: "view",
  row: "<stack>",
  what: "read one stack's repositories again after its read failed",
  shows: "on a stack whose repositories could not be read",
});
const NIGHT_REPAIR = declare({
  id: "backup-night-repair",
  page: "backups",
  opens: "dialog",
  row: "<stack>",
  what: "back up a stack that missed the night on show",
  shows: "on a stack that missed the night",
});
// fix-239/241: "Show a file…" opens a page dialog (no catalog action).
const SNAPSHOT_FILE = declare({
  id: "snapshot-file",
  page: "backups",
  opens: "dialog",
  row: "<stack>/<app>",
  what: "read one file of a snapshot without restoring anything",
});

/** A read that never settles is given up after this long (fix-224). */
// fix-224: overridable only from a test (`globalThis.__HOMELAB_TEST_...__`),
// never read from anything a server or a URL controls.
const UNREAD_GIVE_UP_MS =
  Number(/** @type {any} */ (globalThis).__HOMELAB_TEST_UNREAD_GIVE_UP_MS__) ||
  180_000;

const nowS = () => Math.floor(Date.now() / 1000);

/** The nightly round starts at 03:00 (the planned pill of the week). */
const PLANNED_AT = "03:00";

/** The grid's columns for a range. @type {Record<string, {n: number, every: number}>} */
const RANGES = {
  "7d": { n: 7, every: 1 },
  "30d": { n: 30, every: 3 },
  "90d": { n: 90, every: 7 },
};

/**
 * The restore action a stack's repositories need.
 * @param {boolean} native
 */
const restoreKind = (native) => (native ? "restore-native" : "restore");

/**
 * What a restore opened from a known repository and snapshot already
 * knows (fix-216: the dialog never asks again).
 * @param {boolean} native
 * @param {string} app
 * @param {string} snapshot short id
 * @returns {Record<string, string>}
 */
const restorePreset = (native, app, snapshot) =>
  native ? { snapshot } : { app, snapshot };

/** @param {Element} el @param {{label: string, value?: string}[]} options @param {string} value */
function setOptions(el, options, value) {
  const select = /** @type {HTMLSelectElement} */ (el);
  select.replaceChildren(
    ...options.map((o) => h("option", { value: o.value ?? o.label }, o.label)),
  );
  select.value = value;
  drawnSelect(select)?.refresh();
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/backups.css");
  const fromUrl = viewFromSearch(location.search);
  const params = new URLSearchParams(location.search);
  const S = {
    /** @type {string | null} */ night: fromUrl.night,
    offset: 0,
    stacks: fromUrl.stacks,
    /** @type {"nights" | "month"} */
    view: params.get("view") === "month" ? "month" : "nights",
    /** @type {"7d" | "30d" | "90d"} */
    range: /** @type {any} */ (
      ["7d", "30d", "90d"].includes(params.get("range") ?? "")
        ? params.get("range")
        : "30d"
    ),
    /** @type {"quiet" | "plain"} */ weekend: "quiet",
    cursor: monthOfNow(),
    weekStack: /** @type {string | null} */ (null),
  };
  /** @type {string[]} */
  let names = [];
  /** @type {Record<string, StackRead>} */
  let reads = {};
  /** @type {Record<string, number>} */
  const startedAt = {};
  /** @type {any[] | null} */
  let retired = null;
  /** @type {string | null} */
  let retiredError = null;
  /** @type {number | null} */
  let readAt = null;
  const abort = new AbortController();
  root.classList.add("bk-page");
  /** @type {(() => void)[]} */
  const cleanups = [() => root.classList.remove("bk-page")];
  /** Stack → CT number, from the fleet. @type {Map<string, number>} */
  const vmids = new Map();
  /** repository key → what the row stands for. @type {Map<string, {stack: string, native: boolean, repo: Repo, read: StackRead}>} */
  const refs = new Map();
  /** Stack → its colour slot by size. @type {Map<string, string>} */
  let slots = new Map();
  const markOf = (/** @type {string} */ s) =>
    slots.get(s) ?? "var(--muted-foreground)";

  const writeUrl = () => {
    let search = setParams(
      location.search,
      searchFromView({
        night: S.night,
        stacks: S.stacks,
        undrilled: false,
        q: "",
        sort: [],
      }),
    );
    search = setParams(search, {
      view: S.view === "month" ? "month" : null,
      range: S.range === "30d" ? null : S.range,
      drills: null,
      q: null,
      sort: null,
    });
    if (search !== location.search)
      history.replaceState(history.state, "", location.pathname + search);
  };

  // ── header ────────────────────────────────────────────────────────────
  const refreshBtn = drivable(
    button("Refresh", {
      cls: "kp-button--secondary",
      title: "Read every stack's repositories from the host again",
      onClick: () => void loadAll(true),
    }),
    REFRESH,
  );
  const backupBtn = viaForm(
    button("Back up now…", {
      cls: "kp-button--secondary",
      title:
        "Run a backup now of every stack that keeps data (or only the stacks you turned on in the grid); the dialog lists them first",
      onClick: () => {
        const want = names.filter((n) => {
          const r = reads[n];
          return (
            r?.status === "ok" &&
            !r.noBackup &&
            (!S.stacks.size || S.stacks.has(n))
          );
        });
        void openBatch("backup", want);
      },
    }),
    "batch",
  );
  const restoreBtn = drivable(
    button("Restore…", {
      cls: "kp-button--primary",
      title: "Open the Restore flow: pick an app and a night, then restore it",
      onClick: () => openRestorePicker(),
    }),
    RESTORE_PICK,
  );
  const head = pageHeader({
    title: "Backups",
    desc: "Every night each stack that keeps data writes a restic snapshot. See which nights are covered, open any repository, and bring data back with Restore.",
    live: "read",
    actions: [refreshBtn, backupBtn],
    primary: restoreBtn,
  });

  // ── KPI strip ─────────────────────────────────────────────────────────
  const strip = kpiStrip(
    [
      { key: "last", label: "Last night" },
      { key: "newest", label: "Newest snapshot" },
      { key: "repos", label: "Repositories" },
      {
        key: "drills",
        label: "Restore drills",
        title:
          "Click to show only the repositories never drilled in the table; click again to show all",
        toggle: {
          pressed: false,
          onToggle: () => toggleUndrilled(),
          drive: { id: UNDRILLED },
        },
      },
      { key: "retired", label: "Kept from retired" },
    ],
    { loading: true },
  );
  const tile = (/** @type {string} */ k) =>
    /** @type {ReturnType<typeof import("../ui.js").kpi>} */ (
      strip.tiles.get(k)
    );

  const band = attentionBand([]);

  // ── the coverage: grid of nights, or one month ────────────────────────
  const viewItem = (
    /** @type {"nights" | "month"} */ value,
    /** @type {string} */ label,
  ) =>
    drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-segmented__item",
          role: "radio",
          "aria-checked": String(S.view === value),
          "data-value": value,
        },
        label,
      ),
      VIEW,
      value,
    );
  const viewSwitch = h(
    "div",
    {
      class: "kp-segmented",
      "data-kp-segmented": "single",
      role: "radiogroup",
      "aria-label": "Backups view",
    },
    viewItem("nights", "Nights"),
    viewItem("month", "Month"),
  );
  const readAgo = agoEl("read", null);
  const rangeBox = h("div", {
    class: "kp-range",
    "data-kp-range": S.range,
    "data-kp-range-presets": "7d 30d 90d",
    "data-kp-range-custom": "off",
    "aria-label": "Nights to show",
  });
  const rangeRow = gridRow({
    label: "Nights shown",
    hideLabel: true,
    control: rangeBox,
  });
  const toolbar = formGrid(
    [
      gridRow({
        label: "View",
        hideLabel: true,
        control: viewSwitch,
        value: readAgo,
      }),
      rangeRow,
    ],
    "bk-toolbar",
  );

  const nightsHeat = h("div", {
    class: "kp-heat",
    "data-kp-heat": "",
    "data-kp-heat-toggle": "",
  });
  const monthHeat = h("div", { class: "kp-heat", "data-kp-heat": "" });
  const calendar = h("section", {
    class: "kp-calendar kp-calendar--busy-whole",
    "data-kp-calendar": "",
    "data-kp-calendar-month": `${S.cursor.year}-${String(S.cursor.month).padStart(2, "0")}`,
    "data-kp-heading-level": "3",
    "data-kp-time-zone": "Europe/Brussels",
    "data-kp-week-starts-on": "1",
    "data-kp-key": "backups",
    lang: "en-GB",
    "aria-label": "Backups per night",
  });
  const nightsBox = h("div", { class: "bk-nights" }, nightsHeat);
  const monthBox = h("div", { class: "bk-monthbox" }, calendar, monthHeat);
  const covCard = h(
    "section",
    {
      class: "kp-card bk-card",
      "data-kp-card": "plain",
      "data-kp-state": "loading",
      "aria-label": "Backups per night",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h(
        "div",
        { class: "kp-card__data" },
        h("div", { class: "bk-cov__body" }, nightsBox, monthBox),
      ),
    ),
  );
  // kp's heat legend (setHeat `legend`), under each grid it explains.
  /** @type {Pick<import("/static/kp/js/heat.js").HeatSpec, "legend" | "legendKeys">} */
  const heatLegend = {
    legend: [
      { tone: "ok", label: "backed up" },
      { tone: "warn", label: "some stacks missed" },
      { tone: "bad", label: "missed" },
      { tone: "before", label: "before any history" },
      { tone: "none", label: "keeps no data" },
    ],
    legendKeys: [
      [["←", "→", "↑", "↓"], "move"],
      ["Enter", "pin the night"],
      [["Page Up", "Page Down"], "earlier or later"],
      ["Esc", "unpin"],
    ],
  };
  const pinRead = h(
    "p",
    { class: "kp-formgrid__control", role: "status", "data-bk-pin": "" },
    "No night pinned. Press a cell to pin its night.",
  );
  const shownRead = h(
    "p",
    { class: "kp-formgrid__control", role: "status", "data-bk-shown": "" },
    "All stacks shown. Press a stack name to show only that one.",
  );
  const repairBtn = viaForm(
    button("Back up now", {
      cls: "kp-button--sm",
      onClick: () => {
        if (repairBtn.dataset.bkStack)
          void openAction(repairBtn.dataset.bkStack, "backup");
      },
    }),
    "backup",
  );
  repairBtn.hidden = true;
  const unpinBtn = drivable(
    button("Unpin ×", {
      cls: "kp-button--sm kp-button--ghost",
      title: "Unpin and go back to last night (Esc)",
      onClick: () => pin(null),
    }),
    UNPIN,
  );
  unpinBtn.hidden = true;
  const reads3 = formGrid(
    [
      h(
        "div",
        { class: "kp-formgrid__row" },
        h("span", { class: "kp-formgrid__label" }, "Pinned"),
        pinRead,
        h("div", { class: "kp-formgrid__actions" }, repairBtn, unpinBtn),
      ),
      h(
        "div",
        { class: "kp-formgrid__row" },
        h("span", { class: "kp-formgrid__label" }, "Shown"),
        shownRead,
      ),
    ],
    "bk-reads",
  );
  const cov = section({
    id: "bk-coverage",
    title: "Backups per night",
    desc: "Shows, for each stack, whether a backup landed on each night, so a missed night stands out at a glance and one press pins it; the Month option shows one month as a calendar with the same grid beside it.",
  });
  cov.body.append(toolbar, covCard, reads3);

  // ── one night ─────────────────────────────────────────────────────────
  const nightSel = h("select", {
    class: "kp-field__input",
    id: NIGHT_PICK,
    "data-kp-select": "",
  });
  const weekSel = h("select", {
    class: "kp-field__input",
    id: WEEK_STACK,
    "data-kp-select": "",
  });
  const nightSummaryEl = h(
    "p",
    {
      class: "kp-formgrid__value bk-summary",
      role: "status",
      "data-bk-night-summary": "",
    },
    "…",
  );
  const weekEl = h("div", {
    class: "kp-week",
    "data-kp-week": "",
    "data-kp-week-upcoming": "8",
    "aria-label": "Snapshots of one stack in a week",
  });
  let list = h("ul", {
    class: "kp-statuslist kp-statuslist--rows",
    "aria-label": "Stacks on this night",
  });
  const listBox = h("div", { class: "bk-night__list" }, list);
  const pick = formGrid(
    [
      h(
        "div",
        { class: "kp-formgrid__row" },
        h(
          "label",
          { class: "kp-formgrid__label", for: "bk-week-stack" },
          "Week of",
        ),
        h("div", { class: "kp-formgrid__control" }, weekSel),
        h("p", { class: "kp-formgrid__value" }, "Which stack the week shows."),
      ),
      h(
        "div",
        { class: "kp-formgrid__row" },
        h(
          "label",
          { class: "kp-formgrid__label", for: "bk-night-pick" },
          "Night",
        ),
        h("div", { class: "kp-formgrid__control" }, nightSel),
        nightSummaryEl,
      ),
    ],
    "bk-night__pick",
  );
  // The card holds the pick, the week and the list; only the list is the
  // card's own state (a flat stage inside it). The week is a component with
  // its own busy card and failed plate, so it stands outside that stage and
  // its plate is never hidden by the list's state.
  const nightCard = h(
    "div",
    { class: "kp-card bk-stage", "data-kp-state": "loading" },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, listBox),
    ),
  );
  const nightFrame = h(
    "section",
    {
      class: "kp-card bk-card",
      "data-kp-card": "plain",
      "aria-label": "One night",
    },
    h(
      "div",
      { class: "bk-night__cols" },
      pick,
      h("div", { class: "bk-night__week" }, weekEl),
      nightCard,
    ),
  );
  const nightSec = section({
    id: "bk-night",
    title: "One night",
    desc: "Lists every stack for the night you pick, with a repair next to each miss, and draws one stack's week so a backup that creeps later each night is visible.",
  });
  nightSec.body.append(nightFrame);

  // ── repositories ──────────────────────────────────────────────────────
  const stackbar = h("figure", { class: "kp-stackbar" });
  const facets = h(
    "div",
    {
      class: "kp-facets kp-facets--row",
      "data-kp-facets": "",
      "data-kp-facets-all": "All stacks",
      role: "group",
      "aria-labelledby": "bk-h-stack",
    },
    h("ul", { class: "kp-facets__list" }),
    h("p", { class: "kp-facets__empty", hidden: "" }, "No stacks yet."),
  );
  const facetGrid = formGrid(
    [
      h(
        "div",
        { class: "kp-formgrid__row" },
        h(
          "h3",
          { class: "kp-formgrid__label kp-facets__title", id: "bk-h-stack" },
          "Stack",
        ),
        h(
          "div",
          { class: "kp-formgrid__control", "data-kp-formgrid-span": "wide" },
          facets,
        ),
      ),
    ],
    "bk-repos__facets",
  );
  const barSlot = h("div", { class: "bk-barslot" });
  const shell = reposShell("bk-repo-table", BK_FILTER);
  // The nights column is a strip with a head and an axis, taller than the
  // row it arrives in: rows that grow in would show its text over the next
  // row's for a second, so the table's rows are drawn in place.
  const tableHost = h(
    "div",
    { class: "bk-tablehost", "data-kp-arrive": "none" },
    shell.wrap,
  );
  barSlot.append(selectionBar("bk-repo-table"));
  const readState = h("span", { class: "bk-readstate" });
  const repos = section({
    id: "bk-repos",
    title: "Repositories",
    desc: "One restic repository per app that keeps data (a native service has its own), grouped by stack: which stack takes the space, the last 7 nights, the restore drill, and every snapshot under a row.",
  });
  repos.body.append(
    stackbar,
    h("div", { class: "bk-repos__chooser" }, facetGrid, barSlot),
    tableHost,
  );
  repos.foot.hidden = false;
  repos.foot.append(
    readState,
    h(
      "span",
      null,
      kbd("/"),
      " filter · ",
      kbd("Shift"),
      " click a header: a second sort key",
    ),
  );

  // ── kept from retired stacks ──────────────────────────────────────────
  const retiredCount = h("span", { class: "bk-count" }, "reading…");
  const ret = section({
    id: "bk-retired",
    title: "Kept from retired stacks",
    desc: "When a stack, app or native unit leaves the files, its backups, /appdata and vault copies stay here until you wipe them (was the Retired page).",
    collapsible: true,
    open: fromUrl.section === "removed",
    badge: retiredCount,
  });

  root.replaceChildren(
    head.el,
    strip.el,
    band.el,
    cov.el,
    nightSec.el,
    repos.el,
    ret.el,
  );

  // ── attach what the page carries (the package's components) ───────────
  cleanups.push(
    attachSegmented(root),
    attachRanges(root),
    attachHeats(root),
    attachCalendars(root, {
      decorate: (part, info) => {
        if (info.kind === "day" && info.value)
          drivable(part, MONTH_DAY, info.value);
        else if (info.kind === "month-prev") drivable(part, MONTH_PREV);
        else if (info.kind === "month-next") drivable(part, MONTH_NEXT);
        else if (info.kind === "today") drivable(part, MONTH_TODAY);
      },
    }),
    attachWeeks(root),
    attachCardStates(root),
    attachStackbars(root),
    attachFacets(root),
    attachSelectionBars(root),
    attachHoverCards(root),
    attachFootprints(root),
  );
  const detachStatus = attachStatusLists(root);
  cleanups.push(detachStatus);
  const tables = attachDataTables(root, {
    detail: (row) => {
      const ref = refs.get(row.dataset.kpRowKey ?? "");
      if (!ref) return null;
      return snapshotList(ref, Date.now(), (x) => snapshotActions(ref, x));
    },
    decorate: (part, info) => {
      if (info.kind === "row-group-toggle" && info.value)
        drivable(part, FOLD, info.value);
      else if (info.kind === "row-toggle" && info.value)
        drivable(part, OPEN_REPO, info.value);
    },
  });
  cleanups.push(tables);
  cleanups.push(
    attachRowSelect(root),
    attachUptimes(root),
    attachBarCells(root),
  );
  const table = () =>
    /** @type {import("/static/kp/js/datatable.js").DataTableHandle | null} */ (
      dataTable(shell.wrap)
    );
  cleanups.push(bindTableUrl(table(), "repos"));

  // ── painting ──────────────────────────────────────────────────────────
  const paintLive = () => {
    const done = names.filter((n) => reads[n]?.status !== "pending").length;
    const failed = names.filter((n) => reads[n]?.status === "failed");
    if (!current().fleet)
      head.live?.set(null, "waiting for the host's report of the fleet…");
    else if (done < names.length)
      head.live?.set(null, `reading ${done} of ${names.length} stacks…`);
    else head.live?.set(readAt);
    readState.textContent = !current().fleet
      ? "Waiting for the host's report of the fleet…"
      : `Read from the host's snapshot cache · ${done - failed.length} of ${names.length} stacks answered${failed.length ? ` · ${failed.join(", ")} could not be read` : ""}`;
    // redesign-final (exact counts): "N of M stacks answered" is the
    // coverage rows of the stacks that answered.
    countOf(readState, nightsHeat, '.kp-heat__row[data-kp-read="ok"]');
    readState.dataset.bkRead =
      names.length > 0 && done === names.length ? "all" : "some";
    setAgo(readAgo, done === names.length && names.length ? readAt : null);
  };

  const paintKpis = () => {
    const k = kpis(reads, nowS(), retired ? retired.length : null);
    const loading = !current().fleet || !k.settled;
    for (const t of strip.tiles.values()) t.set({ loading });
    const pressed = undrilledOn();
    if (loading) {
      tile("drills").set({ toggle: { ...tileToggle(), pressed } });
      return;
    }
    const ln = k.lastNight;
    tile("last").set({
      value: ln.expected ? String(ln.covered) : "—",
      unit: ln.expected ? `/ ${ln.expected}` : "",
      ctx: !ln.expected
        ? "no stack keeps data"
        : ln.missed.length
          ? `${ln.missed.length} missed: ${ln.missed.join(", ")}`
          : "every stack covered",
      ctxTone: !ln.expected ? null : ln.missed.length ? "bad" : "ok",
      tone: ln.missed.length ? "bad" : null,
      title: `The night of ${nightDate(ln.night)}`,
    });
    tile("newest").set(
      k.newest
        ? {
            value: `${humanDuration(nowS() - k.newest.time)} ago`,
            ctx: k.newest.where,
            title: formatDateTime(k.newest.time),
          }
        : { value: "—", ctx: "no snapshot yet" },
    );
    const rp = k.repositories;
    tile("repos").set({
      value: String(rp.count),
      ctx: `${rp.snapshots} snapshots · ${rp.stacks} ${rp.stacks === 1 ? "stack" : "stacks"}`,
    });
    const d = k.drills;
    tile("drills").set({
      value: String(d.passed),
      unit: `/ ${d.total}`,
      ctx: d.failed
        ? `${d.failed} failed${d.never ? ` · ${d.never} never drilled` : ""}`
        : d.never
          ? `${d.never} never drilled`
          : d.total
            ? "every repository passed"
            : "no repository yet",
      ctxTone: d.failed ? "bad" : d.never ? "warn" : d.total ? "ok" : null,
      tone: d.failed ? "bad" : d.passed < d.total ? "warn" : null,
      toggle: { ...tileToggle(), pressed },
    });
    tile("retired").set(
      retired
        ? {
            value: String(retired.length),
            ctx: retired.length
              ? `${retired.length} waiting to be wiped`
              : "nothing waiting to be wiped",
          }
        : retiredError
          ? { value: "—", ctx: "could not be read" }
          : { loading: true },
    );
    paintBand(k);
  };
  const tileToggle = () => ({
    pressed: undrilledOn(),
    onToggle: () => toggleUndrilled(),
  });

  /** @param {ReturnType<typeof kpis>} k */
  const paintBand = (k) => {
    /** @type {import("../ui.js").Attention[]} */
    const items = [];
    if (k.lastNight.missed.length)
      items.push({
        key: "missed",
        tone: "bad",
        title: `${k.lastNight.missed.length} of ${k.lastNight.expected} stacks made no snapshot last night`,
        text: `${k.lastNight.missed.join(", ")} — pin the night in the grid to see each one, or back them up now.`,
      });
    if (k.drills.never)
      items.push({
        key: "drills",
        tone: "warn",
        title: `${k.drills.never} of ${k.drills.total} repositories were never restore-drilled`,
        text: "A drill restores the newest snapshot into a scratch directory and compares it; nothing live is touched.",
        action: drivable(
          button("Drill them now…", {
            cls: "kp-button--sm kp-button--secondary",
            title:
              "List every repository that never had a drill, each with its own Verify restore",
            onClick: () =>
              openDrills(
                allRepos(reads).filter(
                  (r) => drillState(r) === "never" && r.newest_snapshot,
                ),
              ),
          }),
          DRILL_NOW,
        ),
      });
    band.set(items);
  };

  const fleetReady = () => !!current().fleet;
  // The fleet read itself failed (store.js writes "could not read the fleet"
  // into the link text and keeps `fleet` null): the page says failed, not
  // "reading" for ever.
  const fleetFailed = () =>
    !current().fleet && /could not read the fleet/.test(current().link ?? "");
  const nn = () => nightsNow(nowS());

  /**
   * Live view reaches what the grids draw through kp-themes 11.2.0's decorate
   * hook: the night cells, the stack names and the paging buttons.
   * @type {(part: HTMLElement, info: import("/static/kp/js/heat.js").HeatDecorateInfo) => void}
   */
  const heatDecorate = (part, info) => {
    if (info.kind === "row-label")
      drivable(part, STACK_FILTER, info.value ?? "");
    else if (info.kind === "cell") {
      const at = (info.value ?? "").indexOf("/");
      const row = (info.value ?? "").slice(0, at);
      const col = (info.value ?? "").slice(at + 1);
      if (row === "fleet") drivable(part, FLEET_NIGHT, col);
      else drivable(part, NIGHT, `${row}/${col}`);
    } else if (info.kind === "pager-earlier") drivable(part, EARLIER);
    else if (info.kind === "pager-later") drivable(part, LATER);
    else if (info.kind === "pager-today") drivable(part, TODAY);
  };
  /** The window the nights grid shows, for its Earlier / Today / Later bar. @param {string[]} nights */
  const pagingFor = (nights) => ({
    offset: S.offset / RANGES[S.range].n,
    label: `${nightDate(nights[0])} – ${nightDate(nights[nights.length - 1])}`,
    min: 0,
  });
  /** Stamp what the grids drew that is not a button the hook hands over. @param {HTMLElement} grid */
  const decorateHeat = (grid) => {
    // The rows the page already read: "N of M stacks answered" counts them.
    for (const row of /** @type {HTMLElement[]} */ ([
      ...grid.querySelectorAll(".kp-heat__row[data-row-key]"),
    ])) {
      const r = reads[row.dataset.rowKey ?? ""];
      if (r) row.dataset.kpRead = r.status;
    }
  };

  /** The state of the whole coverage card, from what is known. */
  const coverageState = () => {
    if (!fleetReady()) return fleetFailed() ? "error" : "loading";
    if (names.length === 0) return "empty";
    // Nothing has answered yet: the card reads, with its busy card; the grids
    // fill in as the first stacks answer.
    if (names.every((n) => reads[n]?.status === "pending")) return "loading";
    const settled = names.every((n) => reads[n]?.status !== "pending");
    if (settled && names.every((n) => reads[n]?.status === "failed"))
      return "error";
    return "ready";
  };

  const paintCoverage = () => {
    const state = coverageState();
    const now = nn();
    const isMonth = S.view === "month";
    rangeRow.hidden = isMonth;
    nightsBox.hidden = isMonth;
    monthBox.hidden = !isMonth;
    setSegmentedValues(viewSwitch, [S.view]);
    covCard.style.setProperty("--bk-rows", String(Math.max(names.length, 3)));
    // The grids are components with their own busy card and plates; the card
    // around them never hides those.
    setCardState(covCard, "ready");
    if (state !== "ready") {
      setCalendarState(
        calendar,
        state,
        state === "loading"
          ? "Reading the stacks…"
          : state === "empty"
            ? "No stack has a backup history yet"
            : "The backup history could not be read",
      );
      // Both grids carry the card's state themselves (the package's busy card
      // while it reads, its plate when empty or failed), never their last
      // face.
      const gridState = (
        /** @type {string[]} */ nights,
        /** @type {number} */ every,
      ) => ({
        ...heatLegend,
        ...nightsSpec({
          names: [],
          reads: {},
          now,
          nights,
          every,
          weekend: S.weekend,
          markOf,
          fleetReady: false,
          shown: S.stacks,
          label: "Backups per stack",
          state,
          message:
            state === "empty"
              ? "No stack has a backup history yet."
              : "The backup history could not be read.",
        }),
        skeleton: {
          rows: Math.max(names.length, 3),
          columns: nights.length,
        },
      });
      const range = RANGES[S.range];
      const loadingNights = nightRange(now.current, range.n, S.offset);
      setHeat(
        nightsHeat,
        {
          ...gridState(loadingNights, range.every),
          paging: pagingFor(loadingNights),
        },
        { decorate: heatDecorate },
      );
      setHeat(
        monthHeat,
        gridState(monthNights(S.cursor.year, S.cursor.month), 5),
        { decorate: heatDecorate },
      );
      return;
    }
    const range = RANGES[S.range];
    const nights = nightRange(now.current, range.n, S.offset);
    const common = {
      names,
      reads,
      now,
      weekend: S.weekend,
      markOf,
      fleetReady: fleetReady(),
      shown: S.stacks,
    };
    if (!isMonth) {
      setHeat(
        nightsHeat,
        {
          ...heatLegend,
          ...nightsSpec({
            ...common,
            nights,
            every: range.every,
            label: `Backups per stack, last ${range.n} nights`,
          }),
          paging: pagingFor(nights),
        },
        { decorate: heatDecorate },
      );
      decorateHeat(nightsHeat);
      if (S.night && nights.includes(S.night)) setHeatPick(nightsHeat, S.night);
      else setHeatPick(nightsHeat, null);
      setHeatRows(nightsHeat, [...S.stacks]);
    } else {
      const days = monthNights(S.cursor.year, S.cursor.month);
      setHeat(
        monthHeat,
        {
          ...heatLegend,
          ...nightsSpec({
            ...common,
            nights: days,
            every: 5,
            label: `Backups per stack in ${String(S.cursor.month).padStart(2, "0")}/${S.cursor.year}`,
          }),
          toggle: false,
        },
        { decorate: heatDecorate },
      );
      decorateHeat(monthHeat);
      const from = addDays(now.current, -400);
      setCalendarDays(
        calendar,
        calendarDays(reads, now, { from, to: addDays(now.current, 45) }),
      );
      setCalendarLegend(calendar, [
        { tone: "ok", label: "every stack backed up" },
        { tone: "warn", label: "some stacks missed" },
        { tone: "bad", label: "every stack missed" },
        { tone: "future", label: "not run yet" },
      ]);
      setHeatPick(
        monthHeat,
        S.night && days.includes(S.night) ? S.night : null,
      );
      if (S.night) calendarSelect(calendar, S.night);
    }
    shownRead.textContent = S.stacks.size
      ? `Showing only: ${[...S.stacks].sort().join(", ")} (the page filters; the grid reports).`
      : "All stacks shown. Press a stack name to show only that one.";
  };

  const paintPin = () => {
    unpinBtn.hidden = !S.night;
    repairBtn.hidden = true;
    delete repairBtn.dataset.bkStack;
    if (!S.night) {
      pinRead.textContent =
        S.view === "month"
          ? "No night picked. Press a day or a cell."
          : "No night pinned. Press a cell to pin its night.";
      return;
    }
    const now = nn();
    pinRead.textContent = nightSentence(reads, S.night, now);
    const first = names.find(
      (s) =>
        reads[s] &&
        cellState(reads[s], /** @type {string} */ (S.night), now) === "miss",
    );
    if (first) {
      repairBtn.hidden = false;
      repairBtn.textContent = `Back up ${first} now`;
      repairBtn.dataset.bkStack = first;
    }
  };

  // ── one night: the list and the week ──────────────────────────────────
  const nightOn = () => S.night ?? nn().last;
  const mondayOf = (/** @type {string} */ iso) => {
    // date-key: the weekday only decides which Monday the week starts on.
    const wd = new Date(`${iso}T12:00:00Z`).getUTCDay();
    return addDays(iso, -((wd + 6) % 7));
  };
  /** @type {string} */
  let weekStart = "";

  const paintNightPickers = () => {
    const now = nn();
    const last = now.last;
    const nights = Array.from({ length: 14 }, (_, i) => addDays(last, -i));
    const on = nightOn();
    if (!nights.includes(on)) nights.push(on);
    setOptions(
      nightSel,
      nights.map((n) => {
        const missed = names.filter(
          (s) => reads[s] && cellState(reads[s], n, now) === "miss",
        ).length;
        return {
          value: n,
          label: `${nightDate(n)}${n === last ? " (last night)" : ""}${missed ? ` · ${missed} missed` : ""}`,
        };
      }),
      on,
    );
    const keeping = names.filter((s) => {
      const r = reads[s];
      return !(r?.status === "ok" && r.noBackup);
    });
    const wanted =
      S.weekStack && keeping.includes(S.weekStack)
        ? S.weekStack
        : keeping.includes("kyu")
          ? "kyu"
          : keeping[0];
    S.weekStack = wanted ?? null;
    setOptions(
      weekSel,
      keeping.map((s) => ({ value: s, label: s })),
      wanted ?? "",
    );
  };

  const weekEvents = () => {
    const stack = S.weekStack;
    const r = stack ? reads[stack] : null;
    /** @type {import("/static/kp/js/week.js").WeekEvent[]} */
    const out = [];
    if (!stack || !r || r.status !== "ok") return out;
    const now = nn();
    for (let d = 0; d < 7; d++) {
      const iso = addDays(weekStart, d);
      if (iso > now.current) continue;
      const st = cellState(r, iso, now);
      if (st === "before" || st === "none") continue;
      out.push({
        id: `plan-${iso}`,
        start: plannedMs(iso),
        label: "planned",
        state: "reference",
        detail: `${nightDate(iso)}: the nightly round starts at ${PLANNED_AT}.`,
      });
      if (st === "miss")
        out.push({
          id: `${stack}-miss-${iso}`,
          start: plannedMs(iso) + 30_000,
          label: "missed",
          state: "skipped",
          detail: `${nightDate(iso)}: no snapshot${r.reason ? `, ${r.reason}` : ""}.`,
        });
    }
    r.repos.forEach((repo, i) => {
      for (const x of repo.snapshots ?? []) {
        const iso = nightKey(x.time);
        if (iso < weekStart || iso > addDays(weekStart, 6)) continue;
        out.push({
          id: `${stack}-${repo.owner}-${x.short_id}`,
          start: x.time * 1000,
          label: repo.owner,
          series: /** @type {1} */ (Math.min(i + 1, 5)),
          detail: `${nightDate(iso)}: ${repo.owner} snapshot ${x.short_id} at ${formatDateTime(x.time).slice(-5)}.`,
        });
      }
    });
    return out;
  };

  const paintNight = () => {
    const state = coverageState();
    const now = nn();
    const night = nightOn();
    paintNightPickers();
    if (state !== "ready") {
      setCardState(nightCard, state, {
        words: "Reading the stacks…",
        title:
          state === "empty"
            ? "No stack has a backup history yet"
            : "The stacks could not be read",
        text:
          state === "empty"
            ? "The first nightly round fills this view."
            : "None of the stacks answered. Nothing was changed.",
      });
      // The week stands in the same card: it carries the card's state too, so
      // it never shows its stale or blank face while the card reads or fails.
      setWeekState(
        weekEl,
        state,
        state === "loading"
          ? "Reading the stacks…"
          : state === "empty"
            ? "No stack has a backup history yet"
            : "The stacks could not be read",
      );
      nightSummaryEl.textContent = "…";
      return;
    }
    setCardState(nightCard, "ready");
    const next = buildStatusList(nightRows(names, reads, night, now), {
      label: `Stacks on ${nightDate(night)}`,
    });
    next.className = "kp-statuslist kp-statuslist--rows";
    list.replaceWith(next);
    list = next;
    attachStatusLists(listBox);
    for (const b of /** @type {HTMLElement[]} */ ([
      ...list.querySelectorAll("[data-kp-status-retry-button]"),
    ])) {
      const id = b
        .closest("[data-kp-status-id]")
        ?.getAttribute("data-kp-status-id");
      if (id) {
        viaForm(b, "backup");
        drivable(b, NIGHT_REPAIR, id);
      }
    }
    nightSummaryEl.textContent = nightSummary(countStatus(list), night);
    if (!weekStart || nightSel.dataset.bkWeekFor !== night) {
      weekStart = mondayOf(night);
      nightSel.dataset.bkWeekFor = night;
    }
    setWeekStart(weekEl, weekStart);
    setWeekEvents(weekEl, weekEvents());
    const stack = S.weekStack;
    const r = stack ? reads[stack] : null;
    setWeekLegend(
      weekEl,
      r && r.status === "ok"
        ? r.repos.map((repo, i) => ({
            series: /** @type {1} */ (Math.min(i + 1, 5)),
            label: `${repo.owner} snapshot`,
          }))
        : [],
    );
    if (!stack || !r)
      setWeekState(weekEl, "empty", "Pick a stack that keeps data.");
    else if (r.status === "pending")
      setWeekState(weekEl, "loading", "Reading the stack…");
    else if (r.status === "failed")
      setWeekState(weekEl, "error", `${stack} could not be read: ${r.reason}`);
    else setWeekState(weekEl, "ready");
  };

  // ── repositories ──────────────────────────────────────────────────────
  const paintRepos = () => {
    const dt = table();
    if (!dt) return;
    const now = nn();
    const nowSec = nowS();
    // The rows first; the table is told after.
    const selected = dt.selected();
    const groups = names.map((s) => ({
      stack: s,
      read: reads[s] ?? { status: "pending" },
    }));
    const all = allRepos(reads);
    const sizes = stackSizes(all);
    slots = new Map(
      sizes.map((s, i) => [s.stack, `var(--chart-${(i % 5) + 1})`]),
    );
    refs.clear();
    const max = Math.max(1, ...all.map((r) => r.size_bytes ?? 0));
    const frag = document.createDocumentFragment();
    for (const g of groups) {
      const r = g.read;
      const native = r.status === "ok" && r.native;
      const total = r.status === "ok" ? r.repos.length : 0;
      const why =
        r.status === "pending"
          ? `reading… (${Math.round((Date.now() - (startedAt[g.stack] ?? Date.now())) / 1000)} s)`
          : r.status === "failed"
            ? `could not read it: ${r.reason}`
            : r.noBackup
              ? "keeps no data by design"
              : r.repos.length === 0
                ? r.reason
                  ? `no repository found on the backup target (${r.reason})`
                  : "no repository found on the backup target"
                : null;
      /** @type {HTMLElement | null} */
      let act = null;
      if (r.status === "ok" && total)
        act = viaForm(
          button("Restore…", {
            cls: "kp-button--sm",
            title: `Restore one of ${g.stack}'s apps`,
            attrs: {
              "data-action": restoreKind(native),
              "data-kp-action": "restore",
            },
            onClick: (e) => {
              e.stopPropagation();
              void openAction(g.stack, restoreKind(native));
            },
          }),
          restoreKind(native),
        );
      else if (r.status === "failed")
        act = drivable(
          button("Retry", {
            cls: "kp-button--sm kp-button--secondary",
            title: `Read ${g.stack}'s repositories again`,
            onClick: (e) => {
              e.stopPropagation();
              void retryOne(g.stack);
            },
          }),
          RETRY,
          g.stack,
        );
      frag.append(
        groupRow({
          stack: g.stack,
          vmid: vmids.get(g.stack) ?? null,
          read: r,
          total,
          restore: act,
          why,
        }),
      );
      if (r.status !== "ok") continue;
      for (const repo of r.repos) {
        const key = `${g.stack}/${repo.owner}`;
        refs.set(key, { stack: g.stack, native, repo, read: r });
        const newest = repo.newest_snapshot;
        const restore = viaForm(
          button("Restore…", {
            cls: "kp-button--sm",
            title: newest
              ? "Restore this app; the newest snapshot is picked, change it in the dialog"
              : "This repository has no snapshot to restore",
            attrs: {
              "data-action": restoreKind(native),
              "data-kp-action": "restore",
            },
            onClick: () => {
              if (newest)
                void openAction(g.stack, restoreKind(native), {
                  preset: restorePreset(native, repo.owner, newest.short_id),
                });
            },
          }),
          restoreKind(native),
        );
        if (!newest) restore.setAttribute("disabled", "");
        frag.append(
          repoRow({ stack: g.stack, native, repo, read: r, now, max, restore }),
        );
      }
    }
    shell.tbody.replaceChildren(frag);
    attachIdmarks(shell.wrap);
    feedCells(shell.wrap, refs, now, setUptime, setBarCell);
    dt.refresh();
    attachActionColumns(shell.wrap);
    // The ticks, the opened rows and the folds survive a repaint by key.
    const keep = selected.filter((k) => refs.has(k));
    if (keep.length) dt.select(keep);
    drivableHeads();

    const settled =
      names.length > 0 && names.every((n) => reads[n]?.status !== "pending");
    const anyOk = names.some((n) => reads[n]?.status === "ok");
    if (fleetFailed()) {
      dt.busy(null);
      dt.fail("The fleet could not be read. Nothing was changed.");
    } else if (!fleetReady() || (!anyOk && !settled)) {
      dt.busy({ text: "Reading the repositories…", overlay: true });
      dt.state("loading");
    } else if (settled && !anyOk && names.length) {
      dt.busy(null);
      dt.fail("None of the stacks answered. Nothing was changed.");
    } else {
      dt.busy(
        settled
          ? null
          : {
              text: `Reading the repositories… ${names.filter((n) => reads[n]?.status !== "pending").length} of ${names.length} stacks`,
              overlay: false,
            },
      );
      dt.state("ready");
    }
    paintBar(sizes, all.length);
    void nowSec;
  };

  /** Live view reaches the sort headers by their column. */
  const drivableHeads = () => {
    /** @type {Record<number, string>} */
    const rows = { 2: "app", 5: "snaps", 6: "size", 7: "drill" };
    const heads = /** @type {HTMLTableCellElement[]} */ ([
      ...shell.wrap.querySelectorAll("thead th[data-kp-sort]"),
    ]);
    heads.forEach((th) => {
      const col = th.cellIndex;
      if (rows[col]) drivable(th, SORT, rows[col]);
    });
  };

  /** @param {{stack: string, bytes: number, count: number}[]} sizes @param {number} total */
  const paintBar = (sizes, total) => {
    const state = !fleetReady()
      ? "loading"
      : coverageState() === "error"
        ? "error"
        : names.length &&
            names.some((n) => reads[n]?.status === "pending") &&
            total === 0
          ? "loading"
          : total === 0
            ? "empty"
            : "ready";
    const sum = sizes.reduce((a, s) => a + s.bytes, 0);
    setStackbar(stackbar, {
      title: "Backup disk, by stack",
      parts:
        state === "ready"
          ? sizes.map((s) => ({
              key: s.stack,
              label: s.stack,
              value: s.bytes,
              tip: `${s.stack}: ${sizeTextOf(s.bytes)} in ${s.count} ${s.count === 1 ? "repository" : "repositories"}`,
            }))
          : [],
      format: sizeTextOf,
      summary: `${total.toLocaleString("en-GB")} ${total === 1 ? "repository" : "repositories"}, ${sizeTextOf(sum)}`,
      state,
      message:
        state === "error"
          ? "The backup repositories could not be read."
          : "Nothing is stored yet.",
      label: "Backup repositories by stack",
    });
    setFacetItems(
      facets,
      state === "ready"
        ? sizes.map((s) => ({
            value: s.stack,
            label: s.stack,
            count: s.count,
          }))
        : [],
    );
    const values = tableStackFilter();
    if (values) setFacetValues(facets, values);
  };

  const tableStackFilter = () => {
    const dt = table();
    const f = dt?.view().filters[COL.stack];
    return Array.isArray(f) && f.length ? f : null;
  };
  const undrilledOn = () => {
    const f = table()?.view().filters[COL.drill];
    return Array.isArray(f) && f.length === 1 && f[0] === "never drilled";
  };
  const toggleUndrilled = () => {
    table()?.filter(COL.drill, undrilledOn() ? null : ["never drilled"]);
    paintKpis();
  };

  // The stack filter is one thing in three places: the bar, the list and the table's own filter.
  const filterStacks = (/** @type {string[]} */ keys) => {
    setFacetValues(facets, keys.length ? keys : null);
    table()?.filter(COL.stack, keys.length ? keys : null);
  };
  stackbar.addEventListener(STACKBAR_PICK_EVENT, (e) => {
    const key = /** @type {CustomEvent} */ (e).detail.key;
    const now = getFacetValues(facets);
    filterStacks(now.length === 1 && now[0] === key ? [] : [key]);
  });
  stackbar.addEventListener(STACKBAR_RETRY_EVENT, () => void loadAll(true));
  facets.addEventListener(FACET_EVENT, () => {
    const values = getFacetValues(facets);
    const all = stackSizes(allRepos(reads)).length;
    table()?.filter(
      COL.stack,
      values.length && values.length < all ? values : null,
    );
  });
  shell.wrap.addEventListener(VIEW_EVENT, () => {
    setFacetValues(facets, tableStackFilter());
    paintKpis();
  });
  shell.wrap.addEventListener("kp-datatable-retry", () => void loadAll(true));

  /** @param {{stack: string, native: boolean, repo: Repo}} ref @param {import("../backupsview.js").Snap} x */
  const snapshotActions = (ref, x) => {
    const restore = viaForm(
      button("Restore…", {
        cls: "kp-button--sm",
        title: `Restore ${ref.repo.owner} to ${x.short_id}`,
        attrs: {
          "data-action": restoreKind(ref.native),
          "data-kp-action": "restore",
        },
        onClick: () =>
          void openAction(ref.stack, restoreKind(ref.native), {
            preset: restorePreset(ref.native, ref.repo.owner, x.short_id),
          }),
      }),
      restoreKind(ref.native),
    );
    /** @type {HTMLElement[]} */
    const out = [restore];
    // A native unit's snapshot is one tar stream, not a tree of files, so
    // its row has no Show a file (restore is the way back for it).
    if (!ref.native) {
      const file = drivable(
        button("Show a file…", {
          cls: "kp-button--sm kp-button--ghost",
          title:
            "Read one file as this snapshot holds it, without restoring anything",
          attrs: { "data-kp-action": "file" },
          onClick: () => openFileDialog(ref.stack, ref.repo, x.short_id),
        }),
        SNAPSHOT_FILE,
        `${ref.stack}/${ref.repo.owner}`,
      );
      out.push(file);
    }
    return out;
  };

  // the selection bar: Verify restore and Back up now on the ticked rows
  barSlot.addEventListener(SELECTION_ACTION_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    /** @type {string[]} */
    const keys = d.keys ?? [];
    const picked = keys.map((k) => refs.get(k)).filter((x) => !!x);
    if (d.action === "verify") {
      openDrills(
        picked
          .filter((x) => x && x.repo.newest_snapshot)
          .map((x) => ({
            .../** @type {any} */ (x).repo,
            stack: /** @type {any} */ (x).stack,
            native: /** @type {any} */ (x).native,
          })),
      );
    } else {
      const stacks = [
        ...new Set(picked.map((x) => /** @type {any} */ (x).stack)),
      ];
      void openBatch("backup", stacks);
    }
  });

  /** @param {string} s */
  const toggleStack = (s) => {
    if (S.stacks.has(s)) S.stacks.delete(s);
    else S.stacks.add(s);
    writeUrl();
    paintAll();
  };

  const page = (/** @type {number} */ dir) => {
    const n = RANGES[S.range].n;
    S.offset = Math.max(0, S.offset + dir * n);
    paintCoverage();
  };

  /** @param {string | null} n */
  const pin = (n) => {
    S.night = n;
    if (n) {
      const range = RANGES[S.range].n;
      const off = offsetFor(nn().current, n, range);
      if (S.offset > off || off >= S.offset + range) S.offset = off;
      S.cursor = { year: Number(n.slice(0, 4)), month: Number(n.slice(5, 7)) };
      weekStart = "";
    }
    writeUrl();
    paintCoverage();
    paintPin();
    paintNight();
  };

  // events of the coverage
  const onPick = (/** @type {Event} */ e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    const key = d.picked ? d.colKey : null;
    if (key === S.night) return;
    S.night = key;
    weekStart = "";
    writeUrl();
    if (S.view === "month" && key) calendarSelect(calendar, key);
    paintPin();
    paintNight();
  };
  nightsHeat.addEventListener(HEAT_PICK_EVENT, onPick);
  monthHeat.addEventListener(HEAT_PICK_EVENT, onPick);
  nightsHeat.addEventListener(HEAT_ROW_EVENT, (e) => {
    S.stacks = new Set(/** @type {CustomEvent} */ (e).detail.keys);
    writeUrl();
    shownRead.textContent = S.stacks.size
      ? `Showing only: ${[...S.stacks].sort().join(", ")} (the page filters; the grid reports).`
      : "All stacks shown. Press a stack name to show only that one.";
    paintCoverage();
  });
  // The grid's own Earlier / Today / Later bar and Page Up / Page Down.
  nightsHeat.addEventListener(HEAT_PAGE_EVENT, (e) => {
    const direction = /** @type {CustomEvent} */ (e).detail.direction;
    if (direction === "today") {
      S.offset = 0;
      pin(null);
    } else if (direction === "earlier" || direction === "later")
      page(direction === "earlier" ? +1 : -1);
  });
  calendar.addEventListener(CALENDAR_PICK_EVENT, (e) => {
    const date = /** @type {CustomEvent} */ (e).detail.date;
    S.night = date;
    weekStart = "";
    writeUrl();
    setHeatPick(monthHeat, date);
    paintPin();
    paintNight();
  });
  calendar.addEventListener(CALENDAR_MONTH_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    S.cursor = { year: d.year, month: d.month };
    paintCoverage();
  });
  viewSwitch.addEventListener(SEGMENTED_EVENT, (e) => {
    S.view =
      /** @type {CustomEvent} */ (e).detail.value === "month"
        ? "month"
        : "nights";
    writeUrl();
    paintCoverage();
    paintPin();
  });
  rangeBox.addEventListener(RANGE_EVENT, (e) => {
    const key = /** @type {CustomEvent} */ (e).detail.preset;
    if (!(key in RANGES)) return;
    S.range = /** @type {any} */ (key);
    S.offset = S.night
      ? offsetFor(nn().current, S.night, RANGES[S.range].n)
      : 0;
    writeUrl();
    paintCoverage();
  });
  for (const el of [nightCard, nightsHeat, monthHeat, weekEl]) {
    for (const ev of [CARD_RETRY_EVENT, "kp-heat-retry", "kp-week-retry"])
      el.addEventListener(ev, () => void loadAll(true));
  }
  nightSel.addEventListener("change", () => {
    const night = nightSel.value;
    pin(night === nn().last && !S.night ? null : night);
  });
  weekSel.addEventListener("change", () => {
    S.weekStack = weekSel.value;
    paintNight();
  });
  weekEl.addEventListener(WEEK_RANGE_EVENT, (e) => {
    weekStart = /** @type {CustomEvent} */ (e).detail.start;
    setWeekEvents(weekEl, weekEvents());
  });
  nightCard.addEventListener(STATUS_RETRY_EVENT, (e) => {
    const id = /** @type {CustomEvent} */ (e).detail.id;
    if (names.includes(id) && reads[id]?.status === "ok")
      void openAction(id, "backup");
    else if (names.includes(id)) void retryOne(id);
  });

  const paintAll = () => {
    paintLive();
    paintKpis();
    paintCoverage();
    paintPin();
    paintNight();
    paintRepos();
  };

  // ── dialogs ───────────────────────────────────────────────────────────
  // redesign-final-h3: Restore… opens the Restore flow (FLOWS.md §5), on
  // the stack the view is narrowed to, or the first with a snapshot.
  const openRestorePicker = () => {
    const withRepos = names.filter(
      (n) =>
        reads[n]?.status === "ok" && /** @type {any} */ (reads[n]).repos.length,
    );
    const first =
      (S.stacks.size ? withRepos.find((n) => S.stacks.has(n)) : null) ??
      withRepos[0] ??
      null;
    history.pushState(null, "", restoreHref(first));
    dispatchEvent(new PopStateEvent("popstate"));
  };

  /**
   * "Verify restore" on a set of repositories (the drill list, or the
   * ticked rows): each its own Verify restore, a job per repository.
   * @param {(Repo & {stack: string, native?: boolean})[]} repoList
   */
  const openDrills = (repoList) => {
    /** @type {{close: () => void} | null} */
    let dlg = null;
    const items = h(
      "ul",
      { class: "bk-drill-list" },
      ...repoList.map((r) => {
        const b = viaForm(
          button("Verify restore…", {
            cls: "kp-button--sm kp-button--secondary",
            title:
              "Restore the newest snapshot into the drill's scratch directory and judge it",
            attrs: { "data-action": "verify-restore" },
            onClick: () => {
              dlg?.close();
              void openAction(r.stack, "verify-restore", {
                preset: {
                  app: r.owner,
                  snapshot: /** @type {any} */ (r.newest_snapshot).short_id,
                },
              });
            },
          }),
          "verify-restore",
        );
        return h(
          "li",
          null,
          h("span", { class: "mono" }, `${r.stack}/${r.owner}`),
          h(
            "span",
            { class: "bk-hint" },
            `newest ${humanDuration(nowS() - (r.newest_snapshot?.time ?? nowS()))} ago`,
          ),
          b,
        );
      }),
    );
    dlg = openDialog({
      title: "Verify restore",
      description:
        "Verify restore restores a repository's newest snapshot into the drill's scratch directory, judges it as the nightly drill does, and empties the scratch again; nothing live is touched. Each runs as its own job.",
      body: [
        items,
        h(
          "p",
          { class: "bk-hint" },
          "The Restore drills tile counts the nightly round's own drills, one repository a night in turn; a verify you start here reports in its job.",
        ),
      ],
      wide: true,
      id: "bk-drills",
    });
  };

  // ── retired ───────────────────────────────────────────────────────────
  /** @type {ReturnType<typeof retiredKit> | null} */
  let retiredKitInstance = null;
  const paintRetired = () => {
    // The kit (its key figure, share bar and table) is drawn when the fold
    // is first opened, so a folded section holds no table that is laid out
    // out of sight; until then the badge counts from the read itself.
    const folded =
      !retiredKitInstance && !(/** @type {HTMLDetailsElement} */ (ret.el).open);
    // redesign-final (exact counts): "N entries" is the retired table's rows.
    if (!folded)
      countOf(retiredCount, ret.body, "tbody > tr:not([data-kp-skeleton-row])");
    retiredCount.textContent = retired
      ? `${retired.length} ${retired.length === 1 ? "entry" : "entries"}`
      : retiredError
        ? "not read"
        : "reading…";
    if (folded) return;
    // feat-kp11demo-12.6: the retired kit (retired.js) draws the key figure,
    // the share bar, the age control, the table and Wipe, and its own
    // loading, empty and failed states.
    if (!retiredKitInstance) {
      retiredKitInstance = retiredKit({ onRetry: () => void loadRetired() });
      ret.body.replaceChildren(retiredKitInstance.el);
    }
    retiredKitInstance.set(
      retiredError
        ? {
            status: "failed",
            error: { what: "the retired list", why: retiredError, fix: "" },
          }
        : !retired
          ? { status: "loading" }
          : { status: "ready", entries: retired },
    );
  };
  cleanups.push(() => retiredKitInstance?.stop());

  // ── loading ───────────────────────────────────────────────────────────
  /**
   * One stack: its repositories and its nights, read side by side.
   * @param {string} name
   * @param {boolean} force
   * @returns {Promise<StackRead>}
   */
  const readOne = async (name, force) => {
    const q = force ? "?refresh=1" : "";
    const enc = encodeURIComponent(name);
    try {
      const [b, c] = await Promise.all([
        fetchJson(
          `/data/backups/${enc}${q}`,
          `${name}'s backups`,
          abort.signal,
        ),
        slowRead(
          `/data/backup-calendar?stack=${enc}${force ? "&refresh=1" : ""}`,
          `${name}'s backup nights`,
          abort.signal,
        ).catch(() => ({ ok: false, error: null })),
      ]);
      if (!b.ok)
        return {
          status: "failed",
          reason: b.error.fix ? `${b.error.why} — ${b.error.fix}` : b.error.why,
        };
      /** @type {Repo[]} */
      const repoList = b.body?.repos ?? [];
      const cal = c.ok ? /** @type {any} */ (c).body : null;
      const calTimes = /** @type {number[] | undefined} */ (
        cal?.stacks?.[name]
      );
      const repoTimes = repoList.flatMap((r) =>
        (r.snapshots ?? []).map((s) => s.time),
      );
      return {
        status: "ok",
        native: b.body?.native === true,
        repos: repoList,
        noBackup: (cal?.no_backup ?? []).includes(name),
        reason: cal?.reasons?.[name] ?? null,
        times: [...new Set([...(calTimes ?? []), ...repoTimes])],
      };
    } catch (e) {
      if (abort.signal.aborted) return { status: "pending" };
      return { status: "failed", reason: String(e) };
    }
  };

  /** A repository the host has not read yet is asked again, a while later. */
  const unread = (/** @type {StackRead} */ r) =>
    r.status === "ok" && r.repos.some((x) => x.error === "not read yet");

  /**
   * @param {string} name
   * @param {boolean} force
   */
  const readAndPaint = async (name, force) => {
    startedAt[name] ??= Date.now();
    let r = await readOne(name, force);
    let wait = 2000;
    while (
      !abort.signal.aborted &&
      unread(r) &&
      Date.now() - startedAt[name] < UNREAD_GIVE_UP_MS
    ) {
      // Still unread by the host: the stack stays "reading…" (with how long
      // it has waited) until it answers or the give-up window ends.
      reads = { ...reads, [name]: { status: "pending" } };
      paintAll();
      await new Promise((res) => setTimeout(res, wait));
      wait = Math.min(wait * 2, 15000);
      r = await readOne(name, false);
    }
    if (abort.signal.aborted) return;
    // fix-224: a stack that never settles is named, with Retry.
    if (unread(r))
      r = {
        status: "failed",
        reason: `the host did not answer within ${Math.round(UNREAD_GIVE_UP_MS / 1000)} s`,
      };
    reads = { ...reads, [name]: r };
    readAt = nowS();
    paintAll();
  };

  /** @param {string} name */
  const retryOne = async (name) => {
    reads = { ...reads, [name]: { status: "pending" } };
    delete startedAt[name];
    paintAll();
    await readAndPaint(name, true);
  };

  let running = false;
  let again = false;
  /** @param {boolean} force */
  const loadAll = async (force) => {
    if (running) {
      again = true;
      return;
    }
    running = true;
    try {
      const fleet = current().fleet;
      names = (fleet?.stacks ?? [])
        .map((/** @type {any} */ s) => s.name)
        .sort((/** @type {string} */ a, /** @type {string} */ b) =>
          a.localeCompare(b),
        );
      vmids.clear();
      for (const s of fleet?.stacks ?? [])
        if (s.vmid != null) vmids.set(s.name, Number(s.vmid));
      for (const s of [...S.stacks])
        if (fleet && !names.includes(s)) S.stacks.delete(s);
      reads = Object.fromEntries(
        names.map((n) => [n, { status: /** @type {const} */ ("pending") }]),
      );
      for (const n of names) delete startedAt[n];
      if (S.night)
        S.offset = offsetFor(nn().current, S.night, RANGES[S.range].n);
      paintAll();
      if (!fleet) return;
      void loadRetired();
      await Promise.allSettled(names.map((n) => readAndPaint(n, force)));
    } finally {
      running = false;
      if (again && !abort.signal.aborted) {
        again = false;
        void loadAll(false);
      }
    }
  };

  const loadRetired = async () => {
    const r = await fetchJson(
      "/data/retired",
      "the retired list",
      abort.signal,
    ).catch(() => null);
    if (abort.signal.aborted || !r) return;
    if (r.ok) {
      retired = r.body?.retired ?? [];
      retiredError = null;
    } else retiredError = r.error.why;
    paintRetired();
    paintKpis();
  };

  // fix-204: one read at a time; a store notice reloads only when the set
  // of stacks itself changed.
  const namesKey = () =>
    (current().fleet?.stacks ?? [])
      .map((/** @type {any} */ s) => s.name)
      .sort()
      .join(",");
  let lastNames = namesKey();
  let lastFailed = fleetFailed();
  const off = subscribe(() => {
    const k = namesKey();
    if (k !== lastNames) {
      lastNames = k;
      void loadAll(false);
    }
    // The fleet read itself failed (or came back): every area says so now.
    const failed = fleetFailed();
    if (failed !== lastFailed) {
      lastFailed = failed;
      paintAll();
    }
  });
  cleanups.push(off);

  // fix-backups-fresh-1: a stack's backup, prune or restore job ended, so
  // its repositories changed; read that stack again (past the host's cache)
  // a moment later, and once more when a slow read finishes.
  const touching = new Set(["backup", "backup-native", "prune", "restore"]);
  /** @type {Set<string>} */
  const stale = new Set();
  /** @type {Set<string>} */
  const rereading = new Set();
  /** @param {string} name */
  const reread = async (name) => {
    if (abort.signal.aborted || !names.includes(name) || rereading.has(name))
      return;
    rereading.add(name);
    try {
      await readAndPaint(name, true);
    } finally {
      rereading.delete(name);
    }
  };
  cleanups.push(
    // fix-wipe-1: "Kept from retired stacks" follows its jobs too.
    listen("action", (j) => {
      if (changesRetired(j)) void loadRetired();
    }),
    listen("action", (j) => {
      if (!j || !touching.has(j.action)) return;
      if (j.state !== "done" && j.state !== "failed") return;
      stale.add(j.stack);
      setTimeout(() => void reread(j.stack), 1500);
    }),
    listen("slow_read", () => {
      for (const n of [...stale]) {
        stale.delete(n);
        setTimeout(() => void reread(n), 500);
      }
    }),
  );

  // `/` (the dashboard's search key) focuses the filter; Esc unpins the night
  // when nothing else has it (a dialog, a field: kp's engine leaves those).
  cleanups.push(
    registerShortcuts("page:backups", [
      {
        keys: "Escape",
        what: "Unpin the chosen night",
        group: GROUP_BACKUPS,
        when: () => S.night != null,
        reason: "No night is pinned",
        run: unlessDialog(() => {
          if (!S.night) return false;
          pin(null);
          return undefined;
        }),
      },
    ]),
  );

  ret.el.addEventListener("toggle", () => {
    if (/** @type {HTMLDetailsElement} */ (ret.el).open) paintRetired();
  });
  paintRetired();
  if (fromUrl.section === "coverage") cov.el.scrollIntoView({ block: "start" });
  if (fromUrl.section === "removed") ret.el.scrollIntoView({ block: "start" });

  // The legacy address of the Drill filter (`?drills=never`) still applies.
  if (params.get("drills") === "never")
    table()?.filter(COL.drill, ["never drilled"]);
  const legacyQ = params.get("q");
  if (legacyQ) table()?.query(legacyQ);

  void loadAll(false);
  return () => {
    abort.abort();
    for (const c of cleanups) c();
  };
}

/** This month, in the viewer's zone, for the calendar's first sight. */
function monthOfNow() {
  const n = nightsNow(nowS()).current;
  return { year: Number(n.slice(0, 4)), month: Number(n.slice(5, 7)) };
}

/** @param {number} bytes */
const sizeTextOf = (bytes) => (bytes <= 0 ? "0 B" : sizeText(bytes));

/**
 * The moment the nightly round is planned on a night: 03:00 in Brussels,
 * found by trying summer and winter time (format.js owns the zone).
 * @param {string} iso YYYY-MM-DD
 */
function plannedMs(iso) {
  for (const off of ["+02:00", "+01:00"]) {
    const ms = Date.parse(`${iso}T${PLANNED_AT}:00${off}`);
    if (formatClock(ms / 1000) === PLANNED_AT) return ms;
  }
  return Date.parse(`${iso}T${PLANNED_AT}:00+01:00`);
}

/**
 * fix-241: "Show a file…" — one file of one snapshot of this app,
 * read-only.
 * @param {string} stack
 * @param {Repo} r
 * @param {string} [pick] the snapshot to start on
 */
function openFileDialog(stack, r, pick) {
  const now = Math.floor(Date.now() / 1000);
  const id = `${stack}-${r.owner}`;
  const snap = /** @type {HTMLSelectElement} */ (
    h("select", {
      class: "kp-field__input",
      id: fieldId(SNAPFILE, `${id}-snap`),
    })
  );
  snap.append(
    ...snapshotPickerRows(r.snapshots ?? [], now).map((row) =>
      h(
        "option",
        { value: row.value },
        `${row.shortId} · ${row.when} · ${row.ago}${row.latest ? " (latest)" : ""}`,
      ),
    ),
  );
  if (pick) {
    const row = snapshotPickerRows(r.snapshots ?? [], now).find(
      (x) => x.shortId === pick,
    );
    if (row) snap.value = row.value;
  }
  const path = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-field__input",
      type: "text",
      id: fieldId(SNAPFILE, `${id}-path`),
      autocomplete: "off",
      spellcheck: "false",
      placeholder: "config/settings.xml",
      "aria-describedby": `${id}-path-hint`,
    })
  );
  const show = h(
    "button",
    { class: "kp-button kp-button--primary", type: "submit" },
    "Show the file",
  );
  const status = h(
    "p",
    { class: "snapfile__status", role: "status", "aria-live": "polite" },
    "Pick a snapshot, type a path, then Show the file.",
  );
  const out = h("pre", { class: "snapfile__out mono", tabindex: "0" });
  const form = h(
    "form",
    { class: "snapfile" },
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: `${id}-snap` }, "Snapshot"),
      snap,
    ),
    h(
      "div",
      { class: "kp-field" },
      h("label", { class: "kp-field__label", for: `${id}-path` }, "File"),
      path,
      h(
        "p",
        { class: "kp-field__help", id: `${id}-path-hint` },
        "Relative to this app's backed-up directory, or the absolute path under it.",
      ),
    ),
    h(
      "div",
      { class: "snapfile__go" },
      show,
      h(
        "span",
        { class: "kp-field__help" },
        "Reads the file from the backup on the host; nothing is restored or written.",
      ),
    ),
    status,
    out,
  );
  /** @type {AbortController | null} */
  let reading = null;
  form.addEventListener("submit", (ev) => {
    ev.preventDefault();
    const typed = path.value.trim();
    if (!typed) {
      status.textContent = "Type the path of the file first.";
      return;
    }
    reading?.abort();
    const abort = new AbortController();
    reading = abort;
    status.textContent = `Reading ${typed} from the snapshot… (up to 30 s; the backup lives on the remote)`;
    out.textContent = "";
    void fetchJson(
      snapshotFileUrl(stack, r.owner, snap.value, typed),
      "the file from the snapshot",
      abort.signal,
    )
      .then((res) => {
        if (abort.signal.aborted) return;
        if (!res.ok) {
          status.textContent = res.error.fix
            ? `Could not read it: ${res.error.why} — ${res.error.fix}`
            : `Could not read it: ${res.error.why}`;
          return;
        }
        const v = snapshotFileView(/** @type {any} */ (res.body));
        status.textContent = v.notes.join(" ");
        status.dataset.tone = v.tone;
        out.textContent = v.text;
      })
      .catch((e) => {
        if (!abort.signal.aborted)
          status.textContent = `Could not read it: ${String(e)}`;
      });
  });
  const d = openDialog({
    title: `A file from ${r.owner}'s backup`,
    description: `Shows one file of ${stack}/${r.owner} as the chosen snapshot holds it (up to 1 MiB, and says so when cut), without restoring anything.`,
    body: [form],
    wide: true,
    id,
  });
  void d.closed.then(() => reading?.abort());
  path.focus();
}
