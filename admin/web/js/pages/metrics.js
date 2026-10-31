// Metrics (redesign-371-metrics, release 3.71.0; rebuilt on kp-themes 11.0.0
// from the demo Kenny approved in full on 2026-10-19, app-homelab-metrics,
// feat-kp11demo-10). Two views of one page, System (replace-grafana) and
// Traffic (replace-goaccess), sharing the window (1h … 30d) in the address,
// so switching keeps it.
//
// Top to bottom: the header (the source of the data; the System | Traffic
// switch on the right), one grid of three rows on shared columns (which
// machines are shown, the window, the refresh), the key figures with their
// 24-hour trends, the attention band (a failing drive), then the charts in
// sections. Every section is a kp chart group of its own: its window keys,
// its zoom chip and its crosshair; the page's window sets every section at
// once and a section's own keys change only that section. The charts are
// fed in place on every read (kp's live update), so a pinned reading, a
// zoom and a source switched off survive each refresh and the page never
// skips one. What the host did in the window is marked on every chart (a
// window of work as a hatched band, a failure as a line) and listed in the
// operations ribbon under the sections.
//
// Data: /data/charts (Prometheus), /data/traffic (Loki), /data/history,
// the fleet store (the host's own CPU, memory, disk and load).

import { errorBox, fetchJson, fetchReport, h, labelCells } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import {
  CLASSES,
  HOST_SECTIONS,
  RANGE_SECONDS,
  RIBBON_LANES,
  STACK_SECTIONS,
  annotations,
  byTitle,
  changeText,
  count,
  drives,
  durationText,
  extremes,
  fill,
  healthPeriods,
  hostHeat,
  hostLegend,
  machinesOf,
  nowValue,
  peak,
  poweredOn,
  requestsNote,
  ribbonRuns,
  statusClasses,
  statusParts,
  statusTone,
  trafficFigures,
  usualMachines,
} from "../metricsview.js";
import { current, subscribe } from "../store.js";
import {
  attachPageCharts,
  failChart,
  feedChart,
  fmtValue,
  hhmm,
} from "../charts.js";
import {
  attentionBand,
  dot,
  ensureStyle,
  keyRow,
  pageHeader,
  section,
  segSwitch,
} from "../ui.js";
import { formatClock, formatDateTime, formatDay, gbText } from "../format.js";
import { choice, setParams } from "../urlstate.js";
import {
  CHART_RANGE_EVENT,
  CHART_RETRY_EVENT,
  CHART_SELECT_EVENT,
  CHART_ZOOM_EVENT,
  chartSelect,
  chartZoom,
  setTrendData,
  RANGE_EVENT,
  attachRanges,
  REFRESH_EVENT,
  attachRefresh,
  refreshOf,
  GANTT_OPEN_EVENT,
  GANTT_WINDOW_EVENT,
  attachGantts,
  setGanttRuns,
  setGanttState,
  setGanttWindow,
  CHANGE_EVENT,
  combobox,
  drawnSelect,
  attachUsualPickers,
  setUsual,
  attachIdmarks,
  idmarkColours,
  attachHoverCards,
  STACKBAR_PICK_EVENT,
  STACKBAR_RETRY_EVENT,
  setStackbar,
  HEAT_PICK_EVENT,
  HEAT_ROW_EVENT,
  attachHeats,
  setHeat,
  setHeatLegend,
  setHeatPick,
  setHeatRows,
  RETRY_EVENT as TABLE_RETRY_EVENT,
  attachDataTables,
  dataTable as tableOf,
  attachBarCells,
  setBarCell,
  attachUptimes,
  setUptime,
} from "/static/kp/dist/kp-themes.min.js";
import { gridRow, formGrid } from "../gridrow.js";
import { syncRows } from "../inplace.js";
import { periodsOf, uptimeQuery } from "../uptimeview.js";
import { REFRESH_S } from "../quiet.js";

const RANGES = /** @type {const} */ (["1h", "6h", "24h", "7d", "30d"]);
const TABS = /** @type {const} */ (["system", "traffic"]);
/** Seconds between two reads, until a person picks another interval. */
const EVERY_S = REFRESH_S;
/** The window every key figure's trend covers. */
const TREND_RANGE = "24h";

// Live view (invariant 39): every control this page draws.
const VIEW = declare({
  id: "metrics-view",
  page: "metrics",
  opens: "view",
  row: "system|traffic",
  what: "switch Metrics between System (the host and its stacks) and Traffic (the gateway's visitors)",
});
const WINDOW = declare({
  id: "metrics-window",
  page: "metrics",
  opens: "view",
  row: "1h|6h|24h|7d|30d",
  what: "show the charts over the last hour, 6 hours, day, week or 30 days",
});
const SEE_DRIVE = declare({
  id: "metrics-see-drive",
  page: "metrics",
  opens: "view",
  row: "<drive>",
  what: "scroll to the Drives table from the failing drive's alert",
});
const HOSTNAME = declare({
  id: "metrics-hostname",
  page: "metrics",
  opens: "view",
  row: "<hostname>",
  what: "turn one hostname on or off in the Requests per hostname chart",
  shows: "on the Traffic view",
  reach: [{ do: "click", control: "metrics-view", row: "traffic" }],
});
const HOSTNAMES_AS = declare({
  id: "metrics-hostnames-as",
  page: "metrics",
  opens: "view",
  row: "heat|table",
  what: "show the hostnames as a heat grid (hostname by hour) or as a sortable table",
  shows: "on the Traffic view",
  reach: [{ do: "click", control: "metrics-view", row: "traffic" }],
});
const REQUESTS_HOST = declareField({
  id: "metrics-requests-host",
  page: "metrics",
  what: "the hostname whose recent requests the Traffic view lists",
});
const SHOWING = declare({
  id: "metrics-showing",
  page: "metrics",
  opens: "view",
  what: "pick whose charts the System view shows: the host or one stack",
});
const SCOPE_FIELD = declareField({
  id: "metrics-scope",
  page: "metrics",
  what: "the machines picker on Metrics: type a stack name to search the list",
});
const SECTION_WINDOW = declare({
  id: "metrics-section-window",
  page: "metrics",
  opens: "view",
  row: "<section>/<window>",
  what: "show one section's charts over the last hour, 6 hours, day, week or 30 days, the others stay as they are",
});
const TILE = declare({
  id: "metrics-kpi",
  page: "metrics",
  opens: "view",
  row: "<tile>",
  what: "a KPI tile: scroll to the chart it sums up",
});
const BACKUPS = declare({
  id: "metrics-drive-backups",
  page: "metrics",
  opens: "view",
  row: "<drive>",
  what: "the failing drive's alert: open Backups to back the stacks up now",
});

/**
 * @typedef {import("../metricsview.js").Panel} Panel
 * @typedef {import("../metricsview.js").Card} Card
 * @typedef {import("../metricsview.js").Section} Section
 * @typedef {import("../charts.js").Annotation} Annotation
 * @typedef {import("../charts.js").Point} Point
 * @typedef {import("../charts.js").Unit} Unit
 * @typedef {{ok: true, body: any} | {ok: false, error: import("../doctor.js").RouteError}} Read
 * @typedef {{key: string, el: HTMLElement, range: string, group: HTMLElement,
 *   zoom: {from: number, to: number} | null, zoomRead: number,
 *   cards: Map<string, {el: HTMLElement, fig: HTMLElement, body: HTMLElement,
 *   setFoot: (parts: import("../dom.js").Child[]) => void,
 *   card: Card}>, section: Section}} Block one section of charts
 */

/** Why a read failed, in one sentence a chart can print in its plot. @param {import("../doctor.js").RouteError} e */
const notReadWords = (e) => `Could not read ${e.what}: ${e.why}.`;

/** What a machine in the address is called to a person. @param {string} m */
const machineName = (m) =>
  m === "host" ? (current().fleet?.host.name ?? "host") : m;

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  ensureStyle("/css/pages/metrics.css");
  root.classList.add("mk-page", "nx-ops");
  const params = new URLSearchParams(location.search);
  const tab = choice(params, "tab", TABS, "system");
  const range = choice(params, "range", RANGES, "24h");
  const machines =
    tab === "system" ? machinesOf(params.get("stack")) : ["host"];
  const lead = machines[0];
  /** The stack the sections show; empty for the host. */
  const stack = lead === "host" ? "" : lead;
  const go = (/** @type {Record<string, string | null>} */ p) =>
    ctx.navigate(`/charts${setParams(location.search, p)}`);
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];

  // ── header ────────────────────────────────────────────────────────────
  const view = segSwitch({
    label: "Metrics view",
    items: [
      {
        value: "system",
        label: "System",
        hint: "The host and its stacks: CPU, memory, disks, temperature",
      },
      {
        value: "traffic",
        label: "Traffic",
        hint: "The gateway's visitors: requests, errors, hostnames, clients",
      },
    ],
    value: tab,
    onChange: (v) => go({ tab: v === "system" ? null : v, stack: null }),
    drive: { id: VIEW },
  });
  const head = pageHeader({
    title: "Metrics",
    desc:
      tab === "system"
        ? "How hard the host and each stack are working, over time, with what the host did marked on every chart."
        : "Who visits the services behind the gateway, how much, and how many requests fail.",
    meta: [
      h(
        "span",
        null,
        tab === "system"
          ? "Source: Prometheus"
          : "Source: the gateway's access log in Loki",
      ),
    ],
    live: false,
    actions: [view.el],
  });

  // ── the three rows on shared columns: what is shown, the window, the refresh ──
  const refreshEl = h("div", {
    class: "kp-refresh",
    "data-kp-refresh": EVERY_S,
    "data-kp-refresh-choices": "0,15,30,60,300",
    "data-kp-refresh-label": "Refresh",
    id: "mk-refresh",
    "aria-label": "Refresh the charts",
  });
  const rangeEl = h("div", {
    class: "kp-range mk-field",
    "data-kp-range": range,
    "data-kp-range-presets": RANGES.join(" "),
    "data-kp-range-custom": "off",
    id: "mk-range",
    "aria-label": "Time range",
  });
  const picker = tab === "system" ? scopePicker() : null;
  const aside = (/** @type {string} */ text) =>
    h("span", { class: "mk-aside" }, text);
  const rows = h(
    "section",
    { class: "kp-card mk-controls", "aria-label": "What this page shows" },
    formGrid([
      gridRow({
        label: "Showing",
        labelFor: picker ? SCOPE_FIELD : "mk-scope-fixed",
        cls: "mk-row",
        control: picker
          ? picker.el
          : h(
              "span",
              { class: "mk-field mk-fixed", id: "mk-scope-fixed" },
              "Every hostname behind the gateway",
            ),
        value: aside(
          picker
            ? "Picks whose charts the page shows: the host or a stack, or up to three machines side by side."
            : "The access log covers every hostname the gateway serves.",
        ),
      }),
      gridRow({
        label: "Window",
        labelId: "mk-range-label",
        cls: "mk-row",
        control: rangeEl,
        value: aside(
          "Sets how far back every section reaches; a section's own keys change only that section.",
        ),
      }),
      // The kit's own refresh row (kp-themes 11.1.1): the bar is a direct
      // child of the grid and draws its label in the label track.
      refreshEl,
      gridRow({
        cls: "mk-row",
        control: aside(
          "Reads the charts again and adds the new readings in place: a pinned reading, a zoom and the sources you switched off stay.",
        ),
        span: "control-value",
      }),
    ]),
  );
  rangeEl.setAttribute("aria-labelledby", "mk-range-label");

  const failed = h("div", { class: "mk-failed" });
  root.replaceChildren(head.el, rows, failed);

  // ── what the host did, read once per tick ─────────────────────────────
  /** @type {import("../activity.js").Entry[]} */
  let entries = [];
  let historyOk = true;
  /** The longest window anything on the page has asked for, seconds. */
  let historySpan = RANGE_SECONDS[range];
  /** @param {number} span seconds */
  const readHistory = async (span) => {
    historySpan = Math.max(historySpan, span);
    const since = Math.floor(Date.now() / 1000) - historySpan;
    const r = await fetchReport(
      `/data/history?since=${since}&limit=500`,
      "the history",
      abort.signal,
    );
    historyOk = r.ok;
    entries = r.ok ? (r.report?.entries ?? []) : [];
  };
  /** @param {number} from @param {number} to */
  const marksIn = (from, to) =>
    annotations(entries, { from, to, ...(stack ? { stack } : {}) });

  // ── a section of charts: its group, its window keys, its zoom chip ────
  /**
   * @param {{key: string, title: string, desc: string}} meta
   * @param {Card[]} cards
   * @returns {Block}
   */
  const block = (meta, cards) => {
    const ranges = h(
      "div",
      {
        class: "kp-chart-ranges",
        role: "group",
        "aria-label": `Time range, ${meta.title}`,
      },
      RANGES.map((r) =>
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--sm",
            "data-kp-chart-range": r,
            title: `Show ${meta.title} over the last ${r}`,
          },
          r,
        ),
      ),
    );
    const bar = h(
      "div",
      { class: "kp-chart-group__bar mk-groupbar" },
      ranges,
      h("span", {
        class: "kp-chart-zoom",
        "data-kp-chart-zoom": "",
        hidden: "",
      }),
    );
    /** @type {Block["cards"]} */
    const made = new Map();
    const panels = h(
      "div",
      { class: "mk-panels" },
      cards.map((c) => {
        const k = card(c);
        made.set(c.key, { ...k, card: c });
        return k.el;
      }),
    );
    const group = h(
      "div",
      {
        class: "kp-chart-group mk-group",
        "data-kp-chart-group": "",
        "data-kp-chart-span": range,
        "data-kp-key": meta.key,
      },
      bar,
      panels,
    );
    const el = h(
      "section",
      {
        class: "mk-section",
        "aria-labelledby": `mk-h-${meta.key}`,
      },
      h(
        "div",
        { class: "mk-section__head" },
        h("h2", { id: `mk-h-${meta.key}` }, meta.title),
        h("p", null, meta.desc),
      ),
      group,
    );
    return {
      key: meta.key,
      el,
      range,
      group,
      zoom: null,
      zoomRead: 0,
      cards: made,
      section: { title: meta.title, desc: meta.desc, cards },
    };
  };

  /**
   * One chart card: heading, one sentence, the chart (kp's, waiting for its
   * first reading at its final height).
   * @param {Card} c
   */
  const card = (c) => {
    const s = section({
      title: c.title,
      desc: c.desc,
      id: `chart-${c.key}`,
      level: "h3",
    });
    s.el.classList.add("mk-card", `mk-span-${c.span}`);
    s.el.dataset.key = c.key;
    const fig = h("figure", {
      class: "kp-chart mk-chart",
      "data-kp-chart": "",
      "data-kp-chart-loading": "",
      "aria-label": c.title,
      ...(c.sources ? { "data-kp-chart-sources": c.sources } : {}),
    });
    s.body.replaceChildren(fig);
    return { el: s.el, fig, body: s.body, setFoot: s.setFoot };
  };

  /** A chart whose read failed: the reason in its plot, with kp's Retry. @param {HTMLElement} fig @param {string} label @param {string} why */
  const notRead = (fig, label, why) => failChart(fig, label, why);

  // ── key figures: kp's trend tiles ─────────────────────────────────────
  /**
   * @typedef {{key: string, label: string, note?: string, target: string,
   *   chart: string, unit: "percent" | "bytes" | "count" | "celsius"}} TileSpec
   */
  /**
   * @param {TileSpec[]} specs
   * @param {string} label
   */
  const tileStrip = (specs, label) => {
    const el = h("div", {
      class: "kp-kpis mk-tiles",
      "data-kp-kpis-columns": "5 3 2 1",
      role: "group",
      "aria-label": label,
    });
    /** @type {Map<string, ReturnType<typeof tile>>} */
    const map = new Map();
    for (const s of specs) {
      const t = tile(s);
      map.set(s.key, t);
      el.append(t.el);
    }
    const group = h(
      "div",
      {
        class: "kp-chart-group mk-tilegroup",
        "data-kp-chart-group": "",
        "data-kp-chart-span": TREND_RANGE,
      },
      el,
    );
    return { el: group, strip: el, tiles: map };
  };

  /** @param {TileSpec} s */
  const tile = (s) => {
    const link = h(
      "a",
      {
        class: "kp-kpi__link",
        href: `${location.pathname}${location.search}#${s.target}`,
        "aria-label": `${s.label}: open the ${s.chart} chart`,
      },
      h("span", { class: "kp-kpi__link-word" }, "Chart"),
      " ↗",
    );
    drivable(link, TILE, s.key);
    link.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      document
        .getElementById(s.target)
        ?.scrollIntoView({ block: "start", behavior: "smooth" });
    });
    const value = h("span", { class: "kp-kpi__value" }, "—");
    const trend = h("span", { class: "kp-kpi__trend" }, "reading…");
    const fig = h("figure", {
      class: "kp-kpi__chart",
      "data-kp-chart": "spark",
      "data-kp-spark-head": "none",
      "data-kp-spark-axis": "relative",
      "aria-label": `${s.label}, last 24 hours`,
    });
    const el = h(
      "div",
      { class: "kp-kpi kp-kpi--trend", "data-key": s.key },
      h(
        "span",
        { class: "kp-kpi__label" },
        s.label,
        s.note
          ? [" ", h("span", { class: "kp-kpi__label-note" }, s.note)]
          : null,
      ),
      link,
      value,
      trend,
      fig,
    );
    return {
      el,
      /**
       * @param {{value: string, unit?: string, trend: (string | Node)[],
       *   hover: string, points?: Point[], step?: number,
       *   tone?: "warn" | "bad" | ""} | null} next null: still reading
       */
      set(next) {
        if (!next) {
          value.replaceChildren("—");
          trend.replaceChildren("reading…");
          link.removeAttribute("data-kp-hovercard");
          el.removeAttribute("data-kp-tone");
          setTrendData(fig, null);
          return;
        }
        value.replaceChildren(
          next.value,
          ...(next.unit ? [h("small", null, next.unit)] : []),
        );
        trend.replaceChildren(...next.trend);
        link.setAttribute("data-kp-hovercard", next.hover);
        if (next.tone)
          el.dataset.kpTone = next.tone === "bad" ? "destructive" : "warning";
        else el.removeAttribute("data-kp-tone");
        setTrendData(fig, {
          timeUnit: "s",
          points: next.points ?? [],
          step: next.step ?? 600,
          unitKind: s.unit,
        });
      },
      /** The read failed: the tile says so instead of pulsing for ever. */
      fail() {
        value.replaceChildren("—");
        trend.replaceChildren("not read");
        link.removeAttribute("data-kp-hovercard");
        el.removeAttribute("data-kp-tone");
        setTrendData(fig, { points: [] });
      },
    };
  };

  /**
   * The trend sentence of a tile: "now 42% · peak 61%".
   * @param {string} now @param {string | null} top
   */
  const nowPeak = (now, top) => [
    "now ",
    h("b", null, now),
    ...(top ? [` · peak ${top}`] : []),
  ];

  /**
   * What the card says when a person rests on a tile: the highest and the
   * lowest reading with their time, then the tile's own context.
   * @param {Point[]} points @param {(v: number) => string} fmt @param {string} [extra]
   */
  const hoverOf = (points, fmt, extra) => {
    const e = extremes(points);
    const words = e
      ? `Highest ${fmt(e.high.value)} at ${formatDateTime(e.high.at)}. Lowest ${fmt(e.low.value)} at ${formatDateTime(e.low.at)}.`
      : "No readings in the last 24 hours.";
    return extra ? `${words} ${extra}` : words;
  };

  /** @type {(() => void)[]} */
  const detachers = [];
  /** A read that failed: every tile says so. @param {Map<string, ReturnType<typeof tile>>} tiles */
  const tilesNotRead = (tiles) => {
    for (const t of tiles.values()) t.fail();
  };
  /** What runs once kp's parts are attached: the loading states of the parts that draw their own. @type {(() => void)[]} */
  const inits = [];

  /** @type {() => Promise<boolean>} */
  let read = async () => true;
  if (tab === "system") read = system();
  else read = traffic();

  root.append(
    keyRow([
      ["Tab", "focus a chart"],
      ["← →", "move through time (Shift: ten steps)"],
      ["Enter", "pin the reading"],
      ["Esc", "release · show all · reset zoom"],
      ["drag", "zoom a section's charts"],
      ["click", "a source turns it on or off"],
    ]),
  );

  // ── attach kp's parts, once the page stands ───────────────────────────
  detachers.push(attachPageCharts(root));
  detachers.push(attachRanges(rangeEl));
  detachers.push(attachRefresh(refreshEl));
  detachers.push(attachHoverCards(root));
  detachers.push(attachIdmarks(root));
  detachers.push(attachHeats(root));
  detachers.push(attachGantts(root));
  if (picker) detachers.push(attachUsualPickers(picker.el));
  for (const f of inits) f();
  for (const b of rangeEl.querySelectorAll(".kp-range__presets [data-value]"))
    drivable(
      /** @type {HTMLElement} */ (b),
      WINDOW,
      /** @type {HTMLElement} */ (b).dataset.value,
    );
  rangeEl.addEventListener(RANGE_EVENT, (e) => {
    const preset = /** @type {CustomEvent} */ (e).detail?.preset;
    if (preset && preset !== range && RANGES.includes(preset))
      go({ range: preset === "24h" ? null : preset });
  });
  if (picker) picker.attach();

  // The refresh control owns the timer, the "updated … ago" line and the
  // retries; the page answers each tick with a read.
  let reading = false;
  const tick = async (/** @type {{done: (ok: boolean) => void}} */ detail) => {
    if (reading) return detail.done(true);
    reading = true;
    let ok = true;
    try {
      ok = await read();
    } catch (e) {
      if (!abort.signal.aborted) ok = false;
    } finally {
      reading = false;
    }
    detail.done(ok);
  };
  refreshEl.addEventListener(
    REFRESH_EVENT,
    (e) => void tick(/** @type {CustomEvent} */ (e).detail),
  );
  // A chart's own Retry reads everything again, through the same control.
  root.addEventListener(CHART_RETRY_EVENT, () =>
    refreshOf(refreshEl)?.now("retry"),
  );
  queueMicrotask(() => refreshOf(refreshEl)?.now("now"));

  return () => {
    abort.abort();
    for (const d of detachers) d();
    for (const s of stops) s();
    root.classList.remove("mk-page", "nx-ops");
  };

  // ── the scope picker: kp's usual picker, up to three machines ─────────
  function scopePicker() {
    const input = h("input", {
      class: "kp-combobox__input",
      id: SCOPE_FIELD,
      type: "text",
      role: "combobox",
      autocomplete: "off",
      "aria-label": "Machines to show",
      "aria-expanded": "false",
      "aria-controls": "mk-scope-list",
    });
    const list = h("ul", {
      class: "kp-combobox__list",
      id: "mk-scope-list",
      role: "listbox",
      hidden: "",
    });
    const usual = () => usualMachines(current().fleet?.stacks ?? []);
    /** The stacks the fleet names now, as one string to compare. */
    const listing = () =>
      (current().fleet?.stacks ?? [])
        .map((s) => `${s.name}:${s.vmid}`)
        .sort()
        .join(",") + (current().fleet?.host.name ?? "");
    const el = h(
      "div",
      {
        class: "kp-combobox kp-usual mk-field",
        "data-kp-combobox": "",
        "data-kp-usual": JSON.stringify(usual()),
        "data-kp-usual-label": "Usual",
        "data-kp-multiple": "",
        "data-kp-max-tags": "3",
        "data-kp-remember": "metrics-scope-compare",
        id: "mk-scope",
      },
      h("ul", { class: "kp-tag-list", "data-kp-tag-list": "" }),
      input,
      list,
      h("p", {
        class: "kp-combobox__status",
        "data-kp-combobox-status": "",
        role: "status",
        "aria-live": "polite",
      }),
    );
    drivable(input, SHOWING);
    /** One option: its name, its colour mark, its detail. @param {string} value @param {string} label @param {string} mark @param {string} meta */
    const option = (value, label, mark, meta) =>
      h(
        "li",
        {
          class: "kp-combobox__option",
          role: "option",
          "data-kp-option": "",
          "data-value": value,
          "data-kp-label": label,
        },
        h("span", { class: "mk-opt__name" }, label),
        h("span", {
          class: "kp-idmark kp-idmark--sm mk-opt__mark",
          "data-kp-idmark": mark,
          "aria-hidden": "true",
        }),
        h("small", { class: "mk-opt__meta" }, meta),
      );
    const paint = () => {
      const f = current().fleet;
      const names = (f?.stacks ?? [])
        .slice()
        .sort((a, b) => a.name.localeCompare(b.name));
      list.replaceChildren(
        option(
          "host",
          `The host${f?.host.name ? ` (${f.host.name})` : ""}`,
          f?.host.name ?? "host",
          "Proxmox host",
        ),
        ...names.map((s) => option(s.name, s.name, s.name, `CT ${s.vmid}`)),
      );
      attachIdmarks(list);
    };
    paint();
    return {
      el,
      attach() {
        const handle = combobox(el);
        handle?.set(machines);
        let listed = listing();
        stops.push(
          subscribe(() => {
            // Only a change in the stacks the fleet names redraws the list:
            // a repaint under a person typing in it would be news of nothing.
            if (listing() === listed) return;
            listed = listing();
            const had = handle?.values() ?? machines;
            paint();
            setUsual(el, usual());
            handle?.refresh();
            handle?.set(had);
          }),
        );
        el.addEventListener(CHANGE_EVENT, (e) => {
          const d = /** @type {CustomEvent} */ (e).detail;
          if (d?.action === "set") return;
          /** @type {string[]} */
          const values = d?.values ?? [];
          const next = values.length ? values : ["host"];
          if (next.join(",") === machines.join(",")) return;
          go({
            stack:
              next.length === 1 && next[0] === "host" ? null : next.join(","),
          });
        });
      },
    };
  }

  // ── System ──────────────────────────────────────────────────────────
  function system() {
    const sections = stack ? STACK_SECTIONS : HOST_SECTIONS;
    /** @type {TileSpec[]} */
    const hostTiles = [
      {
        key: "cpu",
        label: "CPU",
        note: "5 min avg",
        target: "chart-cpu",
        chart: "CPU used",
        unit: "percent",
      },
      {
        key: "mem",
        label: "Memory",
        target: "chart-mem",
        chart: "Memory used",
        unit: "bytes",
      },
      {
        key: "disk",
        label: "Root disk",
        target: "chart-disk",
        chart: "Disk used per filesystem",
        unit: "percent",
      },
      {
        key: "load",
        label: "Load",
        note: "1 min",
        target: "chart-load",
        chart: "Load average",
        unit: "count",
      },
      {
        key: "temp",
        label: "Hottest chip",
        target: "chart-temp",
        chart: "Chip temperature",
        unit: "celsius",
      },
    ];
    /** @type {TileSpec[]} */
    const stackTiles = [
      {
        key: "cpu",
        label: "CPU",
        note: "5 min avg",
        target: "chart-cpu",
        chart: "CPU used",
        unit: "percent",
      },
      {
        key: "mem",
        label: "Memory",
        target: "chart-mem",
        chart: "Memory used",
        unit: "bytes",
      },
      {
        key: "disk",
        label: "Disk",
        target: "chart-disk",
        chart: "Disk used",
        unit: "percent",
      },
      {
        key: "apps",
        label: "Apps running",
        target: "chart-appcpu",
        chart: "CPU per app",
        unit: "count",
      },
      {
        key: "restarts",
        label: "Restarts",
        note: "1 h",
        target: "chart-restarts",
        chart: "Restarts",
        unit: "count",
      },
    ];
    const strip = tileStrip(
      stack ? stackTiles : hostTiles,
      stack ? `Stack ${stack} right now` : "The host right now",
    );
    for (const t of strip.tiles.values()) t.set(null);
    const attention = attentionBand([]);
    const blocks = sections.map((s) =>
      block(
        { key: s.title.toLowerCase().replace(/[^a-z]+/g, "-"), ...s },
        s.cards.map((c) => ({
          ...c,
          desc: fill(c.desc, { cores: current().fleet?.host.cores_total }),
        })),
      ),
    );
    const drivesCard = blocks
      .flatMap((b) => [...b.cards.values()])
      .find((c) => c.card.kind === "drives");
    const drivesView = drivesCard ? drivesTable(drivesCard) : null;
    const compare = machines.length > 1 ? comparison() : null;
    const ribbon = ribbonSection();
    root.append(
      strip.el,
      attention.el,
      ...(compare ? [compare.block.el] : []),
      ...blocks.map((b) => b.el),
      ribbon.el,
    );
    const all = [...(compare ? [compare.block] : []), ...blocks];
    // A section's own window keys: only that section is read again.
    for (const b of all) {
      b.group.addEventListener(CHART_RANGE_EVENT, (e) => {
        const next = /** @type {CustomEvent} */ (e).detail?.range;
        if (!next || next === b.range) return;
        b.range = next;
        b.zoom = null;
        b.zoomRead += 1;
        for (const c of b.cards.values())
          c.fig.setAttribute("data-kp-chart-loading", "");
        void readOne(b);
      });
    }
    // ask-kp11be-1: a zoom (a drag across a chart) reads exactly that window
    // from /data/charts?from=&to=, finer than the preset's points; Reset
    // reads the preset again. The side-by-side comparison keeps kp's own
    // zoom of the points it already holds.
    for (const b of all) {
      if (b === compare?.block) continue;
      b.group.addEventListener(CHART_ZOOM_EVENT, (e) => {
        const z = /** @type {CustomEvent} */ (e).detail;
        const win = z
          ? { from: Math.floor(z.from / 1000), to: Math.ceil(z.to / 1000) }
          : null;
        if (!win && !b.zoom) return;
        b.zoom = win;
        const token = (b.zoomRead += 1);
        for (const c of b.cards.values())
          c.fig.setAttribute("data-kp-chart-loading", "");
        void readOne(b, token);
      });
    }
    for (const b of all)
      for (const key of RANGES) {
        const btn = b.group.querySelector(`[data-kp-chart-range="${key}"]`);
        if (btn)
          drivable(
            /** @type {HTMLElement} */ (btn),
            SECTION_WINDOW,
            `${b.key}/${key}`,
          );
      }
    // fix-371-1: a Host tile opens its own figure here (/charts#chart-cpu);
    // the card stands in its final size from the first frame, so the page
    // scrolls to it at once.
    const asked = location.hash
      ? document.getElementById(decodeURIComponent(location.hash.slice(1)))
      : null;
    if (asked?.matches(".mk-card"))
      requestAnimationFrame(() => asked.scrollIntoView({ block: "start" }));

    /** `/data/charts` for a window, one machine; `win` (unix seconds) is a zoomed window that wins over `rng`. @param {string} rng @param {string} which @param {{from: number, to: number} | null} [win] */
    const fetchCharts = (rng, which, win = null) =>
      fetchJson(
        `/data/charts${setParams("", {
          stack: which && which !== "host" ? which : null,
          ...(win
            ? { from: String(win.from), to: String(win.to) }
            : { range: rng }),
        })}`,
        "the charts",
        abort.signal,
      );

    /** Paint one section from the answer for its window. @param {Block} b @param {Read} r */
    const paintBlock = (b, r) => {
      if (b === compare?.block) return compare.paint(r);
      if (!r.ok) {
        for (const c of b.cards.values())
          if (c.card.kind === "drives")
            drivesView?.notRead(notReadWords(r.error));
          else notRead(c.fig, c.card.title, notReadWords(r.error));
        return;
      }
      const { from, to } = r.body;
      const P = byTitle(r.body.panels);
      const fleet = current().fleet;
      const host = fleet?.host;
      const st = fleet?.stacks.find((s) => s.name === stack);
      const marks = marksIn(from, to);
      for (const [, k] of b.cards) {
        const c = k.card;
        const p = P.get(c.panel);
        if (c.kind === "drives") {
          drivesView?.paint(P);
          continue;
        }
        if (!p) {
          notRead(k.fig, c.title, "This chart is not part of the answer.");
          continue;
        }
        if (p.error) {
          notRead(k.fig, c.title, `Prometheus did not answer: ${p.error}`);
          continue;
        }
        const unit = c.unit ?? "count";
        const threshold =
          c.threshold === "committed"
            ? host?.ram_committed_mb
              ? [
                  host.ram_committed_mb * 1048576,
                  // The second, finer line: all the RAM the host has.
                  ...(host.ram_total_mb ? [host.ram_total_mb * 1048576] : []),
                ]
              : undefined
            : c.threshold === "allowance"
              ? st?.ram_max_mb
                ? st.ram_max_mb * 1048576
                : undefined
              : c.threshold;
        const has = p.series.some((x) => x.points.length);
        feedChart(k.fig, {
          key: c.key,
          label: c.title,
          unit,
          series: has
            ? p.series.map((x) => ({
                label: x.label || c.title,
                points: x.points,
              }))
            : [{ label: c.title, points: [] }],
          from,
          to,
          annotations: marks,
          ...(threshold != null ? { threshold } : {}),
          ...(c.yMax != null ? { yMax: c.yMax } : {}),
          ...(c.typical ? { typical: true } : {}),
        });
      }
    };

    /**
     * One section on its own (its window key was pressed, or it was zoomed
     * or reset). `token` is the zoom read that asked: a newer one drops it.
     * @param {Block} b @param {number} [token]
     */
    const readOne = async (b, token = b.zoomRead) => {
      const asked = b.range;
      const win = b.zoom;
      const r = await (b === compare?.block
        ? compare.fetch(asked)
        : fetchCharts(asked, stack, win));
      await readHistory(
        RANGE_SECONDS[/** @type {keyof typeof RANGE_SECONDS} */ (asked)],
      );
      if (b.range === asked && b.zoomRead === token) paintBlock(b, r);
      ribbon.paint();
    };

    /** The whole page, once. @returns {Promise<boolean>} */
    const readAll = async () => {
      const asked = all.map((b) => b.range);
      const zoomed = all.map((b) => b.zoom);
      const span = Math.max(
        ...all.map(
          (b) =>
            RANGE_SECONDS[/** @type {keyof typeof RANGE_SECONDS} */ (b.range)],
        ),
        RANGE_SECONDS[range],
      );
      /** @type {Map<string, Promise<Read>>} */
      const memo = new Map();
      /** @param {string} rng */
      const get = (rng) => {
        let p = memo.get(rng);
        if (!p) memo.set(rng, (p = fetchCharts(rng, stack)));
        return p;
      };
      const [day, , ...results] = await Promise.all([
        get(TREND_RANGE),
        readHistory(span),
        ...all.map((b) =>
          b === compare?.block
            ? compare.fetch(b.range)
            : b.zoom
              ? fetchCharts(b.range, stack, b.zoom)
              : get(b.range),
        ),
      ]);
      if (!day.ok) {
        failed.replaceChildren(errorBox(day.error));
        tilesNotRead(strip.tiles);
        attention.set([]);
      } else {
        failed.replaceChildren();
        if (stack) paintStackTiles(day.body);
        else paintHostTiles(day.body);
      }
      all.forEach((b, i) => {
        if (b.range === asked[i] && b.zoom === zoomed[i])
          paintBlock(b, results[i]);
      });
      ribbon.paint();
      return results.every((r) => r.ok) && day.ok;
    };

    /** @param {any} body */
    function paintHostTiles(body) {
      const P = byTitle(body.panels);
      const host = current().fleet?.host;
      const T = strip.tiles;
      const step = body.step;
      const cores = host?.cores_total ?? null;
      const cpu = P.get("CPU used")?.series[0]?.points ?? [];
      const pk = peak(cpu);
      const cpuNow = host?.cpu_pct ?? Math.round(cpu.at(-1)?.[1] ?? 0);
      const pct = (/** @type {number} */ v) => `${Math.round(v)}%`;
      T.get("cpu")?.set({
        value: String(cpuNow),
        unit: "%",
        trend: nowPeak(`${cpuNow}%`, pk ? pct(pk.value) : null),
        hover: hoverOf(cpu, pct, cores ? `The host has ${cores} cores.` : ""),
        points: cpu,
        step,
      });
      const gib = (/** @type {number} */ v) =>
        `${(v / 1073741824).toFixed(1)} GiB`;
      const memPts = P.get("Memory used")?.series[0]?.points ?? [];
      if (host) {
        const used = host.ram_used_mb;
        const total = host.ram_total_mb;
        const promised = host.ram_committed_mb ?? 0;
        const mpk = peak(memPts);
        T.get("mem")?.set({
          value: (used / 1024).toFixed(1),
          unit: `of ${(total / 1024).toFixed(0)} GiB`,
          trend: nowPeak(
            `${(used / 1024).toFixed(1)} GiB`,
            mpk ? gib(mpk.value) : null,
          ),
          hover: hoverOf(
            memPts,
            gib,
            `${Math.round((promised / total) * 100)}% of it is promised to stacks.`,
          ),
          points: memPts,
          step,
          tone:
            used / total >= 0.9 ? "bad" : used / total >= 0.75 ? "warn" : "",
        });
        const rootPts =
          P.get("Disk used per filesystem")?.series.find((s) => s.label === "/")
            ?.points ?? [];
        const moved =
          rootPts.length > 1
            ? rootPts[rootPts.length - 1][1] - rootPts[0][1]
            : null;
        const d = host.disk_detail;
        const dpk = peak(rootPts);
        T.get("disk")?.set({
          value: String(host.disk_pct),
          unit: "%",
          trend: nowPeak(`${host.disk_pct}%`, dpk ? pct(dpk.value) : null),
          hover: hoverOf(
            rootPts,
            pct,
            `${d ? `${gbText(d.root_lv_size_gb)} on ${d.root_disk_device.replace(/^\/dev\//, "")}` : "The host's root"}${
              moved == null
                ? ""
                : Math.abs(moved) < 0.5
                  ? ", flat in this window."
                  : `, ${moved > 0 ? "+" : "−"}${Math.abs(moved).toFixed(1)} pts in this window.`
            }`,
          ),
          points: rootPts,
          step,
          tone: host.disk_pct >= 90 ? "bad" : host.disk_pct >= 75 ? "warn" : "",
        });
        const load = host.load1_x100 == null ? null : host.load1_x100 / 100;
        const loadPts = P.get("Load average (5 min)")?.series[0]?.points ?? [];
        const lpk = peak(loadPts);
        const num = (/** @type {number} */ v) => v.toFixed(2);
        T.get("load")?.set({
          value: load == null ? "—" : num(load),
          trend: nowPeak(
            load == null ? "—" : num(load),
            lpk ? num(lpk.value) : null,
          ),
          hover: hoverOf(
            loadPts,
            num,
            load == null || cores == null
              ? "Not reported."
              : load < cores / 2
                ? `Well under ${cores} cores.`
                : load < cores
                  ? `Near ${cores} cores.`
                  : `Above ${cores} cores: work queues up.`,
          ),
          points: loadPts,
          step,
          tone: load != null && cores != null && load >= cores ? "warn" : "",
        });
      }
      const temps =
        P.get("Temperature (hottest sensor per chip)")?.series ?? [];
      const hot = temps
        .map((s) => ({ s, v: s.points.at(-1)?.[1] ?? -Infinity }))
        .sort((a, b) => b.v - a.v)[0];
      const deg = (/** @type {number} */ v) => `${v.toFixed(0)} °C`;
      if (hot) {
        const tpk = peak(hot.s.points);
        T.get("temp")?.set({
          value: hot.v.toFixed(0),
          unit: "°C",
          trend: nowPeak(deg(hot.v), tpk ? deg(tpk.value) : null),
          hover: hoverOf(
            hot.s.points,
            deg,
            `${hot.s.label} is the hottest chip.`,
          ),
          points: hot.s.points,
          step,
          tone: hot.v >= 85 ? "bad" : hot.v >= 75 ? "warn" : "",
        });
      } else
        T.get("temp")?.set({
          value: "—",
          trend: ["no sensor reported"],
          hover: "No temperature sensor reported.",
        });
      // The attention band for a failing drive (the Drives table keeps the rest).
      const list = drives(P);
      const bad = list.filter((d) => !d.ok);
      attention.set(bad.map(attentionFor));
    }

    /** @param {import("../metricsview.js").Drive} d */
    function attentionFor(d) {
      const see = drivable(
        /** @type {HTMLButtonElement} */ (
          h(
            "button",
            {
              type: "button",
              class: "kp-button kp-button--sm",
              title: "Scroll to the Drives table",
            },
            "See the drive",
          )
        ),
        SEE_DRIVE,
        d.name,
      );
      see.addEventListener("click", () => {
        drivesCard?.el.scrollIntoView({ block: "start", behavior: "smooth" });
        drivesView?.open([d.name]);
      });
      const back = h(
        "a",
        {
          class: "kp-button kp-button--sm kp-button--primary",
          href: "/backups",
          title:
            "Open Backups to back up the stacks now, before the drive gets worse",
        },
        "Back up devices now",
      );
      drivable(back, BACKUPS, d.name);
      return {
        key: `drive-${d.name}`,
        tone: /** @type {const} */ ("bad"),
        title: `Drive ${d.name} reports SMART not ok`,
        text: `${d.pending ?? 0} ${d.pending === 1 ? "sector waits" : "sectors wait"} to be re-read and ${d.realloc ?? 0} ${d.realloc === 1 ? "was" : "were"} already swapped for spares. Data on it is at risk.`,
        action: h("span", { class: "mk-acts" }, see, back),
      };
    }

    /** @param {any} body */
    function paintStackTiles(body) {
      const P = byTitle(body.panels);
      const st = current().fleet?.stacks.find((s) => s.name === stack);
      const T = strip.tiles;
      const step = body.step;
      const pct = (/** @type {number} */ v) => `${Math.round(v)}%`;
      const cpu = P.get("CPU (whole container)")?.series[0]?.points ?? [];
      const pk = peak(cpu);
      T.get("cpu")?.set({
        value: cpu.length ? (cpu.at(-1)?.[1] ?? 0).toFixed(0) : "—",
        unit: "%",
        trend: nowPeak(pct(cpu.at(-1)?.[1] ?? 0), pk ? pct(pk.value) : null),
        hover: hoverOf(cpu, pct, "Share of this container's own cores."),
        points: cpu,
        step,
      });
      const memPts =
        P.get("Memory used (whole container)")?.series[0]?.points ?? [];
      const usedMb = st?.ram_used_mb ?? (memPts.at(-1)?.[1] ?? 0) / 1048576;
      const maxMb = st?.ram_max_mb ?? null;
      const size = (/** @type {number} */ mb) =>
        mb >= 1024 ? `${(mb / 1024).toFixed(1)} GiB` : `${Math.round(mb)} MiB`;
      const mpk = peak(memPts);
      T.get("mem")?.set({
        value:
          usedMb >= 1024
            ? (usedMb / 1024).toFixed(1)
            : String(Math.round(usedMb)),
        unit: `${usedMb >= 1024 ? "GiB" : "MiB"}${maxMb ? ` of ${size(maxMb)}` : ""}`,
        trend: nowPeak(size(usedMb), mpk ? size(mpk.value / 1048576) : null),
        hover: hoverOf(
          memPts,
          (v) => size(v / 1048576),
          maxMb
            ? `${Math.round((usedMb / maxMb) * 100)}% of its allowance.`
            : "No allowance set.",
        ),
        points: memPts,
        step,
        tone: maxMb
          ? usedMb / maxMb >= 0.9
            ? "bad"
            : usedMb / maxMb >= 0.75
              ? "warn"
              : ""
          : "",
      });
      const disk =
        P.get("Disk used (root filesystem)")?.series[0]?.points ?? [];
      const dv = disk.at(-1)?.[1];
      const dpk = peak(disk);
      T.get("disk")?.set({
        value: dv == null ? "—" : dv.toFixed(0),
        unit: "%",
        trend: nowPeak(dv == null ? "—" : pct(dv), dpk ? pct(dpk.value) : null),
        hover: hoverOf(disk, pct, "Of its own root disk."),
        points: disk,
        step,
        tone: dv == null ? "" : dv >= 90 ? "bad" : dv >= 75 ? "warn" : "",
      });
      const running = st?.apps_running ?? 0;
      const total = st?.apps_total ?? 0;
      T.get("apps")?.set({
        value: st ? String(running) : "—",
        unit: st ? `of ${total}` : "",
        trend: st
          ? [
              running === total
                ? "every app is up"
                : `${total - running} not running`,
            ]
          : ["not in the fleet"],
        hover: st
          ? `${running} of ${total} apps in this stack run now.`
          : "This stack is not in the fleet.",
        tone: st && running < total ? "warn" : "",
      });
      const restarts = P.get("Restarts (last hour)")?.series ?? [];
      const n = restarts.reduce((a, s) => a + (s.points.at(-1)?.[1] ?? 0), 0);
      /** @type {Point[]} */
      const sum = (restarts[0]?.points ?? []).map((p, i) => [
        p[0],
        restarts.reduce((a, s) => a + (s.points[i]?.[1] ?? 0), 0),
      ]);
      T.get("restarts")?.set({
        value: String(Math.round(n)),
        trend: [n ? "an app is restarting" : "a healthy app reads 0"],
        hover: hoverOf(sum, (v) => String(Math.round(v))),
        points: sum,
        step,
        tone: n ? "warn" : "",
      });
    }

    inits.push(() => {
      detachers.push(attachDataTables(root));
      detachers.push(attachBarCells(root));
      ribbon.init();
      // The Drives table waits in kp's own loading state until its read lands.
      drivesView?.init();
    });
    return readAll;

    /**
     * The machines side by side: one CPU line per machine, in the colour of
     * its mark.
     */
    function comparison() {
      const names = machines.map(machineName);
      const slots = idmarkColours(names);
      const b = block(
        {
          key: "compared",
          title: "Compared",
          desc: `CPU of ${names.join(", ")} side by side, each line in the colour of its mark.`,
        },
        [
          {
            key: "compare-cpu",
            panel: "",
            title: "CPU used",
            desc: "Share of each machine's own cores in use, 5-minute average.",
            span: 3,
            unit: "percent",
            typical: false,
            sources: machines.length,
          },
        ],
      );
      /** @param {string} rng */
      const fetchAll = (rng) =>
        Promise.all(machines.map((m) => fetchCharts(rng, m)));
      return {
        block: b,
        /** @param {string} rng @returns {Promise<Read>} */
        async fetch(rng) {
          const all = await fetchAll(rng);
          const bad = all.find((r) => !r.ok);
          if (bad) return bad;
          return {
            ok: true,
            body: {
              .../** @type {any} */ (all[0]).body,
              machines: all.map((r) => /** @type {any} */ (r).body),
            },
          };
        },
        /** @param {Read} r */
        paint(r) {
          const fig = /** @type {HTMLElement} */ (
            b.cards.get("compare-cpu")?.fig
          );
          if (!r.ok) return notRead(fig, "CPU used", notReadWords(r.error));
          const { from, to } = r.body;
          const marks = annotations(entries, { from, to });
          const series = /** @type {any[]} */ (r.body.machines).map(
            (body, i) => {
              const P = byTitle(body.panels);
              const points =
                (machines[i] === "host"
                  ? P.get("CPU used")
                  : P.get("CPU (whole container)")
                )?.series[0]?.points ?? [];
              return {
                label: names[i],
                points,
                colour: `var(--chart-${slots.get(names[i]) ?? 1})`,
              };
            },
          );
          feedChart(fig, {
            key: "compare-cpu",
            label: "CPU used",
            unit: "percent",
            series,
            from,
            to,
            annotations: marks,
          });
        },
      };
    }

    /** The operations ribbon under the sections. */
    function ribbonSection() {
      const el = h("section", {
        class: "kp-gantt mk-ribbon",
        "data-kp-gantt": "",
        "data-kp-gantt-ranges": RANGES.join(" "),
        "data-kp-gantt-range": range,
        "data-kp-gantt-ghosts": "1 1 1",
        id: "mk-ribbon",
        "aria-label": "What the host did",
      });
      const counted = h(
        "p",
        { class: "mk-ribbon__count", role: "status" },
        "Reading what the host did…",
      );
      const wrap = h(
        "section",
        { class: "mk-section", "aria-labelledby": "mk-h-ribbon" },
        h(
          "div",
          { class: "mk-section__head" },
          h("h2", { id: "mk-h-ribbon" }, "What the host did"),
          h(
            "p",
            null,
            "Lists the operations, updates and failures the host ran in this window, in lanes under the charts that mark them; press a bar to open it in Activity.",
          ),
        ),
        counted,
        el,
      );
      el.addEventListener(GANTT_OPEN_EVENT, () => ctx.navigate("/activity"));
      el.addEventListener(GANTT_WINDOW_EVENT, (e) => {
        const secs = /** @type {CustomEvent} */ (e).detail?.span;
        const key = Object.entries(RANGE_SECONDS).find(
          ([, v]) => v === secs,
        )?.[0];
        if (key && key !== range) go({ range: key === "24h" ? null : key });
      });
      return {
        el: wrap,
        init() {
          setGanttState(el, "loading");
        },
        paint() {
          if (!historyOk) {
            counted.textContent = "";
            return setGanttState(
              el,
              "error",
              "The history could not be read: the dashboard did not answer.",
            );
          }
          const to = Math.floor(Date.now() / 1000);
          const from = to - RANGE_SECONDS[range];
          const runs = ribbonRuns(entries, {
            from,
            to,
            ...(stack ? { stack } : {}),
          });
          setGanttWindow(el, {
            span: RANGE_SECONDS[range],
            live: false,
            to: to * 1000,
          });
          setGanttRuns(el, runs, RIBBON_LANES);
          counted.textContent = `${count(runs.length)} ${runs.length === 1 ? "operation" : "operations"} in this window.`;
        },
      };
    }

    /**
     * The Drives card: kp's data table, one row per drive; a row opens to
     * the drive's two lines. Rows are written once and updated in place, so
     * an open row and the sort survive each read.
     * @param {Block["cards"] extends Map<string, infer V> ? V : never} k
     */
    function drivesTable(k) {
      const tbody = h("tbody");
      const empty = h(
        "div",
        { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
        "No drive was found on the host: Prometheus reports no SMART readings.",
      );
      const wrap = h(
        "div",
        {
          class: "kp-datatable mk-table",
          "data-kp-datatable": "",
          "data-kp-busy-overlay": "",
          "data-kp-expandable": "",
          // On a phone a drive is a card, never a table that scrolls sideways.
          "data-kp-cards": "",
          "data-kp-page-size": "500",
          "data-kp-page-sizes": "none",
          "data-kp-sort-multi": "",
          "data-kp-remember": "metrics-drives",
        },
        h(
          "div",
          { class: "kp-table-wrap" },
          h(
            "table",
            { class: "kp-table" },
            h(
              "caption",
              { class: "kp-sr-only" },
              "Drives of the host; open a drive for its temperature and pending sectors",
            ),
            h(
              "thead",
              null,
              h(
                "tr",
                null,
                h("th", { scope: "col", "data-kp-sort": "text" }, "Drive"),
                h("th", { scope: "col", "data-kp-sort": "text" }, "SMART"),
                h(
                  "th",
                  { scope: "col", "data-kp-sort": "number" },
                  "Temperature",
                ),
                h(
                  "th",
                  {
                    scope: "col",
                    "data-kp-sort": "number",
                    class: "kp-text-end",
                  },
                  "Pending sectors",
                ),
                h(
                  "th",
                  {
                    scope: "col",
                    "data-kp-sort": "number",
                    class: "kp-text-end",
                  },
                  "Reallocated sectors",
                ),
                h(
                  "th",
                  {
                    scope: "col",
                    "data-kp-sort": "number",
                    class: "kp-text-end",
                  },
                  "Powered on",
                ),
              ),
            ),
            tbody,
          ),
        ),
        empty,
      );
      k.body.replaceChildren(wrap);
      labelCells(tbody, () => [
        "",
        "Drive",
        "SMART",
        "Temperature",
        "Pending sectors",
        "Reallocated sectors",
        "Powered on",
      ]);
      k.setFoot([
        "A rising pending or reallocated count is the early warning; temperature above 50 °C shortens a drive's life.",
        "smartctl via node-exporter",
      ]);
      let first = true;
      /** @type {string} */
      let names = "";
      const handle = () => tableOf(wrap);
      /**
       * The 90-day SMART history (`/data/drive-history`): null while it is
       * read, a string when it could not be, else the server's drives.
       * @type {Record<string, {periods: any[]}> | string | null}
       */
      let smart90 = null;
      /** Draw every drive's 90-day line from what was read. */
      const fillSmart90 = () => {
        for (const strip of /** @type {NodeListOf<HTMLElement>} */ (
          tbody.querySelectorAll("[data-mk-smart90]")
        )) {
          const name = strip.dataset.mkSmart90 ?? "";
          const entry =
            typeof smart90 === "object" ? smart90?.[name] : undefined;
          const periods = periodsOf(entry, "day", "SMART ok in");
          setUptime(strip, {
            label: `${name} SMART, last 90 days`,
            periods,
            from: "90 days ago",
            to: "Today",
            count: 90,
            open: "info",
            state:
              smart90 === null
                ? "loading"
                : typeof smart90 === "string"
                  ? "error"
                  : periods.some((p) => p.state !== "none")
                    ? "ready"
                    : "empty",
            ...(typeof smart90 === "string" ? { text: smart90 } : {}),
            message: "No SMART reading kept for this drive yet.",
          });
        }
      };
      const loadSmart90 = async () => {
        smart90 = null;
        fillSmart90();
        const r = await fetchJson(
          `/data/drive-history?${uptimeQuery("day", Math.floor(Date.now() / 1000))}`,
          "the drives' SMART history",
          abort.signal,
        );
        if (abort.signal.aborted) return;
        smart90 = r.ok
          ? (r.body.drives ?? {})
          : `${r.error.what}: ${r.error.why}`;
        fillSmart90();
      };
      return {
        init() {
          handle()?.state("loading");
          void loadSmart90().catch(() => {});
        },
        /** @param {string} why */
        notRead(why) {
          handle()?.fail(why);
        },
        /** @param {string[]} keys */
        open(keys) {
          handle()?.expand(keys);
        },
        /** @param {Map<string, Panel>} P */
        paint(P) {
          const list = drives(P);
          const bad = list.filter((d) => !d.ok);
          const key = list.map((d) => d.name).join(",");
          const rebuild = key !== names;
          if (rebuild) {
            names = key;
            tbody.replaceChildren(
              ...list.flatMap((d) => [driveRow(d), driveDetail(d)]),
            );
            detachers.push(attachBarCells(tbody));
            detachers.push(attachUptimes(tbody));
            queueMicrotask(() => attachPageCharts(tbody));
          }
          for (const d of list) fillDrive(d);
          if (rebuild) fillSmart90();
          const t = handle();
          t?.state("ready");
          t?.refresh();
          if (first && bad.length) t?.expand(bad.map((d) => d.name));
          first = false;
        },
      };

      /** @param {import("../metricsview.js").Drive} d */
      function driveRow(d) {
        const cells = [
          h("td", { "data-label": "Drive" }, h("b", { class: "mono" }, d.name)),
          h("td", { "data-label": "SMART", "data-mk-cell": "smart" }),
          h(
            "td",
            { "data-label": "Temperature", "data-mk-cell": "temp" },
            h("span", { class: "kp-bar-cell" }),
          ),
          h("td", {
            class: "kp-text-end",
            "data-label": "Pending sectors",
            "data-mk-cell": "pending",
          }),
          h("td", {
            class: "kp-text-end",
            "data-label": "Reallocated sectors",
            "data-mk-cell": "realloc",
          }),
          h("td", {
            class: "kp-text-end mk-nowrap",
            "data-label": "Powered on",
            "data-mk-cell": "hours",
          }),
        ];
        return h("tr", { "data-kp-row-key": d.name }, ...cells);
      }
      /** @param {import("../metricsview.js").Drive} d */
      function driveDetail(d) {
        const spark = (
          /** @type {string} */ what,
          /** @type {string} */ label,
        ) =>
          h("figure", {
            class: "kp-chart",
            "data-kp-chart": "spark",
            "data-mk-spark": `${what}:${d.name}`,
            "aria-label": `${label} of ${d.name}`,
          });
        return h(
          "tr",
          { "data-kp-row-detail": "" },
          h(
            "td",
            { colspan: 6 },
            h(
              "div",
              { class: "mk-drive-detail", "data-mk-detail": d.name },
              h("p", { class: "mk-drive-detail__state" }),
              h("div", {
                class: "kp-uptime",
                "data-kp-uptime": "",
                "data-mk-health": d.name,
              }),
              // ask-kp11be-2: the same drive over 90 days, one cell a day.
              h("div", {
                class: "kp-uptime",
                "data-kp-uptime": "",
                "data-kp-uptime-count": "90",
                "data-kp-uptime-fit": "scroll",
                "data-mk-smart90": d.name,
              }),
              h(
                "div",
                { class: "mk-pair" },
                spark("temp", "Temperature"),
                spark("pending", "Pending sectors"),
              ),
            ),
          ),
        );
      }
      /** Write a drive's cells, its sentence and its two lines. @param {import("../metricsview.js").Drive} d */
      function fillDrive(d) {
        const row = /** @type {HTMLElement | null} */ (
          tbody.querySelector(`tr[data-kp-row-key="${CSS.escape(d.name)}"]`)
        );
        if (!row) return;
        const cell = (/** @type {string} */ k) =>
          /** @type {HTMLElement} */ (
            row.querySelector(`[data-mk-cell="${k}"]`)
          );
        const smart = cell("smart");
        smart.setAttribute("data-kp-sort-value", d.ok ? "ok" : "not ok");
        smart.replaceChildren(
          h(
            "span",
            {
              class: `kp-badge mk-nowrap ${d.ok ? "kp-badge--success" : "kp-badge--destructive"}`,
            },
            d.ok ? "ok" : "not ok",
          ),
        );
        const temp = cell("temp");
        const bar = /** @type {HTMLElement} */ (
          temp.querySelector(".kp-bar-cell")
        );
        setBarCell(
          bar,
          d.temp == null
            ? { value: null, text: "—", sortValue: "" }
            : {
                value: d.temp / 80,
                text: `${d.temp.toFixed(0)} °C`,
                warnAt: 0.75,
                badAt: 0.9,
                sortValue: d.temp,
                label: "of 80 °C",
              },
        );
        const num = (
          /** @type {string} */ k,
          /** @type {number | null} */ v,
          /** @type {string} */ text,
          /** @type {boolean} */ warn,
        ) => {
          const c = cell(k);
          c.setAttribute("data-kp-sort-value", v == null ? "" : String(v));
          c.textContent = text;
          c.classList.toggle("mk-bad", warn);
        };
        num(
          "pending",
          d.pending,
          d.pending == null ? "—" : count(d.pending),
          !!d.pending,
        );
        num(
          "realloc",
          d.realloc,
          d.realloc == null ? "—" : count(d.realloc),
          !!d.realloc,
        );
        num("hours", d.hours, poweredOn(d.hours), false);
        const detail = tbody.querySelector(
          `[data-mk-detail="${CSS.escape(d.name)}"]`,
        );
        const sentence = detail?.querySelector(".mk-drive-detail__state");
        if (sentence)
          sentence.textContent = d.ok
            ? `${d.name} reports SMART ok.`
            : `${d.name} reports SMART not ok${d.since ? `, since ${formatDateTime(d.since)}` : ", for this whole window"}.`;
        const strip = /** @type {HTMLElement | null} */ (
          detail?.querySelector("[data-mk-health]") ?? null
        );
        if (strip) {
          const periods = healthPeriods(d);
          setUptime(strip, {
            label: `${d.name} health`,
            periods,
            from: periods[0]?.label,
            to: periods.length > 1 ? periods.at(-1)?.label : undefined,
            open: "info",
          });
        }
        for (const fig of tbody.querySelectorAll(
          `[data-mk-spark$=":${CSS.escape(d.name)}"]`,
        )) {
          const what = /** @type {HTMLElement} */ (fig).dataset.mkSpark?.split(
            ":",
          )[0];
          const pts = what === "temp" ? d.tempPoints : d.pendPoints;
          feedChart(fig, {
            label: what === "temp" ? "Temperature" : "Pending sectors",
            unit: what === "temp" ? "celsius" : "count",
            series: [
              {
                label: what === "temp" ? "Temperature" : "Pending sectors",
                points: pts,
              },
            ],
            from: pts[0]?.[0] ?? 0,
            to: pts.at(-1)?.[0] ?? 0,
          });
        }
      }
    }
  }

  // ── Traffic ─────────────────────────────────────────────────────────
  function traffic() {
    const strip = tileStrip(
      [
        {
          key: "requests",
          label: "Requests",
          target: "chart-requests",
          chart: "Requests over time",
          unit: "count",
        },
        {
          key: "5xx",
          label: "Server errors (5xx)",
          target: "chart-requests",
          chart: "Requests over time",
          unit: "count",
        },
        {
          key: "4xx",
          label: "Client errors (4xx)",
          target: "chart-mix",
          chart: "Status mix",
          unit: "count",
        },
        {
          key: "hosts",
          label: "Hostnames",
          target: "chart-hostnames",
          chart: "Hostnames",
          unit: "count",
        },
        {
          key: "clients",
          label: "Client addresses",
          target: "chart-clients",
          chart: "Busiest clients",
          unit: "count",
        },
      ],
      "Traffic in this window",
    );
    for (const t of strip.tiles.values()) t.set(null);

    const over = card({
      key: "requests",
      panel: "",
      title: "Requests over time",
      desc: "Requests per interval, stacked by status class; red is the server failing.",
      span: 2,
    });
    const mix = section({
      title: "Status mix",
      desc: "Share of all requests per status class in this window; press a part to show only that class in the chart.",
      id: "chart-mix",
      level: "h3",
    });
    mix.el.classList.add("mk-card", "mk-span-1");
    mix.el.dataset.key = "mix";
    const mixBar = h("figure", {
      class: "kp-stackbar mk-mix",
      "aria-label": "Status mix",
    });
    const mixNote = h("p", { class: "mk-muted mk-sm" }, "");
    mix.body.replaceChildren(mixBar, mixNote);
    setStackbar(mixBar, { state: "loading", label: "Status mix" });
    const perHost = card({
      key: "perhost",
      panel: "",
      title: "Requests per hostname",
      desc: "Requests per interval for each hostname; press a name in Hostnames below to switch one on or off.",
      span: 3,
      sources: 8,
    });
    // Hostnames: a heat grid (hostname by hour) or a sortable table.
    const hostsView = segSwitch({
      label: "Hostnames as",
      items: [
        { value: "heat", label: "Heat grid", hint: "Hostname by hour" },
        { value: "table", label: "Table", hint: "Exact counts, sortable" },
      ],
      value: "heat",
      onChange: (v) => setHostsView(v),
      drive: { id: HOSTNAMES_AS },
    });
    const hostsCard = section({
      title: "Hostnames",
      desc: "Requests per hostname: a heat grid by hour, or a table with exact counts and each hostname's share. Press a name to switch it on or off in the chart; press an hour to zoom the chart to it.",
      id: "chart-hostnames",
      level: "h3",
      tools: [hostsView.el],
    });
    hostsCard.el.classList.add("mk-card", "mk-span-2");
    hostsCard.el.dataset.key = "hostnames";
    const heatEl = h("div", {
      class: "kp-heat mk-heat",
      "data-kp-heat": "",
      "data-kp-heat-toggle": "",
      id: "traffic-heat",
      "aria-label": "Requests per hostname per hour",
    });
    const heatBox = h("div", { class: "mk-hosts" }, heatEl);
    // kp's heat legend: the value ramp under the grid.
    setHeatLegend(heatEl, [
      {
        ramp: true,
        less: "Fewer",
        more: "more requests for that hostname",
      },
    ]);
    const hostsBody = h("tbody");
    const hostsTable = h(
      "div",
      {
        class: "kp-datatable mk-table",
        "data-kp-datatable": "",
        "data-kp-busy-overlay": "",
        "data-kp-page-size": "500",
        "data-kp-page-sizes": "none",
        "data-kp-sort-multi": "",
        "data-kp-remember": "metrics-hostnames",
        hidden: "",
      },
      h(
        "div",
        { class: "kp-table-wrap" },
        h(
          "table",
          { class: "kp-table" },
          h(
            "caption",
            { class: "kp-sr-only" },
            "Requests per hostname in this window",
          ),
          h(
            "thead",
            null,
            h(
              "tr",
              null,
              h("th", { scope: "col", "data-kp-sort": "text" }, "Hostname"),
              h(
                "th",
                { scope: "col", "data-kp-sort": "number", class: "mk-n" },
                "Requests",
              ),
              h(
                "th",
                { scope: "col", "data-kp-sort": "number" },
                "Share of all",
              ),
            ),
          ),
          hostsBody,
        ),
      ),
      h(
        "div",
        { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
        "No requests reached the gateway in this window.",
      ),
    );
    hostsCard.body.replaceChildren(heatBox, hostsTable);
    /** @param {string} v */
    function setHostsView(v) {
      heatBox.hidden = v !== "heat";
      hostsTable.hidden = v !== "table";
    }
    const clientsCard = section({
      title: "Busiest clients",
      desc: "The addresses that made the most requests.",
      id: "chart-clients",
      level: "h3",
    });
    clientsCard.el.classList.add("mk-card", "mk-span-1");
    clientsCard.el.dataset.key = "clients";
    const clientsBody = h("tbody");
    const clientsTable = h(
      "div",
      {
        class: "kp-datatable mk-table",
        "data-kp-datatable": "",
        "data-kp-busy-overlay": "",
        "data-kp-page-size": "500",
        "data-kp-page-sizes": "none",
        "data-kp-sort-multi": "",
        "data-kp-remember": "metrics-clients",
      },
      h(
        "div",
        { class: "kp-table-wrap" },
        h(
          "table",
          { class: "kp-table" },
          h("caption", { class: "kp-sr-only" }, "Busiest clients"),
          h(
            "thead",
            null,
            h(
              "tr",
              null,
              h("th", { scope: "col", "data-kp-sort": "text" }, "Client"),
              h("th", { scope: "col", "data-kp-sort": "number" }, "Share"),
            ),
          ),
          clientsBody,
        ),
      ),
      h(
        "div",
        { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
        "No client addresses in this window.",
      ),
    );
    clientsCard.body.replaceChildren(clientsTable);

    // ask-kp11be-9: the newest requests to one hostname (method, path,
    // status, duration; the server leaves the client address out).
    const REQ_LIMIT = 50;
    const reqSel = h("select", {
      class: "kp-field__input",
      id: REQUESTS_HOST,
      "data-kp-select": "",
      disabled: "",
    });
    const reqNote = h("span", { role: "status" }, "Reading the hostnames…");
    const reqBody = h("tbody");
    const reqTable = h(
      "div",
      {
        class: "kp-datatable mk-table",
        "data-kp-datatable": "",
        "data-kp-busy-overlay": "",
        "data-kp-page-size": String(REQ_LIMIT),
        "data-kp-page-sizes": "none",
        "data-kp-sort-multi": "",
        "data-kp-remember": "metrics-requests",
      },
      h(
        "div",
        { class: "kp-table-wrap" },
        h(
          "table",
          { class: "kp-table" },
          h(
            "caption",
            { class: "kp-sr-only" },
            "The newest requests to one hostname",
          ),
          h(
            "thead",
            null,
            h(
              "tr",
              null,
              h("th", { scope: "col", "data-kp-sort": "text" }, "Method"),
              h("th", { scope: "col", "data-kp-sort": "text" }, "Path"),
              h("th", { scope: "col", "data-kp-sort": "number" }, "Status"),
              h(
                "th",
                { scope: "col", "data-kp-sort": "number", class: "mk-n" },
                "Duration",
              ),
            ),
          ),
          reqBody,
        ),
      ),
      h(
        "div",
        { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
        "No request to this hostname in this window.",
      ),
    );
    const reqCard = section({
      title: "Recent requests",
      desc: "The newest requests to one hostname behind the gateway, from its access log: method, path without its query, status and how long the gateway took. Client addresses are never shown here.",
      id: "chart-request-list",
      level: "h3",
    });
    reqCard.el.classList.add("mk-card", "mk-span-3");
    reqCard.el.dataset.key = "request-list";
    reqCard.body.replaceChildren(
      formGrid(
        [
          gridRow({
            label: "Hostname",
            labelFor: REQUESTS_HOST,
            control: reqSel,
            value: reqNote,
          }),
        ],
        "mk-requests__pick",
      ),
      reqTable,
    );

    const group = h(
      "div",
      {
        class: "kp-chart-group mk-group",
        "data-kp-chart-group": "",
        "data-kp-chart-span": range,
        "data-kp-key": "traffic",
      },
      h(
        "div",
        { class: "kp-chart-group__bar mk-groupbar" },
        h("span", {
          class: "kp-chart-zoom",
          "data-kp-chart-zoom": "",
          hidden: "",
        }),
      ),
      h(
        "div",
        { class: "mk-panels" },
        over.el,
        mix.el,
        perHost.el,
        hostsCard.el,
        clientsCard.el,
        reqCard.el,
      ),
    );
    root.append(
      strip.el,
      h("section", { class: "mk-section", "aria-label": "Traffic" }, group),
    );
    inits.push(() => {
      detachers.push(attachDataTables(root));
      detachers.push(attachBarCells(root));
      tableOf(hostsTable)?.state("loading");
      tableOf(clientsTable)?.state("loading");
      tableOf(reqTable)?.state("loading");
      setHeat(heatEl, {
        state: "loading",
        skeleton: { rows: 8, columns: 24 },
        label: "Requests per hostname per hour",
      });
    });

    // The pieces that follow one another: the share bar presses a class in
    // the stacked chart, the heat grid and the chart's legend press each
    // other, an hour zooms the chart.
    /** @type {string[]} */
    let hostNames = [];
    let bucketMs = 3600_000;
    // A part shows that class alone in the chart; the same part again shows
    // every class (the demo's behaviour for the Status class keys).
    let pickedClass = -1;
    /**
     * The bar's last full spec, drawn again with the pressed class.
     * @type {Parameters<typeof setStackbar>[1] | null}
     */
    let mixSpec = null;
    const paintMix = () => {
      if (!mixSpec) return;
      setStackbar(mixBar, {
        ...mixSpec,
        picked: pickedClass >= 0 ? [CLASSES[pickedClass].label] : [],
      });
    };
    mixBar.addEventListener(STACKBAR_PICK_EVENT, (e) => {
      const key = /** @type {CustomEvent} */ (e).detail?.key;
      const i = CLASSES.findIndex((c) => c.label === key);
      if (i < 0) return;
      chartSelect(over.fig, null);
      pickedClass = pickedClass === i ? -1 : i;
      if (pickedClass >= 0) chartSelect(over.fig, pickedClass, true);
      paintMix();
    });
    mixBar.addEventListener(STACKBAR_RETRY_EVENT, () =>
      refreshOf(refreshEl)?.now("retry"),
    );
    heatEl.addEventListener(HEAT_PICK_EVENT, (e) => {
      const d = /** @type {CustomEvent} */ (e).detail;
      chartZoom(
        group,
        d.picked
          ? { from: Number(d.colKey), to: Number(d.colKey) + bucketMs }
          : null,
      );
    });
    heatEl.addEventListener(HEAT_ROW_EVENT, (e) => {
      const keys = /** @type {string[]} */ (
        /** @type {CustomEvent} */ (e).detail.keys
      );
      chartSelect(perHost.fig, null);
      for (const k of keys)
        chartSelect(perHost.fig, hostNames.indexOf(k), true);
    });
    perHost.fig.addEventListener(CHART_SELECT_EVENT, (e) => {
      const on = /** @type {number[]} */ (
        /** @type {CustomEvent} */ (e).detail?.on ?? []
      );
      const names = on.map((i) => hostNames[i]).filter(Boolean);
      setHeatRows(heatEl, names);
      paintRows(on);
    });
    group.addEventListener(CHART_ZOOM_EVENT, (e) => {
      if (!(/** @type {CustomEvent} */ (e).detail)) setHeatPick(heatEl, null);
    });
    /** The table's rows, pressed as the chart's sources are on. @param {number[]} on */
    const paintRows = (on) => {
      for (const row of hostsBody.querySelectorAll("tr[data-kp-row-key]")) {
        const key = /** @type {HTMLElement} */ (row).dataset.kpRowKey ?? "";
        const j = hostNames.indexOf(key || "no hostname");
        const pressed = j >= 0 && on.includes(j);
        row.classList.toggle("is-on", pressed);
        row.setAttribute("aria-pressed", String(pressed));
      }
    };

    // The hostname whose requests are listed, and the one the table shows.
    let reqHost = "";
    let reqShown = "";
    let reqSeq = 0;
    /** @param {string[]} names the hostnames, busiest first */
    const fillRequestHosts = (names) => {
      const list = names.filter(Boolean);
      reqSel.replaceChildren(
        ...(list.length ? list : ["no hostname"]).map((n) =>
          h("option", { value: list.length ? n : "" }, n),
        ),
      );
      reqSel.disabled = !list.length;
      reqHost = list.includes(reqHost) ? reqHost : (list[0] ?? "");
      reqSel.value = reqHost;
      drawnSelect(reqSel)?.refresh();
      if (!reqHost) {
        reqBody.replaceChildren();
        reqNote.textContent = "No hostname reached the gateway in this window.";
        tableOf(reqTable)?.state("ready");
        tableOf(reqTable)?.refresh();
      }
    };
    const readRequests = async () => {
      if (!reqHost) return;
      const host = reqHost;
      const seq = ++reqSeq;
      // A new hostname shows its loading state; a reread of the same one
      // keeps its rows on screen until the new ones arrive.
      if (reqShown !== host) {
        reqNote.textContent = `Reading the newest requests to ${host}…`;
        tableOf(reqTable)?.state("loading");
      }
      const r = await fetchJson(
        `/data/traffic/requests${setParams("", { host, range, limit: String(REQ_LIMIT) })}`,
        "the recent requests",
        abort.signal,
      );
      if (seq !== reqSeq || abort.signal.aborted) return;
      if (!r.ok) {
        reqShown = "";
        reqNote.textContent = notReadWords(r.error);
        tableOf(reqTable)?.fail(notReadWords(r.error));
        return;
      }
      const samples =
        /** @type {import("../metricsview.js").RequestSample[]} */ (
          r.body.samples ?? []
        );
      reqBody.replaceChildren(
        ...samples.map((s, i) =>
          h(
            "tr",
            { "data-kp-row-key": `${s.at_ms}-${i}` },
            h(
              "td",
              { "data-label": "Method" },
              h("span", { class: "mono" }, s.method || "—"),
            ),
            h(
              "td",
              { "data-label": "Path", title: s.path },
              h("span", { class: "mono mk-path" }, s.path || "/"),
            ),
            h(
              "td",
              { "data-label": "Status", "data-kp-sort-value": s.status },
              h(
                "span",
                { class: `kp-badge kp-badge--${statusTone(s.status)}` },
                String(s.status),
              ),
            ),
            h(
              "td",
              {
                class: "mk-n",
                "data-label": "Duration",
                "data-kp-sort-value": s.duration_ms ?? -1,
              },
              durationText(s.duration_ms),
            ),
          ),
        ),
      );
      reqShown = host;
      reqNote.textContent = requestsNote(
        host,
        samples.length,
        REQ_LIMIT,
        range,
      );
      tableOf(reqTable)?.state("ready");
      tableOf(reqTable)?.refresh();
    };
    reqSel.addEventListener("change", () => {
      reqHost = reqSel.value;
      void readRequests();
    });

    return async () => {
      const [r] = await Promise.all([
        fetchJson(
          `/data/traffic${setParams("", { range })}`,
          "the traffic",
          abort.signal,
        ),
        readHistory(RANGE_SECONDS[range]),
      ]);
      if (!r.ok) {
        failed.replaceChildren(errorBox(r.error));
        const why = notReadWords(r.error);
        notRead(over.fig, "Requests over time", why);
        notRead(perHost.fig, "Requests per hostname", why);
        mixSpec = null;
        setStackbar(mixBar, {
          state: "error",
          message: why,
          label: "Status mix",
        });
        setHeat(heatEl, {
          state: "error",
          skeleton: { rows: 8, columns: 24 },
          message: why,
          label: "Requests per hostname per hour",
        });
        tableOf(hostsTable)?.fail(why);
        tableOf(clientsTable)?.fail(why);
        reqShown = "";
        reqNote.textContent = why;
        tableOf(reqTable)?.fail(why);
        tilesNotRead(strip.tiles);
        return false;
      }
      failed.replaceChildren();
      const b = r.body;
      const marks = marksIn(b.from, b.to);
      const P = byTitle(b.panels ?? []);
      const status = P.get("Requests per status");
      const hostsP = P.get("Requests per hostname");
      const classes = statusClasses(status?.series ?? []);
      const f = trafficFigures({
        classes,
        prevTotal: b.prev_total ?? null,
        errors: b.errors?.rows ?? [],
      });

      // Requests over time, stacked by class.
      if (status?.error)
        notRead(
          over.fig,
          "Requests over time",
          `Loki did not answer: ${status.error}`,
        );
      else
        feedChart(over.fig, {
          key: "requests",
          label: "Requests over time",
          unit: "count",
          stacked: true,
          columns: true,
          series: classes[0]?.points.length
            ? classes
            : [{ label: "Requests", points: [] }],
          from: b.from,
          to: b.to,
          annotations: marks,
        });

      // Status mix: one bar over the exact total.
      if (f.total === 0) {
        mixSpec = null;
        setStackbar(mixBar, {
          state: "empty",
          message: "No requests reached the gateway in this window.",
          label: "Status mix",
        });
      } else {
        mixSpec = {
          title: "Status mix",
          total: f.total,
          summary: `${count(f.total)} requests`,
          format: count,
          label: "Status mix",
          parts: statusParts(classes),
        };
        paintMix();
      }
      mixNote.textContent = f.top4xx
        ? `${f.top4xx.status} is the biggest 4xx (${f.top4xx.host}${f.top4xx.path}, ${count(f.top4xx.n)} times).`
        : b.errors?.error
          ? `The biggest error answers were not read: ${b.errors.error}`
          : "No 4xx answers in this window.";

      // Requests per hostname: the legend's totals are the Hostnames
      // table's own numbers (one count, two places, never two sums).
      const rows = /** @type {[string, number][]} */ (b.hosts?.rows ?? []);
      const hostSeries = hostLegend(hostsP?.series ?? [], rows);
      hostNames = hostSeries.map((s) => s.label);
      if (hostsP?.error)
        notRead(
          perHost.fig,
          "Requests per hostname",
          `Loki did not answer: ${hostsP.error}`,
        );
      else
        feedChart(perHost.fig, {
          key: "perhost",
          label: "Requests per hostname",
          unit: "count",
          series: hostSeries.length
            ? hostSeries
            : [{ label: "Requests", points: [] }],
          from: b.from,
          to: b.to,
          annotations: marks,
        });

      // Hostnames: the heat grid …
      const heat = hostHeat(hostSeries, b.from, b.to);
      bucketMs = heat.bucket * 1000;
      const label = (/** @type {number} */ at) =>
        heat.unit === "day"
          ? formatDay(at).slice(0, 5)
          : heat.bucket >= 3600
            ? formatClock(at).slice(0, 2)
            : formatClock(at);
      if (!heat.rows.length)
        setHeat(heatEl, {
          state: "empty",
          skeleton: { rows: 8, columns: 24 },
          message: "No requests reached the gateway in this window.",
          label: "Requests per hostname per hour",
        });
      else if (hostsP?.error || b.hosts?.error)
        setHeat(heatEl, {
          state: "error",
          skeleton: { rows: 8, columns: 24 },
          message: `Loki did not answer: ${hostsP?.error ?? b.hosts?.error}`,
          label: "Requests per hostname per hour",
        });
      else
        setHeat(heatEl, {
          label: "Requests per hostname per hour",
          every: heat.every,
          columns: heat.columns.map((c) => ({
            key: String(c.at * 1000),
            label: label(c.at),
          })),
          rows: heat.rows.map((row, i) => {
            const top = Math.max(1, ...row.counts);
            return {
              key: row.host,
              label: row.host,
              mark: `var(--chart-${(i % 5) + 1})`,
              pressed: false,
              cells: row.counts.map((n, k) => ({
                tone: /** @type {const} */ ("level"),
                level: n / top,
                label: `${count(n)} requests`,
                tip: `${row.host}, ${formatDateTime(heat.columns[k].at)} to ${formatClock(heat.columns[k].at + heat.bucket)}: ${count(n)} requests`,
              })),
            };
          }),
        });
      for (const rowEl of heatEl.querySelectorAll(
        ".kp-heat__row[data-row-key]",
      )) {
        const btn = rowEl.querySelector(".kp-heat__label-button");
        if (btn)
          drivable(
            /** @type {HTMLElement} */ (btn),
            HOSTNAME,
            /** @type {HTMLElement} */ (rowEl).dataset.rowKey ?? "",
          );
      }
      // … and the table.
      syncRows(
        hostsBody,
        rows.map(([name, n]) => {
          const shown = name || "no hostname";
          const tr = h(
            "tr",
            {
              "data-kp-row-key": name,
              class: "mk-rowpick",
              tabindex: "0",
              "aria-pressed": "false",
              title: "Switch this hostname on or off in the chart above",
            },
            h(
              "td",
              { "data-label": "Hostname" },
              h(
                "span",
                { class: "mk-host" },
                h("span", {
                  class: "kp-idmark kp-idmark--sm",
                  "data-kp-idmark": shown,
                  "aria-hidden": "true",
                }),
                h("span", { class: "mono" }, shown),
              ),
            ),
            h(
              "td",
              {
                class: "mk-n",
                "data-label": "Requests",
                "data-kp-sort-value": n,
              },
              count(n),
            ),
            h(
              "td",
              { "data-label": "Share of all" },
              h("span", { class: "kp-bar-cell" }),
            ),
          );
          const share = f.total ? n / f.total : 0;
          setBarCell(
            /** @type {HTMLElement} */ (tr.querySelector(".kp-bar-cell")),
            {
              value: share,
              text: `${(share * 100).toFixed(1)}%`,
              tone: null,
              sortValue: share,
            },
          );
          drivable(tr, HOSTNAME, shown);
          const pickIt = () => {
            const i = hostNames.indexOf(shown);
            if (i >= 0) chartSelect(perHost.fig, i);
          };
          tr.addEventListener("click", pickIt);
          tr.addEventListener("keydown", (e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              pickIt();
            }
          });
          return tr;
        }),
      );
      attachIdmarks(hostsBody);
      // The first load ends the loading state; a table left in "loading"
      // re-marks itself (data-kp-state, aria-busy) on every refresh.
      tableOf(hostsTable)?.state("ready");
      tableOf(hostsTable)?.refresh();
      fillRequestHosts(rows.map(([name]) => name));
      void readRequests();

      // Busiest clients.
      const clients = /** @type {[string, number][]} */ (b.clients?.rows ?? []);
      const busiest = clients[0]?.[1] ?? 0;
      if (b.clients?.error)
        tableOf(clientsTable)?.fail(`Loki did not answer: ${b.clients.error}`);
      else {
        syncRows(
          clientsBody,
          clients.slice(0, 8).map(([c, n]) => {
            const tr = h(
              "tr",
              { "data-kp-row-key": c || "-" },
              c
                ? h(
                    "td",
                    { "data-label": "Client" },
                    h("span", { class: "mono" }, c),
                  )
                : h(
                    "td",
                    {
                      "data-label": "Client",
                      class: "mk-muted",
                      title:
                        "The access log line carried no client address for these requests",
                    },
                    "unknown (not logged)",
                  ),
              h(
                "td",
                { "data-label": "Share" },
                h("span", { class: "kp-bar-cell" }),
              ),
            );
            const share = f.total ? n / f.total : 0;
            setBarCell(
              /** @type {HTMLElement} */ (tr.querySelector(".kp-bar-cell")),
              {
                value: busiest ? n / busiest : 0,
                text: `${Math.round(share * 100)}%`,
                tone: null,
                sortValue: share,
              },
            );
            return tr;
          }),
        );
        tableOf(clientsTable)?.state("ready");
        tableOf(clientsTable)?.refresh();
      }

      // The tiles.
      const T = strip.tiles;
      const step = b.step ?? 600;
      /** @type {Point[]} */
      const sum = (classes[0]?.points ?? []).map((p, i) => [
        p[0],
        classes.reduce((a, c) => a + (c.points[i]?.[1] ?? 0), 0),
      ]);
      const n0 = (/** @type {number} */ v) => count(v);
      T.get("requests")?.set({
        value: count(f.total),
        trend: [changeText(f.change, range)],
        hover: hoverOf(sum, n0, "Requests per interval in this window."),
        points: sum,
        step,
      });
      const s5 = (f.shares["5xx"] ?? 0) * 100;
      T.get("5xx")?.set({
        value: s5.toFixed(1),
        unit: "%",
        trend: [
          `${count(f.counts["5xx"] ?? 0)} requests${f.spikeAt ? ` · spike at ${hhmm(f.spikeAt)}` : ""}`,
        ],
        hover: hoverOf(
          (classes[3]?.points ?? []).map((p) => [p[0], p[1]]),
          n0,
          "Server errors per interval.",
        ),
        points: classes[3]?.points ?? [],
        step,
        tone: s5 >= 1 ? "bad" : s5 > 0.1 ? "warn" : "",
      });
      const s4 = (f.shares["4xx"] ?? 0) * 100;
      T.get("4xx")?.set({
        value: s4.toFixed(1),
        unit: "%",
        trend: [
          f.top4xx
            ? `mostly ${f.top4xx.status} on ${f.top4xx.host}`
            : `${count(f.counts["4xx"] ?? 0)} requests`,
        ],
        hover: hoverOf(
          classes[2]?.points ?? [],
          n0,
          "Client errors per interval.",
        ),
        points: classes[2]?.points ?? [],
        step,
      });
      T.get("hosts")?.set({
        value: String(rows.length),
        trend: [
          rows[0] ? `busiest: ${rows[0][0] || "no hostname"}` : "no requests",
        ],
        hover: rows[0]
          ? `The busiest hostname carries ${f.total ? Math.round((rows[0][1] / f.total) * 100) : 0}% of all requests.`
          : "No requests in this window.",
      });
      const unknown = clients.find(([c]) => c === "")?.[1] ?? 0;
      T.get("clients")?.set({
        value:
          b.clients_total != null
            ? count(b.clients_total)
            : String(clients.length),
        trend: [
          f.total
            ? `${Math.round((unknown / f.total) * 100)}% without an address`
            : "no requests",
        ],
        hover: busiest
          ? `The busiest address made ${f.total ? Math.round((busiest / f.total) * 100) : 0}% of all requests.`
          : "No requests in this window.",
      });
      return true;
    };
  }
}

void [dot, fmtValue];
