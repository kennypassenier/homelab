// The Stacks page's list on kp-themes 11 (feat-kp11demo-12, the approved
// stacks demo): the fleet as a kp datatable that groups by state and opens a
// row in place, or as kp entity cards that keep one height; one kp selection
// bar over both for Deploy, Back up now and Update…; a Deploy that waits ten
// seconds behind an undo toast (or asks first, the dashboard's batch
// dialog). Everything it draws is the package's: the table's own loading
// layer, empty and failed states, the entity grid's skeleton cards and
// plates. fleetmodel.js is the pure half.

import { openAction, openBatch, reopenBatch } from "./actiondialog.js";
import { send } from "./act.js";
import {
  LIVE,
  fetchJson,
  h,
  labelCells,
  selectCell,
  stateWord,
  tableBlock,
} from "./dom.js";
import { declare, drivable, viaForm } from "./drivable.js";
import { driven } from "./drivehooks.js";
import { gb } from "./fleet.js";
import {
  cpuDay,
  STATE_GROUPS,
  STATE_WORDS,
  entityOf,
  lastBackups,
  needsAttention,
  stateGroup,
} from "./fleetmodel.js";
import { formatDateTime } from "./format.js";
import { stackHref } from "./router.js";
import { current } from "./store.js";
import { updateHrefFor } from "./updateflow.js";
import { keyRow, section, segSwitch } from "./ui.js";
import { setBarCell } from "/static/kp/js/barcell.js";
import { setEntityGridState } from "/static/kp/js/entity-card.js";
import { setEntityGridQuiet } from "./inplace.js";
import { bindSelection } from "./selectbind.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import {
  setSelectionBusy,
  setSelectionResult,
} from "/static/kp/js/selection-bar.js";
import { undoToast } from "/static/kp/js/undo-toast.js";
import { dataTable, attachDataTables } from "/static/kp/js/datatable.js";

/** @typedef {import("./stacksview.js").Row} Row */

// Live view (invariant 39): every control of this list.
const VIEW = declare({
  id: "stacks-view",
  page: "overview",
  opens: "view",
  row: "cards|table",
  what: "show the stacks as cards or as a table",
});
const GROUP = declare({
  id: "stacks-group",
  page: "overview",
  opens: "view",
  row: "state|none",
  what: "group the table's rows by state (needs attention, running, parked) or not at all",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
const SHOW = declare({
  id: "stacks-show",
  page: "overview",
  opens: "view",
  row: "all|attention|parked",
  what: "show every card, only the stacks that need attention, or only the parked ones",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
const ASKS = declare({
  id: "stacks-deploy-asks",
  page: "overview",
  opens: "view",
  row: "undo|confirm",
  what: "choose how a batch Deploy asks for safety: ten seconds to undo it, or a dialog that asks first",
});
const CLEAR = declare({
  id: "stacks-untick",
  page: "overview",
  opens: "view",
  what: "untick every ticked stack",
  shows: "while a stack is ticked",
  reach: [{ do: "click", control: "stacks-tick", row: "*" }],
});
const SEARCH = declare({
  id: "stacks-search",
  page: "overview",
  opens: "view",
  what: "the table's search box: find a stack by name, state, backup or any column",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
const TICK = declare({
  id: "stacks-tick",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "tick or untick one stack for a batch action",
  selects: true,
});
const TICK_ALL = declare({
  id: "stacks-tick-all",
  page: "overview",
  opens: "view",
  what: "tick every stack shown (click again to untick them)",
  shows: "in the table view",
  reach: [{ do: "click", control: "stacks-view", row: "table" }],
});
const BATCH = declare({
  id: "stacks-batch",
  page: "overview",
  opens: "dialog",
  row: "backup|update|deploy",
  what: "act on the ticked stacks: deploy them (ten seconds to undo, or a batch dialog that asks first), back them up in one batch dialog, or update them (the Update flow)",
  shows: "while a stack is ticked",
  reach: [{ do: "click", control: "stacks-tick", row: "*" }],
});
const OPEN = declare({
  id: "stacks-open",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "open one stack's hub (its name in the table or on its card)",
});
const ROW_ACTION = declare({
  id: "stacks-row-action",
  page: "overview",
  opens: "dialog",
  row: "<stack>",
  what: "a card's Deploy or Back up now: open that stack's action dialog",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
const ROW_UPDATE = declare({
  id: "stacks-row-update",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "a card's Update: open the Update flow for that stack",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
const ROW_LOGS = declare({
  id: "stacks-row-logs",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "a card's Logs: open the stack hub's Logs tab",
  shows: "in the cards view",
  reach: [{ do: "click", control: "stacks-view", row: "cards" }],
});
const SHOW_ALL = declare({
  id: "stacks-show-all",
  page: "overview",
  opens: "view",
  what: "clear the table's search and filters (the empty table's Clear the search and filters)",
  shows: "when the search and the filters match no stack",
});
const EMPTY_NEW = declare({
  id: "stacks-empty-new",
  page: "overview",
  opens: "dialog",
  what: "the empty fleet's New stack",
  shows: "while the fleet has no stack",
});

/** The view a person last chose, kept in this browser only. */
const VIEW_KEY = "homelab.stacks.view";
/** How a batch Deploy asks for safety, kept in this browser only. */
const ASKS_KEY = "homelab.stacks.deploy-asks";
/** How long a Deploy waits behind its undo toast. */
export const UNDO_MS = 10000;

/**
 * @param {string} key
 * @param {string} fallback
 */
function remembered(key, fallback) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

/** @param {number} n @param {string} one */
const plural = (n, one) => `${n} ${n === 1 ? one : `${one}s`}`;

/**
 * The Stacks list: a card with its controls, the selection bar and either
 * the table or the cards.
 * @param {{navigate: (href: string) => void, openNew: () => void,
 *   retry: () => void}} ctx
 */
export function fleetList(ctx) {
  const q0 = new URLSearchParams(location.search);
  /** @type {"table" | "cards"} */
  /**
   * The last 24 h of each running stack's CPU for the cards' trend row, read
   * for the whole fleet in one request (/data/fleet-cpu) and kept for five
   * minutes.
   * @type {Map<string, number[]>}
   */
  const trends = new Map();
  let trendsAt = 0;
  let trendsReading = false;
  let view =
    (q0.get("view") ?? remembered(VIEW_KEY, "table")) === "cards"
      ? "cards"
      : "table";
  /** @type {"state" | "none"} */
  let group = q0.get("group") === "none" ? "none" : "state";
  /** @type {"all" | "attention" | "parked"} */
  let show =
    /** @type {any} */ (q0.get("show")) === "attention"
      ? "attention"
      : /** @type {any} */ (q0.get("show")) === "parked"
        ? "parked"
        : "all";
  /** @type {"undo" | "confirm"} */
  let asks = remembered(ASKS_KEY, "undo") === "confirm" ? "confirm" : "undo";
  /** @type {Set<string>} */
  const ticked = new Set();
  /** @type {Row[]} */
  let rows = [];
  /** @type {any} */
  let calendar = null;
  /** The newest paint: loading, failed, or the rows. @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let failMessage = "";
  let tableSig = "";

  const writeUrl = () => {
    const p = new URLSearchParams(location.search);
    const set = (/** @type {string} */ k, /** @type {string | null} */ v) =>
      v ? p.set(k, v) : p.delete(k);
    set("view", view === "table" ? null : view);
    set("group", group === "state" ? null : group);
    set("show", show === "all" ? null : show);
    const s = p.toString() ? `?${p}` : "";
    if (s !== location.search)
      history.replaceState(history.state, "", location.pathname + s);
  };

  // ── controls: label | control on two shared tracks ───────────────────
  const views = segSwitch({
    label: "View",
    value: view,
    items: [
      {
        value: "table",
        label: "Table",
        hint: "Every stack as a table row, grouped by state; open a row for its apps and backups",
      },
      {
        value: "cards",
        label: "Cards",
        hint: "Every stack as a card with its memory and its buttons",
      },
    ],
    onChange: (v) => {
      view = v === "cards" ? "cards" : "table";
      try {
        localStorage.setItem(VIEW_KEY, view);
      } catch {
        // a private window: the choice lives in the address only
      }
      writeUrl();
      paint();
    },
    mark: (b, v) => void drivable(b, VIEW, v),
  });
  const groups = segSwitch({
    label: "Group by",
    value: group,
    items: [
      {
        value: "state",
        label: "State",
        hint: "Needs attention first, then running, then parked (folded)",
      },
      {
        value: "none",
        label: "No groups",
        hint: "One list, in the order you sort it",
      },
    ],
    onChange: (v) => {
      group = v === "none" ? "none" : "state";
      writeUrl();
      paint(true);
    },
    mark: (b, v) => void drivable(b, GROUP, v),
  });
  const shows = segSwitch({
    label: "Show",
    value: show,
    items: [
      { value: "all", label: "All", hint: "Every stack" },
      {
        value: "attention",
        label: "Needs attention",
        hint: "Only the stacks that are down, degraded, missed a backup or carry a red flag",
      },
      {
        value: "parked",
        label: "Parked",
        hint: "Only the stacks that are parked",
      },
    ],
    onChange: (v) => {
      show =
        v === "attention" ? "attention" : v === "parked" ? "parked" : "all";
      writeUrl();
      paint();
    },
    mark: (b, v) => void drivable(b, SHOW, v),
  });
  const safety = segSwitch({
    label: "Deploy asks",
    value: asks,
    items: [
      {
        value: "undo",
        label: "With undo",
        hint: "Deploy at once and keep ten seconds to take it back",
      },
      {
        value: "confirm",
        label: "Asks first",
        hint: "Open the batch dialog that lists the stacks and asks again before anything starts",
      },
    ],
    onChange: (v) => {
      asks = v === "confirm" ? "confirm" : "undo";
      try {
        localStorage.setItem(ASKS_KEY, asks);
      } catch {
        // kept for this visit only
      }
    },
    mark: (b, v) => void drivable(b, ASKS, v),
  });
  /** @param {string} label @param {HTMLElement} control @param {string} id */
  const ctlRow = (label, control, id) =>
    h(
      "div",
      { class: "fl-ctl__row", id },
      h("b", { class: "fl-ctl__label" }, label),
      h("div", { class: "fl-ctl__control" }, control),
    );
  const groupRow = ctlRow("Group by", groups.el, "fl-row-group");
  const showRow = ctlRow("Show", shows.el, "fl-row-show");
  const controls = h(
    "div",
    { class: "fl-ctl" },
    ctlRow("View", views.el, "fl-row-view"),
    groupRow,
    showRow,
    ctlRow("Deploy asks", safety.el, "fl-row-asks"),
  );

  // ── the selection bar: one for the table and the cards ───────────────
  const bar = h(
    "div",
    {
      class: "kp-selection fl-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "fl-body",
      "data-kp-placement": "sticky-top",
      "data-kp-undo-ms": String(UNDO_MS),
      id: "fl-bar",
      "aria-label": "Selected stacks",
    },
    h(
      "div",
      { class: "kp-selection__actions" },
      batchButton(
        "deploy",
        "Deploy",
        "kp-button kp-button--primary",
        "Deploy every ticked stack, one after the other, each backed up first",
      ),
      batchButton(
        "backup",
        "Back up now",
        "kp-button",
        "Back up every ticked stack now",
      ),
      batchButton(
        "update",
        "Update",
        "kp-button",
        "Update the ticked stacks in the Update flow: see what changes, then back up, update and verify",
      ),
    ),
  );
  /**
   * @param {string} action
   * @param {string} label
   * @param {string} cls
   * @param {string} title
   */
  function batchButton(action, label, cls, title) {
    return drivable(
      viaForm(
        h(
          "button",
          {
            type: "button",
            class: cls,
            "data-kp-selection-action": action,
            "data-batch": action,
            title,
          },
          label,
        ),
        "batch",
      ),
      BATCH,
      action,
    );
  }
  const toastBed = h("div", {
    class: "fl-toasts",
    "aria-label": "Toasts of the stacks list",
  });

  // ── the table ───────────────────────────────────────────────────────
  const block = tableBlock({
    remember: "stacks",
    caption: "The stacks of the host; open a row for its apps, backups and CPU",
    captionHidden: true,
    search: "Search the stacks",
    nothing: "The host runs no stacks yet.",
    select: { label: "Tick every stack shown", actions: [] },
    columns: [
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "State", sort: "text", filter: "choice" },
      { label: "Apps up", sort: "number", cls: "kp-text-end" },
      { label: "Restarts", sort: "number", cls: "kp-text-end" },
      { label: "RAM", sort: "number", filter: "range" },
      { label: "CPU", sort: "number" },
      { label: "Last backup", sort: "number" },
    ],
  });
  // The package's selection bar replaces the table's own action strip.
  block.wrap.querySelector("[data-kp-datatable-actions]")?.remove();
  for (const [k, v] of [
    ["data-kp-expandable", ""],
    ["data-kp-expand-single", ""],
    ["data-kp-column-menu", ""],
    ["data-kp-filter-mode", "add"],
    ["data-kp-page-size", "100"],
  ])
    block.wrap.setAttribute(k, v);
  block.wrap.classList.add("fl-table");
  const headRow = /** @type {HTMLElement} */ (
    block.wrap.querySelector("thead tr")
  );
  const ths = /** @type {HTMLElement[]} */ ([...headRow.children]);
  ths[1].setAttribute("data-kp-column-locked", "");
  const selectAll = /** @type {HTMLInputElement} */ (
    block.wrap.querySelector("[data-kp-select-all]")
  );
  drivable(selectAll, TICK_ALL);
  const search = /** @type {HTMLInputElement | null} */ (
    block.wrap.querySelector("[data-kp-datatable-search]")
  );
  if (search) drivable(search, SEARCH);
  const clearBtn = block.wrap.querySelector("[data-kp-datatable-clear]");
  if (clearBtn) drivable(/** @type {HTMLElement} */ (clearBtn), SHOW_ALL);
  const COLS = 8;

  // ── the cards ───────────────────────────────────────────────────────
  const grid = h("div", {
    class: "kp-entity-grid fl-cards",
    "data-kp-entity-grid": "",
    "data-kp-entity-min": "19rem",
    "data-kp-entity-meters": "1",
    "data-kp-selectable": "",
    "data-kp-entity-link": "/stacks/{key}",
    "data-kp-entity-inline": "3",
    "aria-label": "Stacks",
  });

  const body = h("div", { class: "fl-body", id: "fl-body" }, block.wrap, grid);
  const errs = h("div", { class: "fl-errors", "aria-live": "polite" });
  errs.hidden = true;
  const foot = keyRow([
    [["/"], "find"],
    [["j", "k"], "move"],
    [["x"], "tick"],
    [["Enter"], "open"],
    [["Esc"], "untick all"],
  ]);
  const card = section({
    title: "All stacks",
    desc: "Every stack the host runs, live. Open a stack for its hub, tick several to act on them at once.",
    id: "stack-list",
    body: [controls, errs, bar, body, toastBed],
    foot: ["Read from the host, live · CPU is the last reading", foot],
  });
  card.el.classList.add("fl-card");

  const sel = bindSelection(
    card.el,
    bar,
    () => ({
      deploy: (items) => {
        const ok = items.filter(
          (i) => rows.find((r) => r.name === i.key)?.state.label !== "parked",
        );
        return ok.length === items.length
          ? ""
          : `${ok.length} of ${items.length} can be deployed`;
      },
    }),
    () => {
      // The bar's own "untick all" is a Live view control too.
      const clear = bar.querySelector(".kp-selection__clear");
      if (clear instanceof HTMLElement) drivable(clear, CLEAR);
    },
  );
  const detachTables = attachDataTables(card.el);
  const detachMarks = attachIdmarks(card.el);
  const table = dataTable(block.wrap);

  // ── rules, actions ───────────────────────────────────────────────────

  /** @param {string} action @param {string[]} keys */
  const sendBatch = async (action, keys) => {
    const r = await send(
      "POST",
      "/data/actions/batch",
      { action, stacks: keys, args: {} },
      `${action} ${plural(keys.length, "stack")}`,
    );
    if (!r.ok) {
      setSelectionBusy(bar, null);
      setSelectionResult(bar, {
        done: 0,
        refused: keys.map((key) => ({ key, reason: r.error.why })),
      });
      throw new Error(r.error.why);
    }
    setSelectionBusy(bar, null);
    void reopenBatch(r.body.batch);
  };

  bar.addEventListener("kp-selection-action", (e) => {
    const { action, button, keys } = /** @type {CustomEvent} */ (e).detail;
    if (action === "update") {
      ctx.navigate(updateHrefFor(keys));
      return;
    }
    // Live view replays and "Asks first" use the batch dialog: its preview
    // lists each stack's command and its guard before anything starts.
    if (action === "backup" || asks === "confirm" || driven()) {
      void openBatch(action, keys);
      return;
    }
    // Deploy with undo: the rows say deploying at once, the toast counts
    // ten seconds, and the deploy is sent when the clock ends.
    markWord(keys, "deploying");
    undoToast(`Deploying ${plural(keys.length, "stack")}`, {
      region: toastBed,
      ms: UNDO_MS,
      clock: "ring",
      commit: () => sendBatch("deploy", keys),
      undo: () => markWord(keys, null),
      failed: "The host refused the deploy",
    });
    void button;
  });

  /**
   * The state word of ticked rows while a deploy waits.
   * @param {string[]} keys @param {string | null} word
   */
  function markWord(keys, word) {
    for (const k of keys) {
      const cell = block.tbody.querySelector(
        `tr[data-kp-row-key="${CSS.escape(k)}"] .kp-state-word`,
      );
      if (cell instanceof HTMLElement) {
        const r = rows.find((x) => x.name === k);
        cell.textContent = word ?? r?.state.label ?? "";
      }
    }
  }

  // ── cards: the actions ───────────────────────────────────────────────
  grid.addEventListener("kp-entity-action", (e) => {
    const { key, id } = /** @type {CustomEvent} */ (e).detail;
    if (id === "update") ctx.navigate(updateHrefFor([key]));
    else if (id === "logs") ctx.navigate(stackHref(key, "logs"));
    else void openAction(key, id);
  });
  grid.addEventListener("kp-entity-grid-retry", () => ctx.retry());
  // The empty plate's button: New stack when the host runs none, else it
  // shows every card again.
  grid.addEventListener("kp-entity-new", () => {
    if (!rows.length) ctx.openNew();
    else if (show !== "all") {
      show = "all";
      writeUrl();
      paint();
    }
  });
  block.wrap.addEventListener("kp-datatable-retry", () => ctx.retry());

  /** Mark what the entity cards drew, so Live view reaches it. */
  const decorateCards = () => {
    for (const el of /** @type {HTMLElement[]} */ ([
      ...grid.querySelectorAll(":scope > .kp-card--entity"),
    ])) {
      const key = el.getAttribute("data-kp-entity-key") ?? "";
      const name = el.querySelector(".kp-entity__name");
      if (name instanceof HTMLElement && !name.dataset.drive)
        drivable(name, OPEN, key);
      const box = el.querySelector("input[data-kp-select-row]");
      if (box instanceof HTMLElement && !box.dataset.drive)
        drivable(box, TICK, key);
      const r = rows.find((x) => x.name === key);
      const wants = r ? (entityOf(r, gb).actions ?? []) : [];
      const buttons = /** @type {HTMLElement[]} */ ([
        ...el.querySelectorAll("button.kp-entity__action"),
      ]);
      buttons.forEach((b, i) => {
        if (b.dataset.drive) return;
        const id = wants[i]?.id;
        if (id === "update") drivable(b, ROW_UPDATE, key);
        else if (id === "logs") drivable(b, ROW_LOGS, key);
        else drivable(b, ROW_ACTION, key);
      });
    }
  };
  new MutationObserver(decorateCards).observe(grid, {
    childList: true,
    subtree: true,
  });

  // ── painting ─────────────────────────────────────────────────────────
  /** @param {Row} r */
  const cpuCell = (r) => ({
    value:
      r.state.label === "parked" || r.cpuPct == null
        ? null
        : Math.min(0.95, r.cpuPct / 25),
    text:
      r.state.label === "parked" || r.cpuPct == null
        ? "off"
        : `${Math.round(r.cpuPct * 10) / 10} %`,
    sortValue: r.cpuPct ?? -1,
    warnAt: 0.75,
    badAt: 0.9,
  });
  /** @param {Row} r */
  const ramCell = (r) => ({
    value:
      r.state.label === "parked" || r.ramPct == null ? null : r.ramPct / 100,
    text:
      r.state.label === "parked" || r.ramPct == null ? "off" : `${r.ramPct} %`,
    sortValue: r.ramPct ?? -1,
    warnAt: 0.75,
    badAt: 0.9,
    mark: 0.8,
    markLabel: "alert at 80 %",
    label: "of its memory limit",
  });

  /** @param {Row} r */
  const detailOf = (r) => {
    const fleetStack = current().fleet?.stacks.find((s) => s.name === r.name);
    const apps = fleetStack?.apps ?? [];
    const times = lastBackups(calendar?.stacks?.[r.name]);
    return h(
      "tr",
      { "data-kp-row-detail": "" },
      h(
        "td",
        { colspan: String(COLS) },
        h(
          "div",
          { class: "fl-detail" },
          h(
            "div",
            { class: "fl-detail__part" },
            h("h4", null, "Apps"),
            apps.length
              ? h(
                  "ul",
                  { class: "kp-action-list" },
                  ...apps.map((a) =>
                    h(
                      "li",
                      null,
                      h(
                        "span",
                        { class: "kp-action-list__main" },
                        h("strong", { class: "kp-action-list__title" }, a.name),
                        h(
                          "span",
                          { class: "kp-action-list__text" },
                          a.health ?? (a.running ? "running" : "stopped"),
                        ),
                      ),
                      stateWord(a.running ? "up" : "down", ["up", "down"]),
                    ),
                  ),
                )
              : h(
                  "p",
                  { class: "fl-detail__none" },
                  "This stack declares no apps.",
                ),
          ),
          h(
            "div",
            { class: "fl-detail__part" },
            h("h4", null, "Last three backups"),
            times.length
              ? h(
                  "dl",
                  { class: "kp-kv" },
                  ...times.map((t) =>
                    h(
                      "div",
                      { class: "kp-kv__row" },
                      h("dt", null, formatDateTime(t)),
                    ),
                  ),
                )
              : h(
                  "p",
                  { class: "fl-detail__none" },
                  r.backup.state === "nodata"
                    ? "This stack keeps no data."
                    : r.state.label === "parked"
                      ? "A parked stack is not backed up."
                      : r.backup.text,
                ),
            h("h4", null, "CPU"),
            h(
              "p",
              { class: "fl-detail__none" },
              r.cpuPct == null
                ? "Not measured."
                : `${Math.round(r.cpuPct * 10) / 10} % of its cores now.`,
            ),
          ),
        ),
      ),
    );
  };

  /** @param {Row} r */
  const rowOf = (r) => {
    const link = drivable(
      h(
        "a",
        { href: stackHref(r.name), class: "fl-name" },
        h("span", {
          class: "kp-idmark kp-idmark--sm",
          "data-kp-idmark": r.name,
          "aria-hidden": "true",
        }),
        h("b", null, r.name),
      ),
      OPEN,
      r.name,
    );
    const ramEl = h("span", { class: "kp-bar-cell", "data-fl-ram": r.name });
    const cpuEl = h("span", { class: "kp-bar-cell", "data-fl-cpu": r.name });
    setBarCell(ramEl, ramCell(r));
    setBarCell(cpuEl, cpuCell(r));
    const g = stateGroup(r);
    const tr = h(
      "tr",
      {
        "data-kp-row-key": r.name,
        "data-kp-row-label": r.name,
        ...(group === "state" ? { "data-kp-group-of": g } : {}),
      },
      selectCell(r.name, `Tick ${r.name}`),
      h(
        "td",
        { "data-kp-sort-value": r.name },
        h(
          "span",
          { class: "fl-pair" },
          link,
          h("span", { class: "kp-timestamp" }, `CT ${r.vmid}`),
        ),
      ),
      h(
        "td",
        {
          "data-kp-sort-value": r.state.label,
          "data-kp-filter-value": r.state.label,
        },
        h(
          "span",
          {
            class: "kp-state-word",
            "data-kp-words": STATE_WORDS,
            [LIVE]: "state",
          },
          r.state.label,
        ),
      ),
      h(
        "td",
        {
          class: "kp-text-end",
          "data-kp-sort-value": String(
            r.appsTotal ? r.appsUp / r.appsTotal : 0,
          ),
          [LIVE]: "apps",
        },
        `${r.appsUp}/${r.appsTotal}`,
      ),
      h(
        "td",
        {
          class: "kp-text-end",
          "data-kp-sort-value": String(r.restarts),
          [LIVE]: "restarts",
        },
        String(r.restarts),
      ),
      h("td", null, ramEl),
      h("td", null, cpuEl),
      h(
        "td",
        { class: "fl-nowrap", "data-kp-sort-value": String(r.backup.at ?? 0) },
        r.backup.at ? formatDateTime(r.backup.at) : r.backup.text,
      ),
    );
    const box = tr.querySelector("input[data-kp-select-row]");
    if (box instanceof HTMLElement) drivable(box, TICK, r.name);
    return [tr, detailOf(r)];
  };

  /** @param {string} key @param {string} label */
  const groupRowEl = (key, label) =>
    h(
      "tr",
      {
        "data-kp-row-group": key,
        ...(key === "parked" ? { "data-kp-folded": "" } : {}),
      },
      h(
        "th",
        { scope: "rowgroup", colspan: String(COLS) },
        label,
        " ",
        h("span", { class: "kp-badge", "data-kp-group-count": "" }),
      ),
    );

  const sigOf = (/** @type {Row[]} */ list) =>
    JSON.stringify([
      group,
      list.map((r) => [
        r.name,
        r.vmid,
        r.state.label,
        r.appsUp,
        r.appsTotal,
        r.restarts,
        r.backup.at,
        // The age text ticks with the clock ("39 min 41 s ago"); a row that
        // has a time shows that time, so only a row without one is keyed by
        // its words ("reading…", "keeps no data"). Otherwise every push
        // rebuilt the table and took the focus and the hover with it.
        r.backup.at ? "" : r.backup.text,
        r.flags.map((f) => f.key),
        r.problem,
        r.apps,
      ]),
      calendar ? 1 : 0,
    ]);

  /** Tick the boxes of the keys in `ticked`, the way a click would. */
  const reapplyTicks = () => {
    for (const box of /** @type {NodeListOf<HTMLInputElement>} */ (
      body.querySelectorAll("input[data-kp-select-row]")
    )) {
      const key = box
        .closest("[data-kp-row-key]")
        ?.getAttribute("data-kp-row-key");
      const want = !!key && ticked.has(key) && !box.disabled;
      if (box.checked !== want) {
        box.checked = want;
        box.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }
  };
  body.addEventListener("change", (e) => {
    const box = e.target;
    if (
      !(box instanceof HTMLInputElement) ||
      !box.matches("[data-kp-select-row]")
    )
      return;
    const key = box
      .closest("[data-kp-row-key]")
      ?.getAttribute("data-kp-row-key");
    if (!key) return;
    if (box.checked) ticked.add(key);
    else ticked.delete(key);
  });

  /** @param {boolean} [force] */
  function paint(force = false) {
    const tableOn = view === "table";
    // Only the view that is on is drawn, so a Live view lookup by id and row
    // never lands on the hidden twin.
    if (tableOn && grid.childElementCount) {
      grid.replaceChildren();
      grid.removeAttribute("data-kp-state");
      grid.removeAttribute("aria-busy");
    } else if (!tableOn && block.tbody.childElementCount) {
      block.tbody.replaceChildren();
      tableSig = "";
    }
    block.wrap.hidden = !tableOn;
    grid.hidden = tableOn;
    groupRow.hidden = !tableOn;
    showRow.hidden = tableOn;
    views.set(view);
    groups.set(group);
    shows.set(show);
    safety.set(asks);
    if (status === "loading") {
      if (tableOn) block.loading({ words: "Reading the fleet" });
      else setEntityGridState(grid, "loading", { count: 6 });
      return;
    }
    if (status === "failed") {
      if (tableOn)
        block.failed({
          what: "the fleet",
          why: failMessage,
          fix: "Check that the host answers, then try again",
        });
      else setEntityGridState(grid, "error", { message: failMessage });
      return;
    }
    if (tableOn) {
      const sig = sigOf(rows);
      if (force || sig !== tableSig) {
        tableSig = sig;
        /** @type {Node[]} */
        const out = [];
        if (group === "state") {
          for (const g of STATE_GROUPS) {
            const members = rows.filter((r) => stateGroup(r) === g.key);
            if (members.length) {
              out.push(groupRowEl(g.key, g.label));
              for (const r of members) out.push(...rowOf(r));
            }
          }
        } else for (const r of rows) out.push(...rowOf(r));
        block.tbody.replaceChildren(...out);
        labelCells(block.tbody, () => [
          "",
          "Stack",
          "State",
          "Apps up",
          "Restarts",
          "RAM",
          "CPU",
          "Last backup",
        ]);
        block.ready();
        reapplyTicks();
      } else {
        // The numbers that move every reading change in place; the rows
        // keep their node, their open detail and their focus.
        for (const r of rows) {
          const ram = block.tbody.querySelector(
            `[data-fl-ram="${CSS.escape(r.name)}"]`,
          );
          const cpu = block.tbody.querySelector(
            `[data-fl-cpu="${CSS.escape(r.name)}"]`,
          );
          if (ram instanceof HTMLElement) setBarCell(ram, ramCell(r));
          if (cpu instanceof HTMLElement) setBarCell(cpu, cpuCell(r));
        }
        block.ready({ refresh: false });
      }
      return;
    }
    const list = rows.filter((r) =>
      show === "all"
        ? true
        : show === "parked"
          ? r.state.label === "parked"
          : needsAttention(r),
    );
    if (!rows.length)
      setEntityGridState(grid, "empty", {
        title: "The host runs no stacks yet",
        message:
          "A stack is one container with the apps it runs. Start one from a preset or from a bundle.",
        actionLabel: "New stack",
      });
    else if (!list.length)
      setEntityGridState(grid, "empty", {
        title:
          show === "parked" ? "No stack is parked" : "No stack needs attention",
        message: "Show All to see every stack again.",
        actionLabel: "Show all",
      });
    else {
      void readTrends();
      setEntityGridQuiet(
        grid,
        list.map((r) => entityOf(r, gb, trends.get(r.name))),
      );
    }
    const plateBtn = grid.querySelector(".kp-stateplate button");
    if (
      plateBtn instanceof HTMLElement &&
      !rows.length &&
      !plateBtn.dataset.drive
    )
      drivable(plateBtn, EMPTY_NEW);
    decorateCards();
    reapplyTicks();
    const counts = {
      all: rows.length,
      attention: rows.filter(needsAttention).length,
      parked: rows.filter((r) => r.state.label === "parked").length,
    };
    shows.counts(counts);
  }

  const readTrends = async () => {
    const names = rows
      .filter((r) => r.state.label !== "parked")
      .map((r) => r.name);
    if (trendsReading || !names.length || Date.now() - trendsAt < 300_000)
      return;
    trendsReading = true;
    trendsAt = Date.now();
    const r = await fetchJson(
      "/data/fleet-cpu",
      "the stacks' CPU trends",
    ).catch(() => null);
    for (const n of names) trends.set(n, r?.ok ? cpuDay(r.body, n) : []);
    trendsReading = false;
    if (view === "cards") paint();
  };
  /** Everything the page knows changed. */
  const set = (/** @type {{rows: Row[], calendar: any}} */ m) => {
    rows = m.rows;
    calendar = m.calendar;
    for (const n of [...ticked])
      if (!rows.some((r) => r.name === n)) ticked.delete(n);
    status = "ready";
    paint();
  };

  paint();
  return {
    el: card.el,
    set,
    /** @param {"loading" | "failed"} s @param {string} [why] */
    state: (s, why = "") => {
      status = s;
      failMessage = why;
      paint();
    },
    tick: (/** @type {string[]} */ stacks) => {
      ticked.clear();
      for (const s of stacks) ticked.add(s);
      reapplyTicks();
    },
    ticked: () => [...ticked],
    table: () => table,
    stop: () => {
      sel.stop();
      detachTables();
      detachMarks();
    },
  };
}
