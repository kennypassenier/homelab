// Live view's handle on kp-themes' "Reset the sort" button. A plain click on
// a header already adds a sort key (every datatable carries
// data-kp-sort-multi="click"); the reset clears them, and it is the Live view
// control `table-reset-sort` on the table's remembered name. Installed once
// by main.js.

import { declare, drivable } from "./drivable.js";

export const RESET_SORT = declare({
  id: "table-reset-sort",
  page: "host",
  opens: "view",
  row: "<table>",
  what: "a table's Reset sort: clear every sort key it was given (each header clicked is a further key)",
  shows: "while a table is sorted",
  reach: [{ do: "click", control: "sort-host-containers", row: "status" }],
});

const KP_RESET = "[data-kp-datatable-sort-reset]";

/** Marks kp's Reset the sort buttons under `root` for Live view. @param {ParentNode} root */
function markResets(root) {
  for (const b of root.querySelectorAll(`${KP_RESET}:not([data-drive])`)) {
    const wrap = b.closest("[data-kp-datatable]");
    const el = /** @type {HTMLElement} */ (b);
    drivable(
      el,
      RESET_SORT,
      /** @type {HTMLElement | null} */ (wrap)?.dataset.kpRemember || "table",
    );
    el.title = "Clear every sort key this table was given";
  }
}

/** Marks kp's Reset the sort buttons for Live view, now and as tables are drawn. */
export function installTableResets() {
  markResets(document);
  new MutationObserver((ms) => {
    for (const m of ms)
      for (const n of m.addedNodes)
        if (n instanceof Element) {
          if (n.matches(KP_RESET))
            markResets(/** @type {any} */ (n.parentNode));
          else markResets(n);
        }
  }).observe(document.body, { childList: true, subtree: true });
}
