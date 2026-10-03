// redesign-371-metrics / redesign-371-map (3.71.0): small LOCAL additions
// to the shared building blocks in js/ui.js, for the Metrics and Map
// pages, named and shaped like them so they move into ui.js by changing
// one import (reported to the foundation):
//
//   pageHeader  ui.js's header plus a `meta` row under the description
//               (the live chip, "updated 4 s ago", the data's source)
//   kpi         ui.js's tile plus a `meter` (with a tick mark) in the
//               sparkline's row, a tone on the context line, and a click
//               that scrolls to the card the tile summarises
//   segmented   a one-of-N switch (System | Traffic, 1h … 30d) that keeps
//               the segmented look; every button declared for Live view
//   cardFoot    a card's xs muted foot line (source left, legend right)
//   dataTable   a kp datatable without its own search bar (sortable,
//               Shift-click multi-sort, remembered per table)
//   keyRow      the "Tab focus a chart · ← → move …" line at a page's foot
//   shareBar    the 60 px "used of" bar with its number beside it
//
// Styles: css/pages/metrics.css (`mk-` prefix), kp-themes tokens only.

import { h } from "../dom.js";
import { drivable } from "../drivable.js";
import { liveStatus, pageHeader as uiHeader, sparkline } from "../ui.js";

/** Load a page's own stylesheet once (the shell links only app.css).
 * @param {string} href */
export function ensureStyle(href) {
  if (document.querySelector(`link[data-page-style="${href}"]`)) return;
  const l = document.createElement("link");
  l.rel = "stylesheet";
  l.href = href;
  l.dataset.pageStyle = href;
  document.head.append(l);
}

/**
 * The page header (ui.js `pageHeader`) with a meta row under the
 * description: chips, the live status, the source. The description stays
 * the title's next sibling paragraph (invariants 36, 40).
 * @param {import("../ui.js").HeaderSpec & {meta?: (Node | string)[],
 *   liveVerb?: string}} spec
 */
export function pageHeader(spec) {
  const head = uiHeader({ ...spec, live: false });
  const live = liveStatus(spec.liveVerb ?? "updated");
  const meta = h(
    "div",
    { class: "mk-head__meta" },
    ...(spec.meta ?? []),
    live.el,
  );
  head.el.append(meta);
  head.el.classList.add("mk-head");
  return { ...head, meta, live };
}

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
 *   ctx?: string, ctxTone?: "ok" | "warn" | "bad" | "", title?: string,
 *   tone?: "warn" | "bad" | "" | null, spark?: number[], colour?: string,
 *   meter?: {pct: number, mark?: number | null, tone?: string} | null,
 *   target?: string}} Kpi `target`: the id of the card the tile sums up
 */

/**
 * One KPI tile in the foundation's `nx-kpi` markup: label, value + unit,
 * context, and in the last row a sparkline or a meter. A tile with a
 * `target` is a link that scrolls to that card. `set` repaints in place.
 * @param {Kpi} k
 */
export function kpi(k) {
  const label = h("span", { class: "nx-kpi__label" });
  const value = h("span", { class: "nx-kpi__value" });
  const ctx = h("span", { class: "nx-kpi__ctx" });
  const foot = h("span", { class: "nx-kpi__spark", "aria-hidden": "true" });
  const el = h(
    k.target ? "a" : "div",
    {
      class: "nx-kpi mk-kpi",
      ...(k.target ? { href: `#${k.target}` } : {}),
    },
    label,
    value,
    ctx,
    foot,
  );
  /** @type {Kpi} */
  let cur = { ...k };
  el.addEventListener("click", (e) => {
    if (!cur.target) return;
    // The page's own scroll, never a navigation (main.js routes links).
    e.preventDefault();
    document
      .getElementById(cur.target)
      ?.scrollIntoView({ block: "start", behavior: "smooth" });
  });
  /** @param {Partial<Kpi>} next */
  const set = (next) => {
    cur = { ...cur, ...next };
    delete el.dataset.loading;
    label.textContent = cur.label;
    value.replaceChildren(
      cur.value ?? "—",
      ...(cur.unit ? [h("small", null, cur.unit)] : []),
    );
    ctx.textContent = cur.ctx ?? "";
    ctx.className = `nx-kpi__ctx${cur.ctxTone ? ` mk-ctx--${cur.ctxTone}` : ""}`;
    el.dataset.tone = cur.tone ?? "";
    el.dataset.key = cur.key ?? cur.label;
    if (cur.title) el.title = cur.title;
    if (cur.meter) foot.replaceChildren(meter(cur.meter));
    else if (cur.spark && cur.spark.length > 1)
      foot.replaceChildren(
        sparkline(cur.spark, cur.colour ? { colour: cur.colour } : {}),
      );
    else foot.replaceChildren();
  };
  set({});
  return { el, set };
}

/**
 * A row of KPI tiles; while `loading` each is its own skeleton in the final
 * geometry.
 * @param {Kpi[]} tiles
 * @param {{loading?: boolean, label?: string, drive?: string}} [opts]
 *   drive: the declared Live view control every tile is, on its own row
 *   (the tile's key)
 */
export function kpiStrip(tiles, opts = {}) {
  /** @type {Map<string, ReturnType<typeof kpi>>} */
  const map = new Map();
  const el = h("div", {
    class: "nx-kpis",
    role: "group",
    "aria-label": opts.label ?? "Key figures",
  });
  el.style.setProperty("--kpi-n", String(Math.max(1, tiles.length)));
  for (const t of tiles) {
    const k = kpi(opts.loading ? { ...t, value: "", ctx: "" } : t);
    if (opts.loading) k.el.dataset.loading = "";
    if (opts.drive) drivable(k.el, opts.drive, t.key ?? t.label);
    map.set(t.key ?? t.label, k);
    el.append(k.el);
  }
  return { el, tiles: map };
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
 * A one-of-N segmented switch; each button is the declared Live view
 * control `drive` on its own row (the option's value).
 * @param {{label: string, options: [string, string, string?][],
 *   value: string, onPick: (v: string) => void, drive: string}} spec
 *   options: value, label, and an optional one-line hint
 */
export function segmented(spec) {
  const el = h("div", {
    class: "mk-seg",
    role: "group",
    "aria-label": spec.label,
  });
  /** @param {string} v */
  const paint = (v) => {
    for (const b of el.querySelectorAll("button"))
      b.setAttribute("aria-pressed", String(b.dataset.value === v));
  };
  for (const [v, text, hint] of spec.options) {
    const b = h(
      "button",
      {
        type: "button",
        "data-value": v,
        "aria-pressed": String(v === spec.value),
        title: hint ?? `${spec.label}: ${text}`,
      },
      text,
    );
    drivable(b, spec.drive, v);
    b.addEventListener("click", () => {
      paint(v);
      spec.onPick(v);
    });
    el.append(b);
  }
  return { el, set: paint };
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

/**
 * The page's keyboard line (DESIGN_LANGUAGE §10 "Hints").
 * @param {[string, string][]} pairs key, what it does
 */
export const keyRow = (pairs) =>
  h(
    "p",
    { class: "mk-keys", "aria-label": "Keyboard and pointer shortcuts" },
    ...pairs.map(([k, what]) =>
      h("span", null, h("kbd", { class: "nx-kbd" }, k), what),
    ),
  );
