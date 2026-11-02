// The static skeleton of the Stacks list: the kp datatable (its columns,
// its attributes, its Live view marks) and the entity-card grid.

import { h, tableBlock } from "./dom.js";
import { drivable } from "./drivable.js";
import { SEARCH, SHOW_ALL, TICK_ALL } from "./fleetcontrols.js";

/** The table block, configured for the Stacks list. */
export function buildTable() {
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
  return block;
}

/** The entity-card grid. */
export function buildGrid() {
  return h("div", {
    class: "kp-entity-grid fl-cards",
    "data-kp-entity-grid": "",
    "data-kp-entity-min": "19rem",
    "data-kp-entity-meters": "1",
    "data-kp-selectable": "",
    "data-kp-entity-link": "/stacks/{key}",
    "data-kp-entity-inline": "3",
    "aria-label": "Stacks",
  });
}
