// feat-shell-6 (redesign 3.71.0, DESIGN_LANGUAGE.md §10, Kenny approved
// 2026-10-03): the ONE time chart every page draws, so its controls mean
// the same everywhere. The demos' `OPS.timeChart` (ops-kit.js) as a module:
//
//   legend   hover / focus = isolate that source (the others fade);
//            click = turn it on or off — one or several, no modifier keys
//            (Kenny, 2026-10-03); "Show all" (and Esc) resets
//   plot     hover = crosshair on every chart of the group + a tooltip with
//            each visible source's value, the time, the change over the
//            hour before (▲/▼ and the amount) and the events within reach;
//            click = pin the tooltip (✕ releases it)
//   drag     = zoom every chart of the group to that span; a "zoomed ·
//            Reset" chip; double-click or Esc resets
//   keys     ←/→ move the cursor (Shift: ten points), Home/End, Enter pins,
//            Esc releases → clears the selection → resets the zoom
//   events   deploys, restarts, backups, alerts, Live view steps as markers;
//            hover = what happened, click = pin it, with its link
//
// Colours are kp-themes tokens only (`--chart-1..5` in order, or a series'
// own colour); from the sixth source on the colours repeat with a dashed
// line, so no two sources look the same. Every class is `tc-` in app.css.
//
// Live view (redesign-371-metrics review): a source, Show all and the zoom
// chip's Reset are declared controls, found on whichever page draws the
// chart (`at` is the address shown now).

import { declare, drivable } from "./drivable.js";
import { h } from "./dom.js";

/** A chart control lives wherever a chart is drawn: the page shown now. */
const here = () => location.pathname + location.search;
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
});
const ZOOM_RESET = declare({
  id: "chart-zoom-reset",
  page: "metrics",
  opens: "view",
  at: here,
  what: "the zoom chip's Reset: every chart of the page back to its whole window",
});

/**
 * The dash of the `i`-th source's line: solid for the first five (each its
 * own `--chart-n`), dashed when the colours come round again.
 * @param {number} i
 * @returns {string | null}
 */
export const dashOf = (i) =>
  i < 5 ? null : ["5 3", "1.5 2.5", "8 3 1.5 3"][Math.floor(i / 5 - 1) % 3];

const SVGNS = "http://www.w3.org/2000/svg";

/**
 * @typedef {[number, number]} Point unix seconds, value
 * @typedef {{label: string, points: Point[], colour?: string,
 *   n?: number}} Series `n`: a total the legend shows while nothing is hovered
 * @typedef {{at: number, label: string, tone?: "bad" | "warn" | "info",
 *   href?: string}} Annotation
 * @typedef {"percent" | "bytes" | "rate" | "celsius" | "count" | "flag"} Unit
 * @typedef {{label: string, series: Series[], unit?: Unit, from: number,
 *   to: number, height?: number, annotations?: Annotation[],
 *   threshold?: number, yMax?: number, stacked?: boolean,
 *   group?: ChartGroup, key?: string,
 *   onSelect?: (on: number[]) => void}} ChartSpec
 *   `key`: the chart's name for Live view (its card), `label` otherwise;
 *   `onSelect`: called with the sources turned on (none = all shown)
 *   whenever the selection changes — a click, Show all, Esc, `select`
 * @typedef {{draw: () => void, cursorAt: (t: number | null, own: boolean) => void,
 *   destroy: () => void, host: HTMLElement}} ChartState
 */

// ---------- formatting (pure) ----------

/** @param {number} b @param {number} [d] */
export function bytes(b, d = 1) {
  const u = ["B", "KiB", "MiB", "GiB", "TiB"];
  let i = 0;
  while (Math.abs(b) >= 1024 && i < u.length - 1) {
    b /= 1024;
    i++;
  }
  return `${b.toFixed(b >= 100 || i === 0 ? 0 : d)} ${u[i]}`;
}

/**
 * A value in its unit, as the axis and the tooltip print it.
 * @param {number} v
 * @param {Unit} [unit]
 */
export function fmtValue(v, unit) {
  if (unit === "percent")
    return `${v.toFixed(v !== 0 && Math.abs(v) < 10 ? 1 : 0)}%`;
  if (unit === "bytes") return bytes(v);
  if (unit === "rate") return `${bytes(v)}/s`;
  if (unit === "celsius") return `${v.toFixed(0)} °C`;
  if (Math.abs(v) >= 10000) return `${(v / 1000).toFixed(1)}k`;
  if (Math.abs(v) >= 100) return v.toFixed(0);
  return v.toFixed(v % 1 ? 2 : 0);
}

const pad = (/** @type {number} */ n) => String(n).padStart(2, "0");
const MON = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ");
const DOW = "Sun Mon Tue Wed Thu Fri Sat".split(" ");
/** @param {number} t */
export const hhmm = (t) => {
  const d = new Date(t * 1000);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
};
/** @param {number} t */
const day = (t) => {
  const d = new Date(t * 1000);
  return `${DOW[d.getDay()]} ${d.getDate()} ${MON[d.getMonth()]}`;
};
/** @param {number} t */
const dayTime = (t) => `${day(t)}, ${hhmm(t)}`;

/**
 * The top of the y axis: a round number just above the highest value.
 * @param {number} v
 * @param {Unit} [unit]
 */
export function niceMax(v, unit) {
  if (unit === "percent") return v > 50 ? 100 : v > 25 ? 50 : v > 10 ? 25 : 10;
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10])
    if (m * p >= v * 1.05) return m * p;
  return 10 * p;
}

/**
 * The index of the point nearest to `t`.
 * @param {Point[]} points
 * @param {number} t
 */
export function indexAt(points, t) {
  let bi = 0;
  for (let i = 1; i < points.length; i++)
    if (Math.abs(points[i][0] - t) < Math.abs(points[bi][0] - t)) bi = i;
  return bi;
}

/**
 * A source's change over the hour before `i`: the value now minus the
 * value an hour of points earlier (or the first point).
 * @param {Point[]} points
 * @param {number} i
 */
export function hourChange(points, i) {
  const step = points.length > 1 ? points[1][0] - points[0][0] || 3600 : 3600;
  const back = Math.max(0, i - Math.round(3600 / step));
  return points[i][1] - points[back][1];
}

/**
 * The legend's selection after a click on source `i` of `n`: on or off,
 * several allowed; all on is the same as none (everything shown).
 * @param {Set<number>} sel
 * @param {number} i
 * @param {number} n
 * @returns {Set<number>}
 */
export function toggleSource(sel, i, n) {
  const next = new Set(sel);
  if (next.has(i)) next.delete(i);
  else next.add(i);
  if (next.size === n) next.clear();
  return next;
}

// ---------- the group: shared crosshair and zoom ----------

/**
 * Charts that share one crosshair and one zoom (every chart of a page by
 * default). A page that draws two unrelated time ranges makes its own.
 */
export class ChartGroup {
  constructor() {
    /** @type {Set<ChartState>} */
    this.charts = new Set();
    /** @type {{from: number, to: number} | null} */
    this.zoom = null;
    /** @type {Set<(z: {from: number, to: number} | null) => void>} */
    this.listeners = new Set();
    /** Whether Esc anywhere on the page resets this group's zoom yet. */
    this.escWired = false;
  }
  /** @param {{from: number, to: number} | null} z */
  setZoom(z) {
    this.zoom = z;
    for (const c of this.charts) c.draw();
    for (const f of this.listeners) f(z);
  }
  /** @param {(z: {from: number, to: number} | null) => void} f */
  onZoom(f) {
    this.listeners.add(f);
    return () => this.listeners.delete(f);
  }
}

/** The page's default group. */
export const pageCharts = new ChartGroup();

/**
 * The "zoomed: 14:00–16:30 · Reset" chip of a group, hidden while the
 * charts show their whole window; a page puts it once, in the toolbar's
 * view & state zone.
 * @param {ChartGroup} [group]
 * @returns {{el: HTMLElement, stop: () => void}}
 */
export function zoomChip(group = pageCharts) {
  const text = h("span", null);
  const reset = h(
    "button",
    { type: "button", class: "tc-zoom__reset" },
    "Reset",
  );
  reset.addEventListener("click", () => group.setZoom(null));
  drivable(reset, ZOOM_RESET);
  const el = h(
    "span",
    { class: "tc-zoom", hidden: "", role: "status" },
    text,
    reset,
  );
  const paint = (/** @type {{from: number, to: number} | null} */ z) => {
    el.hidden = z == null;
    if (z) text.textContent = `Zoomed: ${hhmm(z.from)}–${hhmm(z.to)} · `;
  };
  paint(group.zoom);
  return { el, stop: group.onZoom(paint) };
}

/** @param {string} tag @param {Record<string, string | number | null | undefined>} attrs */
function s(tag, attrs) {
  const el = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs))
    if (v != null) el.setAttribute(k, String(v));
  return el;
}

/**
 * Draw an interactive time chart into `host`.
 * @param {HTMLElement} host
 * @param {ChartSpec} opt
 * @returns {ChartState & {select: (i: number | null, on?: boolean) => void}}
 */
export function timeChart(host, opt) {
  const group = opt.group ?? pageCharts;
  const { series, unit } = opt;
  const height = opt.height ?? 168;
  const annotations = opt.annotations ?? [];
  const stacked = opt.stacked ?? false;
  /** @type {Set<number>} */
  let sel = new Set();
  /** @type {number | null} */
  let hoverSeries = null;
  /** @type {number | null} */
  let pinned = null;
  /** @type {number | null} */
  let cursor = null;
  /** @type {{x0: number, x1: number} | null} */
  let brush = null;
  const plot = h("div", {
    class: "tc-plot",
    tabindex: "0",
    role: "application",
    "aria-label": `${opt.label}: chart. Arrow keys move through time, Enter pins the reading, Esc releases.`,
  });
  const tip = h("div", { class: "tc-tip", hidden: "", role: "status" });
  const legend = h("div", {
    class: "tc-legend",
    role: "group",
    "aria-label": `${opt.label}: sources`,
  });
  const reset = h(
    "button",
    {
      class: "tc-legend__reset",
      type: "button",
      hidden: "",
      title: "Show every source again (Esc)",
    },
    "Show all",
  );
  const driveKey = opt.key ?? opt.label;
  drivable(reset, SHOW_ALL, driveKey);
  host.classList.add("tc");
  host.replaceChildren(plot, tip);
  if (series.length > 1)
    host.append(
      legend,
      h(
        "p",
        { class: "tc-legend__hint" },
        h(
          "span",
          { class: "tc-hint--pointer" },
          "Hover a source to single it out · click to keep it on or off · Show all resets",
        ),
        h(
          "span",
          { class: "tc-hint--touch" },
          "Tap a source to keep it on or off · Show all resets",
        ),
      ),
    );
  const colour = (/** @type {number} */ i) =>
    series[i].colour ?? `var(--chart-${(i % 5) + 1})`;
  /** A legend or tooltip swatch: striped where the line is dashed. */
  const swatch = (/** @type {number} */ i) =>
    dashOf(i)
      ? `repeating-linear-gradient(90deg, ${colour(i)} 0 3px, transparent 3px 5px)`
      : colour(i);
  const visible = () =>
    series.map((_, i) => i).filter((i) => sel.size === 0 || sel.has(i));
  const range = () => group.zoom ?? { from: opt.from, to: opt.to };
  /** @type {null | {x: (t: number) => number, inv: (px: number) => number,
   *   W: number, P: {l: number, r: number, t: number, b: number},
   *   from: number, to: number, svg: SVGSVGElement}} */
  let geo = null;

  /** A new selection, told to `onSelect`. @param {Set<number>} next */
  const setSel = (next) => {
    sel = next;
    opt.onSelect?.([...sel].sort((a, b) => a - b));
  };
  /** @type {ChartState & {select: (i: number | null, on?: boolean) => void}} */
  const state = {
    host,
    draw,
    cursorAt: (t, own) => {
      cursor = t;
      if (!own && t == null) hoverSeries = null;
      draw();
      if (!own) tip.hidden = pinned == null;
    },
    destroy: () => {
      group.charts.delete(state);
      ro?.disconnect();
    },
    // Source `i` on or off (a toggle without `on`); null: show all.
    select: (i, on) => {
      if (i == null) setSel(new Set());
      else if (on === undefined || on !== sel.has(i))
        setSel(toggleSource(sel, i, series.length));
      draw();
    },
  };

  function draw() {
    if (series.length === 0 || series[0].points.length === 0) {
      plot.replaceChildren();
      return;
    }
    const { from, to } = range();
    const W = Math.max(240, host.clientWidth);
    const H = height;
    const P = { l: 48, r: 10, t: 16, b: 22 };
    const x = (/** @type {number} */ t) =>
      P.l + ((t - from) / (to - from || 1)) * (W - P.l - P.r);
    const vis = visible();
    const inR = (/** @type {Point} */ p) => p[0] >= from - 1 && p[0] <= to + 1;
    let hi = opt.yMax ?? 0;
    if (opt.yMax == null) {
      if (stacked)
        series[0].points.forEach((p, i) => {
          if (inR(p))
            hi = Math.max(
              hi,
              vis.reduce((a, si) => a + (series[si].points[i]?.[1] ?? 0), 0),
            );
        });
      else
        for (const si of vis)
          for (const p of series[si].points)
            if (inR(p)) hi = Math.max(hi, p[1]);
      if (opt.threshold != null) hi = Math.max(hi, opt.threshold * 1.08);
    }
    hi = niceMax(hi || 1, unit);
    const y = (/** @type {number} */ v) => H - P.b - (v / hi) * (H - P.t - P.b);
    const svg = /** @type {SVGSVGElement} */ (
      s("svg", { viewBox: `0 0 ${W} ${H}`, height: H, "aria-hidden": "true" })
    );
    const clip = `tc${Math.random().toString(36).slice(2, 8)}`;
    const defs = s("defs", {});
    const cp = s("clipPath", { id: clip });
    cp.append(s("rect", { x: P.l, y: 0, width: W - P.l - P.r, height: H }));
    defs.append(cp);
    svg.append(defs);
    for (let i = 0; i <= 2; i++) {
      const v = (hi * i) / 2;
      svg.append(
        s("line", {
          class: "tc-grid",
          x1: P.l,
          x2: W - P.r,
          y1: y(v),
          y2: y(v),
        }),
      );
      const tx = s("text", {
        class: "tc-tick",
        x: P.l - 6,
        y: y(v) + 4,
        "text-anchor": "end",
      });
      tx.textContent = fmtValue(v, unit === "flag" ? "count" : unit);
      svg.append(tx);
    }
    const span = to - from;
    const stepT =
      span <= 2 * 3600
        ? 900
        : span <= 8 * 3600
          ? 3600
          : span <= 86400 * 1.2
            ? 6 * 3600
            : 86400;
    const n = Math.max(2, Math.floor((W - P.l - P.r) / 70));
    let every = 1;
    while (span / (stepT * every) > n) every++;
    for (
      let t = Math.ceil(from / (stepT * every)) * stepT * every;
      t <= to;
      t += stepT * every
    ) {
      const tx = s("text", {
        class: "tc-tick",
        x: x(t),
        y: H - 6,
        "text-anchor": "middle",
      });
      tx.textContent = stepT >= 86400 ? day(t).replace(/^\w+ /, "") : hhmm(t);
      svg.append(tx);
    }
    const g = s("g", { "clip-path": `url(#${clip})` });
    svg.append(g);
    if (opt.threshold != null)
      g.append(
        s("line", {
          class: "tc-threshold",
          x1: P.l,
          x2: W - P.r,
          y1: y(opt.threshold),
          y2: y(opt.threshold),
        }),
      );
    for (const a of annotations)
      if (a.at >= from && a.at <= to)
        g.append(
          s("line", {
            class: "tc-ann",
            x1: x(a.at),
            x2: x(a.at),
            y1: P.t - 4,
            y2: H - P.b,
          }),
        );
    const base = series[0].points.map(() => 0);
    series.forEach((sr, si) => {
      if (!vis.includes(si)) return;
      const dim = hoverSeries != null && hoverSeries !== si;
      const pts = sr.points.map((p, i) => [
        x(p[0]),
        y(stacked ? (base[i] ?? 0) + p[1] : p[1]),
      ]);
      const d = pts
        .map((p, i) => `${i ? "L" : "M"}${p[0].toFixed(1)},${p[1].toFixed(1)}`)
        .join("");
      if (stacked) {
        const back = sr.points
          .map((p, i) => [x(p[0]), y(base[i] ?? 0)])
          .reverse()
          .map((p) => `L${p[0].toFixed(1)},${p[1].toFixed(1)}`)
          .join("");
        const area = s("path", { d: `${d}${back}Z`, class: "tc-area" });
        area.style.fill = colour(si);
        area.style.opacity = dim ? "0.06" : "0.22";
        g.append(area);
        sr.points.forEach((p, i) => (base[i] = (base[i] ?? 0) + p[1]));
      } else if (vis.length <= 2) {
        const last = pts[pts.length - 1];
        const area = s("path", {
          class: "tc-area",
          d: `${d}L${last[0]},${y(0)}L${pts[0][0]},${y(0)}Z`,
        });
        area.style.fill = colour(si);
        area.style.opacity = dim ? "0.02" : vis.length === 1 ? "0.12" : "0.06";
        g.append(area);
      }
      if (pts.length === 1) {
        const dot = s("circle", {
          class: "tc-point",
          cx: pts[0][0],
          cy: pts[0][1],
          r: 3.5,
        });
        dot.style.fill = colour(si);
        g.append(dot);
      }
      const line = s("path", {
        class: "tc-line",
        d,
        "stroke-width": hoverSeries === si ? 2.5 : 1.75,
      });
      line.style.stroke = colour(si);
      const dash = dashOf(si);
      if (dash) line.setAttribute("stroke-dasharray", dash);
      line.style.opacity = dim ? "0.18" : "1";
      g.append(line);
    });
    for (const a of annotations) {
      if (a.at < from || a.at > to) continue;
      const m = s("circle", {
        class: `tc-mark tc-mark--${a.tone ?? "info"}`,
        cx: x(a.at),
        cy: P.t - 8,
        r: 5,
        "data-at": a.at,
      });
      const title = s("title", {});
      title.textContent = `${a.label} · ${dayTime(a.at)}; click to pin`;
      m.append(title);
      svg.append(m);
    }
    if (brush)
      svg.append(
        s("rect", {
          class: "tc-brush",
          x: Math.min(brush.x0, brush.x1),
          y: P.t,
          width: Math.abs(brush.x1 - brush.x0),
          height: H - P.t - P.b,
        }),
      );
    const t = pinned ?? cursor;
    if (t != null && t >= from && t <= to) {
      svg.append(
        s("line", {
          class: pinned != null ? "tc-xhair tc-xhair--pinned" : "tc-xhair",
          x1: x(t),
          x2: x(t),
          y1: P.t,
          y2: H - P.b,
        }),
      );
      const acc = series[0].points.map(() => 0);
      series.forEach((sr, si) => {
        if (!vis.includes(si)) return;
        const i = indexAt(sr.points, t);
        const v = sr.points[i][1] + (stacked ? acc[i] : 0);
        if (stacked) sr.points.forEach((p, k) => (acc[k] += p[1]));
        const c = s("circle", {
          class: "tc-snap",
          cx: x(sr.points[i][0]),
          cy: y(v),
          r: hoverSeries === si ? 4.5 : 3.5,
        });
        c.style.fill = colour(si);
        svg.append(c);
      });
    }
    plot.replaceChildren(svg);
    geo = {
      x,
      inv: (px) => from + ((px - P.l) / (W - P.l - P.r)) * (to - from),
      W,
      P,
      from,
      to,
      svg,
    };
    paintLegend();
    paintTip();
  }

  const items = series.map((sr, si) => {
    const val = h("b", { class: "tc-num" });
    const sw = h("i", { class: "tc-swatch" });
    sw.style.background = swatch(si);
    const b = h(
      "button",
      {
        type: "button",
        class: "tc-legend__item",
        title: `${sr.label}: hover to single it out, click to keep it on or off`,
      },
      sw,
      h("span", null, sr.label),
      val,
    );
    drivable(b, SOURCE, `${driveKey}/${sr.label}`);
    const hover = (/** @type {number | null} */ v) => () => {
      hoverSeries = v;
      draw();
    };
    b.addEventListener("mouseenter", hover(si));
    b.addEventListener("mouseleave", hover(null));
    b.addEventListener("focus", hover(si));
    b.addEventListener("blur", hover(null));
    b.addEventListener("click", () => {
      // Kenny, 2026-10-03: a click toggles one source; no modifier keys.
      setSel(toggleSource(sel, si, series.length));
      draw();
    });
    return { b, val };
  });
  if (series.length > 1) legend.append(...items.map((i) => i.b), reset);
  reset.addEventListener("click", () => {
    setSel(new Set());
    draw();
  });

  function paintLegend() {
    if (series.length < 2) return;
    const t = pinned ?? cursor;
    series.forEach((sr, si) => {
      const on = sel.size === 0 || sel.has(si);
      const pts = sr.points;
      const v = t != null ? pts[indexAt(pts, t)][1] : pts[pts.length - 1]?.[1];
      items[si].b.setAttribute("aria-pressed", String(sel.has(si)));
      items[si].b.toggleAttribute("data-off", !on);
      items[si].val.textContent =
        sr.n != null && t == null
          ? sr.n.toLocaleString("en-GB")
          : v == null
            ? ""
            : fmtValue(v, unit);
    });
    reset.hidden = sel.size === 0;
  }

  function paintTip() {
    const t = pinned ?? cursor;
    if (t == null || !geo || t < geo.from || t > geo.to) {
      tip.hidden = true;
      return;
    }
    const rows = visible()
      .map((si) => {
        const sr = series[si];
        const i = indexAt(sr.points, t);
        return { si, sr, v: sr.points[i][1], d: hourChange(sr.points, i) };
      })
      .sort((a, b) => b.v - a.v);
    const reach = (geo.to - geo.from) / 30;
    const near = annotations.filter((a) => Math.abs(a.at - t) < reach);
    const head = h("b", null, dayTime(Math.round(t / 60) * 60));
    if (pinned != null) {
      const x = h(
        "button",
        {
          type: "button",
          class: "tc-tip__x",
          "aria-label": "Release the pinned reading (Esc)",
          title: "Release (Esc)",
        },
        "✕",
      );
      x.addEventListener("click", () => {
        pinned = null;
        draw();
      });
      head.append(h("span", { class: "tc-tip__pin" }, "pinned", x));
    }
    tip.replaceChildren(
      head,
      ...rows.map((r) => {
        const sw = h("i", { class: "tc-swatch" });
        sw.style.background = swatch(r.si);
        return h(
          "div",
          {
            class:
              hoverSeries === r.si
                ? "tc-tip__row tc-tip__row--hot"
                : "tc-tip__row",
          },
          sw,
          h("span", null, r.sr.label || opt.label),
          h("span", { class: "tc-num" }, fmtValue(r.v, unit)),
          h(
            "span",
            {
              class: `tc-num tc-tip__d${r.d > 0 ? " up" : r.d < 0 ? " down" : ""}`,
            },
            r.d === 0
              ? "±0"
              : `${r.d > 0 ? "▲" : "▼"} ${fmtValue(Math.abs(r.d), unit)}`,
          ),
        );
      }),
      h("p", { class: "tc-tip__foot" }, "change over the hour before"),
      ...near.map((a) =>
        h(
          "p",
          { class: "tc-tip__ev" },
          h("span", { class: `tc-dot tc-dot--${a.tone ?? "info"}` }),
          `${hhmm(a.at)} ${a.label}`,
          ...(pinned != null && a.href
            ? [h("a", { href: a.href }, " Open")]
            : []),
        ),
      ),
    );
    tip.hidden = false;
    tip.classList.toggle("tc-tip--pinned", pinned != null);
    const px = geo.x(t);
    const w = 240;
    tip.style.left = `${px + 14 + w > geo.W ? Math.max(0, px - w - 14) : px + 14}px`;
    tip.style.top = "4px";
  }

  const evT = (/** @type {PointerEvent} */ e) => {
    if (!geo) return null;
    const r = geo.svg.getBoundingClientRect();
    return geo.inv(e.clientX - r.left);
  };
  const all = (/** @type {number | null} */ t) => {
    for (const c of group.charts) c.cursorAt(t, c === state);
  };
  plot.addEventListener("pointerdown", (e) => {
    if (!geo) return;
    const mark = /** @type {Element} */ (e.target).closest?.(".tc-mark");
    if (mark) {
      pinned = Number(mark.getAttribute("data-at"));
      all(pinned);
      return;
    }
    const r = geo.svg.getBoundingClientRect();
    brush = { x0: e.clientX - r.left, x1: e.clientX - r.left };
    plot.setPointerCapture?.(e.pointerId);
  });
  plot.addEventListener("pointermove", (e) => {
    if (!geo) return;
    if (brush) {
      const r = geo.svg.getBoundingClientRect();
      brush.x1 = Math.max(
        geo.P.l,
        Math.min(geo.W - geo.P.r, e.clientX - r.left),
      );
      draw();
      return;
    }
    const t = evT(e);
    if (t == null || t < geo.from || t > geo.to) return;
    all(t);
  });
  plot.addEventListener("pointerup", (e) => {
    if (!brush || !geo) return;
    const b = brush;
    brush = null;
    if (Math.abs(b.x1 - b.x0) > 6) {
      const a = geo.inv(Math.min(b.x0, b.x1));
      const z = geo.inv(Math.max(b.x0, b.x1));
      pinned = null;
      group.setZoom({ from: a, to: z });
    } else {
      const t = evT(e);
      if (t == null) return;
      pinned =
        pinned != null && Math.abs(geo.x(pinned) - geo.x(t)) < 4 ? null : t;
      draw();
    }
  });
  plot.addEventListener("pointerleave", () => {
    if (!brush) all(null);
  });
  plot.addEventListener("dblclick", () => group.setZoom(null));
  plot.addEventListener("keydown", (e) => {
    const sr = series[visible()[0]];
    if (!sr || sr.points.length === 0) return;
    let i = indexAt(
      sr.points,
      pinned ?? cursor ?? sr.points[sr.points.length - 1][0],
    );
    const step = e.shiftKey ? 10 : 1;
    if (e.key === "ArrowLeft") i -= step;
    else if (e.key === "ArrowRight") i += step;
    else if (e.key === "Home") i = 0;
    else if (e.key === "End") i = sr.points.length - 1;
    else if (e.key === "Enter") {
      pinned = pinned != null ? null : cursor;
      draw();
      return;
    } else if (e.key === "Escape") {
      e.stopPropagation();
      if (pinned != null) pinned = null;
      else if (sel.size) setSel(new Set());
      else if (group.zoom) group.setZoom(null);
      draw();
      return;
    } else return;
    e.preventDefault();
    i = Math.max(0, Math.min(sr.points.length - 1, i));
    const t = sr.points[i][0];
    if (pinned != null) pinned = t;
    all(t);
  });
  plot.addEventListener("focus", () => {
    if (cursor == null && series[0]?.points.length) {
      cursor = series[0].points[series[0].points.length - 1][0];
      draw();
    }
  });
  /** @type {ResizeObserver | null} */
  const ro =
    typeof ResizeObserver === "function"
      ? new ResizeObserver(() => draw())
      : null;
  ro?.observe(host);
  group.charts.add(state);
  // Esc anywhere outside a chart (and outside a dialog) resets the zoom.
  if (!group.escWired && typeof document !== "undefined") {
    group.escWired = true;
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Escape" || !group.zoom) return;
      const t = /** @type {Element | null} */ (e.target);
      if (t?.closest?.(".tc-plot, dialog")) return;
      group.setZoom(null);
    });
  }
  draw();
  return state;
}
