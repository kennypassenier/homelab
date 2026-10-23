// The Stacks page's two glance cards on kp-themes 11 (feat-kp11demo-12,
// approved): "Which stacks use the most" (kp bullets, ranked worst first,
// with a share bar of the root volume beside it) and "Every stack, every
// night" (kp heat: stacks against the last nights, one summary row of how
// many missed each night). Both draw their own loading, empty and failed
// states, so a slow or failed read keeps their footprint.

import { h } from "./dom.js";
import { declare, drivable } from "./drivable.js";
import { hungryRows, nightKey, nightsGrid, readsOf } from "./fleetmodel.js";
import { formatDateTime, formatDay, gbText } from "./format.js";
import { gb } from "./fleet.js";
import { current } from "./store.js";
import { section, segSwitch } from "./ui.js";
import { setBullets } from "/static/kp/js/bullets.js";
import {
  setHeat,
  setHeatLegend,
  HEAT_PICK_EVENT,
  HEAT_ROW_EVENT,
} from "/static/kp/js/heat.js";
import { setStackbar, STACKBAR_PICK_EVENT } from "/static/kp/js/stackbar.js";
import { attachHoverCards } from "/static/kp/js/hovercard.js";

/** @typedef {import("./stacksview.js").Row} Row */

const MEASURE = declare({
  id: "stacks-hungry-measure",
  page: "overview",
  opens: "view",
  row: "ram|cpu|disk",
  what: "rank the stacks by memory, CPU or disk",
});
const NIGHTS = declare({
  id: "stacks-nights-window",
  page: "overview",
  opens: "view",
  row: "14|30",
  what: "show the last 14 or the last 30 nights of backups per stack",
});
const HUNGRY_ROW = declare({
  id: "stacks-hungry-open",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "open a ranked stack's hub",
});
const NIGHT_PICK = declare({
  id: "stacks-night-pick",
  page: "overview",
  opens: "view",
  row: "<stack>/<night>",
  what: "pick one stack's night in the grid: the list under it shows that backup",
});
const NIGHT_ROW = declare({
  id: "stacks-night-stack",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "press a stack's name in the nights grid: the list under it shows its last three backups",
});

const MEASURE_KEY = "homelab.stacks.hungry";
const NIGHTS_KEY = "homelab.stacks.nights";
/** @param {string} key @param {string} fallback */
const remembered = (key, fallback) => {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
};

/** @param {number} n @param {string} one */
const plural = (n, one) => `${n} ${n === 1 ? one : `${one}s`}`;

/**
 * Capacity percent per stack from `/data/capacity`.
 * @typedef {Map<string, {cpu: number | null, mem: number | null, disk: number | null}>} Capacity
 */

/**
 * @param {{navigate: (href: string) => void, stackHref: (n: string) => string,
 *   retry: () => void}} ctx
 */
export function fleetGlance(ctx) {
  /** @type {"ram" | "cpu" | "disk"} */
  let measure = /** @type {any} */ (remembered(MEASURE_KEY, "ram"));
  if (!["ram", "cpu", "disk"].includes(measure)) measure = "ram";
  let win = remembered(NIGHTS_KEY, "14") === "30" ? 30 : 14;
  /** @type {string | null} */
  let only = null;
  /** @type {Row[]} */
  let rows = [];
  /** @type {any} */
  let calendar = null;
  /** @type {Capacity | null} */
  let capacity = null;
  /** @type {"ready" | "loading" | "failed"} */
  let capState = "loading";
  /** @type {"ready" | "loading" | "failed"} */
  let calState = "loading";
  let capWhy = "";
  let calWhy = "";
  let fleetState = "loading";

  // ── Which stacks use the most, and who fills the root disk ───────────
  const measures = segSwitch({
    label: "Measure",
    value: measure,
    items: [
      {
        value: "ram",
        label: "Memory",
        hint: "Rank by the share of each stack's memory limit in use",
      },
      {
        value: "cpu",
        label: "CPU",
        hint: "Rank by the share of each stack's cores in use now",
      },
      {
        value: "disk",
        label: "Disk",
        hint: "Rank by the share of each stack's volume that is full",
      },
    ],
    onChange: (v) => {
      measure = /** @type {any} */ (v);
      try {
        localStorage.setItem(MEASURE_KEY, v);
      } catch {
        // kept for this visit only
      }
      paintHungry();
    },
    mark: (b, v) => void drivable(b, MEASURE, v),
  });
  const line = h("p", { class: "fg-line", role: "status" });
  const list = h("div", {
    class: "kp-bullets fg-bullets",
    "data-kp-bullets": "",
  });
  const split = h("figure", {
    class: "kp-stackbar kp-stackbar--lg",
    "data-kp-stackbar": "",
  });
  const diskPane = h(
    "div",
    { class: "fg-pane fg-pane--disk" },
    h("h3", { class: "fg-pane__title" }, "Who fills the root disk"),
    split,
  );
  const rankPane = h(
    "div",
    { class: "fg-pane fg-pane--rank" },
    h("h3", { class: "fg-pane__title" }, "Hungriest stacks"),
    h(
      "div",
      { class: "fg-ctl" },
      h("b", { class: "fg-ctl__label" }, "Measure"),
      measures.el,
    ),
    line,
    list,
  );
  const hungry = section({
    title: "Which stacks use the most",
    desc: "Ranks the stacks by what they use of their allowance, worst first, and splits the root disk; pressing a part of the bar that is a stack narrows the ranking to it.",
    id: "stack-hungry",
    body: h("div", { class: "fg-grid" }, diskPane, rankPane),
    foot: [
      "Read from the host and Prometheus, live",
      "amber from 75 % · red from 90 %",
    ],
  });

  /** @param {Row[]} list */
  const diskParts = (list) => {
    void list;
    const f = current().fleet;
    const d = f?.host.disk_detail;
    if (!f || !d) return null;
    const total = Math.round(d.root_lv_size_gb);
    const dirs = d.top_dirs.map(([path, size]) => ({
      key: path,
      label: path,
      value: Math.round(size * 10) / 10,
    }));
    const named = dirs.reduce((t, p) => t + p.value, 0);
    const used = (f.host.disk_pct / 100) * d.root_lv_size_gb;
    const other = Math.max(0, Math.round((used - named) * 10) / 10);
    return {
      total,
      parts: [
        ...dirs,
        ...(other > 0
          ? [{ key: "other", label: "Everything else", value: other }]
          : []),
      ],
    };
  };

  function paintHungry() {
    measures.set(measure);
    const state =
      fleetState === "loading" || (capState === "loading" && measure !== "ram")
        ? "loading"
        : fleetState === "failed" ||
            (capState === "failed" && measure !== "ram")
          ? "error"
          : "ready";
    const shown = hungryRows(rows, measure, capacity ?? new Map(), only, gb);
    setBullets(list, {
      threshold: 75,
      bad: 90,
      max: 100,
      sort: "worst",
      rows: shown,
      state:
        state === "ready" && shown.length === 0 && rows.length
          ? "empty"
          : state,
      message:
        capState === "failed"
          ? capWhy
          : "The host did not answer, so no stack can be ranked.",
      emptyTitle: "No stack is running",
      emptyText: "Start a stack and it is ranked here.",
      loadingRows: 5,
      loadingWords: "Reading the fleet",
    });
    line.textContent =
      state !== "ready"
        ? ""
        : only
          ? `Only ${only}. Press its part of the bar again to see every stack.`
          : `The ${Math.min(5, shown.length)} of ${plural(rows.filter((r) => r.state.label !== "parked").length, "running stack")} that use the most, worst ${measure === "ram" ? "memory" : measure === "cpu" ? "CPU" : "disk"} first.`;
    const dp = diskParts(rows);
    setStackbar(split, {
      title: dp ? `Root volume, ${gbText(dp.total)}` : "Root volume",
      total: dp?.total,
      format: (v) => gbText(v),
      restLabel: "Free space",
      marks: [{ at: 0.8, label: "amber at 80 %" }],
      parts: dp?.parts ?? [],
      label: "Who fills the root disk",
      state:
        fleetState === "loading"
          ? "loading"
          : fleetState === "failed"
            ? "error"
            : dp
              ? "ready"
              : "empty",
      message:
        fleetState === "failed"
          ? "The host did not answer, so its disk is unknown."
          : "The host reports no filesystem detail yet.",
    });
  }
  split.addEventListener(STACKBAR_PICK_EVENT, (e) => {
    const key = /** @type {CustomEvent} */ (e).detail.key;
    if (rows.some((r) => r.name === key)) {
      only = only === key ? null : key;
      paintHungry();
    }
  });
  list.addEventListener("kp-bullets-pick", (e) => {
    const key = /** @type {CustomEvent} */ (e).detail.key;
    ctx.navigate(ctx.stackHref(key));
  });
  list.addEventListener("kp-bullets-retry", () => ctx.retry());
  split.addEventListener("kp-stackbar-retry", () => ctx.retry());
  new MutationObserver(() => {
    for (const b of list.querySelectorAll(
      ".kp-bullets__link[data-kp-bullets-key]",
    )) {
      if (!(b instanceof HTMLElement) || b.dataset.drive) continue;
      drivable(b, HUNGRY_ROW, b.dataset.kpBulletsKey ?? "");
    }
  }).observe(list, { childList: true, subtree: true });

  // ── Every stack, every night ──────────────────────────────────────────
  const nightsSeg = segSwitch({
    label: "Nights",
    value: String(win),
    items: [
      {
        value: "14",
        label: "14 nights",
        hint: "The last two weeks, to see a stack that failed twice",
      },
      {
        value: "30",
        label: "30 nights",
        hint: "The last month, to judge how long backups are kept",
      },
    ],
    onChange: (v) => {
      win = v === "30" ? 30 : 14;
      try {
        localStorage.setItem(NIGHTS_KEY, String(win));
      } catch {
        // kept for this visit only
      }
      readout.replaceChildren();
      paintNights();
    },
    mark: (b, v) => void drivable(b, NIGHTS, v),
  });
  const heat = h("div", {
    class: "kp-heat fg-heat",
    "data-kp-heat": "",
    "data-kp-heat-toggle": "",
    "data-kp-heat-latest": "end",
    id: "fg-nights",
  });
  const readout = h("div", { class: "fg-readout", "aria-live": "polite" });
  const nightsCtl = h(
    "div",
    { class: "fg-ctl" },
    h("b", { class: "fg-ctl__label" }, "Nights"),
    nightsSeg.el,
  );
  const nights = section({
    title: "Every stack, every night",
    desc: "Draws the stacks against the last nights as a grid of coloured cells, with one summary row of how many stacks missed each night, so a stack that fails often shows as a stripe.",
    id: "stack-nights",
    body: h("div", { class: "fg-nights" }, nightsCtl, heat, readout),
    foot: [
      "The backup calendar the Backups page reads",
      "a parked stack is paused, never failed",
    ],
  });

  // kp's heat legend under the grid.
  setHeatLegend(heat, [
    { tone: "ok", label: "backed up" },
    { tone: "bad", label: "missed" },
    { tone: "paused", label: "paused (parked)" },
  ]);

  const narrow = () => matchMedia("(max-width: 40rem)").matches;
  const count = () => (narrow() ? Math.min(7, win) : win);
  const every = () => {
    if (narrow()) return 1;
    const rem =
      parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    const label =
      parseFloat(
        getComputedStyle(heat).getPropertyValue("--kp-heat-label") || "0",
      ) * rem;
    const room = Math.max(1, heat.clientWidth - label);
    return Math.max(2, Math.ceil((3.5 * rem) / (room / count())) + 1);
  };
  let seenWidth = 0;
  // Only a drawn grid depends on the width (its labels thin out): a grid in a
  // state is the package's plate and is left as it stands.
  const nightsState = () =>
    fleetState === "loading" || calState === "loading"
      ? "loading"
      : fleetState === "failed" || calState === "failed"
        ? "error"
        : rows.length
          ? "ready"
          : "empty";
  const resize = new ResizeObserver(() => {
    if (nightsState() !== "ready") return;
    if (Math.round(heat.clientWidth) !== seenWidth) paintNights();
  });
  resize.observe(heat);

  /** @param {string} night */
  const dayOf = (night) => {
    const [y, m, d] = night.split("-").map(Number);
    return formatDay(Date.UTC(y, m - 1, d, 12) / 1000, { timeZone: "UTC" });
  };

  function paintNights() {
    nightsSeg.set(String(win));
    seenWidth = Math.round(heat.clientWidth);
    const state = nightsState();
    if (state !== "ready") {
      setHeat(heat, {
        columns: [],
        rows: [],
        state,
        message:
          state === "empty"
            ? "The host runs no stacks, so no backup is recorded."
            : calWhy || "The host did not answer, so no night is known.",
        skeleton: { rows: 8, columns: count() },
        label: "Backups per stack per night",
      });
      return;
    }
    const g = nightsGrid(
      rows,
      readsOf(calendar, rows),
      Date.now() / 1000,
      count(),
      { every: every(), narrow: narrow() },
      dayOf,
    );
    nightsModel = g.nights;
    setHeat(heat, {
      label: "Backups per stack per night",
      columns: g.columns,
      toggle: true,
      summary: g.summary,
      rows: g.rows,
    });
  }
  /** @type {string[]} */
  let nightsModel = [];
  heat.addEventListener(HEAT_PICK_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (!d.picked || d.colKey == null || !d.rowKey) {
      readout.replaceChildren();
      return;
    }
    const r = rows.find((x) => x.name === d.rowKey);
    const times = calendar?.stacks?.[d.rowKey] ?? [];
    const hit = /** @type {number[]} */ (times).filter((t) => {
      const k = nightKey(t);
      return k === d.colKey;
    });
    readout.replaceChildren(
      h(
        "ul",
        { class: "kp-action-list" },
        h(
          "li",
          null,
          h(
            "span",
            { class: "kp-action-list__main" },
            h(
              "strong",
              { class: "kp-action-list__title" },
              `${d.rowKey} on ${dayOf(String(d.colKey))}`,
            ),
            h(
              "span",
              { class: "kp-action-list__text" },
              hit.length
                ? `Backed up at ${formatDateTime(Math.max(...hit)).slice(11)}`
                : r?.state.label === "parked"
                  ? "Paused: the stack is parked."
                  : "No backup was taken that night.",
            ),
          ),
        ),
      ),
    );
  });
  heat.addEventListener(HEAT_ROW_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (!d.pressed || !d.key) {
      readout.replaceChildren();
      return;
    }
    const times = /** @type {number[]} */ (calendar?.stacks?.[d.key] ?? [])
      .slice()
      .sort((a, b) => b - a)
      .slice(0, 3);
    readout.replaceChildren(
      h(
        "ul",
        { class: "kp-action-list" },
        ...(times.length
          ? times.map((t) =>
              h(
                "li",
                null,
                h(
                  "span",
                  { class: "kp-action-list__main" },
                  h(
                    "strong",
                    { class: "kp-action-list__title" },
                    `${d.key}, ${formatDateTime(t)}`,
                  ),
                ),
              ),
            )
          : [h("li", null, `${d.key} has no backups.`)]),
      ),
    );
  });
  // Live view reaches a row's name and a night's cell by their keys.
  new MutationObserver(() => {
    for (const el of heat.querySelectorAll(
      ".kp-heat__label-button[data-key], button.kp-heat__cell[data-row-key]",
    )) {
      if (!(el instanceof HTMLElement) || el.dataset.drive) continue;
      if (el.matches(".kp-heat__label-button")) {
        if (el.dataset.key !== "all")
          drivable(el, NIGHT_ROW, el.dataset.key ?? "");
      } else if (el.dataset.rowKey !== "all")
        drivable(el, NIGHT_PICK, `${el.dataset.rowKey}/${el.dataset.colKey}`);
    }
  }).observe(heat, { childList: true, subtree: true });

  attachHoverCards(nights.el);

  return {
    els: [hungry.el, nights.el],
    /**
     * @param {{rows?: Row[], calendar?: any, capacity?: Capacity | null,
     *   fleet?: "ready" | "loading" | "failed",
     *   cal?: "ready" | "loading" | "failed", calWhy?: string,
     *   cap?: "ready" | "loading" | "failed", capWhy?: string}} m
     */
    set: (m) => {
      if (m.rows) rows = m.rows;
      if ("calendar" in m) calendar = m.calendar;
      if ("capacity" in m) capacity = m.capacity ?? null;
      if (m.fleet) fleetState = m.fleet;
      if (m.cal) calState = m.cal;
      if (m.calWhy != null) calWhy = m.calWhy;
      if (m.cap) capState = m.cap;
      if (m.capWhy != null) capWhy = m.capWhy;
      paintHungry();
      paintNights();
    },
    stop: () => resize.disconnect(),
    nights: () => nightsModel,
  };
}
