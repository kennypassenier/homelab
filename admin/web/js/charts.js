// The ONE time chart every page draws is kp-themes' own (10.0.0, served at
// /static/kp/js/chart.js: legend, plot, drag-zoom, keys, event marks — see
// /home/kenny/Projects/kp-themes/docs/USER_GUIDE.md). This module is the
// thin adapter between it and the pages that used to draw with the
// hand-built js/timechart.js (feat-shell-6, redesign 3.71.0), kept small
// on purpose: everything the legend, the crosshair, the drag-zoom, the
// keyboard and the event markers do is kp's own, unchanged here.
//
// Three things stay here because kp has no equivalent:
//
//   seconds    every page still works in unix seconds (`/data/charts` and
//              `/data/traffic` answer in seconds, and js/format.js's
//              formatDateTime/formatClock/formatDay — the dashboard's one
//              date formatter, rule 52/fix-216 — take seconds too); kp's
//              ChartPoint is milliseconds. The boundary is this module:
//              toKpData() converts in, nothing else ever sees kp's ms.
//   live view  chart-source, chart-show-all and chart-zoom-reset are
//              declared controls (drivable.js) so `homelab ui click`
//              reaches them; kp's `decorate` option is the hook, wired
//              once below, but the declarations themselves are the
//              dashboard's own catalog, not kp's concern.
//   pageCharts a single page-wide handle a page can call .setZoom(null)
//              and .onZoom() on. kp's own group lives in the DOM (a
//              `[data-kp-chart-group]` element, or — what every page here
//              uses — the document's own "loose" group for charts outside
//              one), not as a JS object; pageCharts is a thin wrapper
//              around chartZoom(document, …) and the CHART_ZOOM_EVENT it
//              fires on the document.
//
// Everything else a page used to import from timechart.js — the chart
// itself, its legend, Show all, the zoom chip's "Reset" — kp draws and
// wires on its own; a page only feeds it data (setChartData(), or here,
// timeChart()) and reads its printed time (kp's own numericTime(),
// dd/mm/yyyy HH:mm, Europe/Brussels — already rule 52, so no `time` option
// is given).

import { declare, drivable } from "./drivable.js";
import { formatClock } from "./format.js";
import {
  attachCharts,
  CHART_SELECT_EVENT,
  CHART_ZOOM_EVENT,
  chartSelect,
  chartZoom,
  dashOf as kpDashOf,
  detachChart,
  formatChartValue,
  setChartData,
} from "/static/kp/js/chart.js";

/**
 * @typedef {[number, number]} Point unix seconds, value
 * @typedef {{label: string, points: Point[], colour?: string,
 *   n?: number}} Series `n`: a total the legend shows while nothing is hovered
 * @typedef {{at: number, label: string, tone?: "bad" | "warn" | "info",
 *   href?: string}} Annotation
 * @typedef {"percent" | "bytes" | "rate" | "celsius" | "count" | "flag"} Unit
 * @typedef {{label: string, series: Series[], unit?: Unit, from: number,
 *   to: number, height?: number, annotations?: Annotation[],
 *   threshold?: number, yMax?: number, stacked?: boolean, key?: string,
 *   onSelect?: (on: number[]) => void}} ChartSpec
 *   `key`: the chart's name for Live view (its card), `label` otherwise;
 *   `onSelect`: called with the sources turned on (none = all shown)
 *   whenever the selection changes — a click, Show all, Esc, `select`
 * @typedef {{destroy: () => void, host: HTMLElement,
 *   select: (i: number | null, on?: boolean) => void}} ChartState
 */

// ---------- Live view: declared once, same ids as timechart.js ----------

const here = () =>
  typeof location === "undefined" ? null : location.pathname + location.search;
const SOURCE = declare({
  id: "chart-source",
  page: "metrics",
  opens: "view",
  row: "<card>/<label>",
  at: here,
  what: "a chart's legend source: turn it on or off in that chart",
});
const SHOW_ALL = declare({
  id: "chart-show-all",
  page: "metrics",
  opens: "view",
  row: "<card>",
  at: here,
  what: "a chart's Show all: every source on again",
  shows: "while a chart has a source turned off",
  reach: [{ do: "click", control: "chart-source", row: "*" }],
});
const ZOOM_RESET = declare({
  id: "chart-zoom-reset",
  page: "metrics",
  opens: "view",
  at: here,
  what: "the zoom chip's Reset: every chart of the page back to its whole window",
  shows: "while the charts are zoomed in (drag across a chart)",
});

/** @type {import("/static/kp/js/chart.js").ChartDecorate} */
const decorate = (part, info) => {
  if (info.kind === "source")
    drivable(part, SOURCE, `${info.key ?? ""}/${info.label ?? ""}`);
  else if (info.kind === "show-all") drivable(part, SHOW_ALL, info.key ?? "");
  else if (info.kind === "zoom-reset") drivable(part, ZOOM_RESET);
};

/** @type {import("/static/kp/js/chart.js").ChartOptions} */
const CHART_OPTIONS = { decorate };

/** The dash of the `i`-th source's line — kp's own (0 = solid). */
export const dashOf = kpDashOf;

/** @param {number} s unix seconds */
const toMs = (s) => Math.round(s * 1000);

/** A moment's clock, as every page already prints it (js/format.js, rule 52). @param {number} t unix seconds */
export const hhmm = (t) => formatClock(t);

/**
 * A value in its unit, as the axis and the tooltip print it — kp's own
 * formatting (formatChartValue), the old `fmtValue(v, unit)` shape kept so
 * call sites outside the chart (a KPI tile's own figure) need no change.
 * @param {number} v
 * @param {Unit} [unit]
 */
export const fmtValue = (v, unit) =>
  formatChartValue(v, "tip", { unitKind: unit });

/**
 * A byte count in binary steps ("1.5 GiB") — kp's own formatting, the old
 * `bytes()` shape kept for the one page (fleetview.js) that prints a rate
 * as "`${bytes(v)}/s`" itself.
 * @param {number} b
 */
export const bytes = (b) => formatChartValue(b, "tip", { unitKind: "bytes" });

/**
 * `{at, label, tone}` (timechart.js's tones) to kp's `ChartEvent`.
 * @param {Annotation} a
 * @returns {import("/static/kp/js/chart.js").ChartEvent}
 */
const toKpEvent = (a) => ({
  at: toMs(a.at),
  label: a.label,
  tone: a.tone === "bad" ? "critical" : a.tone === "warn" ? "warning" : "info",
  href: a.href,
});

/** @param {ChartSpec} opt @returns {import("/static/kp/js/chart.js").ChartData} */
const toKpData = (opt) => ({
  label: opt.label,
  key: opt.key,
  unitKind: opt.unit,
  series: opt.series.map((s) => ({
    label: s.label,
    points: s.points.map(([t, v]) => [toMs(t), v]),
    colour: s.colour,
    total: s.n,
  })),
  events: (opt.annotations ?? []).map(toKpEvent),
  from: toMs(opt.from),
  to: toMs(opt.to),
  yMax: opt.yMax,
  threshold: opt.threshold,
  stacked: opt.stacked,
  height: opt.height,
});

/**
 * A page-wide handle on the document's own "loose" chart group (every
 * chart drawn without a `[data-kp-chart-group]` wrapper shares one):
 * `.zoom` the window a drag or a zoom chip's Reset set, `.setZoom(null)`
 * to reset it from elsewhere (a window switch), `.onZoom(f)` to follow it
 * (hide the "window in words" line while zoomed).
 */
export const pageCharts = (() => {
  /** @type {{from: number, to: number} | null} */
  let zoomNow = null;
  /** @type {Set<(z: {from: number, to: number} | null) => void>} */
  const listeners = new Set();
  if (typeof document !== "undefined")
    document.addEventListener(CHART_ZOOM_EVENT, (e) => {
      zoomNow = /** @type {CustomEvent} */ (e).detail ?? null;
      for (const f of listeners) f(zoomNow);
    });
  return {
    get zoom() {
      return zoomNow;
    },
    /** @param {{from: number, to: number} | null} z */
    setZoom(z) {
      if (typeof document !== "undefined") chartZoom(document, z);
    },
    /** @param {(z: {from: number, to: number} | null) => void} f */
    onZoom(f) {
      listeners.add(f);
      return () => listeners.delete(f);
    },
  };
})();

/**
 * The "Zoomed · 14:00–16:30 · Reset" chip, hidden while the charts show
 * their whole window; a page puts it once, in the toolbar's view & state
 * zone. kp wires it to the page's own group (the document's loose one) as
 * soon as a chart of that group attaches — `data-kp-chart-zoom` is kp's
 * own marker; its words and its Reset button are kp's (chartWords()).
 * @returns {{el: HTMLElement, stop: () => void}}
 */
export function zoomChip() {
  const el = document.createElement("span");
  el.setAttribute("data-kp-chart-zoom", "");
  el.hidden = true;
  // kp clears and re-hides the chip itself once the group's last chart
  // detaches (ChartGroup.stop()); nothing is left for a page to undo.
  return { el, stop: () => {} };
}

/**
 * Draw an interactive time chart into `host` — kp's own chart, fed through
 * setChartData()/attachCharts(). `host` becomes the `[data-kp-chart]`
 * element itself (a fresh one is expected each call, as every card already
 * draws: a poll replaces its box rather than updating it in place).
 * @param {HTMLElement} host
 * @param {ChartSpec} opt
 * @returns {ChartState}
 */
export function timeChart(host, opt) {
  host.setAttribute("data-kp-chart", "");
  setChartData(host, toKpData(opt));
  if (opt.onSelect)
    host.addEventListener(CHART_SELECT_EVENT, (e) =>
      opt.onSelect?.(/** @type {CustomEvent} */ (e).detail.on),
    );
  attachCharts(host.ownerDocument ?? document, CHART_OPTIONS);
  return {
    host,
    select: (i, on) => chartSelect(host, i, on),
    destroy: () => detachChart(host),
  };
}
