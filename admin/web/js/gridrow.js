// feat-kpadopt-2 (Kenny, 2026-10-19: "every form, fact list and button group
// on .kp-formgrid"): the one way the operations pages lay out a label, a
// control, a reading and a button on shared column tracks. kp-themes'
// `.kp-formgrid` owns the tracks; this only builds its rows, so no page
// writes a left/right flex row of its own.

import { h } from "./dom.js";

/**
 * @typedef {import("./dom.js").Child} Child
 * @typedef {{label?: Child, labelFor?: string, labelId?: string,
 *   control?: Child | Child[], value?: Child | Child[],
 *   actions?: Child | Child[], buttons?: boolean, wide?: boolean,
 *   hidden?: boolean, cls?: string, hideLabel?: boolean,
 *   span?: "control-value" | "wide" | "all"}} GridRow
 */

/**
 * One row of a form grid: label | control | value | actions. `wide` lets
 * the control run over the value track; `buttons` is a row of buttons.
 * @param {GridRow} spec
 * @returns {HTMLElement}
 */
export function gridRow(spec) {
  /** @type {Child[]} */
  const kids = [];
  if (spec.label != null) {
    kids.push(
      spec.labelFor
        ? h(
            "label",
            { class: "kp-formgrid__label", for: spec.labelFor },
            spec.label,
          )
        : h(
            "span",
            {
              class: "kp-formgrid__label",
              id: spec.labelId ?? null,
              "aria-hidden": spec.hideLabel ? "true" : null,
            },
            spec.label,
          ),
    );
  }
  const list = (/** @type {Child | Child[] | undefined} */ x) =>
    x == null ? [] : Array.isArray(x) ? x : [x];
  if (spec.control != null)
    kids.push(
      h(
        "div",
        {
          class: "kp-formgrid__control",
          "data-kp-formgrid-span": spec.span ?? (spec.wide ? "wide" : null),
        },
        ...list(spec.control),
      ),
    );
  if (spec.value != null)
    kids.push(h("div", { class: "kp-formgrid__value" }, ...list(spec.value)));
  if (spec.actions != null)
    kids.push(
      h("div", { class: "kp-formgrid__actions" }, ...list(spec.actions)),
    );
  const row = h(
    "div",
    {
      class: `kp-formgrid__row${spec.buttons ? " kp-formgrid__row--buttons" : ""}${spec.cls ? ` ${spec.cls}` : ""}`,
    },
    ...kids,
  );
  row.hidden = spec.hidden === true;
  return row;
}

/**
 * A form grid around rows.
 * @param {HTMLElement[]} rows
 * @param {string} [cls]
 * @returns {HTMLElement}
 */
export const formGrid = (rows, cls = "") =>
  h("div", { class: `kp-formgrid${cls ? ` ${cls}` : ""}` }, ...rows);

/**
 * A plain button group for a row's control track.
 * @param {Child[]} buttons
 */
export const buttonGroup = (buttons) =>
  h("div", { class: "kp-formgrid__buttons" }, ...buttons);
