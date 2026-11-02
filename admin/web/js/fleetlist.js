// The Stacks page's list on kp-themes 11 (feat-kp11demo-12, the approved
// stacks demo): the fleet as a kp datatable that groups by state and opens a
// row in place, or as kp entity cards that keep one height; one kp selection
// bar over both for Deploy, Back up now and Update…; a Deploy that waits ten
// seconds behind an undo toast (or asks first, the dashboard's batch
// dialog). Everything it draws is the package's: the table's own loading
// layer, empty and failed states, the entity grid's skeleton cards and
// plates. fleetmodel.js is the pure half.

import { openAction } from "./actiondialog.js";
import { h } from "./dom.js";
import { drivable } from "./drivable.js";
import { CLEAR } from "./fleetcontrols.js";
import { UNDO_MS, createSelectionBar, wireBatch } from "./fleetbar.js";
import { buildGrid, buildTable } from "./fleetdom.js";
import { decorateCards } from "./fleetcards.js";
import { createPainter } from "./fleetpaint.js";
import { reapplyTicks, wireTicks } from "./fleetticks.js";
import { createToolbar, initialViewState, writeUrl } from "./fleettoolbar.js";
import { stackHref } from "./router.js";
import { bindSelection } from "./selectbind.js";
import { updateHrefFor } from "./updateflow.js";
import { keyRow, section } from "./ui.js";
import {
  attachIdmarks,
  attachDataTables,
  dataTable,
} from "/static/kp/dist/kp-themes.min.js";

export { UNDO_MS };

/** @typedef {import("./stacksview.js").Row} Row */

/**
 * Wire the cards' own events: their buttons, retry and the empty plate.
 * @param {import("./fleetpaint.js").PaintEnv} env
 * @param {{navigate: (href: string) => void, openNew: () => void,
 *   retry: () => void}} ctx
 * @param {(force?: boolean) => void} paint
 */
function wireCards(env, ctx, paint) {
  const { grid, block, st } = env;
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
    if (!st.rows.length) ctx.openNew();
    else if (st.show !== "all") {
      st.show = "all";
      writeUrl(st);
      paint();
    }
  });
  block.wrap.addEventListener("kp-datatable-retry", () => ctx.retry());
  new MutationObserver(() => decorateCards(env)).observe(grid, {
    childList: true,
    subtree: true,
  });
}

/** The card that frames the list. @param {HTMLElement} listBox @param {HTMLElement} errs @param {HTMLElement} toastBed */
function buildCard(listBox, errs, toastBed) {
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
    body: [errs, listBox, toastBed],
    foot: ["Read from the host, live · CPU is the last reading", foot],
  });
  card.el.classList.add("fl-card");
  return card;
}

/**
 * The Stacks list: a card with its controls, the selection bar and either
 * the table or the cards.
 * @param {{navigate: (href: string) => void, openNew: () => void,
 *   retry: () => void}} ctx
 */
export function fleetList(ctx) {
  /** @type {import("./fleetpaint.js").ListState} */
  const st = {
    ...initialViewState(),
    rows: [],
    calendar: null,
    status: "loading",
    failMessage: "",
    tableSig: "",
    ticked: new Set(),
  };
  /** @type {(force?: boolean) => void} */
  let paint = () => {};
  const tb = createToolbar(st, (force) => paint(force));
  const bar = createSelectionBar(tb.safety.el);
  const toastBed = h("div", {
    class: "fl-toasts",
    "aria-label": "Toasts of the stacks list",
  });
  const block = buildTable();
  const grid = buildGrid();
  const body = h("div", { class: "fl-body", id: "fl-body" }, block.wrap, grid);
  // The bar stays OUTSIDE the element it watches (#fl-body): kp's selection
  // redraws its own count, and inside its target that redraw feeds itself.
  const listBox = h("div", { class: "fl-list" }, bar, tb.cardBar, body);
  const errs = h("div", { class: "fl-errors", "aria-live": "polite" });
  errs.hidden = true;
  const card = buildCard(listBox, errs, toastBed);

  const sel = bindSelection(
    card.el,
    bar,
    () => ({
      deploy: (items) => {
        const ok = items.filter(
          (i) =>
            st.rows.find((r) => r.name === i.key)?.state.label !== "parked",
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

  const env = { st, block, grid, body, tb };
  wireBatch({ bar, toastBed, tbody: block.tbody, st, ctx });
  wireTicks(env);
  ({ paint } = createPainter(env));
  wireCards(env, ctx, (force) => paint(force));

  paint();
  return {
    el: card.el,
    /** Everything the page knows changed. @param {{rows: Row[], calendar: any}} m */
    set: (m) => {
      st.rows = m.rows;
      st.calendar = m.calendar;
      for (const n of [...st.ticked])
        if (!st.rows.some((r) => r.name === n)) st.ticked.delete(n);
      st.status = "ready";
      paint();
    },
    /** @param {"loading" | "failed"} s @param {string} [why] */
    state: (s, why = "") => {
      st.status = s;
      st.failMessage = why;
      paint();
    },
    tick: (/** @type {string[]} */ stacks) => {
      st.ticked.clear();
      for (const s of stacks) st.ticked.add(s);
      reapplyTicks(env);
    },
    ticked: () => [...st.ticked],
    table: () => table,
    stop: () => {
      sel.stop();
      detachTables();
      detachMarks();
    },
  };
}
