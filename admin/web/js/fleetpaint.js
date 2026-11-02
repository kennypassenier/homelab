// Painting the Stacks list: only the view that is on is drawn (the table's
// rows, regrouped when the grouping asks, or the entity cards), the loading
// and failed states, the ticks that survive a repaint, and the cards' CPU
// trends. The state it reads is the list's own (fleetlist.js owns it).

import { fetchJson, labelCells } from "./dom.js";
import { drivable } from "./drivable.js";
import { EMPTY_NEW } from "./fleetcontrols.js";
import { decorateCards, emptyCards, shownRows } from "./fleetcards.js";
import { cpuCell, groupRowEl, ramCell, rowOf, sigOf } from "./fleetcells.js";
import { gb } from "./fleet.js";
import { setEntityGridQuiet } from "./inplace.js";
import {
  STATE_GROUPS,
  cpuDay,
  entityOf,
  needsAttention,
  stateGroup,
} from "./fleetmodel.js";
import { reapplyTicks } from "./fleetticks.js";
import { placeTools } from "./fleettoolbar.js";
import {
  setBarCell,
  setEntityGridState,
} from "/static/kp/dist/kp-themes.min.js";

/** @typedef {import("./stacksview.js").Row} Row */

/** How long the cards' CPU trends are kept. */
const TRENDS_MS = 300_000;

/**
 * @typedef {object} ListState
 * @property {import("./fleettoolbar.js").ViewState["view"]} view
 * @property {import("./fleettoolbar.js").ViewState["group"]} group
 * @property {import("./fleettoolbar.js").ViewState["show"]} show
 * @property {import("./fleettoolbar.js").ViewState["asks"]} asks
 * @property {Row[]} rows
 * @property {any} calendar
 * @property {"loading" | "failed" | "ready"} status The newest paint: loading, failed, or the rows.
 * @property {string} failMessage
 * @property {string} tableSig
 * @property {Set<string>} ticked
 */

/**
 * @typedef {object} PaintEnv
 * @property {ListState} st
 * @property {ReturnType<typeof import("./dom.js").tableBlock>} block
 * @property {HTMLElement} grid
 * @property {HTMLElement} body
 * @property {ReturnType<typeof import("./fleettoolbar.js").createToolbar>} tb
 */

/** Show the table's or the cards' chrome and the switches' values. @param {PaintEnv} env */
function showView({ st, block, grid, tb }) {
  const tableOn = st.view === "table";
  // Only the view that is on is drawn, so a Live view lookup by id and row
  // never lands on the hidden twin.
  if (tableOn && grid.childElementCount) {
    grid.replaceChildren();
    grid.removeAttribute("data-kp-state");
    grid.removeAttribute("aria-busy");
  } else if (!tableOn && block.tbody.childElementCount) {
    block.tbody.replaceChildren();
    st.tableSig = "";
  }
  block.wrap.hidden = !tableOn;
  grid.hidden = tableOn;
  tb.groups.el.hidden = !tableOn;
  tb.shows.el.hidden = tableOn;
  tb.cardBar.hidden = tableOn;
  placeTools(tb, block.wrap, tableOn);
  tb.views.set(st.view);
  tb.groups.set(st.group);
  tb.shows.set(st.show);
  tb.safety.set(st.asks);
}

/** The table's rows, grouped by state or flat. @param {ListState} st */
function tableNodes(st) {
  /** @type {Node[]} */
  const out = [];
  if (st.group === "state") {
    for (const g of STATE_GROUPS) {
      const members = st.rows.filter((r) => stateGroup(r) === g.key);
      if (members.length) {
        out.push(groupRowEl(g.key, g.label));
        for (const r of members) out.push(...rowOf(r, st.group, st.calendar));
      }
    }
  } else for (const r of st.rows) out.push(...rowOf(r, st.group, st.calendar));
  return out;
}

/** @param {PaintEnv} env @param {boolean} force */
function paintTable({ st, block, body, grid, tb }, force) {
  const sig = sigOf(st.rows, st.group, st.calendar);
  if (force || sig !== st.tableSig) {
    st.tableSig = sig;
    block.tbody.replaceChildren(...tableNodes(st));
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
    reapplyTicks({ st, block, body, grid, tb });
    return;
  }
  // The numbers that move every reading change in place; the rows keep
  // their node, their open detail and their focus.
  for (const r of st.rows) {
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

/**
 * The painter of one list: `paint` draws the view that is on, the cards'
 * CPU trends are read on demand and kept for five minutes.
 * @param {PaintEnv} env
 */
export function createPainter(env) {
  const { st, block, grid, tb } = env;
  /** @type {Map<string, number[]>} */
  const trends = new Map();
  let trendsAt = 0;
  let trendsReading = false;

  const readTrends = async () => {
    const names = st.rows
      .filter((r) => r.state.label !== "parked")
      .map((r) => r.name);
    if (trendsReading || !names.length || Date.now() - trendsAt < TRENDS_MS)
      return;
    trendsReading = true;
    trendsAt = Date.now();
    const r = await fetchJson(
      "/data/fleet-cpu",
      "the stacks' CPU trends",
    ).catch(() => null);
    for (const n of names) trends.set(n, r?.ok ? cpuDay(r.body, n) : []);
    trendsReading = false;
    if (st.view === "cards") paint();
  };

  /** The loading or failed state of the shown view; true when it drew one. */
  function paintStatus() {
    const tableOn = st.view === "table";
    if (st.status === "loading") {
      if (tableOn) block.loading({ words: "Reading the fleet" });
      else setEntityGridState(grid, "loading", { count: 6 });
      return true;
    }
    if (st.status !== "failed") return false;
    if (tableOn)
      block.failed({
        what: "the fleet",
        why: st.failMessage,
        fix: "Check that the host answers, then try again",
      });
    else setEntityGridState(grid, "error", { message: st.failMessage });
    return true;
  }

  function paintCards() {
    const list = shownRows(st);
    const empty = emptyCards(st, list);
    if (empty) setEntityGridState(grid, "empty", empty);
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
      !st.rows.length &&
      !plateBtn.dataset.drive
    )
      drivable(plateBtn, EMPTY_NEW);
    decorateCards(env);
    reapplyTicks(env);
    tb.shows.counts({
      all: st.rows.length,
      attention: st.rows.filter(needsAttention).length,
      parked: st.rows.filter((r) => r.state.label === "parked").length,
    });
  }

  /** @param {boolean} [force] */
  function paint(force = false) {
    showView(env);
    if (paintStatus()) return;
    if (st.view === "table") paintTable(env, force);
    else paintCards();
  }
  return { paint };
}
