// The Containers card's row builders: one guest as a table row, as a card
// of the entity grid, as a pressure-list row and as the opened-row sentence.

import { h } from "../dom.js";
import { gib, guestTable } from "../host.js";
import { formGrid, gridRow } from "../gridrow.js";
import { stackHref } from "../router.js";
import { setBarCell } from "/static/kp/dist/kp-themes.min.js";

/** @typedef {ReturnType<typeof guestTable>[number]} GuestRow */

/** @param {number | null} used @param {number | null} max */
export const share = (used, max) => (used != null && max ? used / max : null);

/** @param {GuestRow} g the sentence under an opened row */
export function guestDetail(g) {
  return formGrid([
    gridRow({
      label: g.name || `CT ${g.vmid}`,
      control: h(
        "p",
        { class: "hk-detail" },
        `${g.name || `CT ${g.vmid}`} is ${g.status}${g.lock ? `, locked by ${g.lock}` : ""}. ${g.ramUsed == null ? "It is not running, so it has no CPU or memory figures." : `It uses ${gib(g.ramUsed)} of its ${gib(g.ramMax ?? 0)} GiB and ${g.cpuPct ?? 0} % of one core.`}`,
      ),
      actions: [
        g.stack
          ? h(
              "a",
              {
                class: "kp-button kp-button--sm",
                href: stackHref(g.stack),
              },
              "Open stack",
            )
          : h(
              "button",
              {
                type: "button",
                class: "kp-button kp-button--sm",
                disabled: "",
                "data-kp-disabled-reason": "This guest is not a stack",
              },
              "Open stack",
            ),
        h(
          "a",
          {
            class: "kp-button kp-button--sm",
            href: `/activity?view=host-log${g.stack ? `&source=${encodeURIComponent(g.stack)}` : ""}`,
          },
          "Open its log",
        ),
      ],
      buttons: true,
    }),
  ]);
}

/**
 * refresh-no-replay-1: the row is built and filled here, then merged into
 * the rows already there by the caller (`syncRows`).
 * @param {GuestRow} g
 */
export function guestRow(g) {
  const tr = h(
    "tr",
    {
      "data-kp-row-key": String(g.vmid),
      "data-vmid": g.vmid,
      "data-status": g.running ? "running" : "stopped",
    },
    h(
      "td",
      { "data-label": "Guest", "data-kp-sort-value": String(g.vmid) },
      h("b", null, g.name || `CT ${g.vmid}`),
      " ",
      h(
        "span",
        { class: "hk-ct" },
        `${g.kind === "unmanaged" && g.vmid <= 101 ? "VM" : "CT"} ${g.vmid}`,
      ),
    ),
    h(
      "td",
      { "data-label": "Stack" },
      g.stack
        ? h("a", { class: "hk-link", href: stackHref(g.stack) }, g.stack)
        : h("span", { class: "hk-muted" }, "—"),
    ),
    h(
      "td",
      {
        "data-label": "State",
        "data-kp-filter-value": g.running ? "running" : "stopped",
      },
      h(
        "span",
        {
          class: `kp-badge${g.running ? " kp-badge--success" : g.tone === "bad" ? " kp-badge--destructive" : ""}`,
        },
        g.status,
      ),
    ),
    h(
      "td",
      { "data-label": "CPU" },
      h("span", {
        class: "kp-bar-cell",
        "data-kp-bar": "",
        "data-hk-cpu": String(g.vmid),
      }),
    ),
    h(
      "td",
      { "data-label": "Memory" },
      h("span", {
        class: "kp-bar-cell",
        "data-kp-bar": "",
        "data-hk-mem": String(g.vmid),
      }),
    ),
    h("td", { "data-label": "Lock" }, g.lock || "—"),
    h(
      "td",
      { "data-label": "Kind", "data-kp-filter-value": g.kind },
      g.kind === "unmanaged" ? "not managed" : g.kind,
    ),
  );
  const s = share(g.ramUsed, g.ramMax);
  const cpu = /** @type {HTMLElement} */ (tr.querySelector("[data-hk-cpu]"));
  const mem = /** @type {HTMLElement} */ (tr.querySelector("[data-hk-mem]"));
  setBarCell(cpu, {
    value: g.cpuPct == null ? null : g.cpuPct / 100,
    text: g.cpuPct == null ? "—" : `${g.cpuPct} %`,
    sortValue: g.cpuPct ?? -1,
    label: "of one core",
  });
  setBarCell(mem, {
    value: s,
    text:
      g.ramUsed == null || g.ramMax == null
        ? "—"
        : `${gib(g.ramUsed)} of ${gib(g.ramMax)} GiB`,
    sortValue: s ?? -1,
    warnAt: 0.75,
    badAt: 0.9,
    label: "of its memory",
  });
  return tr;
}

/**
 * @param {GuestRow} g one card of the entity grid
 * @returns {import("/static/kp/dist/kp-themes.min.js").EntityData}
 */
export function guestCard(g) {
  return {
    key: String(g.vmid),
    name: g.name || `CT ${g.vmid}`,
    vmid: String(g.vmid),
    state: {
      label: g.status,
      tone: g.running
        ? "success"
        : g.tone === "bad"
          ? "destructive"
          : undefined,
    },
    figures: [
      {
        value: g.stack ?? (g.kind === "template" ? "template" : "none"),
        label: "stack",
      },
      { value: g.lock || "none", label: "lock" },
    ],
    flags:
      g.kind === "unmanaged" ? [{ label: "not managed", tone: "warning" }] : [],
    meters: [
      { label: "CPU", value: g.cpuPct == null ? null : g.cpuPct / 100 },
      {
        label: "RAM",
        value: share(g.ramUsed, g.ramMax),
        text:
          g.ramUsed == null || g.ramMax == null
            ? undefined
            : `${gib(g.ramUsed)} of ${gib(g.ramMax)} GiB`,
      },
    ],
    actions: [
      { id: "log", label: "Open its log" },
      {
        id: "stack",
        label: "Open stack",
        disabled: g.stack ? undefined : "This guest is not a stack",
      },
    ],
  };
}

/** @param {GuestRow} g one row of the memory-pressure list */
export function pressureRow(g) {
  return {
    key: String(g.vmid),
    lead: `CT ${g.vmid}`,
    label: g.name || `CT ${g.vmid}`,
    detail: `${gib(g.ramUsed ?? 0)} of ${gib(g.ramMax ?? 0)} GiB`,
    value: share(g.ramUsed, g.ramMax),
    pick: true,
  };
}
