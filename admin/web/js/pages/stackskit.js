// redesign-stacks (3.71.0): the pieces the Stacks and Apps pages need that
// the foundation's ui.js does not have yet, under the names and shapes
// ui.js would give them, so they can move there when the pages are merged
// (the brief: never edit ui.js from a page helper):
//
//   kpiStrip        ui.js's own strip; only a tile's context line gains
//                   a `meter` (used of limit, 6 px bar) or a status `dot`
//                   (ui.js's Kpi should take both)
//   segSwitch       a segmented view switch (Table / Cards), aria-pressed
//   drawer          a side panel on a native <dialog> (DESIGN_LANGUAGE §6,
//                   the shell's nx-drawer classes) with a head, body, foot
//   keyRow          the compact shortcut row under a list ("j k move …")
//   art             the small empty and all-clear line drawings of
//                   graphics.html (≤ 2 KB each, currentColor + tokens)
//   ensureStyle     load a page's own stylesheet once

import { h } from "../dom.js";
import { kpiStrip as uiKpiStrip } from "../ui.js";

/** @param {string} href */
export function ensureStyle(href) {
  if (document.querySelector(`link[data-page-style="${href}"]`)) return;
  const l = document.createElement("link");
  l.rel = "stylesheet";
  l.href = href;
  l.dataset.pageStyle = href;
  document.head.append(l);
}

/**
 * @typedef {import("../ui.js").Kpi & {meter?: number | null,
 *   dot?: "ok" | "warn" | "bad"}} Kpi
 */

/**
 * ui.js's KPI strip, with what the Stacks demo adds to a tile's context
 * line: a used-of-limit `meter` (6 px bar) or a status `dot` before its
 * words. Everything else is ui.js's own `kpiStrip` / `kpi`; only the
 * context line is drawn here, after ui.js has drawn the tile.
 * @param {Kpi[]} tiles
 * @param {{loading?: boolean}} [opts]
 * @returns {{el: HTMLElement, tiles: Map<string, {el: HTMLElement,
 *   set: (k: Partial<Kpi>) => void}>}}
 */
export function kpiStrip(tiles, opts = {}) {
  const strip = uiKpiStrip(tiles, opts);
  strip.el.classList.add("sk-kpis");
  strip.el.setAttribute("aria-label", "The host at a glance");
  /** @type {Map<string, {el: HTMLElement, set: (k: Partial<Kpi>) => void}>} */
  const map = new Map();
  for (const t of tiles) {
    const key = t.key ?? t.label;
    const base = strip.tiles.get(key);
    if (!base) continue;
    base.el.classList.add("sk-kpi");
    const ctx = /** @type {HTMLElement} */ (
      base.el.querySelector(".nx-kpi__ctx")
    );
    ctx.classList.add("sk-kpi__ctx");
    /** @type {Kpi} */
    let cur = { ...t };
    const set = (/** @type {Partial<Kpi>} */ next) => {
      cur = { ...cur, ...next };
      base.set(next);
      /** @type {Node[]} */
      const parts = [];
      if (cur.meter != null) {
        const fill = h("span");
        fill.style.inlineSize = `${Math.max(0, Math.min(100, cur.meter))}%`;
        parts.push(
          h(
            "span",
            {
              class: `sk-meter${cur.tone === "bad" ? " sk-meter--bad" : cur.tone === "warn" ? " sk-meter--warn" : ""}`,
              role: "meter",
              "aria-valuenow": String(Math.round(cur.meter)),
              "aria-valuemin": "0",
              "aria-valuemax": "100",
              "aria-label": `${cur.label}: ${Math.round(cur.meter)}%`,
            },
            fill,
          ),
        );
      }
      if (cur.dot)
        parts.push(h("span", { class: `sk-dot sk-dot--${cur.dot}` }));
      if (cur.ctx) parts.push(h("span", { class: "sk-kpi__words" }, cur.ctx));
      ctx.replaceChildren(...parts);
    };
    if (!opts.loading) set({});
    map.set(key, { el: base.el, set });
  }
  return { el: strip.el, tiles: map };
}

/**
 * A segmented switch between views of the same rows (Table / Cards):
 * one pressed at a time.
 * @param {{label: string, options: {value: string, label: string,
 *   hint: string}[], value: string, onChange: (v: string) => void,
 *   mark?: (b: HTMLButtonElement, value: string) => void}} spec
 * @returns {{el: HTMLElement, set: (v: string) => void}}
 */
export function segSwitch(spec) {
  /** @type {HTMLButtonElement[]} */
  const buttons = [];
  const el = h("div", {
    class: "sk-seg",
    role: "group",
    "aria-label": spec.label,
  });
  const set = (/** @type {string} */ v) => {
    for (const b of buttons)
      b.setAttribute("aria-pressed", String(b.dataset.value === v));
  };
  for (const o of spec.options) {
    const b = /** @type {HTMLButtonElement} */ (
      h(
        "button",
        { type: "button", "data-value": o.value, title: o.hint },
        o.label,
      )
    );
    b.addEventListener("click", () => {
      set(o.value);
      spec.onChange(o.value);
    });
    spec.mark?.(b, o.value);
    buttons.push(b);
    el.append(b);
  }
  set(spec.value);
  return { el, set };
}

/**
 * A side panel on a native <dialog> (the shell's `nx-drawer`): a title and
 * one sentence, a ✕, the body, the actions at the bottom right. Esc and ✕
 * close it; `onClose` runs once it has closed.
 * @param {{title: string, desc: string, body: Node[], foot?: Node[],
 *   label?: string, onClose?: () => void}} spec
 * @returns {{el: HTMLDialogElement, close: () => void}}
 */
export function drawer(spec) {
  const x = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost kp-button--sm kp-dialog__close",
      "aria-label": "Close",
      title: "Close this panel (Esc)",
    },
    "✕",
  );
  // Live view: a panel's own buttons are found among its own, by name.
  x.dataset.drive = "close";
  const d = /** @type {HTMLDialogElement} */ (
    h(
      "dialog",
      {
        class: "kp-dialog nx-drawer sk-drawer",
        "aria-label": spec.label ?? spec.title,
      },
      h(
        "div",
        { class: "nx-drawer__head" },
        h("h2", { class: "kp-dialog__title" }, spec.title),
        x,
        h("p", { class: "section-head__desc" }, spec.desc),
      ),
      h("div", { class: "nx-drawer__body sk-drawer__body" }, ...spec.body),
      ...(spec.foot?.length
        ? [h("div", { class: "nx-drawer__foot" }, ...spec.foot)]
        : []),
    )
  );
  const close = () => {
    if (d.open) d.close();
  };
  x.addEventListener("click", close);
  d.addEventListener("close", () => {
    d.remove();
    spec.onClose?.();
  });
  document.body.append(d);
  d.showModal();
  return { el: d, close };
}

/**
 * The compact key row under a list: each key as a `kbd` chip with what it
 * does beside it.
 * @param {[string[], string][]} keys
 */
export function keyRow(keys) {
  return h(
    "p",
    { class: "sk-keys", "aria-label": "Keyboard shortcuts" },
    ...keys.map(([ks, what]) =>
      h(
        "span",
        null,
        ...ks.map((k) => h("kbd", { class: "nx-kbd" }, k)),
        ` ${what}`,
      ),
    ),
  );
}

const SVG = "http://www.w3.org/2000/svg";

/**
 * An SVG line drawing from a list of shapes, 120×72, in currentColor.
 * @param {string} label for the test hooks; the drawing itself is hidden
 * @param {[string, Record<string, string>][]} shapes
 * @returns {SVGSVGElement}
 */
function drawing(label, shapes) {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("class", "sk-art");
  svg.setAttribute("viewBox", "0 0 120 72");
  svg.setAttribute("width", "120");
  svg.setAttribute("height", "72");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.6");
  svg.setAttribute("aria-hidden", "true");
  svg.dataset.art = label;
  for (const [tag, attrs] of shapes) {
    const e = document.createElementNS(SVG, tag);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
    svg.append(e);
  }
  return svg;
}

/** The empty and all-clear drawings of graphics.html (approved). */
export const art = {
  /** Nothing matches a filter: a magnifier over an empty line. */
  noMatch: () =>
    drawing("no-match", [
      ["circle", { cx: "52", cy: "32", r: "18" }],
      [
        "path",
        { d: "M65 45l17 17", "stroke-width": "3", "stroke-linecap": "round" },
      ],
      ["path", { d: "M44 32h16", opacity: ".5", "stroke-linecap": "round" }],
    ]),
  /** All clear: a shield with a tick, in the success colour. */
  allClear: () =>
    drawing("all-clear", [
      [
        "path",
        {
          d: "M60 8l30 11v17c0 16-13 25-30 30C43 61 30 52 30 36V19z",
          class: "sk-art__ok-fill",
        },
      ],
      [
        "path",
        {
          d: "M47 37l9 9 18-19",
          class: "sk-art__ok",
          "stroke-width": "3",
          "stroke-linecap": "round",
          "stroke-linejoin": "round",
        },
      ],
    ]),
  /** No stacks yet: an empty container with a plus. */
  noStacks: () =>
    drawing("no-stacks", [
      ["rect", { x: "22", y: "14", width: "76", height: "46", rx: "6" }],
      ["path", { d: "M22 26h76", opacity: ".5" }],
      [
        "path",
        { d: "M34 38h30M34 47h20", opacity: ".35", "stroke-linecap": "round" },
      ],
      ["circle", { cx: "88", cy: "50", r: "13", class: "sk-art__accent" }],
      [
        "path",
        {
          d: "M88 44v12M82 50h12",
          class: "sk-art__accent-line",
          "stroke-linecap": "round",
        },
      ],
    ]),
  /** No tiles yet: an empty grid of four tiles, one dashed. */
  noTiles: () =>
    drawing("no-tiles", [
      ["rect", { x: "26", y: "10", width: "30", height: "22", rx: "5" }],
      ["rect", { x: "64", y: "10", width: "30", height: "22", rx: "5" }],
      ["rect", { x: "26", y: "40", width: "30", height: "22", rx: "5" }],
      [
        "rect",
        {
          x: "64",
          y: "40",
          width: "30",
          height: "22",
          rx: "5",
          "stroke-dasharray": "4 4",
          class: "sk-art__accent-line",
        },
      ],
    ]),
};
