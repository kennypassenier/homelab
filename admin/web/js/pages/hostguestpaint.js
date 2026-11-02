// The Containers card's painters: the reading or failed plates of its three
// views, the table rows, and the memory-pressure list.

import { drivable, declare } from "../drivable.js";
import { guestTable } from "../host.js";
import { setBulletsQuiet, syncNode, syncRows } from "../inplace.js";
import {
  setBullets,
  setEntityGridState,
} from "/static/kp/dist/kp-themes.min.js";
import { guestDetail, guestRow, pressureRow } from "./hostguestrows.js";

const SORT_GUESTS = declare({
  id: "sort-host-containers",
  page: "host",
  opens: "view",
  row: "id|container|status|memory|cpu|stack",
  what: "sort the Containers table: each header clicked is a further sort key (again reverses it, a third time takes it out)",
});

/** @typedef {ReturnType<typeof guestTable>} GuestView */

/**
 * The reading or failed plates of all three views.
 * @param {{dt: ReturnType<typeof import("/static/kp/dist/kp-themes.min.js").dataTable>, cards: HTMLElement, bullets: HTMLElement}} v
 * @param {string | null} why set: the read failed with this reason
 */
export function paintPending({ dt, cards, bullets }, why) {
  if (why) {
    dt?.busy(null);
    dt?.fail(why);
    setEntityGridState(cards, "error", { message: why });
    setBullets(bullets, {
      threshold: 0.75,
      rows: [],
      state: "error",
      message: why,
    });
    return;
  }
  dt?.busy({
    text: "Asking the host for its containers (pct list)…",
    overlay: true,
  });
  dt?.state("loading");
  setEntityGridState(cards, "loading");
  setBullets(bullets, {
    threshold: 0.75,
    rows: [],
    state: "loading",
    loadingWords: "Reading the guests…",
  });
}

/**
 * @param {{dt: ReturnType<typeof import("/static/kp/dist/kp-themes.min.js").dataTable>, tbody: HTMLElement, table: HTMLElement}} v
 * @param {GuestView} view
 */
export function paintRows({ dt, tbody, table }, view) {
  // refresh-no-replay-1: the rows are merged into the rows already there,
  // so an opened row stays open and no bar wipes in again.
  syncRows(tbody, view.map(guestRow));
  // An opened row's sentence says what the guest uses now.
  for (const open of /** @type {NodeListOf<HTMLTableRowElement>} */ (
    tbody.querySelectorAll(":scope > tr[data-kp-row-detail]")
  )) {
    const key = open.previousElementSibling?.getAttribute("data-kp-row-key");
    const g = view.find((x) => String(x.vmid) === key);
    const mine = open.cells[0]?.firstElementChild;
    if (g && mine) syncNode(mine, guestDetail(g));
  }
  dt?.refresh();
  const heads = /** @type {HTMLElement[]} */ ([
    ...table.querySelectorAll("thead th[data-kp-sort]"),
  ]);
  ["id", "stack", "status", "cpu", "memory"].forEach((k, i) => {
    if (heads[i]) drivable(heads[i], SORT_GUESTS, k);
  });
}

/**
 * @param {HTMLElement} bullets
 * @param {GuestView} view
 * @returns {number} how many guests have a memory share
 */
export function paintPressure(bullets, view) {
  const withRam = view.filter((g) => g.ramUsed != null && g.ramMax);
  setBulletsQuiet(bullets, {
    threshold: 0.75,
    bad: 0.9,
    max: 1,
    format: (v) => `${Math.round(v * 100)} %`,
    rows: withRam.map(pressureRow),
    state: "ready",
    emptyTitle: "No guest is running",
    emptyText: "A running guest shows its memory share here.",
    loadingRows: 8,
  });
  return withRam.length;
}
