// The Stacks list's view state and its toolbar: which view, grouping and
// filter are on (kept in the address, the view and the Deploy asks also in
// this browser), the four segmented switches, and where the one toolbar sits.

import { notify } from "./actui.js";
import { h } from "./dom.js";
import { drivable } from "./drivable.js";
import { ASKS, GROUP, SHOW, VIEW } from "./fleetcontrols.js";
import { segSwitch } from "./ui.js";

/** The view a person last chose, kept in this browser only. */
const VIEW_KEY = "homelab.stacks.view";
/** How a batch Deploy asks for safety, kept in this browser only. */
const ASKS_KEY = "homelab.stacks.deploy-asks";

/**
 * @typedef {object} ViewState
 * @property {"table" | "cards"} view
 * @property {"state" | "none"} group
 * @property {"all" | "attention" | "parked"} show
 * @property {"undo" | "confirm"} asks
 */

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

/** Keep a choice in this browser; a private window keeps it for the visit. @param {string} key @param {string} value */
function remember(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // refused: the choice lives in the address and in memory only
  }
}

/** The view state the address and this browser say. @returns {ViewState} */
export function initialViewState() {
  const q0 = new URLSearchParams(location.search);
  const show = q0.get("show");
  return {
    view:
      (q0.get("view") ?? remembered(VIEW_KEY, "table")) === "cards"
        ? "cards"
        : "table",
    group: q0.get("group") === "none" ? "none" : "state",
    show:
      show === "attention" ? "attention" : show === "parked" ? "parked" : "all",
    asks: remembered(ASKS_KEY, "undo") === "confirm" ? "confirm" : "undo",
  };
}

/** Mirror the non-default choices into the address. @param {ViewState} st */
export function writeUrl(st) {
  const p = new URLSearchParams(location.search);
  const set = (/** @type {string} */ k, /** @type {string | null} */ v) =>
    v ? p.set(k, v) : p.delete(k);
  set("view", st.view === "table" ? null : st.view);
  set("group", st.group === "state" ? null : st.group);
  set("show", st.show === "all" ? null : st.show);
  const s = p.toString() ? `?${p}` : "";
  if (s !== location.search)
    history.replaceState(history.state, "", location.pathname + s);
}

/** @param {ViewState} st @param {(force?: boolean) => void} paint */
function viewSwitches(st, paint) {
  const views = segSwitch({
    label: "View",
    value: st.view,
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
      st.view = v === "cards" ? "cards" : "table";
      remember(VIEW_KEY, st.view);
      writeUrl(st);
      paint();
    },
    mark: (b, v) => void drivable(b, VIEW, v),
  });
  const groups = segSwitch({
    label: "Group by",
    value: st.group,
    items: [
      {
        value: "state",
        label: "By state",
        hint: "Needs attention first, then running, then parked (folded)",
      },
      {
        value: "none",
        label: "No groups",
        hint: "One list, in the order you sort it",
      },
    ],
    onChange: (v) => {
      st.group = v === "none" ? "none" : "state";
      writeUrl(st);
      paint(true);
    },
    mark: (b, v) => void drivable(b, GROUP, v),
  });
  return { views, groups };
}

/** @param {ViewState} st @param {(force?: boolean) => void} paint */
function filterSwitches(st, paint) {
  const shows = segSwitch({
    label: "Show",
    value: st.show,
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
      st.show =
        v === "attention" ? "attention" : v === "parked" ? "parked" : "all";
      writeUrl(st);
      paint();
    },
    mark: (b, v) => void drivable(b, SHOW, v),
  });
  const safety = segSwitch({
    label: "Deploy asks",
    value: st.asks,
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
      st.asks = v === "confirm" ? "confirm" : "undo";
      remember(ASKS_KEY, st.asks);
    },
    mark: (b, v) => void drivable(b, ASKS, v),
  });
  return { shows, safety };
}

/**
 * The switches and the one toolbar that holds View, Group by and Show.
 * ux-stacknew-1 (Kenny, 2026-11-01): View and Group by live in the table's
 * own toolbar (the cards' toolbar when the cards are shown), Deploy asks in
 * the selection bar beside the batch Deploy it governs. paint() moves the
 * toolbar to whichever view is on screen.
 * @param {ViewState} st
 * @param {(force?: boolean) => void} paint
 */
export function createToolbar(st, paint) {
  const { views, groups } = viewSwitches(st, paint);
  const { shows, safety } = filterSwitches(st, paint);
  const tools = h(
    "div",
    {
      class: "fl-tools",
      role: "group",
      "aria-label": "How the stacks are shown",
    },
    views.el,
    groups.el,
    shows.el,
  );
  const cardBar = h("div", { class: "fl-cardbar" });
  return { views, groups, shows, safety, tools, cardBar, warned: false };
}

/**
 * The toolbar goes where the shown view's own toolbar is: the table's, or
 * the cards' (the table's is hidden with the table).
 * @param {ReturnType<typeof createToolbar>} tb
 * @param {HTMLElement} tableWrap
 * @param {boolean} tableOn
 */
export function placeTools(tb, tableWrap, tableOn) {
  const host = tableOn
    ? tableWrap.querySelector(".kp-datatable__bar")
    : tb.cardBar;
  if (host) {
    if (tb.tools.parentElement !== host) host.append(tb.tools);
    return;
  }
  // The table's bar is not drawn yet (or never will be): keep the tools
  // reachable in the cards' bar, say so once, and let the next paint retry.
  tb.cardBar.hidden = false;
  if (tb.tools.parentElement !== tb.cardBar) tb.cardBar.append(tb.tools);
  if (!tb.warned) {
    tb.warned = true;
    notify(
      "The fleet table's toolbar is missing; showing the tools above the list.",
      "error",
    );
  }
}
