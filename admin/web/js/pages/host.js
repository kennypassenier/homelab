// The Host page (redesign-host, release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/host.html, implemented
// exactly). It lives under System in the new navigation. Top to bottom:
// the header (live "Reachable · N ms", Run host checks as the primary
// action, Open the console and Host log beside it, the rest in `···`), the
// KPI strip, then two columns — Containers, Host actions grouped by intent
// and Disk on the left (8/12), Connection, About, Templates and Host checks
// on the right (4/12) — and last host.toml as a folded card that shows
// only what the file changes.
//
// Data: the fleet store (load, disk detail, stacks' measured use),
// /data/host/guests (pct list, every 30 s), /data/ping (every 30 s: the
// chip and the latency strip), /data/templates, /data/host-settings,
// /data/disk-growth (the growth line, when Prometheus answers),
// /data/doctor on request, and the action catalog. redesign-host-4: the
// host sends how full the thin pool is, what the guests are promised on
// it, every guest's memory and CPU, its own uptime, the root disk's kind
// and whether its daemon is a signed release; a host older than 3.71.0
// does not, and the page says so instead of inventing a number.

import { act, catalogReady, onAct } from "../act.js";
import { openAction } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { STANDING_STACK } from "../checks.js";
import { doctorRows } from "../doctor.js";
import { mountWhenCheck } from "../whencheck.js";
import { fetchJson, h, labelCells, slowReport, tableBlock } from "../dom.js";
import { declare, declareField, drivable, viaForm } from "../drivable.js";
import { humanDuration } from "../format.js";
import {
  actionBlurb,
  daemonSignature,
  diskBreakdown,
  gib,
  guestTable,
  guestVisible,
  HOST_FIGURES,
  hostActionGroups,
  hostChecks,
  hostSettingsView,
  hostTile,
  hostUptime,
  rootGrowth,
  rootVolumeLine,
  settingVisible,
  versionText,
} from "../host.js";
import { finished } from "../jobs.js";
import { mountJobPanel } from "../jobpanel.js";
import { openRollback } from "../rollbackdialog.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import {
  countOf,
  attentionBand,
  keyRow,
  dot,
  moreMenu,
  pageHeader,
  section,
  segSwitch,
  skeleton,
  swatch,
  toggleGroup,
} from "../ui.js";
import {
  attachTrendCharts,
  setTrendData,
  runFlashButton,
  attachPatterns,
  REFRESH_EVENT,
  attachRefresh,
  refreshOf,
  attachSegmented,
  SEGMENTED_EVENT,
  VIEW_EVENT,
  attachDataTables,
  dataTable as tableOf,
  attachBarCells,
  setBarCell,
  attachStackbars,
  setStackbar,
  attachUptimes,
  setUptime,
  ENTITY_ACTION_EVENT,
  ENTITY_GRID_RETRY_EVENT,
  attachEntityCards,
  setEntityGrid,
  setEntityGridState,
  BULLETS_PICK_EVENT,
  BULLETS_RETRY_EVENT,
  attachBullets,
  setBullets,
  attachStatusLists,
  buildStatusList,
  countStatus,
  CARD_RETRY_EVENT,
  attachCardStates,
  setCardState,
  attachFootprints,
  attachHoverCards,
  registerShortcuts,
} from "/static/kp/dist/kp-themes.min.js";
import { periodsOf, shareText, uptimeQuery } from "../uptimeview.js";
import { GROUP_HOST, unlessDialog } from "../shortcuts.js";
import { gridRow, formGrid } from "../gridrow.js";
import {
  setBulletsQuiet,
  setEntityGridQuiet,
  syncNode,
  syncRows,
} from "../inplace.js";
import { setParams } from "../urlstate.js";
import { REFRESH_S, changeGate } from "../quiet.js";

/** Seconds between two readings of the container list and the ping. */
const EVERY_S = REFRESH_S;
/** How many pings the latency strip keeps. */
const PINGS = 12;
/** Seconds between two readings of the host's trends (1-minute data). */
const TREND_EVERY_S = 60;

// Live view (invariant 39): every control on this page that runs
// something or opens a menu is declared here; the action tiles and
// "Build a template" reach their dialogs as catalog forms.
// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const KPI_CHART = declare({
  id: "host-kpi-chart",
  page: "host",
  opens: "view",
  row: "<figure>",
  what: "open one of the host's figures on Charts (CPU, memory, load, root disk, disk traffic, network, CPU temperature, swap)",
});
const GUEST_SEARCH = declareField({
  id: "host-guest-search",
  page: "host",
  what: "search the containers by name",
});

const RUN_CHECKS = declare({
  id: "run-host-checks",
  page: "host",
  opens: "run",
  what: "read the doctor's host-level checks now (about 30 s)",
  // kit round (chassis-rs 3.9.0): the Host checks card's own Run checks was a
  // second button for the same read; the header's one button took its place.
  was: ["read-host-checks"],
});
const CHECK_CONFIRM = declare({
  id: "host-check-confirm",
  page: "host",
  opens: "run",
  row: "<check id>",
  what: "keep today's date as the last time the stack-less check (the restic password chain) was confirmed (answer-check with ok); nothing else changes",
  shows: 'on the Host page\'s "When something goes wrong, check this" list',
});
const PING = declare({
  id: "ping-host",
  page: "host",
  opens: "run",
  what: "send one ping over the pinned line and time the answer",
});
const COPY_PIN = declare({
  id: "copy-host-pin",
  page: "host",
  opens: "run",
  what: "copy the pinned certificate fingerprint",
});
const MORE = declare({
  id: "host-more",
  page: "host",
  opens: "run",
  what: "open the header's menu: presets and the runbook",
});
const GUEST_FILTER = declare({
  id: "container-filter",
  page: "host",
  opens: "run",
  row: "all|running|stopped",
  what: "turn one status of the Containers table on or off (all: every row)",
});
const SETTINGS_VIEW = declare({
  id: "host-settings-view",
  page: "host",
  opens: "run",
  row: "changed|all",
  what: "show only the host settings host.toml changes, or all of them",
});
const SORT_GUESTS = declare({
  id: "sort-host-containers",
  page: "host",
  opens: "view",
  row: "id|container|status|memory|cpu|stack",
  what: "sort the Containers table: each header clicked is a further sort key (again reverses it, a third time takes it out)",
});
const SORT_SETTINGS = declare({
  id: "sort-host-settings",
  page: "host",
  opens: "view",
  row: "setting|group|value|default",
  what: "sort the host settings table: each header clicked is a further sort key (again reverses it, a third time takes it out)",
});

/**
 * fix-371-1: one Host tile, a link to its figure on Charts: label, the
 * big number, "now · peak · context", and the 24 h trend, which answers a
 * pointer with its moment and value. kp-themes' trend tile
 * (`.kp-kpi--trend`: the whole tile is its `.kp-kpi__link`, the trend is
 * the time chart's spark variant with the axis and the reading chip, drawn
 * by `attachTrendCharts`). Loading until `set` gives it a tile.
 * @param {import("../host.js").HostFigure} fig
 */
function trendTile(fig) {
  const label = h("span", { class: "kp-kpi__label" }, fig.label);
  const value = h("span", { class: "kp-kpi__value" }, skeleton("3ch"));
  const ctx = h("span", { class: "kp-kpi__trend" }, skeleton("80%"));
  const figure = h("figure", {
    class: "kp-kpi__chart",
    "data-kp-chart": "spark",
    "data-kp-spark-head": "none",
    "data-kp-spark-axis": "relative",
    "data-kp-chart-loading": "",
    "aria-label": `${fig.label}, last 24 hours`,
    style: `--kp-chart-series: var(--chart-${fig.colour})`,
  });
  const link = drivable(
    h(
      "a",
      {
        class: "kp-kpi__link",
        href: fig.chart ? `/charts#chart-${fig.chart}` : "/charts",
        title: "Open this figure on Charts",
      },
      h("span", { class: "kp-kpi__link-word" }, "Charts"),
      " ↗",
    ),
    KPI_CHART,
    fig.key,
  );
  const el = h(
    "div",
    {
      class: "kp-kpi kp-kpi--trend hx-kpi",
      "data-key": fig.key,
      "data-loading": "",
    },
    label,
    link,
    value,
    ctx,
    figure,
  );
  return {
    fig,
    el,
    /** @param {import("../host.js").HostTile} t */
    set: (t) => {
      delete el.dataset.loading;
      el.title = t.title;
      label.textContent = t.label;
      value.replaceChildren(
        t.value,
        ...(t.unit ? [h("small", null, t.unit)] : []),
      );
      ctx.replaceChildren(
        ...(t.now ? ["now ", h("b", null, t.now), " · "] : []),
        t.rest,
      );
      // The points are unix seconds (kp's timeUnit).
      setTrendData(figure, {
        timeUnit: "s",
        points: t.points,
        step: 600,
        // The reading chip and tooltip print the figure's own unit.
        ...(fig.rate
          ? { unitKind: "bytes/s" }
          : fig.unit === "%"
            ? { unitKind: "percent" }
            : { unit: fig.unit, digits: fig.digits ?? 1 }),
      });
      attachTrendCharts(document);
    },
  };
}

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void, focus?: "checks"}} ctx `focus`: the card to open on (redesign-final-h5)
 * @returns {() => void}
 */
export function mount(root, ctx) {
  // `nx-ops`: the shared blocks in the ops-kit look the Host demo uses.
  root.classList.add("hk-page", "nx-ops");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];

  // ── header ────────────────────────────────────────────────────────────
  const reach = h(
    "span",
    { class: "hk-chip", id: "host-reach" },
    dot(""),
    "Checking the line…",
  );
  const daemon = h("span", { id: "host-daemon" });
  const runChecks = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        title: "Read the doctor's host-level checks (about 30 s)",
        onclick: () => {
          checksCard.el.scrollIntoView({ block: "nearest" });
          void loadChecks().catch(() => {});
        },
      },
      "Run host checks",
    ),
    RUN_CHECKS,
  );
  const menu = moreMenu({
    label: "More: presets, download the runbook",
    items: [
      {
        label: "Presets",
        hint: "The stack recipes a new stack starts from",
        href: "/presets",
      },
      {
        label: "Download the runbook",
        hint: "The disaster-recovery runbook, as Markdown",
        href: "/data/download/runbook",
        download: "DR_RUNBOOK.md",
      },
    ],
    drive: { id: MORE },
  });
  stops.push(menu.stop);
  const head = pageHeader({
    title: "Host",
    sub: current().fleet?.host.name ?? "",
    desc: "The Proxmox machine every stack runs on: how loaded it is, what runs on it, and the actions that act on the host itself.",
    meta: [reach, daemon],
    live: "measured",
    actions: [
      h(
        "a",
        {
          class: "kp-button",
          href: "/console",
          title: "Open a root shell on the host in this browser",
        },
        "Open the console",
      ),
      h(
        "a",
        {
          class: "kp-button",
          href: "/activity?view=host-log",
          title: "Follow the host daemon's own log, live",
        },
        "Host log",
      ),
    ],
    primary: runChecks,
    more: menu.el,
  });
  const sub = /** @type {HTMLElement | null} */ (
    head.title.querySelector(".nx-head-sub")
  );

  const attention = attentionBand();
  // fix-371-1 (Kenny approved the Host tiles demo 2026-10-04:
  // redesign-3.71/demos-3711/host-kpis.html): eight tiles, each with its
  // 24 h sparkline, the 15-minute average as the big number ("avg 15
  // min"), now and the day's peak under it, and a link to the figure on
  // Charts. Drawn in their final size from the first frame.
  /** @type {Map<string, import("../host.js").TrendFigure> | null} */
  let trend = null;
  /** @type {string | null} why the trends could not be read */
  let trendWhy = null;
  const tiles = HOST_FIGURES.map((f) => trendTile(f));
  const kpisEl = h(
    "div",
    {
      class: "kp-kpis hx-kpis",
      "data-kp-kpis-columns": "4 2 1",
      role: "group",
      "aria-label": "The host's figures",
    },
    ...tiles.map((t) => t.el),
  );

  // ── Containers ────────────────────────────────────────────────────────
  // The demo's three views of the same guests (feat-kp11demo-11): a table
  // you can sort, filter and open, cards of one height with two meters, and
  // the pressure list that puts the guest closest to its memory limit first.
  /** @type {import("../host.js").Guest[] | null} */
  let guests = null;
  /** @type {string | null} */
  let guestsWhy = null;
  const q0 = new URLSearchParams(location.search);
  /** @type {"table" | "cards" | "pressure"} */
  let gView = ["table", "cards", "pressure"].includes(
    q0.get("containers") ?? "",
  )
    ? /** @type {any} */ (q0.get("containers"))
    : "table";
  let gShow = new Set();
  const gFilter = toggleGroup({
    label: "Status",
    all: { label: "All", hint: "Every container, running or not" },
    chips: [
      {
        value: "running",
        label: "Running",
        hint: "Click to show or hide running containers",
      },
      {
        value: "stopped",
        label: "Stopped",
        hint: "Click to show or hide stopped containers",
      },
    ],
    onChange: (on) => {
      gShow = on;
      gDt()?.filter(3, on.size ? [...on] : null);
    },
    drive: { id: GUEST_FILTER },
  });
  const gViewItem = (
    /** @type {"table" | "cards" | "pressure"} */ v,
    /** @type {string} */ label,
  ) =>
    h(
      "button",
      {
        type: "button",
        class: "kp-segmented__item",
        role: "radio",
        "aria-checked": String(gView === v),
        "data-value": v,
      },
      label,
    );
  const gViewSeg = h(
    "div",
    {
      class: "kp-segmented",
      "data-kp-segmented": "single",
      role: "radiogroup",
      "aria-label": "View of the containers",
    },
    gViewItem("table", "Table"),
    gViewItem("cards", "Cards"),
    gViewItem("pressure", "Pressure"),
  );
  const gRead = h("span", { class: "hk-muted", role: "status" });
  const gTbody = h("tbody", { id: "host-guests" });
  const th = (
    /** @type {string} */ label,
    /** @type {Record<string, string>} */ a = {},
  ) => h("th", { scope: "col", ...a }, label);
  const gTableEl = h(
    "div",
    {
      class: "kp-datatable hk-dt",
      id: "host-containers-table",
      "data-kp-datatable": "",
      "data-kp-cards": "",
      "data-kp-expandable": "",
      "data-kp-page-size": "100",
      "data-kp-page-sizes": "none",
      "data-kp-sort-multi": "click",
      "data-kp-remember": "host-containers",
      "data-kp-busy-overlay": "",
      "data-kp-state": "loading",
    },
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("input", {
        class: "kp-datatable__search",
        type: "search",
        id: GUEST_SEARCH,
        "data-kp-datatable-search": "",
        placeholder: "Search the containers   /",
        "aria-label": "Search the containers (press / to jump here)",
      }),
    ),
    // The failed plate is the package's own: fail(reason) draws the reason
    // at heading size with its Retry, filling the table's footprint.
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h("caption", { class: "kp-sr-only" }, "Guests on the host"),
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            th("Guest", { "data-kp-sort": "number" }),
            th("Stack", { "data-kp-sort": "text" }),
            th("State", { "data-kp-sort": "text", "data-kp-filter": "choice" }),
            th("CPU", { "data-kp-sort": "number" }),
            th("Memory", { "data-kp-sort": "number" }),
            th("Lock", { "data-kp-sort": "text" }),
            th("Kind", { "data-kp-sort": "text", "data-kp-filter": "choice" }),
          ),
        ),
        gTbody,
      ),
    ),
    h(
      "div",
      { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
      h(
        "div",
        { "data-kp-datatable-empty-none": "" },
        h("p", { class: "kp-empty__title" }, "No guests on this host"),
        h("p", null, "Create a stack and its container shows here."),
      ),
      h(
        "div",
        { "data-kp-datatable-empty-nomatch": "", hidden: "" },
        h("p", { class: "kp-empty__title" }, "No guest matches"),
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--secondary",
            "data-kp-datatable-clear": "",
          },
          "Clear the search and filters",
        ),
      ),
    ),
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("p", {
        class: "kp-datatable__status",
        "data-kp-datatable-status": "",
        role: "status",
        "aria-live": "polite",
      }),
      h("div", { class: "kp-datatable__pager", "data-kp-datatable-pager": "" }),
    ),
  );
  const gCards = h("div", {
    class: "kp-entity-grid",
    "data-kp-entity-grid": "",
    "data-kp-entity-link": "/stacks/{key}",
    role: "list",
  });
  const gBullets = h("div", { class: "kp-bullets", "data-kp-bullets": "" });
  const gViews = h("div", { class: "hk-ho-view" }, gTableEl, gCards, gBullets);
  const gDt = () => tableOf(gTableEl);
  const guestsAgo = agoEl("read");
  const guestsCard = section({
    id: "host-containers",
    title: "Containers",
    desc: "Every container and VM on the host, managed by this orchestrator or not: as a table, as cards with meters, or as a pressure list.",
    foot: [`Read with pct list every ${EVERY_S} s`, guestsAgo],
  });
  guestsCard.body.append(
    formGrid(
      [
        gridRow({
          label: "View",
          hideLabel: true,
          control: gViewSeg,
          value: gRead,
        }),
        gridRow({ label: "Status", hideLabel: true, control: gFilter.el }),
      ],
      "hk-toolbar",
    ),
    gViews,
  );
  /** @param {number | null} used @param {number | null} max */
  const share = (used, max) => (used != null && max ? used / max : null);
  /** @param {ReturnType<typeof guestTable>[number]} g */
  function guestDetail(g) {
    return formGrid([
      gridRow({
        label: g.name || `CT ${g.vmid}`,
        control: h(
          "p",
          { class: "hk-detail" },
          `${g.name || `CT ${g.vmid}`} is ${g.status}${g.lock ? `, locked by ${g.lock}` : ""}. ${g.ramUsed == null ? "It is not running, so it has no CPU or memory figures." : `It uses ${gib(g.ramUsed)} of its ${gib(g.ramMax ?? 0)} GiB and ${g.cpuPct ?? 0} % of one core.`}`,
        ),
        actions: [
          g.stack
            ? h(
                "a",
                {
                  class: "kp-button kp-button--sm",
                  href: stackHref(g.stack),
                },
                "Open stack",
              )
            : h(
                "button",
                {
                  type: "button",
                  class: "kp-button kp-button--sm",
                  disabled: "",
                  "data-kp-disabled-reason": "This guest is not a stack",
                },
                "Open stack",
              ),
          h(
            "a",
            {
              class: "kp-button kp-button--sm",
              href: `/activity?view=host-log${g.stack ? `&source=${encodeURIComponent(g.stack)}` : ""}`,
            },
            "Open its log",
          ),
        ],
        buttons: true,
      }),
    ]);
  }

  // The table leaves its loading state once; later refreshes keep it ready.
  let guestsReady = false;
  const paintGuests = () => {
    const dt = gDt();
    const showFor = (/** @type {typeof gView} */ v) => {
      gTableEl.hidden = v !== "table";
      gCards.hidden = v !== "cards";
      gBullets.hidden = v !== "pressure";
    };
    showFor(gView);
    if (!guests) {
      guestsReady = false;
      if (guestsWhy) {
        dt?.busy(null);
        dt?.fail(guestsWhy);
        setEntityGridState(gCards, "error", { message: guestsWhy });
        setBullets(gBullets, {
          threshold: 0.75,
          rows: [],
          state: "error",
          message: guestsWhy,
        });
      } else {
        dt?.busy({
          text: "Asking the host for its containers (pct list)…",
          overlay: true,
        });
        dt?.state("loading");
        setEntityGridState(gCards, "loading");
        setBullets(gBullets, {
          threshold: 0.75,
          rows: [],
          state: "loading",
          loadingWords: "Reading the guests…",
        });
      }
      return;
    }
    const view = guestTable(guests, current().fleet);
    const running = view.filter((r) => r.running).length;
    gFilter.counts({
      all: view.length,
      running,
      stopped: view.length - running,
    });
    // the table
    // refresh-no-replay-1: the rows are built, filled and then merged into
    // the rows already there, so an opened row stays open and no bar wipes in
    // again; only a new guest arrives.
    syncRows(
      gTbody,
      view.map((g) => {
        const tr = h(
          "tr",
          {
            "data-kp-row-key": String(g.vmid),
            "data-vmid": g.vmid,
            "data-status": g.running ? "running" : "stopped",
          },
          h(
            "td",
            { "data-label": "Guest", "data-kp-sort-value": String(g.vmid) },
            h("b", null, g.name || `CT ${g.vmid}`),
            " ",
            h(
              "span",
              { class: "hk-ct" },
              `${g.kind === "unmanaged" && g.vmid <= 101 ? "VM" : "CT"} ${g.vmid}`,
            ),
          ),
          h(
            "td",
            { "data-label": "Stack" },
            g.stack
              ? h("a", { class: "hk-link", href: stackHref(g.stack) }, g.stack)
              : h("span", { class: "hk-muted" }, "—"),
          ),
          h(
            "td",
            {
              "data-label": "State",
              "data-kp-filter-value": g.running ? "running" : "stopped",
            },
            h(
              "span",
              {
                class: `kp-badge${g.running ? " kp-badge--success" : g.tone === "bad" ? " kp-badge--destructive" : ""}`,
              },
              g.status,
            ),
          ),
          h(
            "td",
            { "data-label": "CPU" },
            h("span", {
              class: "kp-bar-cell",
              "data-kp-bar": "",
              "data-hk-cpu": String(g.vmid),
            }),
          ),
          h(
            "td",
            { "data-label": "Memory" },
            h("span", {
              class: "kp-bar-cell",
              "data-kp-bar": "",
              "data-hk-mem": String(g.vmid),
            }),
          ),
          h("td", { "data-label": "Lock" }, g.lock || "—"),
          h(
            "td",
            { "data-label": "Kind", "data-kp-filter-value": g.kind },
            g.kind === "unmanaged" ? "not managed" : g.kind,
          ),
        );
        const s = share(g.ramUsed, g.ramMax);
        const cpu = /** @type {HTMLElement} */ (
          tr.querySelector("[data-hk-cpu]")
        );
        const mem = /** @type {HTMLElement} */ (
          tr.querySelector("[data-hk-mem]")
        );
        setBarCell(cpu, {
          value: g.cpuPct == null ? null : g.cpuPct / 100,
          text: g.cpuPct == null ? "—" : `${g.cpuPct} %`,
          sortValue: g.cpuPct ?? -1,
          label: "of one core",
        });
        setBarCell(mem, {
          value: s,
          text:
            g.ramUsed == null || g.ramMax == null
              ? "—"
              : `${gib(g.ramUsed)} of ${gib(g.ramMax)} GiB`,
          sortValue: s ?? -1,
          warnAt: 0.75,
          badAt: 0.9,
          label: "of its memory",
        });
        return tr;
      }),
    );
    // An opened row's sentence says what the guest uses now.
    for (const open of /** @type {NodeListOf<HTMLTableRowElement>} */ (
      gTbody.querySelectorAll(":scope > tr[data-kp-row-detail]")
    )) {
      const key = open.previousElementSibling?.getAttribute("data-kp-row-key");
      const g = view.find((x) => String(x.vmid) === key);
      const mine = open.cells[0]?.firstElementChild;
      if (g && mine) syncNode(mine, guestDetail(g));
    }
    dt?.refresh();
    if (!guestsReady) {
      dt?.busy(null);
      dt?.state("ready");
      guestsReady = true;
    }
    const th2 = /** @type {HTMLElement[]} */ ([
      ...gTableEl.querySelectorAll("thead th[data-kp-sort]"),
    ]);
    ["id", "stack", "status", "cpu", "memory"].forEach((k, i) => {
      if (th2[i]) drivable(th2[i], SORT_GUESTS, k);
    });
    // the cards
    setEntityGridQuiet(
      gCards,
      view.map((g) => ({
        key: String(g.vmid),
        name: g.name || `CT ${g.vmid}`,
        vmid: String(g.vmid),
        state: {
          label: g.status,
          tone: g.running
            ? "success"
            : g.tone === "bad"
              ? "destructive"
              : undefined,
        },
        figures: [
          {
            value: g.stack ?? (g.kind === "template" ? "template" : "none"),
            label: "stack",
          },
          { value: g.lock || "none", label: "lock" },
        ],
        flags:
          g.kind === "unmanaged"
            ? [{ label: "not managed", tone: "warning" }]
            : [],
        meters: [
          { label: "CPU", value: g.cpuPct == null ? null : g.cpuPct / 100 },
          {
            label: "RAM",
            value: share(g.ramUsed, g.ramMax),
            text:
              g.ramUsed == null || g.ramMax == null
                ? undefined
                : `${gib(g.ramUsed)} of ${gib(g.ramMax)} GiB`,
          },
        ],
        actions: [
          { id: "log", label: "Open its log" },
          {
            id: "stack",
            label: "Open stack",
            disabled: g.stack ? undefined : "This guest is not a stack",
          },
        ],
      })),
    );
    // the pressure list
    const withRam = view.filter((g) => g.ramUsed != null && g.ramMax);
    setBulletsQuiet(gBullets, {
      threshold: 0.75,
      bad: 0.9,
      max: 1,
      format: (v) => `${Math.round(v * 100)} %`,
      rows: withRam.map((g) => ({
        key: String(g.vmid),
        lead: `CT ${g.vmid}`,
        label: g.name || `CT ${g.vmid}`,
        detail: `${gib(g.ramUsed ?? 0)} of ${gib(g.ramMax ?? 0)} GiB`,
        value: share(g.ramUsed, g.ramMax),
        pick: true,
      })),
      state: "ready",
      emptyTitle: "No guest is running",
      emptyText: "A running guest shows its memory share here.",
      loadingRows: 8,
    });
    gRead.textContent =
      gView === "pressure"
        ? `Memory share of ${withRam.length.toLocaleString("en-GB")} running guests, worst first; amber from 75 %.`
        : `${view.length.toLocaleString("en-GB")} guests: ${running.toLocaleString("en-GB")} running.`;
  };
  gViewSeg.addEventListener(SEGMENTED_EVENT, (e) => {
    const v = /** @type {CustomEvent} */ (e).detail.value;
    gView = v === "cards" || v === "pressure" ? v : "table";
    history.replaceState(
      history.state,
      "",
      location.pathname +
        setParams(location.search, {
          containers: gView === "table" ? null : gView,
        }),
    );
    paintGuests();
  });
  gViews.addEventListener(ENTITY_ACTION_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    const g =
      guests &&
      guestTable(guests, current().fleet).find(
        (x) => String(x.vmid) === String(d.key),
      );
    if (!g) return;
    if (d.id === "stack" && g.stack) ctx.navigate(stackHref(g.stack));
    else ctx.navigate("/activity?view=host-log");
  });
  gViews.addEventListener(BULLETS_PICK_EVENT, (e) => {
    const key = /** @type {CustomEvent} */ (e).detail.key;
    gViewSeg
      .querySelector('[data-value="table"]')
      ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    gDt()?.expand([String(key)]);
  });
  for (const ev of [
    "kp-datatable-retry",
    ENTITY_GRID_RETRY_EVENT,
    BULLETS_RETRY_EVENT,
  ])
    gViews.addEventListener(ev, () => void loadGuests().catch(() => {}));
  const guestsGate = changeGate();
  const loadGuests = async () => {
    const r = await fetchJson(
      "/data/host/guests",
      "the host's containers",
      abort.signal,
    );
    if (!r.ok) {
      guestsWhy = `${r.error.why}${r.error.fix ? ` — ${r.error.fix}` : ""}`;
      if (!guests) paintGuests();
      return false;
    }
    guestsWhy = null;
    setAgo(guestsAgo, r.body.measured_at);
    // refresh-one-minute-1: the same containers draw nothing again.
    if (!guestsGate.isNew(r.body.guests ?? []) && guests) return true;
    guests = r.body.guests ?? [];
    paintGuests();
    paintKpis();
    return true;
  };

  // ── Host actions ──────────────────────────────────────────────────────
  const groupsBox = h(
    "div",
    { class: "hk-act-groups", id: "host-actions" },
    h("p", { class: "hk-muted", role: "status" }, "Reading the actions…"),
  );
  const runningBox = h("div", { class: "hk-running" });
  const actionsCard = section({
    id: "host-actions-card",
    title: "Host actions",
    desc: "What the dashboard can do to the host itself; each opens a dialog that says what will happen first.",
    foot: [
      "Every action becomes a job you can follow on Activity",
      h("a", { class: "hk-link", href: "/activity" }, "Open Activity"),
    ],
  });
  actionsCard.el.classList.add("actions-area");
  actionsCard.body.append(groupsBox, runningBox);
  void catalogReady().then((catalog) => {
    if (abort.signal.aborted) return;
    if (!catalog) {
      groupsBox.replaceChildren(
        h(
          "p",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          "The dashboard did not send its action catalog; reload the page.",
        ),
      );
      return;
    }
    const entries = catalog.actions.filter((a) => a.target === "host");
    groupsBox.replaceChildren(
      ...hostActionGroups(entries).map((g) =>
        h(
          "div",
          { class: "hk-act-group", "data-group": g.title },
          h(
            "h3",
            null,
            g.title,
            g.full
              ? h(
                  "span",
                  { class: "hk-lock", title: "Needs the full-access token" },
                  "full access",
                )
              : null,
          ),
          g.actions.map((a) =>
            h(
              "button",
              {
                type: "button",
                class: `hk-act${a.destructive ? " hk-act--destructive" : ""}`,
                "data-action": a.action,
                title: `${a.label}: opens a dialog that says what will happen, then starts it`,
                onclick: () =>
                  void openAction(catalog.host_target, a.action, {
                    openRollback,
                  }),
              },
              h("b", null, a.label),
              h("span", null, actionBlurb(a.what)),
              h("em", { "aria-hidden": "true" }, "›"),
            ),
          ),
        ),
      ),
    );
  });
  /** @type {Map<number, () => void>} */
  const panels = new Map();
  const paintRunning = () => {
    const live = act.jobs.filter(
      (j) => j.stack === "_host" && !finished(j.state),
    );
    for (const j of live)
      if (!panels.has(j.job)) {
        const p = mountJobPanel(j.job, { framed: true });
        panels.set(j.job, p.stop);
        runningBox.prepend(p.element);
      }
  };
  stops.push(onAct("jobs", paintRunning));
  paintRunning();

  // ── Disk ──────────────────────────────────────────────────────────────
  // Two kp stack bars (feat-kp11demo-11): what fills the root volume, and
  // the thin pool: what the guests wrote, and what they were promised on
  // top of it (an amber part, so an over-promise runs the bar past the end).
  const headRow = (
    /** @type {string} */ title,
    /** @type {string} */ sentence,
    /** @type {HTMLElement} */ read,
  ) =>
    h(
      "div",
      { class: "kp-formgrid__row hk-disk__head" },
      h("h3", { class: "kp-formgrid__label" }, title),
      h("p", { class: "kp-formgrid__control" }, sentence),
      read,
    );
  const rootRead = h("p", {
    class: "kp-formgrid__value",
    id: "host-disk-read",
    role: "status",
  });
  const poolRead = h("p", {
    class: "kp-formgrid__value",
    id: "host-pool-read",
    role: "status",
  });
  const rootBar = h("figure", { class: "kp-stackbar kp-stackbar--lg" });
  const poolBar = h("figure", { class: "kp-stackbar kp-stackbar--lg" });
  const diskBody = h(
    "div",
    { id: "host-disk", class: "kp-formgrid hk-disk" },
    headRow("Root volume", "What fills the host's own disk.", rootRead),
    rootBar,
    headRow(
      "Thin pool for the guests",
      "Written by the guests, against what all guests were promised.",
      poolRead,
    ),
    poolBar,
  );
  const growthLine = h("span", { id: "host-growth" }, "Growth: reading");
  const diskAgo = agoEl("read");
  const diskCard = section({
    id: "host-disk-card",
    title: "Disk",
    desc: "What fills the root disk, and the pool every container's own disk is carved from.",
    foot: [growthLine, diskAgo],
  });
  diskCard.body.append(diskBody);
  const gbText = (/** @type {number} */ gb) => `${Math.round(gb)} GB`;
  const paintDisk = () => {
    const f = current().fleet;
    const d = f ? diskBreakdown(f) : null;
    if (!f) {
      for (const bar of [rootBar, poolBar])
        setStackbar(bar, {
          title: "Reading the disks",
          parts: [],
          state: "loading",
        });
      return;
    }
    if (!d) {
      for (const bar of [rootBar, poolBar])
        setStackbar(bar, {
          title: "Disk",
          parts: [],
          state: "empty",
          message: "The host has not read its disks yet.",
        });
      return;
    }
    const used = d.dirs.reduce((a, x) => a + x.gb, 0);
    setStackbar(rootBar, {
      title: "Root volume",
      total: d.rootGb,
      format: gbText,
      restLabel: "Free",
      marks: [{ at: 0.8, label: "80 % warning level" }],
      parts: d.dirs.map((x) => ({ key: x.path, label: x.path, value: x.gb })),
      state: "ready",
      label: "What fills the root volume",
    });
    rootRead.textContent = `${rootVolumeLine(f)}: ${gbText(used)} of ${gbText(d.rootGb)} in the biggest folders, ${d.usedPct}% used.`;
    const t = f.host.disk_detail?.thin_pool;
    if (!d.poolGb || !t) {
      setStackbar(poolBar, {
        title: "Thin pool",
        parts: [],
        state: "empty",
        message: "This host version does not report its thin pool.",
      });
      poolRead.textContent = "Not reported by this host version.";
    } else {
      const written = (d.poolGb * t.data_pct) / 100;
      const unwritten = Math.max(0, t.promised_gb - written);
      setStackbar(poolBar, {
        title: "Thin pool",
        total: d.poolGb,
        format: gbText,
        restLabel: "Free in the pool",
        marks: [],
        parts: [
          { key: "written", label: "Written by the guests", value: written },
          ...(unwritten > 0
            ? [
                {
                  key: "promised",
                  label: "Promised, not written yet",
                  value: unwritten,
                  tone: /** @type {const} */ ("warning"),
                },
              ]
            : []),
        ],
        state: "ready",
        label: "Thin pool: written and promised",
      });
      const over = t.promised_gb - d.poolGb;
      poolRead.textContent = `${gbText(written)} written of ${gbText(d.poolGb)}; ${gbText(t.promised_gb)} promised${over > 0 ? `, ${gbText(over)} more than the pool holds` : ""}.`;
    }
    setAgo(diskAgo, f.host.disk_detail?.measured_at ?? null);
  };
  /** The last /data/disk-growth answer; null until it arrives. */
  /** @type {Awaited<ReturnType<typeof fetchJson>> | null} */
  let growthRes = null;
  /**
   * The growth line needs two answers, the fit and the root volume's size
   * (the fleet read). Whichever arrives last paints it: with the size still
   * unknown, 0 GB a week would read "Root is not growing", which is wrong.
   */
  const paintGrowth = () => {
    const r = growthRes;
    if (!r) return;
    const rootGb = current().fleet?.host.disk_detail?.root_lv_size_gb ?? 0;
    if (r.ok && !(rootGb > 0)) return;
    const line = r.ok ? rootGrowth(r.body.rows ?? [], rootGb) : null;
    growthLine.textContent =
      line ??
      (r.ok
        ? "Growth: not enough history yet to say how fast root fills"
        : `Growth not measured: ${r.error.why}`);
  };
  const loadGrowth = async () => {
    const r = await fetchJson("/data/disk-growth", "disk growth", abort.signal);
    if (abort.signal.aborted) return;
    growthRes = r;
    paintGrowth();
  };

  // ── Connection ────────────────────────────────────────────────────────
  /** @type {(number | null)[]} */
  const pings = [];
  let pingedOnce = false;
  /** @type {any} */
  let lineFacts = null;
  let lineOk = /** @type {boolean | null} */ (null);
  const facts = h("dl", { class: "kp-kv", id: "host-line" });
  const lat = h("div", {
    class: "kp-uptime",
    id: "host-lat",
    "data-kp-uptime": "",
    "data-kp-uptime-count": "12",
    "data-kp-uptime-fit": "scroll",
  });
  // ask-kp11be-2: the host's reachability over the last 24 hours (one cell an
  // hour) and 90 days (one a day), from the dashboard's own capped history.
  /** @param {string} id @param {number} count */
  const reachStrip = (id, count) =>
    h("div", {
      class: "kp-uptime",
      id,
      "data-kp-uptime": "",
      "data-kp-uptime-count": String(count),
      "data-kp-uptime-fit": "scroll",
    });
  const reach24 = reachStrip("host-reach-24", 24);
  const reach90 = reachStrip("host-reach-90", 90);
  const reachFigs = h("p", { class: "hk-muted", id: "host-reach-figs" });
  /** @param {HTMLElement} el @param {string} label @param {"day" | "hour"} unit @param {any} entry @param {string | null} why */
  const paintReach = (el, label, unit, entry, why) => {
    const periods = periodsOf(entry, unit, "reachable");
    setUptime(el, {
      label,
      periods,
      from: unit === "day" ? "90 days ago" : "24 hours ago",
      to: unit === "day" ? "Today" : "Now",
      count: unit === "day" ? 90 : 24,
      open: "info",
      state: why
        ? "error"
        : periods.some((p) => p.state !== "none")
          ? "ready"
          : "empty",
      ...(why ? { text: why } : {}),
      message: "No ping measured yet: the watch pings the host once a minute.",
    });
  };
  /** The strips' own busy card while the history is read. */
  const loadingReach = () => {
    setUptime(reach24, {
      label: "Last 24 hours",
      count: 24,
      state: "loading",
    });
    setUptime(reach90, {
      label: "Last 90 days",
      count: 90,
      state: "loading",
    });
  };
  let reachDrawn = false;
  const loadReach = async () => {
    // refresh-one-minute-1: the strips show their loading card only before
    // the first read; a refresh keeps the drawn strips until new ones land.
    if (!reachDrawn) loadingReach();
    const now = Math.floor(Date.now() / 1000);
    const [d, hr] = await Promise.all([
      fetchJson(
        `/data/uptime?${uptimeQuery("day", now)}`,
        "the host's reachability history",
        abort.signal,
      ),
      fetchJson(
        `/data/uptime?${uptimeQuery("hour", now)}`,
        "the host's reachability history",
        abort.signal,
      ),
    ]);
    if (abort.signal.aborted) return true;
    reachDrawn = true;
    const why = !d.ok
      ? `${d.error.what}: ${d.error.why}`
      : !hr.ok
        ? `${hr.error.what}: ${hr.error.why}`
        : null;
    const day = d.ok ? d.body.keys?.["ping:host"] : undefined;
    const hour = hr.ok ? hr.body.keys?.["ping:host"] : undefined;
    paintReach(reach24, "Last 24 hours", "hour", hour, why);
    paintReach(reach90, "Last 90 days", "day", day, why);
    reachFigs.textContent = why
      ? "Reachability history not read."
      : `Reachable: ${shareText(hour?.uptime, "24 hours")} · ${shareText(day?.uptime, "90 days")}. Pinged once a minute; kept 90 days.`;
    return !why;
  };
  reach24.addEventListener("kp-uptime-retry", () => void loadReach());
  reach90.addEventListener("kp-uptime-retry", () => void loadReach());
  const pingDot = dot("");
  const pingOut = h("span", { id: "host-ping-out" }, "Pinging…");
  const pingBtn = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Send one ping over the pinned line and time the answer",
        onclick: () => void ping().catch(() => {}),
      },
      "Ping again",
    ),
    PING,
  );
  const pingCount = h("span", null, "last pings");
  const lineCard = section({
    id: "host-connection",
    title: "Connection",
    desc: "The pinned TLS line this dashboard holds to the host daemon.",
    foot: [pingBtn, pingCount],
  });
  // The three strips share one parent, so while they load one busy card
  // stands over the three of them (uptime.js groupHost; since kp-themes
  // 11.2.0 the measure counts that card, kp-gaps/homelab.md line 128).
  lineCard.body.append(
    facts,
    h("div", { class: "hk-strips" }, lat, reach24, reach90),
    reachFigs,
    h("p", { class: "hk-ping-out", role: "status" }, pingDot, pingOut),
  );
  /**
   * The pinned fingerprint, shortened, with kp's copy button for the whole.
   * @param {string} pin
   */
  const pinCell = (pin) => {
    const pinCopy = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--ghost kp-button--sm kp-copyable__button",
          "data-kp-flash-words": "Copied,Press Ctrl C",
          "aria-label": "Copy the whole fingerprint",
        },
        "Copy",
      ),
      COPY_PIN,
    );
    pinCopy.addEventListener("click", () => {
      void runFlashButton(pinCopy, async () => {
        try {
          await navigator.clipboard.writeText(pin);
          return "Copied";
        } catch {
          return { error: "Press Ctrl C", word: "Press Ctrl C" };
        }
      });
    });
    const cell = h(
      "span",
      { class: "kp-copyable" },
      h(
        "span",
        { class: "kp-copyable__value mono", title: pin },
        `${pin.slice(0, 20)}…`,
      ),
      pinCopy,
    );
    attachPatterns(cell);
    return cell;
  };
  let lastUptime = "";
  const paintLine = () => {
    const f = current().fleet;
    /** @type {[string, Node | string][]} */
    const rows = [];
    if (lineFacts) {
      rows.push([
        "Address",
        h("span", { class: "mono" }, lineFacts.address ?? "not configured"),
      ]);
      rows.push(["Set in", lineFacts.address_source ?? "—"]);
      rows.push([
        "Pinned",
        lineFacts.pin ? pinCell(lineFacts.pin) : "no pin compiled in",
      ]);
    }
    rows.push([
      "Host says",
      lineOk === true
        ? h("span", { class: "hk-status" }, dot("ok"), "matches the pin")
        : lineOk === false
          ? h(
              "span",
              { class: "hk-status" },
              dot("bad"),
              "no answer over the pinned line",
            )
          : f?.host.tls_fingerprint || "not asked yet",
    ]);
    facts.replaceChildren(
      ...rows.map(([k, v]) =>
        h("div", { class: "kp-kv__row" }, h("dt", null, k), h("dd", null, v)),
      ),
    );
    /** @type {import("/static/kp/dist/kp-themes.min.js").UptimeSpec} */
    const uptimeSpec = {
      label: "Line to the host",
      periods: pings.map((ms, i) => ({
        key: `p${i}`,
        label:
          i === pings.length - 1 ? "Now" : `${pings.length - 1 - i} pings ago`,
        state: ms == null ? "down" : ms > 30 ? "degraded" : "up",
        tip:
          ms == null
            ? "no answer"
            : `answered in ${ms} ms${ms > 30 ? " (slow)" : ""}`,
      })),
      from: `${Math.max(pings.length, 1)} pings ago`,
      to: "Now",
      count: PINGS,
      // Every ping unanswered is a failed read, drawn as the package's
      // failed plate; a mix of answers and silences is the strip itself.
      state: pings.length
        ? pings.every((ms) => ms == null)
          ? "error"
          : "ready"
        : pingedOnce
          ? "empty"
          : "loading",
      error: "The host did not answer over the pinned line.",
      open: "info",
    };
    // The strip is redrawn only when what it shows changed: a store event
    // that moves nothing here must not repaint it (and with it a state the
    // package or a reader has put it in).
    const uptimeKey = JSON.stringify(uptimeSpec);
    if (uptimeKey !== lastUptime) {
      lastUptime = uptimeKey;
      setUptime(lat, uptimeSpec);
    }
    pingCount.textContent = pings.length
      ? `last ${pings.length} ping${pings.length === 1 ? "" : "s"}`
      : "no ping yet";
  };
  lat.addEventListener("kp-uptime-retry", () => void ping().catch(() => {}));
  /** @param {boolean} [quiet] a background refresh: no "Pinging…" flash */
  const ping = async (quiet = false) => {
    const silent = quiet && pingedOnce;
    if (!silent) {
      pingBtn.setAttribute("disabled", "");
      pingOut.textContent = "Pinging…";
    }
    const r = await fetchJson("/data/ping", "the ping", abort.signal);
    pingBtn.removeAttribute("disabled");
    pingedOnce = true;
    if (!r.ok) {
      lineOk = false;
      pings.push(null);
      while (pings.length > PINGS) pings.shift();
      pingOut.textContent = `No answer: ${r.error.why}`;
    } else {
      lineFacts = r.body.facts;
      lineOk = !!r.body.ok;
      // redesign-final Low: the dashboard keeps the last twelve round
      // trips it measured, so the strip opens full (the demo's twelve).
      const recent = Array.isArray(r.body.recent) ? r.body.recent : null;
      if (recent && recent.length) pings.splice(0, pings.length, ...recent);
      else pings.push(r.body.ok ? r.body.ms : null);
      while (pings.length > PINGS) pings.shift();
      pingOut.textContent = r.body.ok
        ? `Answered in ${r.body.ms} ms: ${r.body.message}`
        : `No answer after ${r.body.ms} ms: ${r.body.message}`;
    }
    pingDot.dataset.state = lineOk ? "ok" : "down";
    reach.replaceChildren(
      dot(lineOk ? "ok" : "bad"),
      lineOk && r.ok ? `Reachable · ${r.body.ms} ms` : "Not reachable",
    );
    attention.set(
      lineOk
        ? []
        : [
            {
              key: "unreachable",
              tone: "bad",
              title: "The host does not answer over the pinned line",
              text: pingOut.textContent ?? "",
              action: h(
                "button",
                {
                  type: "button",
                  class: "kp-button kp-button--sm",
                  onclick: () => void ping().catch(() => {}),
                },
                "Ping again",
              ),
            },
          ],
    );
    paintLine();
    return lineOk === true;
  };

  // ── About, Templates ──────────────────────────────────────────────────
  const about = h("dl", { class: "kp-kv", id: "host-about" });
  const aboutCard = section({
    id: "host-about-card",
    title: "About this host",
    desc: "What the machine is.",
  });
  aboutCard.body.append(about);
  const paintAbout = () => {
    const s = current();
    const f = s.fleet;
    if (!f) return;
    const d = f.host.disk_detail;
    const sig = daemonSignature(f.host);
    const up = hostUptime(f.host);
    /** @type {[string, Node | string][]} */
    const rows = [
      ["Name", f.host.name],
      [
        "Daemon",
        h(
          "span",
          { id: "host-daemon-signature", title: sig.title },
          `${versionText(s.hostVersion, s.hostBuild)} `,
          h(
            "span",
            { class: sig.tone === "ok" ? null : "hk-muted" },
            `(${sig.text})`,
          ),
        ),
      ],
      [
        "Hardware",
        `${f.host.cores_total ? `${f.host.cores_total} cores · ` : ""}${gib(f.host.ram_total_mb).replace(/\.0$/, "")} GiB RAM`,
      ],
      [
        "Root disk",
        d
          ? `${d.root_disk_device || "unknown"} · ${d.root_disk_total_gb.toFixed(0)} GB`
          : "not read yet",
      ],
      [
        "Up for",
        up ??
          h("span", { class: "hk-muted" }, "not reported by this host version"),
      ],
    ];
    about.replaceChildren(
      ...rows.map(([k, v]) =>
        h("div", { class: "kp-kv__row" }, h("dt", null, k), h("dd", null, v)),
      ),
    );
  };
  const tplList = h(
    "ul",
    { class: "hk-tpl", id: "host-templates" },
    h(
      "li",
      { role: "status" },
      h("span", { class: "hk-sk kp-skeleton" }),
      "Asking the host (pveam and the template containers)…",
    ),
  );
  const tplBuild = viaForm(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Bake a fresh golden template: opens the dialog first",
        onclick: () => void openAction("_host", "template-build"),
      },
      "Build a template",
    ),
    "template-build",
  );
  const tplCard = section({
    id: "host-templates-card",
    title: "Templates",
    desc: "The golden templates a new stack is cloned from.",
    foot: [tplBuild, "read with the page"],
  });
  tplCard.body.append(tplList);
  const loadTemplates = async () => {
    const r = await fetchJson("/data/templates", "the templates", abort.signal);
    if (!r.ok) {
      tplList.replaceChildren(
        h(
          "li",
          { class: "kp-alert kp-alert--destructive error", role: "alert" },
          `Could not read ${r.error.what}: ${r.error.why}`,
        ),
      );
      return;
    }
    const t = r.body.templates;
    tplList.replaceChildren(
      ...t.clones.map((/** @type {[number, string]} */ c) =>
        h(
          "li",
          null,
          dot("ok"),
          h(
            "span",
            null,
            h("span", { class: "mono" }, `CT ${c[0]}`),
            ` · ${c[1]}`,
          ),
        ),
      ),
      ...t.os.map((/** @type {string} */ o) =>
        h(
          "li",
          null,
          dot(""),
          h("span", { class: "mono" }, o.replace("local:vztmpl/", "")),
        ),
      ),
      ...(t.clones.length
        ? []
        : [
            h(
              "li",
              { class: "hk-muted" },
              "No golden template yet: build one.",
            ),
          ]),
    );
  };

  // ── Host checks ───────────────────────────────────────────────────────
  // The doctor's checks as a kp status list (feat-kp11demo-11): a mark, the
  // check, its detail and, under a warning or a failure, the remedy the host
  // already computed. The read takes about 30 s: the card lays the theme's
  // busy card over the list with the seconds it has run; no progress is
  // invented, because the host reports none.
  const checksNote = h("span", { role: "status", id: "host-checks-note" });
  let checksList = h("ul", { class: "kp-statuslist", id: "host-checks" });
  const checksKp = h(
    "section",
    {
      class: "kp-card hk-checkscard",
      "data-kp-card": "plain",
      "data-kp-state": "empty",
      "aria-label": "Host checks",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, checksList),
    ),
  );
  const checksCard = section({
    id: "host-checks-card",
    title: "Host checks",
    desc: "The doctor's checks about the host itself: disk, state file, clock, certificates.",
    foot: [checksNote],
  });
  // The count line lives in the card's foot and shows only when it has words.
  const setNote = (/** @type {string} */ text) => {
    checksNote.textContent = text;
    checksCard.foot.hidden = !text;
  };
  setNote("");
  checksCard.body.append(checksKp);
  const notRead = () =>
    setCardState(checksKp, "empty", {
      title: "Not read yet",
      text: "Reading takes about 30 s on pve; the result stays here until you leave.",
      action: null,
    });
  notRead();
  /** @param {any} report */
  const paintChecks = (report) => {
    const rows = doctorRows({ ...report, checks: hostChecks(report) });
    const next = buildStatusList(
      rows.map((x) => ({
        id: x.name,
        label: x.name,
        state:
          x.health.tone === "ok"
            ? "ok"
            : x.health.tone === "warn"
              ? "warn"
              : "failed",
        detail: x.detail,
        fix: x.remedy || "",
      })),
      { label: "Host checks", retry: false },
    );
    next.id = "host-checks";
    checksList.replaceWith(next);
    checksList = next;
    attachStatusLists(checksKp);
    const c = countStatus(checksList);
    setNote(
      rows.length
        ? `${c.ok.toLocaleString("en-GB")} of ${c.total.toLocaleString("en-GB")} checks pass, ${c.warn.toLocaleString("en-GB")} with a reservation, ${c.failed.toLocaleString("en-GB")} failed`
        : "",
    );
    setCardState(checksKp, rows.length ? "ready" : "empty", {
      title: "The host has no checks yet",
      text: "The first read after the host starts fills this list.",
    });
    return rows.length;
  };
  let checking = false;
  const loadChecks = async () => {
    if (checking) return;
    checking = true;
    runChecks.setAttribute("disabled", "");
    const began = Date.now();
    let haveRows = false;
    // The theme's own busy card carries the wait; the host reports no
    // progress, so none is drawn and no words are counted out.
    setCardState(checksKp, "loading");
    setNote("");
    try {
      const r = await slowReport(
        "/data/doctor",
        "the doctor",
        abort.signal,
        (b) => {
          if (!hostChecks(b.report).length) return;
          paintChecks(b.report);
          haveRows = true;
        },
      );
      const secs = Math.round((Date.now() - began) / 1000);
      if (!r.ok) {
        setCardState(checksKp, "error", {
          title: "The doctor did not answer",
          text: `After ${humanDuration(secs)}: ${r.error.why}`,
        });
        setNote("");
        return;
      }
      const n = paintChecks(r.report);
      if (checksNote.textContent)
        setNote(`${checksNote.textContent}, read in ${humanDuration(secs)}.`);
      void n;
    } finally {
      checking = false;
      runChecks.removeAttribute("disabled");
    }
  };
  checksKp.addEventListener(
    CARD_RETRY_EVENT,
    () => void loadChecks().catch(() => {}),
  );

  /** @param {number} n */
  const skeletonRows = (n) =>
    Array.from({ length: n }, () =>
      h(
        "tr",
        { "aria-hidden": "true" },
        Array.from({ length: 6 }, (_, i) =>
          h(
            "td",
            { class: i === 3 || i === 4 ? "hk-hide-phone" : null },
            h("span", { class: "hk-sk kp-skeleton" }),
          ),
        ),
      ),
    );

  // ── Host settings (host.toml) ─────────────────────────────────────────
  /** @type {import("../host.js").SettingRow[]} */
  let sRows = [];
  let sAll = false;
  const sDesc = h(
    "span",
    null,
    "host.toml as the host reads it. Read-only here; Settings changes them.",
  );
  const sView = segSwitch({
    label: "Which settings",
    value: "changed",
    items: [
      {
        value: "changed",
        label: "Changed",
        hint: "Only the settings that differ from their default",
      },
      { value: "all", label: "All", hint: "Every setting host.toml knows" },
    ],
    onChange: (v) => {
      sAll = v === "all";
      paintSettings();
    },
    drive: { id: SETTINGS_VIEW },
  });
  // The settings are the dashboard's data table: its own search, its own
  // sort keys (Live view reaches each header) and the package's loading,
  // empty and failed states.
  const sBlock = tableBlock({
    remember: "host-settings",
    caption: "Host settings as host.toml sets them",
    captionHidden: true,
    search: "Search settings",
    state: "loading",
    nothing: "host.toml changes nothing: every setting is at its default.",
    columns: [
      { label: "Setting", sort: "text" },
      { label: "Group", sort: "text" },
      { label: "Value", sort: "text" },
      { label: "Default", sort: "text" },
    ],
  });
  const sBody = sBlock.tbody;
  sBody.id = "host-settings";
  [...sBlock.wrap.querySelectorAll("thead th")].forEach((th, i) =>
    drivable(
      /** @type {HTMLElement} */ (th),
      SORT_SETTINGS,
      ["setting", "group", "value", "default"][i] ?? "setting",
    ),
  );
  const settingsCard = section({
    id: "host-settings-card",
    title: "Host settings",
    desc: "",
    collapsible: true,
    open: true,
    tools: [
      h(
        "a",
        {
          class: "kp-button kp-button--sm",
          href: "/settings",
          title: "Change host.toml on the Settings page",
        },
        "Edit in Settings",
      ),
    ],
  });
  /** @type {HTMLElement} */ (
    settingsCard.el.querySelector(".section-head__desc")
  ).replaceChildren(sDesc);
  settingsCard.body.append(
    h("div", { class: "hk-filters" }, sView.el),
    sBlock.wrap,
  );
  const paintSettings = () => {
    const shown = sRows.filter((x) => sAll || x.set);
    sBody.replaceChildren(
      ...shown.map((x) =>
        h(
          "tr",
          {
            class: x.set ? "hk-set-row" : null,
            "data-key": x.key,
            "data-set": x.set ? "1" : "0",
          },
          h(
            "td",
            null,
            h(
              "div",
              { class: "hk-name" },
              h("span", null, x.label),
              h("small", { class: "mono" }, x.key),
            ),
          ),
          h("td", { class: "hk-muted" }, x.group),
          h("td", { class: x.set ? "mono" : "mono hk-muted" }, x.value),
          h("td", { class: "mono hk-muted" }, x.def),
        ),
      ),
    );
    labelCells(sBody, () => ["Setting", "Group", "Value", "Default"]);
    sBlock.ready();
  };
  const loadSettings = async () => {
    sBlock.loading({ words: "Reading host.toml from the host…" });
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      sBlock.failed(r.error);
      return;
    }
    const v = hostSettingsView(r.body.page);
    sRows = v.rows;
    sDesc.textContent = `host.toml as the host reads it: ${v.changed} of ${v.rows.length} settings changed from their default. Read-only here; Settings changes them.`;
    paintSettings();
    // redesign-final (exact counts): the changed settings are the table's
    // rows marked changed.
    countOf(sDesc, sBody, ':scope > tr[data-set="1"]');
    sView.counts({ changed: v.changed, all: v.rows.length });
  };

  // ── one refresh control (feat-kp11demo-11) ────────────────────────────
  // Replaces the three timers: the figures, the containers and the line to
  // the host are read together, at the interval you choose; R reads now.
  const refreshEl = h("div", {
    class: "kp-refresh",
    "data-kp-refresh": String(EVERY_S),
    "data-kp-look": "segmented",
    "data-kp-refresh-choices": "0,15,30,60,300",
    "data-kp-remember": "host-refresh",
    "data-kp-refresh-label": "Refresh",
    "data-kp-stale-after": String(EVERY_S * 3),
  });
  const refreshRead = h(
    "span",
    { class: "hk-muted", role: "status" },
    "The figures, the containers and the line to the host are read together.",
  );
  // The kit's own refresh row: the bar is a direct child of the grid and
  // draws its label in the label track; the sentence sits under it.
  const refreshRow = formGrid(
    [refreshEl, gridRow({ control: refreshRead, span: "control-value" })],
    "hk-refresh",
  );
  refreshEl.addEventListener(REFRESH_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    void Promise.all([
      loadGuests(),
      loadTrend().then(() => true),
      ping(true).then(() => true),
      loadReach(),
    ])
      .then((all) => {
        const ok = all.every(Boolean);
        d.done(ok);
        refreshRead.textContent = ok
          ? "Read the host figures, the containers and the line to the host."
          : "The last read failed; the figures stay as they were.";
      })
      .catch(() => d.done(false));
  });
  void refreshOf;
  void VIEW_EVENT;

  // manual-checks-as-info-1: the stack-less standing check (the restic
  // password chain), information for when something goes wrong.
  const whenWrong = mountWhenCheck({
    stack: STANDING_STACK,
    id: "host-when-wrong",
    drive: CHECK_CONFIRM,
    signal: abort.signal,
    emptyText: "No check that only a person can look at is registered yet.",
  });
  stops.push(whenWrong.stop);

  // ── assemble ──────────────────────────────────────────────────────────
  root.replaceChildren(
    head.el,
    attention.el,
    kpisEl,
    refreshRow,
    guestsCard.el,
    h(
      "div",
      { class: "hk-cols" },
      h("div", { class: "hk-col-main" }, actionsCard.el, diskCard.el),
      h(
        "div",
        { class: "hk-col-side" },
        lineCard.el,
        aboutCard.el,
        tplCard.el,
        checksCard.el,
        whenWrong.el,
      ),
    ),
    settingsCard.el,
    keyRow([
      ["/", "search containers"],
      ["click a header", "sort"],
      ["G H", "go to Host"],
      ["R", "refresh now"],
      ["Esc", "show every container"],
      ["Ctrl K", "any host action"],
    ]),
  );

  const settingsTables = attachDataTables(settingsCard.body, {});
  const tableDetach = attachDataTables(guestsCard.body, {
    detail: (row) => {
      const g = guests
        ? guestTable(guests, current().fleet).find(
            (x) => String(x.vmid) === row.dataset.kpRowKey,
          )
        : null;
      return g ? guestDetail(g) : null;
    },
  });
  stops.push(
    settingsTables,
    tableDetach,
    attachSegmented(root),
    attachBarCells(root),
    attachStackbars(root),
    attachUptimes(root),
    attachEntityCards(root),
    attachBullets(root),
    attachStatusLists(root),
    attachCardStates(root),
    attachHoverCards(root),
    attachFootprints(root),
    attachRefresh(root),
  );
  // The line strip shows its busy card from the first paint, not from the
  // first answer (it is in the page now, which the card needs).
  paintLine();

  const paintKpis = () => {
    if (trend == null && trendWhy == null) return;
    const f = current().fleet ?? null;
    for (const t of tiles)
      t.set(hostTile(t.fig, trend?.get(t.fig.key) ?? null, f, trendWhy));
  };
  const trendGate = changeGate();
  const loadTrend = async () => {
    const r = await fetchJson(
      "/data/host/trend",
      "the host's trends",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (r.ok) {
      /** @type {import("../host.js").TrendFigure[]} */
      const figs = r.body.figures ?? [];
      if (!trendGate.isNew(figs) && trend && !trendWhy) return;
      trend = new Map(figs.map((x) => [x.key, x]));
      trendWhy = null;
    } else {
      trend = new Map();
      trendWhy = `${r.error.what}: ${r.error.why}`;
    }
    paintKpis();
  };
  const render = () => {
    const s = current();
    const f = s.fleet;
    if (!f) return;
    if (sub) sub.textContent = ` ${f.host.name}`;
    daemon.textContent = s.hostVersion ? `host daemon ${s.hostVersion}` : "";
    head.live?.set(f.measured_at);
    paintKpis();
    paintDisk();
    paintAbout();
    paintLine();
    paintGrowth();
  };

  // `/` (the dashboard's search key) searches the containers; Esc shows every
  // container again. kp's engine keeps both out of fields and dialogs.
  const stopKeys = registerShortcuts("page:host", [
    {
      keys: "Escape",
      what: "Show every container again",
      group: GROUP_HOST,
      run: unlessDialog(() => {
        gFilter.reset();
      }),
    },
  ]);

  const unsub = subscribe(() => {
    render();
    paintGuests();
  });
  render();
  void loadSettings().catch(() => {});
  void loadTemplates().catch(() => {});
  void loadGrowth().catch(() => {});
  // The first read of the figures, the containers and the line goes through
  // the refresh control, so its "updated … ago" is true from the start.
  // Every data area wears its reading state from the first frame, before any
  // answer: the package's busy card, never a blank.
  paintGuests();
  refreshOf(refreshEl)?.now();
  // The doctor's host checks read as the page opens (about 30 s; the card
  // carries the busy card meanwhile and keeps the result until you leave).
  void loadChecks().catch(() => {});
  // redesign-final-h5: `/host?section=doctor` (the old Doctor's address)
  // opens this page on its Host checks card and reads them, instead of
  // mounting the retired Doctor page inside it.
  if (ctx.focus === "checks") {
    requestAnimationFrame(() =>
      checksCard.el.scrollIntoView({ block: "start" }),
    );
  }
  return () => {
    abort.abort();
    unsub();
    stopKeys();
    stops.forEach((s) => s());
    for (const s of panels.values()) s();
    root.classList.remove("hk-page", "nx-ops");
  };
}
