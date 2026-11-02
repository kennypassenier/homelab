// The Stacks list's cards: what Live view needs marked on the entity cards
// the package draws, which stacks the Show filter keeps, and the empty plate.

import { drivable } from "./drivable.js";
import {
  ROW_ACTION,
  ROW_LOGS,
  ROW_UPDATE,
  OPEN,
  TICK,
} from "./fleetcontrols.js";
import { gb } from "./fleet.js";
import { entityOf, needsAttention } from "./fleetmodel.js";

/** @typedef {import("./fleetpaint.js").PaintEnv} PaintEnv */
/** @typedef {import("./fleetpaint.js").ListState} ListState */
/** @typedef {import("./stacksview.js").Row} Row */

/** Mark what the entity cards drew, so Live view reaches it. @param {PaintEnv} env */
export function decorateCards({ grid, st }) {
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
    const r = st.rows.find((x) => x.name === key);
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
}

/** @param {ListState} st @param {Row[]} list */
export function emptyCards(st, list) {
  if (!st.rows.length)
    return {
      title: "The host runs no stacks yet",
      message:
        "A stack is one container with the apps it runs. Start one from a preset or from a bundle.",
      actionLabel: "New stack",
    };
  if (list.length) return null;
  return {
    title:
      st.show === "parked" ? "No stack is parked" : "No stack needs attention",
    message: "Show All to see every stack again.",
    actionLabel: "Show all",
  };
}

/** @param {ListState} st */
export function shownRows(st) {
  return st.rows.filter((r) =>
    st.show === "all"
      ? true
      : st.show === "parked"
        ? r.state.label === "parked"
        : needsAttention(r),
  );
}
