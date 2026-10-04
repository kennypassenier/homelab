// redesign-final Low (Kenny's rule: multi-sort without Shift): on every
// kp datatable that sorts on several keys, a plain click on a header adds
// that column as a further key — ascending, then descending, then out
// again — exactly what kp-themes does on Shift+click; the same for Enter
// and Space. kp's own "Reset the sort" clears them, and it is the Live view
// control `table-reset-sort` on the table's remembered name. The dashboard's
// own sortable tables (ui.js `sortHead`, `sortableTable`) follow the same
// rule through sortstate.js `nextSort`. Installed once by main.js.

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

const HEAD = "[data-kp-datatable][data-kp-sort-multi] th[data-kp-sort]";
const KP_RESET = "[data-kp-datatable-sort-reset]";

/**
 * The event a plain press on a multi-sort header becomes: the same press
 * with Shift held, which kp-themes reads as "add this column as a key".
 * @param {Event} e
 * @returns {Event | null} null when the press stays as it is
 */
export function asAddKey(e) {
  if (!(e instanceof MouseEvent || e instanceof KeyboardEvent)) return null;
  if (e.shiftKey) return null;
  const t = /** @type {Element | null} */ (e.target);
  if (!t || typeof t.closest !== "function" || !t.closest(HEAD)) return null;
  if (e instanceof KeyboardEvent) {
    if (e.key !== "Enter" && e.key !== " ") return null;
    return new KeyboardEvent(e.type, {
      key: e.key,
      code: e.code,
      bubbles: true,
      cancelable: true,
      shiftKey: true,
    });
  }
  return new MouseEvent(e.type, {
    bubbles: true,
    cancelable: true,
    shiftKey: true,
    clientX: e.clientX,
    clientY: e.clientY,
  });
}

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

/** Installs the plain-click multi-sort on the whole document. */
export function installSortClick() {
  for (const type of ["click", "keydown"])
    document.addEventListener(
      type,
      (e) => {
        const next = asAddKey(e);
        if (!next) return;
        e.stopImmediatePropagation();
        e.preventDefault();
        /** @type {Element} */ (e.target).dispatchEvent(next);
      },
      true,
    );
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
