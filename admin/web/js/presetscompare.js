// The Presets page's second card on kp-themes 11 (feat-kp11demo-12.5, the
// approved presets demo with the rejected compare section redone): tick two
// or three presets in a kp datatable, press Compare, and read only the
// settings that differ; below it, a kp share bar shows what the chosen
// preset would do to the host's memory. Every part loads, empties and fails
// with the theme's own states: the table's busy layer, the card's busy card
// over the whole footprint, the share bar's own loading.

import { h, selectCell, tableBlock } from "./dom.js";
import { declare, drivable } from "./drivable.js";
import { gbText } from "./format.js";
import { ramText } from "./presetsview.js";
import { current } from "./store.js";
import { ensureStyle, section } from "./ui.js";
import { setCardState } from "/static/kp/dist/kp-themes.min.js";
import { attachDataTables } from "/static/kp/dist/kp-themes.min.js";
import { bindSelection } from "./selectbind.js";
import { setStackbar } from "/static/kp/dist/kp-themes.min.js";

/** @typedef {import("./presetsview.js").PresetView} PresetView */

const COMPARE = declare({
  id: "presets-compare",
  page: "presets",
  opens: "view",
  what: "put the two or three ticked presets side by side, marking the settings that differ",
  shows: "while two or three presets are ticked",
  reach: [{ do: "click", control: "presets-pick", row: "*" }],
});
const PICK = declare({
  id: "presets-pick",
  page: "presets",
  opens: "view",
  row: "<preset>",
  selects: true,
  what: "tick or untick one preset to compare it",
});
const UNTICK = declare({
  id: "presets-untick",
  page: "presets",
  opens: "view",
  what: "untick every ticked preset",
  shows: "while a preset is ticked",
  reach: [{ do: "click", control: "presets-pick", row: "*" }],
});
const PICK_ALL = declare({
  id: "presets-pick-all",
  page: "presets",
  opens: "view",
  what: "tick every preset shown (click again to untick them)",
});
const COST_OF = declare({
  id: "presets-cost-of",
  page: "presets",
  opens: "view",
  what: "choose the preset whose memory the share bar adds to the host's",
});

/**
 * The settings a preset can be compared on, as words.
 * @param {PresetView} p
 * @param {{divisor: number, min_mb: number, max_mb: number} | null} swap
 * @returns {string[][]}
 */
export function settingsOf(p, swap) {
  const swapMb = swap
    ? Math.max(
        swap.min_mb,
        Math.min(swap.max_mb, Math.round(p.ram_mb / swap.divisor)),
      )
    : null;
  return [
    ["RAM", ramText(p.ram_mb)],
    [
      "Cores",
      p.cores_set === false ? `${p.cores} (fleet default)` : String(p.cores),
    ],
    [
      "Disk",
      p.disk_set === false
        ? `${p.disk_gb} GB (fleet default)`
        : `${p.disk_gb} GB`,
    ],
    ...(swapMb == null ? [] : [["Swap", `${swapMb} MB`]]),
    ["GPU", p.gpu ? "passed through" : "no"],
    ["VPN", p.vpn ? "tunnel device" : "no"],
    ["Apps", (p.apps ?? []).length ? (p.apps ?? []).join(", ") : "none"],
  ];
}

/**
 * Whether a selection can be compared, in words (empty: it can).
 * @param {number} n
 */
export const compareRule = (n) =>
  n < 2
    ? "Tick at least two presets to compare"
    : n > 3
      ? "Compare at most three presets at a time"
      : "";

/**
 * The compare card.
 * @param {{swap: () => {divisor: number, min_mb: number, max_mb: number} | null,
 *   retry: () => void}} deps
 */
export function presetsCompare(deps) {
  ensureStyle("/css/pages/presets.css");
  /** @type {PresetView[]} */
  let presets = [];
  /** @type {"loading" | "failed" | "ready"} */
  let status = "loading";
  let why = "";
  /** @type {string[]} */
  let chosen = [];
  let costOf = "";

  const block = tableBlock({
    remember: "presets-pick",
    caption: "The presets; tick two or three to compare them",
    captionHidden: true,
    search: null,
    nothing: "No preset to compare yet.",
    select: { label: "Select every preset", actions: [] },
    columns: [
      { label: "Preset", sort: "text" },
      { label: "What it is for", sort: null },
      { label: "RAM", sort: "number", cls: "kp-text-end" },
      { label: "Cores", sort: "number", cls: "kp-text-end" },
    ],
  });
  block.wrap.querySelector("[data-kp-datatable-actions]")?.remove();
  block.wrap.id = "ps-pick";
  const selectAll = block.wrap.querySelector("[data-kp-select-all]");
  if (selectAll instanceof HTMLElement) drivable(selectAll, PICK_ALL);

  const bar = h(
    "div",
    {
      class: "kp-selection ps-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "ps-pick",
      "data-kp-placement": "sticky-top",
      id: "ps-compare-bar",
      "aria-label": "Presets to compare",
    },
    h(
      "div",
      { class: "kp-selection__actions" },
      drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--primary",
            "data-kp-selection-action": "compare",
            title: "Put the ticked presets side by side",
          },
          "Compare",
        ),
        COMPARE,
      ),
    ),
  );
  const out = h("div", { class: "ps-compare" });
  // The compare table sits in a kp card with the four data states, so its
  // loading is the package's busy card over the whole footprint.
  const outCard = h(
    "section",
    {
      class: "kp-card ps-compare-card",
      "data-kp-card": "plain",
      "data-kp-state": "loading",
      "aria-label": "The presets side by side",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, out),
    ),
  );
  const cost = h("figure", { class: "kp-stackbar", "data-kp-stackbar": "" });
  const costSel = /** @type {HTMLSelectElement} */ (
    h("select", {
      class: "kp-field__input",
      "aria-label": "The preset to cost",
    })
  );
  drivable(costSel, COST_OF);
  const costRow = h(
    "div",
    { class: "ps-cost" },
    h("b", { class: "ps-cost__label" }, "What would this cost?"),
    h("div", { class: "ps-cost__control" }, costSel),
  );

  const sec = section({
    title: "Compare presets, and see what one would cost the host",
    desc: "Ticking two or three presets puts them side by side with only the rows that differ marked, and a bar shows what the chosen preset would do to the memory of the host.",
    id: "ps-compare",
    body: [bar, block.wrap, outCard, costRow, cost],
    foot: ["Read from the working copy's presets/ and the host's memory"],
  });
  sec.el.classList.add("ps-compare-sec");

  const detachTables = attachDataTables(sec.el);
  const sel = bindSelection(
    sec.el,
    bar,
    () => ({ compare: (items) => compareRule(items.length) }),
    undefined,
    (c) => drivable(c, UNTICK),
  );

  /** Host memory in GB: its size, and what the stacks may use now. */
  const hostMemory = () => {
    const f = current().fleet;
    if (!f) return null;
    const total = f.host.ram_total_mb;
    const committed =
      f.host.ram_committed_mb ??
      f.stacks.reduce((t, s) => t + (s.ram_max_mb ?? 0), 0);
    return { total: total / 1024, committed: committed / 1024 };
  };

  const hint = () => {
    out.replaceChildren();
    setCardState(outCard, "empty", {
      title: "Nothing to compare yet",
      text: "Tick two or three presets and press Compare.",
    });
  };
  /** @param {string[]} keys */
  const compare = (keys) => {
    setCardState(outCard, "ready");
    const ps = keys
      .map((k) => presets.find((p) => p.name === k))
      .filter((p) => p != null);
    const sets = ps.map(
      (p) =>
        new Map(/** @type {[string, string][]} */ (settingsOf(p, deps.swap()))),
    );
    const names = [...(sets[0]?.keys() ?? [])];
    out.replaceChildren(
      h(
        "div",
        { class: "kp-table-wrap" },
        h(
          "table",
          { class: "kp-table" },
          h("caption", { class: "kp-sr-only" }, "Presets side by side"),
          h(
            "thead",
            null,
            h(
              "tr",
              null,
              h("th", { scope: "col" }, "Setting"),
              ...ps.map((p) => h("th", { scope: "col" }, p.name)),
            ),
          ),
          h(
            "tbody",
            null,
            ...names.map((name) => {
              const vals = sets.map((m) => m.get(name) ?? "");
              const differs = new Set(vals).size > 1;
              return h(
                "tr",
                differs ? { "data-differs": "" } : null,
                h(
                  "th",
                  { scope: "row" },
                  name,
                  differs
                    ? h("span", { class: "kp-sr-only" }, " (differs)")
                    : null,
                ),
                ...vals.map((v) => h("td", null, v)),
              );
            }),
          ),
        ),
      ),
    );
  };

  const drawCost = () => {
    const p = presets.find((x) => x.name === costOf) ?? presets[0];
    const mem = hostMemory();
    if (status === "loading" || !mem) {
      setStackbar(cost, { parts: [], state: "loading", label: "Host memory" });
      return;
    }
    if (status === "failed" || !p) {
      setStackbar(cost, {
        parts: [],
        state: status === "failed" ? "error" : "empty",
        message: status === "failed" ? why : "No preset to cost.",
        label: "Host memory",
      });
      return;
    }
    const gb = Math.round((p.ram_mb / 1024) * 10) / 10;
    const total = Math.round(mem.total * 10) / 10;
    const now = Math.round(mem.committed * 10) / 10;
    const over = (now + gb) / total >= 0.8;
    const left = Math.round((total - now - gb) * 10) / 10;
    setStackbar(cost, {
      title: `What would the ${p.name} preset cost? Host memory, ${gbText(total)}`,
      total,
      format: (v) => gbText(v),
      restLabel: "Free",
      marks: [{ at: 0.8, label: "80 % of the host" }],
      summary:
        left >= 0
          ? `${gbText(left)} would stay free`
          : `${gbText(-left)} over the host`,
      parts: [
        { key: "now", label: "Allowed today", value: now },
        {
          key: "preset",
          label: `Plus ${p.name}`,
          value: gb,
          ...(over ? { tone: /** @type {const} */ ("warning") } : {}),
        },
      ],
      label: "Host memory",
      state: "ready",
    });
  };

  const draw = () => {
    if (status === "loading") {
      block.loading({ words: "Reading the presets" });
      setCardState(outCard, "loading", { words: "Reading the presets" });
      costSel.replaceChildren();
      costSel.disabled = true;
      drawCost();
      return;
    }
    if (status === "failed") {
      block.failed({ what: "the presets", why, fix: "Try the page again" });
      setCardState(outCard, "error", {
        title: "Could not read the presets",
        text: why,
        retry: false,
      });
      costSel.disabled = true;
      drawCost();
      return;
    }
    costSel.disabled = !presets.length;
    if (!costOf || !presets.some((p) => p.name === costOf))
      costOf = presets[0]?.name ?? "";
    costSel.replaceChildren(
      ...presets.map((p) =>
        h(
          "option",
          { value: p.name, ...(p.name === costOf ? { selected: "" } : {}) },
          p.name,
        ),
      ),
    );
    block.tbody.replaceChildren(
      ...presets.map((p) => {
        const tr = h(
          "tr",
          { "data-kp-row-key": p.name, "data-kp-row-label": p.name },
          selectCell(p.name, `Select ${p.name}`),
          h("td", { "data-kp-sort-value": p.name }, h("b", null, p.name)),
          h("td", null, p.description),
          h(
            "td",
            { class: "kp-text-end", "data-kp-sort-value": String(p.ram_mb) },
            ramText(p.ram_mb),
          ),
          h(
            "td",
            { class: "kp-text-end", "data-kp-sort-value": String(p.cores) },
            String(p.cores),
          ),
        );
        const box = tr.querySelector("input[data-kp-select-row]");
        if (box instanceof HTMLElement) drivable(box, PICK, p.name);
        return tr;
      }),
    );
    block.ready();
    // The ticks follow the keys still there.
    for (const box of /** @type {NodeListOf<HTMLInputElement>} */ (
      block.tbody.querySelectorAll("input[data-kp-select-row]")
    )) {
      const on = chosen.includes(box.value);
      if (box.checked !== on) {
        box.checked = on;
        box.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }
    if (!presets.length) {
      setCardState(outCard, "empty", {
        title: "No preset to compare",
        text: "Write a preset with New preset… and it can be compared here.",
      });
    } else {
      setCardState(outCard, "ready");
      if (chosen.length >= 2 && chosen.length <= 3) compare(chosen);
      else hint();
    }
    drawCost();
  };

  costSel.addEventListener("change", () => {
    costOf = costSel.value;
    drawCost();
  });
  bar.addEventListener("kp-selection-change", (e) => {
    chosen = /** @type {CustomEvent} */ (e).detail.keys;
  });
  bar.addEventListener("kp-selection-action", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (d.action === "compare") {
      compare(d.keys);
      costOf = d.keys[0] ?? costOf;
      costSel.value = costOf;
      drawCost();
    }
  });
  block.wrap.addEventListener("kp-datatable-retry", () => deps.retry());

  draw();
  return {
    el: sec.el,
    /**
     * @param {{presets?: PresetView[], status?: "loading" | "failed" | "ready", why?: string}} m
     */
    set: (m) => {
      if (m.presets) presets = m.presets;
      if (m.status) status = m.status;
      if (m.why != null) why = m.why;
      draw();
    },
    /** The host's memory arrived or moved. */
    host: () => drawCost(),
    stop: () => {
      detachTables();
      sel.stop();
    },
  };
}
