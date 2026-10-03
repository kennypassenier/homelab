// redesign-stacks (3.71.0): the pieces the Stacks and Apps pages need that
// the foundation's ui.js does not have yet, under the names and shapes
// ui.js would give them, so they can move there when the pages are merged
// (the brief: never edit ui.js from a page helper):
//
//   kpiStrip / kpi  ui.js's tile plus a `meter` (used of limit, 6 px bar),
//                   a status `dot` before the context line and a 28 px
//                   sparkline drawn from the trend
//   segSwitch       a segmented view switch (Table / Cards), aria-pressed
//   drawer          a side panel on a native <dialog> (DESIGN_LANGUAGE §6,
//                   the shell's nx-drawer classes) with a head, body, foot
//   keyRow          the compact shortcut row under a list ("j k move …")
//   art             the small empty and all-clear line drawings of
//                   graphics.html (≤ 2 KB each, currentColor + tokens)
//   ensureStyle     load a page's own stylesheet once

import { h } from "../dom.js";
import { sparkline } from "../ui.js";

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
 * @typedef {{key?: string, label: string, value?: string, unit?: string,
 *   ctx?: string, tone?: "ok" | "warn" | "bad" | null, href?: string,
 *   spark?: number[], meter?: number | null, dot?: "ok" | "warn" | "bad",
 *   title?: string}} Kpi
 */

/**
 * One KPI tile: label, value, a context line (with a status dot or a
 * used-of-limit meter in front), a sparkline; a link to its detail. `set`
 * repaints it in place, so the strip never reflows.
 * @param {Kpi} k
 * @returns {{el: HTMLElement, set: (k: Partial<Kpi>) => void}}
 */
export function kpi(k) {
  const label = h("span", { class: "nx-kpi__label" });
  const value = h("span", { class: "nx-kpi__value" });
  const ctx = h("span", { class: "nx-kpi__ctx sk-kpi__ctx" });
  const spark = h("span", { class: "nx-kpi__spark", "aria-hidden": "true" });
  const el = h(
    k.href ? "a" : "div",
    { class: "nx-kpi sk-kpi", ...(k.href ? { href: k.href } : {}) },
    label,
    value,
    ctx,
    spark,
  );
  /** @type {Kpi} */
  let cur = { ...k };
  const set = (/** @type {Partial<Kpi>} */ next) => {
    cur = { ...cur, ...next };
    label.textContent = cur.label;
    value.replaceChildren(
      cur.value ?? "—",
      ...(cur.unit ? [h("small", null, cur.unit)] : []),
    );
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
    if (cur.dot) parts.push(h("span", { class: `sk-dot sk-dot--${cur.dot}` }));
    if (cur.ctx) parts.push(h("span", { class: "sk-kpi__words" }, cur.ctx));
    ctx.replaceChildren(...parts);
    el.dataset.tone = cur.tone ?? "";
    if (cur.title) el.title = cur.title;
    spark.replaceChildren(
      ...(cur.spark && cur.spark.length > 1 ? [sparkline(cur.spark)] : []),
    );
    if (cur.href && el instanceof HTMLAnchorElement) el.href = cur.href;
  };
  set({});
  return { el, set };
}

/**
 * A row of 3 to 6 tiles; while `loading` each is its own skeleton in the
 * final geometry (rule 6).
 * @param {Kpi[]} tiles
 * @param {{loading?: boolean}} [opts]
 * @returns {{el: HTMLElement, tiles: Map<string, ReturnType<typeof kpi>>}}
 */
export function kpiStrip(tiles, opts = {}) {
  /** @type {Map<string, ReturnType<typeof kpi>>} */
  const map = new Map();
  const el = h("div", {
    class: "nx-kpis sk-kpis",
    role: "group",
    "aria-label": "The host at a glance",
  });
  el.style.setProperty("--kpi-n", String(Math.max(1, tiles.length)));
  for (const t of tiles) {
    const k = kpi(opts.loading ? { ...t, value: "" } : t);
    if (opts.loading) k.el.dataset.loading = "";
    map.set(t.key ?? t.label, k);
    el.append(k.el);
  }
  return { el, tiles: map };
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
      class: "kp-button kp-button--ghost kp-button--sm",
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
