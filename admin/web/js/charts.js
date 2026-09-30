// Pure layout for the charts (replace-grafana, 2026-09-30): series in,
// SVG path data out. No chart library (tech-charts): the page draws the SVG,
// coloured with kp-themes tokens.

export const W = 560;
export const H = 180;
export const PAD = { left: 56, right: 10, top: 10, bottom: 22 };

/**
 * @typedef {{label: string, points: [number, number][]}} Series
 * @typedef {"cores" | "bytes" | "percent" | "celsius" | "flag"} Unit
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
