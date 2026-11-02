// The cells and rows of the Stacks table: the RAM and CPU bar cells, one
// stack's row, a group's header row and the signature that tells whether
// the rows must be rebuilt. Pure builders: they read no page state.

import { LIVE, h, selectCell } from "./dom.js";
import { drivable } from "./drivable.js";
import { OPEN, TICK } from "./fleetcontrols.js";
import { COLS, detailOf } from "./fleetdetail.js";
import { STATE_WORDS, stateGroup } from "./fleetmodel.js";
import { formatDateTime } from "./format.js";
import { stackHref } from "./router.js";
import { setBarCell } from "/static/kp/dist/kp-themes.min.js";

/** @typedef {import("./stacksview.js").Row} Row */

/** @param {Row} r */
export const cpuCell = (r) => ({
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
export const ramCell = (r) => ({
  value: r.state.label === "parked" || r.ramPct == null ? null : r.ramPct / 100,
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
function nameCell(r) {
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
  return h(
    "td",
    { "data-kp-sort-value": r.name },
    h(
      "span",
      { class: "fl-pair" },
      link,
      h("span", { class: "kp-timestamp" }, `CT ${r.vmid}`),
    ),
  );
}

/** @param {Row} r */
function stateCell(r) {
  return h(
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
  );
}

/**
 * The stack's row and its detail row.
 * @param {Row} r
 * @param {"state" | "none"} group
 * @param {any} calendar
 */
export function rowOf(r, group, calendar) {
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
    nameCell(r),
    stateCell(r),
    h(
      "td",
      {
        class: "kp-text-end",
        "data-kp-sort-value": String(r.appsTotal ? r.appsUp / r.appsTotal : 0),
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
  return [tr, detailOf(r, calendar)];
}

/** @param {string} key @param {string} label */
export const groupRowEl = (key, label) =>
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

/**
 * What decides a rebuild of the table's rows.
 * @param {Row[]} list
 * @param {"state" | "none"} group
 * @param {any} calendar
 */
export const sigOf = (list, group, calendar) =>
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
