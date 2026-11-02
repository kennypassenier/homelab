// The Host page's KPI strip: eight trend tiles (fix-371-1; Kenny approved
// the demo 2026-10-04), each with its 24 h sparkline, read from
// /data/host/trend.

import { fetchJson, h } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { hostTile, HOST_FIGURES } from "../host.js";
import { changeGate } from "../quiet.js";
import { current } from "../store.js";
import { skeleton } from "../ui.js";
import {
  attachTrendCharts,
  setTrendData,
} from "/static/kp/dist/kp-themes.min.js";

// Live view (invariant 39): the tiles' link to Charts is declared here.
const KPI_CHART = declare({
  id: "host-kpi-chart",
  page: "host",
  opens: "view",
  row: "<figure>",
  what: "open one of the host's figures on Charts (CPU, memory, load, root disk, disk traffic, network, CPU temperature, swap)",
});

/**
 * fix-371-1: one Host tile, a link to its figure on Charts: label, the
 * big number, "now · peak · context", and the 24 h trend, which answers a
 * pointer with its moment and value. kp-themes' trend tile
 * (`.kp-kpi--trend`: the whole tile is its `.kp-kpi__link`, the trend is
 * the time chart's spark variant with the axis and the reading chip, drawn
 * by `attachTrendCharts`). Loading until `set` gives it a tile.
 * @param {import("../host.js").HostFigure} fig
 */
function trendTile(fig) {
  const label = h("span", { class: "kp-kpi__label" }, fig.label);
  const value = h("span", { class: "kp-kpi__value" }, skeleton("3ch"));
  const ctx = h("span", { class: "kp-kpi__trend" }, skeleton("80%"));
  const figure = h("figure", {
    class: "kp-kpi__chart",
    "data-kp-chart": "spark",
    "data-kp-spark-head": "none",
    "data-kp-spark-axis": "relative",
    "data-kp-chart-loading": "",
    "aria-label": `${fig.label}, last 24 hours`,
    style: `--kp-chart-series: var(--chart-${fig.colour})`,
  });
  const link = drivable(
    h(
      "a",
      {
        class: "kp-kpi__link",
        href: fig.chart ? `/charts#chart-${fig.chart}` : "/charts",
        title: "Open this figure on Charts",
      },
      h("span", { class: "kp-kpi__link-word" }, "Charts"),
      " ↗",
    ),
    KPI_CHART,
    fig.key,
  );
  const el = h(
    "div",
    {
      class: "kp-kpi kp-kpi--trend hx-kpi",
      "data-key": fig.key,
      "data-loading": "",
    },
    label,
    link,
    value,
    ctx,
    figure,
  );
  return {
    fig,
    el,
    /** @param {import("../host.js").HostTile} t */
    set: (t) => {
      delete el.dataset.loading;
      el.title = t.title;
      label.textContent = t.label;
      value.replaceChildren(
        t.value,
        ...(t.unit ? [h("small", null, t.unit)] : []),
      );
      ctx.replaceChildren(
        ...(t.now ? ["now ", h("b", null, t.now), " · "] : []),
        t.rest,
      );
      // The points are unix seconds (kp's timeUnit).
      setTrendData(figure, {
        timeUnit: "s",
        points: t.points,
        step: 600,
        // The reading chip and tooltip print the figure's own unit.
        ...(fig.rate
          ? { unitKind: "bytes/s" }
          : fig.unit === "%"
            ? { unitKind: "percent" }
            : { unit: fig.unit, digits: fig.digits ?? 1 }),
      });
      attachTrendCharts(document);
    },
  };
}

/**
 * @param {import("./hostenv.js").HostEnv} env
 */
export function createKpis(env) {
  const { abort } = env;
  // fix-371-1 (Kenny approved the Host tiles demo 2026-10-04:
  // redesign-3.71/demos-3711/host-kpis.html): eight tiles, each with its
  // 24 h sparkline, the 15-minute average as the big number ("avg 15
  // min"), now and the day's peak under it, and a link to the figure on
  // Charts. Drawn in their final size from the first frame.
  /** @type {Map<string, import("../host.js").TrendFigure> | null} */
  let trend = null;
  /** @type {string | null} why the trends could not be read */
  let trendWhy = null;
  const tiles = HOST_FIGURES.map((f) => trendTile(f));
  const kpisEl = h(
    "div",
    {
      class: "kp-kpis hx-kpis",
      "data-kp-kpis-columns": "4 2 1",
      role: "group",
      "aria-label": "The host's figures",
    },
    ...tiles.map((t) => t.el),
  );

  const paint = () => {
    if (trend == null && trendWhy == null) return;
    const f = current().fleet ?? null;
    for (const t of tiles)
      t.set(hostTile(t.fig, trend?.get(t.fig.key) ?? null, f, trendWhy));
  };
  const trendGate = changeGate();
  const load = async () => {
    const r = await fetchJson(
      "/data/host/trend",
      "the host's trends",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (r.ok) {
      /** @type {import("../host.js").TrendFigure[]} */
      const figs = r.body.figures ?? [];
      if (!trendGate.isNew(figs) && trend && !trendWhy) return;
      trend = new Map(figs.map((x) => [x.key, x]));
      trendWhy = null;
    } else {
      trend = new Map();
      trendWhy = `${r.error.what}: ${r.error.why}`;
    }
    paint();
  };
  return { el: kpisEl, paint, load };
}
