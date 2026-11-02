// The open detail of a Stacks table row: the stack's apps, its last three
// backups and its CPU.

import { stateWord, h } from "./dom.js";
import { lastBackups } from "./fleetmodel.js";
import { formatDateTime } from "./format.js";
import { current } from "./store.js";

/** @typedef {import("./stacksview.js").Row} Row */

/** The columns of the Stacks table, the tick column included. */
export const COLS = 8;

/** @param {{name: string, health?: string | null, running?: boolean}[]} apps */
function appsPart(apps) {
  return h(
    "div",
    { class: "fl-detail__part" },
    h("h4", null, "Apps"),
    apps.length
      ? h(
          "ul",
          { class: "kp-action-list" },
          ...apps.map((a) =>
            h(
              "li",
              null,
              h(
                "span",
                { class: "kp-action-list__main" },
                h("strong", { class: "kp-action-list__title" }, a.name),
                h(
                  "span",
                  { class: "kp-action-list__text" },
                  a.health ?? (a.running ? "running" : "stopped"),
                ),
              ),
              stateWord(a.running ? "up" : "down", ["up", "down"]),
            ),
          ),
        )
      : h("p", { class: "fl-detail__none" }, "This stack declares no apps."),
  );
}

/**
 * @param {Row} r
 * @param {any} calendar
 */
function backupsPart(r, calendar) {
  const times = lastBackups(calendar?.stacks?.[r.name]);
  return h(
    "div",
    { class: "fl-detail__part" },
    h("h4", null, "Last three backups"),
    times.length
      ? h(
          "dl",
          { class: "kp-kv" },
          ...times.map((t) =>
            h("div", { class: "kp-kv__row" }, h("dt", null, formatDateTime(t))),
          ),
        )
      : h(
          "p",
          { class: "fl-detail__none" },
          r.backup.state === "nodata"
            ? "This stack keeps no data."
            : r.state.label === "parked"
              ? "A parked stack is not backed up."
              : r.backup.text,
        ),
    h("h4", null, "CPU"),
    h(
      "p",
      { class: "fl-detail__none" },
      r.cpuPct == null
        ? "Not measured."
        : `${Math.round(r.cpuPct * 10) / 10} % of its cores now.`,
    ),
  );
}

/**
 * The detail row of a stack.
 * @param {Row} r
 * @param {any} calendar
 */
export function detailOf(r, calendar) {
  const fleetStack = current().fleet?.stacks.find((s) => s.name === r.name);
  const apps = fleetStack?.apps ?? [];
  return h(
    "tr",
    { "data-kp-row-detail": "" },
    h(
      "td",
      { colspan: String(COLS) },
      h(
        "div",
        { class: "fl-detail" },
        appsPart(apps),
        backupsPart(r, calendar),
      ),
    ),
  );
}
