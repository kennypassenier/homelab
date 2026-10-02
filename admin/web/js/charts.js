// Pure layout for the charts (replace-grafana, 2026-09-30): series in,
// SVG path data out. No chart library (tech-charts): the page draws the SVG,
// coloured with kp-themes tokens. `panelEl` (moved here from pages/charts.js
// 2026-09-30, feat-metrics-1) is the one panel both tabs of Metrics draw.

import { badgeCell, h } from "./dom.js";
import { formatDateTime } from "./format.js";

export const W = 560;
export const H = 180;
export const PAD = { left: 56, right: 10, top: 10, bottom: 22 };
const NS = "http://www.w3.org/2000/svg";

/**
 * @param {string} tag
 * @param {Record<string, string>} attrs
 * @param {(Node | string)[]} kids
 */
function svg(tag, attrs, ...kids) {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  for (const k of kids) el.append(k);
  return el;
}

/**
 * fix-220 (Kenny, 2026-10-02): one hue per series, evenly spaced around the
 * wheel in the order the series already come in — the same technique
 * `admin/web/js/topology.js`'s `stackHues` uses for the topology's legend,
 * so a panel with many series (several apps, several drives) still reads as
 * distinct lines rather than the same 6 colours repeating.
 * @param {number} n
 * @returns {number[]}
 */
export function seriesHues(n) {
  return Array.from({ length: n }, (_, i) =>
    Math.round((360 * i) / Math.max(n, 1)),
  );
}

/**
 * Dim every line and legend entry but `i` (`null` restores all) — the same
 * hover/focus isolation `admin/web/js/topology.js`'s `isolate` gives the
 * topology, so a busy panel's own line can be picked out from the rest.
 * @param {SVGElement} plot
 * @param {HTMLElement} legend
 * @param {number | null} i
 */
function isolateSeries(plot, legend, i) {
  for (const el of plot.querySelectorAll(".chart__line")) {
    el.classList.toggle(
      "chart__line--dim",
      i != null && el.getAttribute("data-i") !== String(i),
    );
  }
  for (const el of legend.querySelectorAll(".chart__key")) {
    el.classList.toggle(
      "chart__key--dim",
      i != null && el.getAttribute("data-i") !== String(i),
    );
  }
}

/**
 * @param {Element} el
 * @param {SVGElement} plot
 * @param {HTMLElement} legend
 * @param {number} i
 */
function wireIsolate(el, plot, legend, i) {
  el.addEventListener("mouseenter", () => isolateSeries(plot, legend, i));
  el.addEventListener("mouseleave", () => isolateSeries(plot, legend, null));
  el.addEventListener("focus", () => isolateSeries(plot, legend, i));
  el.addEventListener("blur", () => isolateSeries(plot, legend, null));
}

/**
 * fix-220 (Kenny, 2026-10-02: "bij drive health zie ik geen items?... hoe
 * het van 'ok' naar 'not ok' gaat"): a flag series (SMART health) reads as
 * one constant line per drive, so every healthy drive overlaps into what
 * looks like a single line — a status table shows each drive's own current
 * state instead, with its history only when it actually changed.
 * `allPanels` (the full `/data/charts` answer) lets a "not ok" row name the
 * attribute that is failing, when a sibling SMART panel already measures it
 * for the same device.
 * @param {any} p
 */
function healthTableEl(p) {
  const box = h(
    "figure",
    { class: "chart chart--health" },
    h("figcaption", null, p.panel.title),
  );
  if (p.panel.desc)
    box.append(h("p", { class: "chart__desc measured" }, p.panel.desc));
  if (p.error) {
    box.append(h("p", { class: "chart__error" }, p.error));
    return box;
  }
  if (!p.series.length) {
    box.append(h("p", { class: "chart__empty" }, "No data in this window."));
    return box;
  }
  const rows = p.series.map((/** @type {any} */ s) => {
    const last = s.points.length ? s.points[s.points.length - 1][1] : null;
    const ok = last == null ? null : last >= 1;
    let changedAt = /** @type {number | null} */ (null);
    let prev = s.points.length ? s.points[0][1] >= 1 : null;
    for (const [t, v] of s.points) {
      const now = v >= 1;
      if (prev != null && now !== prev) changedAt = t;
      prev = now;
    }
    return h(
      "tr",
      null,
      h("td", null, s.label || "—"),
      badgeCell({
        label: ok == null ? "unknown" : ok ? "ok" : "not ok",
        tone: ok == null ? "info" : ok ? "ok" : "bad",
      }),
      h(
        "td",
        null,
        changedAt == null
          ? "unchanged in this window"
          : `changed at ${formatDateTime(changedAt)}`,
      ),
    );
  });
  box.append(
    h(
      "table",
      { class: "kp-table chart__health-table" },
      h(
        "thead",
        null,
        h(
          "tr",
          null,
          h("th", null, "Drive"),
          h("th", null, "State"),
          h("th", null, "History"),
        ),
      ),
      h("tbody", null, ...rows),
    ),
  );
  return box;
}

/**
 * One panel: a heading, a one-sentence description of what it shows and how
 * to read it (fix-220, rule 8), the chart, and each series' latest value. A
 * `flag`-unit panel (SMART health) draws as a status table instead of a
 * line chart — see `healthTableEl`.
 * @param {any} p `{panel, series, error?}` from /data/charts or /data/traffic
 * @param {number} from
 * @param {number} to
 */
export function panelEl(p, from, to) {
  if (p.panel.unit === "flag") return healthTableEl(p);
  const unit = p.panel.unit;
  const box = h(
    "figure",
    { class: "chart" },
    h("figcaption", null, p.panel.title),
  );
  if (p.panel.desc)
    box.append(h("p", { class: "chart__desc measured" }, p.panel.desc));
  if (p.error) {
    box.append(h("p", { class: "chart__error" }, p.error));
    return box;
  }
  if (
    !p.series.length ||
    p.series.every((/** @type {any} */ s) => !s.points.length)
  ) {
    box.append(h("p", { class: "chart__empty" }, "No data in this window."));
    return box;
  }
  const L = layout(p.series, from, to, unit);
  const hues = seriesHues(L.paths.length);
  const plot = svg("svg", {
    viewBox: `0 0 ${W} ${H}`,
    class: "chart__svg",
    role: "img",
    "aria-label": p.panel.title,
  });
  for (const t of L.yTicks) {
    plot.append(
      svg("line", {
        x1: String(L.x0),
        x2: String(L.x1),
        y1: String(t.y),
        y2: String(t.y),
        class: "chart__grid",
      }),
      svg(
        "text",
        {
          x: String(L.x0 - 6),
          y: String(t.y + 4),
          class: "chart__tick",
          "text-anchor": "end",
        },
        t.label,
      ),
    );
  }
  plot.append(
    svg(
      "text",
      { x: String(L.x0), y: String(H - 4), class: "chart__tick" },
      formatDateTime(from),
    ),
    svg(
      "text",
      {
        x: String(L.x1),
        y: String(H - 4),
        class: "chart__tick",
        "text-anchor": "end",
      },
      formatDateTime(to),
    ),
  );
  L.paths.forEach((s, i) => {
    const line = svg("path", {
      d: s.d,
      class: "chart__line",
      style: `--chart-hue: ${hues[i]}`,
      "data-i": String(i),
      tabindex: "0",
    });
    plot.append(line);
  });
  box.append(plot);
  const legend = h("ul", { class: "chart__legend" });
  L.paths.forEach((s, i) => {
    const key = h(
      "li",
      {
        class: "chart__key",
        style: `--chart-hue: ${hues[i]}`,
        "data-i": String(i),
        tabindex: "0",
      },
      `${s.label || "now"}: ${s.last == null ? "—" : formatValue(s.last, unit)}`,
    );
    legend.append(key);
  });
  box.append(legend);
  // Hover/focus isolation (rule "al die items zijn amper van elkaar te
  // distinguieren"): wired after both the lines and the legend exist, once
  // per series, shared between a line and its legend entry.
  L.paths.forEach((_s, i) => {
    for (const el of [
      plot.querySelector(`[data-i="${i}"]`),
      legend.querySelector(`[data-i="${i}"]`),
    ]) {
      if (el) wireIsolate(el, plot, legend, i);
    }
  });
  return box;
}

/**
 * @typedef {{label: string, points: [number, number][]}} Series
 * @typedef {"cores" | "bytes" | "percent" | "celsius" | "flag" | "count"} Unit
 */

/**
 * A number the way its unit reads.
 * @param {number} v
 * @param {Unit} unit
 */
export function formatValue(v, unit) {
  switch (unit) {
    case "bytes": {
      const u = ["B", "KiB", "MiB", "GiB", "TiB"];
      let i = 0;
      let x = Math.abs(v);
      while (x >= 1024 && i < u.length - 1) {
        x /= 1024;
        i++;
      }
      return `${(Math.sign(v) * x).toFixed(x >= 100 ? 0 : 1)} ${u[i]}`;
    }
    case "percent":
      return `${v.toFixed(v >= 10 ? 0 : 1)}%`;
    case "celsius":
      return `${v.toFixed(0)} °C`;
    case "cores":
      return v >= 1 ? v.toFixed(2) : `${(v * 1000).toFixed(0)} m`;
    case "flag":
      return v >= 1 ? "ok" : "not ok";
    case "count":
      return v >= 1000 ? `${(v / 1000).toFixed(1)} k` : v.toFixed(0);
    default:
      return String(v);
  }
}

/**
 * Scales, paths and axis labels for one panel.
 * @param {Series[]} series
 * @param {number} from unix seconds
 * @param {number} to unix seconds
 * @param {Unit} unit
 */
export function layout(series, from, to, unit) {
  let lo = Infinity;
  let hi = -Infinity;
  for (const s of series)
    for (const [, v] of s.points) {
      lo = Math.min(lo, v);
      hi = Math.max(hi, v);
    }
  if (!Number.isFinite(lo)) {
    lo = 0;
    hi = 1;
  }
  lo = Math.min(0, lo);
  if (unit === "percent") hi = Math.max(hi, 100);
  if (unit === "flag") hi = Math.max(hi, 1);
  if (hi <= lo) hi = lo + 1;
  const iw = W - PAD.left - PAD.right;
  const ih = H - PAD.top - PAD.bottom;
  const span = Math.max(1, to - from);
  const x = (/** @type {number} */ t) => PAD.left + ((t - from) / span) * iw;
  const y = (/** @type {number} */ v) =>
    PAD.top + ih - ((v - lo) / (hi - lo)) * ih;
  const paths = series.map((s) => ({
    label: s.label,
    d: s.points
      .map(
        ([t, v], i) => `${i ? "L" : "M"}${x(t).toFixed(1)},${y(v).toFixed(1)}`,
      )
      .join(""),
    last: s.points.length ? s.points[s.points.length - 1][1] : null,
  }));
  const yTicks = [lo, (lo + hi) / 2, hi].map((v) => ({
    y: y(v),
    label: formatValue(v, unit),
  }));
  return { paths, yTicks, x0: PAD.left, x1: W - PAD.right, y0: PAD.top + ih };
}
