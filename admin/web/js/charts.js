// Pure layout for the charts (replace-grafana, 2026-09-30): series in,
// SVG path data out. No chart library (tech-charts): the page draws the SVG,
// coloured with kp-themes tokens. `panelEl` (moved here from pages/charts.js
// 2026-09-30, feat-metrics-1) is the one panel both tabs of Metrics draw.

import { h } from "./dom.js";
import { formatTime } from "./format.js";

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
 * One panel: title, the chart, and each series' latest value.
 * @param {any} p `{panel, series, error?}` from /data/charts or /data/traffic
 * @param {number} from
 * @param {number} to
 */
export function panelEl(p, from, to) {
  const unit = p.panel.unit;
  const box = h(
    "figure",
    { class: "chart" },
    h("figcaption", null, p.panel.title),
  );
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
      formatTime(from),
    ),
    svg(
      "text",
      {
        x: String(L.x1),
        y: String(H - 4),
        class: "chart__tick",
        "text-anchor": "end",
      },
      formatTime(to),
    ),
  );
  L.paths.forEach((s, i) =>
    plot.append(
      svg("path", { d: s.d, class: `chart__line chart__line--${i % 6}` }),
    ),
  );
  box.append(plot);
  const legend = h("ul", { class: "chart__legend" });
  L.paths.forEach((s, i) =>
    legend.append(
      h(
        "li",
        { class: `chart__key chart__key--${i % 6}` },
        `${s.label || "now"}: ${s.last == null ? "—" : formatValue(s.last, unit)}`,
      ),
    ),
  );
  box.append(legend);
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
