// The ONE time chart every page draws is kp-themes' own (11.0.0, served at
// /static/kp/js/chart.js: legend, plot, drag-zoom, keys, event marks, bands,
// typical level and runs — see /home/kenny/Projects/kp-themes/docs/USER_GUIDE.md).
// This module is the thin adapter between it and the pages, kept small on
// purpose: everything the legend, the crosshair, the drag-zoom, the keyboard,
// the event marks and the live update do is kp's own, unchanged here.
//
// Two things stay here because kp has no equivalent:
//
//   seconds    every page still works in unix seconds (`/data/charts` and
//              `/data/traffic` answer in seconds, and js/format.js's
//              formatDateTime/formatClock/formatDay — the dashboard's one
//              date formatter, rule 52/fix-216 — take seconds too); kp's
//              ChartPoint is milliseconds. The boundary is this module:
//              chartData() converts in, nothing else ever sees kp's ms.
//   live view  chart-source, chart-show-all and chart-zoom-reset are
//              declared controls (drivable.js) so `homelab ui click`
//              reaches them; kp's `decorate` option is the hook, wired
//              once below, but the declarations themselves are the
//              dashboard's own catalog, not kp's concern.
//
// A page draws a chart by putting a `figure[data-kp-chart]` in a
// `[data-kp-chart-group]`, calling attachPageCharts() once on its root and
// feeding the figure with feedChart() as often as it reads: kp keeps the
// crosshair, the pinned reading, the zoom and the sources a person switched
// off across every feed (a poll never throws a chart away). The group's range
// keys, its zoom chip and its shared crosshair are kp's own markup
// (`kp-chart-group__bar`, `data-kp-chart-range`, `data-kp-chart-zoom`).

import { declare, drivable } from "./drivable.js";
import { formatClock } from "./format.js";
import {
  attachCharts,
  dashOf as kpDashOf,
  formatChartValue,
  setChartData,
} from "/static/kp/js/chart.js";

/**
 * @typedef {[number, number]} Point unix seconds, value
 * @typedef {{label: string, points: Point[], colour?: string,
 *   n?: number}} Series `n`: a total the legend shows while nothing is hovered
 * @typedef {"operation" | "update" | "failure"} EventKind
 * @typedef {{at: number, until?: number, label: string,
 *   tone?: "bad" | "warn" | "info", kind?: EventKind,
 *   href?: string}} Annotation `until`: the event is a window of work from
 *   `at` to here (unix seconds), drawn as a band; without it a line
 * @typedef {"percent" | "bytes" | "rate" | "celsius" | "count" | "flag"} Unit
 * @typedef {{label: string, series: Series[], unit?: Unit, from: number,
 *   to: number, height?: number, annotations?: Annotation[],
 *   threshold?: number, yMax?: number, stacked?: boolean, key?: string,
 *   typical?: boolean}} ChartSpec
 *   `key`: the chart's name for Live view (its card), `label` otherwise;
 *   `typical`: a dashed line at each source's typical level and a ring on
 *   every run of six readings on one side of it (kp's `median` and `runs`)
 * @typedef {import("/static/kp/js/chart.js").ChartData} ChartData
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
 * What each kind of event is called in the key above a chart, and its tone
 * (kp's `kinds`): an operation the host ran, an update, a failure.
 * @type {NonNullable<ChartData["kinds"]>}
 */
export const EVENT_KINDS = {
  operation: { label: "an operation the host ran", tone: "info" },
  update: { label: "an update", tone: "warning" },
  failure: { label: "a failure", tone: "critical" },
};

/** The tone a dashboard annotation has in kp's words. @type {Record<string, "critical" | "warning" | "info">} */
const TONE = { bad: "critical", warn: "warning", info: "info" };

/**
 * `{at, until, kind, tone}` (seconds) to kp's `ChartEvent` (milliseconds):
 * a window of work carries its `until`, so kp draws it as a band.
 * @param {Annotation} a
 * @returns {import("/static/kp/js/chart.js").ChartEvent}
 */
const toKpEvent = (a) => ({
  at: toMs(a.at),
  label: a.label,
  tone: TONE[a.tone ?? "info"],
  ...(a.kind ? { kind: a.kind } : {}),
  ...(a.until != null && a.until > a.at ? { until: toMs(a.until) } : {}),
  ...(a.href ? { href: a.href } : {}),
});

/**
 * The chart's data in kp's shape. A typical-level chart asks kp for the
 * dashed line and the rings, and says it is too early below 48 readings
 * (counting readings); every chart says so when it holds one reading.
 * @param {ChartSpec} opt
 * @returns {ChartData}
 */
export const chartData = (opt) => ({
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
  kinds: EVENT_KINDS,
  band: "hatch",
  from: toMs(opt.from),
  to: toMs(opt.to),
  yMax: opt.yMax,
  threshold: opt.threshold,
  stacked: opt.stacked,
  height: opt.height,
  onePointNote: true,
  ...(opt.typical
    ? { median: "typical", runs: "mark", minPoints: 48, minUnit: "readings" }
    : {}),
});

/**
 * Give a chart its readings (kp's live update: the crosshair, the pinned
 * reading, the zoom and the sources switched off stay as they are). The
 * `figure[data-kp-chart]` is made once by the page; a feed ends its
 * `data-kp-chart-loading`.
 * @param {Element} figure
 * @param {ChartSpec} opt
 */
export function feedChart(figure, opt) {
  setChartData(figure, chartData(opt));
}

/**
 * A chart whose read failed: the reason fills the plot at its height, with
 * kp's Retry (`kp-chart-retry`).
 * @param {Element} figure @param {string} label @param {string} error
 */
export function failChart(figure, label, error) {
  setChartData(figure, { label, series: [], error });
}

/**
 * Draw every `[data-kp-chart]` under `root` with the dashboard's options (its
 * Live view controls). Returns the detach: these charts taken away.
 * @param {ParentNode} root
 */
export const attachPageCharts = (root) => attachCharts(root, CHART_OPTIONS);
