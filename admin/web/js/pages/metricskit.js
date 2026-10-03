// redesign-371-metrics / redesign-371-map (3.71.0): what the Metrics and
// Map pages need beyond the shared building blocks. The blocks themselves
// come from js/ui.js (kpi / kpiStrip, sparkline) and pages/hostkit.js
// (pageHeader with its meta row, segSwitch, keyRow, ensureStyle); this file
// only adds to them, and lists what ui.js should gain (reported to the
// foundation):
//
//   kpiStrip    ui.js's strip and tiles plus: a `meter` (with a tick mark)
//               in the sparkline's row, a tone on the context line (or
//               coloured parts), a sparkline colour, a `target` card the
//               tile scrolls to, each tile a declared Live view control, the
//               strip's own label, and a `set` that ends the skeleton
//   segmented   hostkit's segSwitch with every button a Live view control
//   tableSlot   one attachDataTables stop per card, replaced on each fill
//   cardFoot    a card's xs muted foot line (source left, legend right)
//   dataTable   a kp datatable without its own search bar
//   shareBar    the 60 px "used of" bar with its number beside it
//
// Styles: css/pages/metrics.css (`mk-` prefix), kp-themes tokens only;
// the hostkit blocks keep host.css's `hk-` styles.

import { h } from "../dom.js";
import { drivable } from "../drivable.js";
import { kpiStrip as uiKpiStrip } from "../ui.js";
import { segSwitch } from "./hostkit.js";
import { attachDataTables } from "/static/kp/js/datatable.js";

export { ensureStyle, keyRow, pageHeader } from "./hostkit.js";

/**
 * A chip: a dot and words (status is never colour alone).
 * @param {string} text
 * @param {"ok" | "warn" | "bad" | "info" | "live" | ""} [tone]
 */
export const chip = (text, tone = "") =>
  h("span", { class: "mk-chip" }, dot(tone), text);

/** @param {"ok" | "warn" | "bad" | "info" | "live" | ""} tone */
export const dot = (tone) =>
  h("span", { class: `mk-dot${tone ? ` mk-dot--${tone}` : ""}` });

/**
 * @typedef {{key?: string, label: string, value?: string, unit?: string,
 *   ctx?: string, ctxTone?: "ok" | "warn" | "bad" | "",
 *   ctxParts?: {text: string, tone?: "ok" | "warn" | "bad" | ""}[] | null,
 *   title?: string, tone?: "warn" | "bad" | "" | null, spark?: number[],
 *   colour?: string,
 *   meter?: {pct: number, mark?: number | null, tone?: string} | null,
 *   target?: string}} Kpi `target`: the id of the card the tile sums up;
 *   `ctxParts`: the context line as coloured pieces (e.g. "7 enforced" in
 *   green, "2 open" in amber) instead of `ctx`
 */

/**
 * ui.js's KPI strip, each tile extended in place (see the head of this
 * file). While `loading`, every tile is the foundation's skeleton until its
 * first `set`.
 * @param {Kpi[]} tiles
 * @param {{loading?: boolean, label?: string, drive?: string}} [opts]
 *   drive: the declared Live view control every tile is, on its own row
 *   (the tile's key)
 * @returns {{el: HTMLElement, tiles: Map<string, {el: HTMLElement,
 *   set: (k: Partial<Kpi>) => void}>}}
 */
export function kpiStrip(tiles, opts = {}) {
  const strip = uiKpiStrip(
    tiles.map((t) => ({
      key: t.key,
      label: t.label,
      // A same-site path (invariant 35), the card's id as its hash.
      ...(t.target
        ? { href: `${location.pathname}${location.search}#${t.target}` }
        : {}),
      ...(opts.loading ? { ctx: "" } : {}),
    })),
    { loading: opts.loading },
  );
  if (opts.label) strip.el.setAttribute("aria-label", opts.label);
  /** @type {Map<string, {el: HTMLElement, set: (k: Partial<Kpi>) => void}>} */
  const out = new Map();
  for (const t of tiles) {
    const key = t.key ?? t.label;
    const base = strip.tiles.get(key);
    if (!base) continue;
    const el = base.el;
    el.classList.add("mk-kpi");
    el.dataset.key = key;
    if (opts.drive) drivable(el, opts.drive, key);
    /** @type {Kpi} */
    let cur = { ...t };
    el.addEventListener("click", (e) => {
      if (!cur.target) return;
      // The page's own scroll, never a navigation (main.js routes links).
      e.preventDefault();
      document
        .getElementById(cur.target)
        ?.scrollIntoView({ block: "start", behavior: "smooth" });
    });
    const ctx = /** @type {HTMLElement} */ (el.querySelector(".nx-kpi__ctx"));
    const foot = /** @type {HTMLElement} */ (
      el.querySelector(".nx-kpi__spark")
    );
    /** @param {Partial<Kpi>} next */
    const set = (next) => {
      cur = { ...cur, ...next };
      delete el.dataset.loading;
      base.set({
        label: cur.label,
        value: cur.value,
        unit: cur.unit,
        ctx: cur.ctx ?? "",
        tone: cur.tone || null,
        spark: cur.meter ? [] : (cur.spark ?? []),
        ...(cur.title ? { title: cur.title } : {}),
      });
      ctx.className = `nx-kpi__ctx${cur.ctxTone ? ` mk-ctx--${cur.ctxTone}` : ""}`;
      if (cur.ctxParts?.length)
        ctx.replaceChildren(
          ...cur.ctxParts.flatMap((p, i) => [
            ...(i ? [" · "] : []),
            h(
              "span",
              { class: p.tone ? `mk-ctx--${p.tone}` : "" },
              ...(p.tone ? [dot(p.tone)] : []),
              p.text,
            ),
          ]),
        );
      if (cur.meter) foot.replaceChildren(meter(cur.meter));
      else if (cur.colour)
        foot.querySelector("svg")?.style.setProperty("color", cur.colour);
    };
    out.set(key, { el, set });
  }
  return { el: strip.el, tiles: out };
}

/**
 * hostkit's one-of-N segmented switch; each button is the declared Live
 * view control `drive` on its own row (the option's value).
 * @param {{label: string, options: [string, string, string?][],
 *   value: string, onPick: (v: string) => void, drive: string}} spec
 *   options: value, label, and an optional one-line hint
 */
export function segmented(spec) {
  const seg = segSwitch({
    label: spec.label,
    items: spec.options.map(([value, label, hint]) => ({
      value,
      label,
      hint: hint ?? `${spec.label}: ${label}`,
    })),
    value: spec.value,
    onChange: spec.onPick,
    mark: (b, v) => {
      drivable(b, spec.drive, v);
    },
  });
  /** The value pressed now. */
  const value = () =>
    seg.el.querySelector('[aria-pressed="true"]')?.getAttribute("data-v") ??
    spec.value;
  return { el: seg.el, value };
}

/**
 * One card's datatable behaviour: `attach(body)` wires the table just put
 * in `body` and stops the one it replaced, so a page that repaints every
 * 30 s keeps one listener set per card, not one per repaint.
 * @param {(root: HTMLElement) => () => void} [attachFn] attachDataTables
 *   (a test passes its own)
 */
export function tableSlot(attachFn = attachDataTables) {
  /** @type {(() => void) | null} */
  let stop = null;
  return {
    /** @param {HTMLElement} body */
    attach: (body) => {
      stop?.();
      stop = attachFn(body);
    },
    stop: () => {
      stop?.();
      stop = null;
    },
  };
}

/**
 * A "used of" meter: a fill, a tone from 75% / 90% (or given), and an
 * optional tick (memory promised to stacks).
 * @param {{pct: number, mark?: number | null, tone?: string}} m
 */
export function meter(m) {
  const pct = Math.max(0, Math.min(100, m.pct));
  const fill = h("i");
  fill.style.inlineSize = `${pct}%`;
  const el = h(
    "span",
    { class: `mk-meter${m.tone ? ` mk-meter--${m.tone}` : ""}` },
    fill,
  );
  if (m.mark != null) {
    const tick = h("b", { title: "promised to stacks" });
    tick.style.insetInlineStart = `${Math.max(0, Math.min(100, m.mark))}%`;
    el.append(tick);
  }
  return el;
}

/**
 * A bar with its number beside it, for a share in a table row.
 * @param {number} pct 0..100 of the bar's width
 * @param {string} text the number shown beside it
 * @param {string} [colour] a token
 */
export function shareBar(pct, text, colour) {
  const fill = h("i");
  fill.style.inlineSize = `${Math.max(0.6, Math.min(100, pct))}%`;
  if (colour) fill.style.background = colour;
  return h(
    "span",
    { class: "mk-bar" },
    h("span", null, fill),
    h("span", { class: "mk-num" }, text),
  );
}

/**
 * A card's foot line: the source on the left, a legend or note on the
 * right (DESIGN_LANGUAGE §5).
 * @param {(Node | string)[]} left
 * @param {(Node | string)[]} [right]
 */
export const cardFoot = (left, right = []) =>
  h(
    "div",
    { class: "mk-foot" },
    h("span", null, ...left),
    h("span", null, ...right),
  );

/**
 * A kp datatable with no search bar of its own: sortable headers,
 * Shift-click for a second key, the sort remembered under `remember`; the
 * caller fills `tbody`, then `attachDataTables` (or `refresh()` after a
 * later fill).
 * @param {{remember: string, caption: string,
 *   columns: {label: string, sort?: string, cls?: string}[]}} spec
 */
export function dataTable(spec) {
  const tbody = h("tbody");
  const wrap = h(
    "div",
    {
      class: "kp-datatable mk-table",
      "data-kp-datatable": "",
      "data-kp-sort-multi": "",
      "data-kp-remember": spec.remember,
      "data-kp-page-sizes": "none",
      "data-kp-page-size": "500",
    },
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h("caption", { class: "kp-sr-only" }, spec.caption),
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            ...spec.columns.map((c) =>
              h(
                "th",
                {
                  ...(c.sort && c.sort !== "none"
                    ? { "data-kp-sort": c.sort }
                    : {}),
                  ...(c.cls ? { class: c.cls } : {}),
                },
                c.label,
              ),
            ),
          ),
        ),
        tbody,
      ),
    ),
  );
  return { wrap, tbody };
}
