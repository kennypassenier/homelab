// Metrics (redesign-371-metrics, release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/metrics.html,
// implemented exactly). Two views of one page, System (replace-grafana)
// and Traffic (replace-goaccess), sharing the window (1h … 30d) in the
// address, so switching keeps it.
//
// Top to bottom: the header (live chip, freshness, the data's source; the
// System | Traffic switch on the right), one toolbar row (what is shown,
// the window in words with the events counted, the zoom chip, the window
// switch), the KPI strip, the attention band (a failing drive), the key to
// the event markers, then the charts in sections on a 3-column grid. Every
// chart is the shared time chart (timechart.js): hover a point for its
// reading and the change over the hour before, a plain click on a legend
// source turns it on or off (Show all or Esc resets — Kenny, 2026-10-03:
// "shift-klik wil ik niet, ik wil dat een klik aan/uit is"), drag to zoom
// every chart of the page together, and what the host did in the window
// (/data/history) marked on each. The page reads again every 30 s, unless
// someone is zoomed in, has a reading pinned or a source picked.
//
// Data: /data/charts (Prometheus), /data/traffic (Loki), /data/history,
// the fleet store (the host's own CPU, memory, disk and load).

import { errorBox, fetchJson, fetchReport, h } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import {
  CLASSES,
  HOST_SECTIONS,
  STACK_SECTIONS,
  annotations,
  byTitle,
  changeText,
  count,
  drives,
  fill,
  hostLegend,
  nowValue,
  peak,
  poweredOn,
  statusClasses,
  trafficFigures,
  windowText,
} from "../metricsview.js";
import { current, subscribe } from "../store.js";
import {
  fmtValue,
  hhmm,
  pageCharts,
  timeChart,
  zoomChip,
} from "../timechart.js";
import {
  attentionBand,
  chip,
  dataTable,
  dot,
  ensureStyle,
  keyRow,
  kpiStrip,
  pageHeader,
  section,
  segSwitch,
  shareBar,
  skeletonBlock,
  sparkline,
  tableSlot,
  toolbar,
} from "../ui.js";
import { choice, setParams } from "../urlstate.js";

const RANGES = /** @type {const} */ (["1h", "6h", "24h", "7d", "30d"]);
const TABS = /** @type {const} */ (["system", "traffic"]);
/** Seconds between two reads. */
const EVERY_S = 30;

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
});
const SHOWING = declare({
  id: "metrics-showing",
  page: "metrics",
  opens: "view",
  what: "pick whose charts the System view shows: the host or one stack",
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
 * @typedef {ReturnType<typeof timeChart>} Chart
 */

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
  const stack = tab === "system" ? (params.get("stack") ?? "") : "";
  const go = (/** @type {Record<string, string | null>} */ p) =>
    ctx.navigate(`/charts${setParams(location.search, p)}`);
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  /** @type {Chart[]} */
  let charts = [];
  pageCharts.setZoom(null);

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
      chip(`Live · reads every ${EVERY_S} s`, { tone: "live", dot: true }),
      h(
        "span",
        null,
        tab === "system"
          ? "Source: Prometheus"
          : "Source: the gateway's access log in Loki",
      ),
    ],
    live: "updated",
    actions: [view.el],
  });

  // ── toolbar: what is shown · the window in words · the window ─────────
  const scope =
    tab === "system"
      ? (() => {
          const pick = /** @type {HTMLSelectElement} */ (
            h("select", {
              class: "kp-field__input mk-select",
              "aria-label": "Which machine",
              title: "Show the host's own charts, or one stack's container",
            })
          );
          const paint = () => {
            const names = (current().fleet?.stacks ?? [])
              .map((s) => s.name)
              .sort();
            pick.replaceChildren(
              h(
                "option",
                { value: "" },
                `The host${current().fleet?.host.name ? ` (${current().fleet?.host.name})` : ""}`,
              ),
              ...names.map((n) => h("option", { value: n }, `Stack ${n}`)),
            );
            pick.value = stack;
          };
          paint();
          stops.push(subscribe(paint));
          drivable(pick, SHOWING);
          pick.addEventListener("change", () => go({ stack: pick.value }));
          return h(
            "label",
            { class: "nx-tb__group" },
            h("b", null, "Showing"),
            pick,
          );
        })()
      : h(
          "div",
          { class: "nx-tb__group" },
          h("b", null, "Showing"),
          h("span", { class: "mk-muted" }, "Every hostname behind the gateway"),
        );
  const span = h("span", { class: "mk-window", role: "status" }, "Reading…");
  const zoom = zoomChip(pageCharts);
  stops.push(zoom.stop);
  stops.push(
    pageCharts.onZoom((z) => {
      span.hidden = z != null;
    }),
  );
  const win = segSwitch({
    label: "Window",
    items: RANGES.map((r) => ({
      value: r,
      label: r,
      hint: `Show the last ${r}`,
    })),
    value: range,
    onChange: (v) => go({ range: v === "24h" ? null : v }),
    drive: { id: WINDOW },
  });
  const bar = toolbar({
    groups: [scope, h("div", { class: "mk-window-zone" }, span, zoom.el)],
    state: [win.el],
  });
  bar.el.classList.add("mk-toolbar");

  const failed = h("div", { class: "mk-failed" });
  root.replaceChildren(head.el, bar.el, failed);

  /** @type {import("../timechart.js").Annotation[]} */
  let marks = [];
  /** @param {number} from @param {number} to */
  const readMarks = async (from, to) => {
    const r = await fetchReport(
      `/data/history?since=${from}&limit=500`,
      "the history",
      abort.signal,
    );
    marks = r.ok
      ? annotations(r.report?.entries ?? [], {
          from,
          to,
          ...(stack ? { stack } : {}),
        })
      : [];
  };

  /** Someone is reading a chart: a zoom, a pinned reading, a picked source. */
  const busy = () =>
    pageCharts.zoom != null ||
    root.querySelector(".tc-tip--pinned:not([hidden])") != null ||
    root.querySelector('.tc-legend__item[aria-pressed="true"]') != null ||
    root.querySelector(".tc-plot:focus") != null ||
    // A pointer over a chart or its legend: the reading under it stays put
    // (a repaint would destroy the chart under the hover).
    root.querySelector(".tc:hover") != null;

  const destroyCharts = () => {
    for (const c of charts) c.destroy();
    charts = [];
  };

  /**
   * One chart card: heading, one sentence, the figure "now" on the right,
   * the chart (a skeleton until it is read), an optional foot.
   * @param {{key: string, title: string, desc: string, span: number,
   *   nowLabel?: string}} c
   */
  const card = (c) => {
    const nowB = h("b", null, "");
    const nowS = h("span", null, c.nowLabel ?? "now");
    const now = h("div", { class: "mk-now", hidden: "" }, nowB, nowS);
    const s = section({
      title: c.title,
      desc: c.desc,
      id: `chart-${c.key}`,
      tools: [now],
      level: "h3",
    });
    s.el.classList.add("mk-card", `mk-span-${c.span}`);
    s.el.dataset.key = c.key;
    s.body.replaceChildren(skeletonBlock("168px", `Reading ${c.title}`));
    return {
      el: s.el,
      body: s.body,
      /** @param {string | null} v @param {string} [label] */
      setNow: (v, label) => {
        now.hidden = v == null;
        nowB.textContent = v ?? "";
        if (label) nowS.textContent = label;
      },
      /**
       * The card's foot line: the source left, a note right (section's
       * foot). @param {(Node | string)[]} left @param {(Node | string)[]} [right]
       */
      foot: (left, right = []) => {
        s.foot.replaceChildren(
          h("span", null, ...left),
          h("span", null, ...right),
        );
        s.foot.hidden = false;
      },
    };
  };

  /**
   * Draw a time chart into a card's body.
   * @param {HTMLElement} body
   * @param {Omit<import("../timechart.js").ChartSpec, "from" | "to" | "annotations" | "group">} spec
   *   `key`: the card's key, the chart's name for Live view
   * @param {{from: number, to: number}} w
   */
  const draw = (body, spec, w) => {
    const box = h("div", { class: "mk-chart" });
    const single = spec.series.every((s) => s.points.length === 1);
    body.replaceChildren(
      box,
      ...(single
        ? [
            h(
              "p",
              { class: "mk-note" },
              "Only one reading so far: the line grows as more readings arrive.",
            ),
          ]
        : []),
    );
    const c = timeChart(box, {
      ...spec,
      from: w.from,
      to: w.to,
      annotations: marks,
      group: pageCharts,
    });
    charts.push(c);
    return c;
  };

  /** @param {HTMLElement} body @param {string} why */
  const notRead = (body, why) =>
    body.replaceChildren(h("p", { class: "mk-card-error" }, dot("bad"), why));

  /**
   * A read that failed: every tile says so instead of pulsing for ever, and
   * nothing is left in the attention band from an earlier read.
   * @param {ReturnType<typeof kpiStrip>} strip
   * @param {ReturnType<typeof attentionBand> | null} band
   */
  const tilesNotRead = (strip, band) => {
    for (const t of strip.tiles.values())
      t.set({
        value: "—",
        unit: "",
        ctx: "not read",
        ctxTone: "",
        ctxParts: null,
        tone: "",
        spark: [],
        meter: null,
      });
    band?.set([]);
  };

  /** @param {string} title @param {string} desc @param {Node[]} cards */
  const sect = (title, desc, cards) =>
    h(
      "section",
      { class: "mk-section", "aria-label": title },
      h(
        "div",
        { class: "mk-section__head" },
        h("h2", null, title),
        h("p", null, desc),
      ),
      h("div", { class: "mk-panels" }, ...cards),
    );

  /** @type {() => Promise<void>} */
  let read = async () => {};
  if (tab === "system") read = system();
  else read = traffic();

  root.append(
    keyRow([
      ["Tab", "focus a chart"],
      ["← →", "move through time (Shift: ten steps)"],
      ["Enter", "pin the reading"],
      ["Esc", "release · show all · reset zoom"],
      ["drag", "zoom every chart"],
      ["click", "a source turns it on or off"],
    ]),
  );

  const tick = async () => {
    if (busy()) return;
    await read();
  };
  void read().catch(() => {});
  const timer = setInterval(() => void tick().catch(() => {}), EVERY_S * 1000);

  return () => {
    abort.abort();
    clearInterval(timer);
    destroyCharts();
    pageCharts.setZoom(null);
    for (const s of stops) s();
    root.classList.remove("mk-page", "nx-ops");
  };

  // ── System ──────────────────────────────────────────────────────────
  function system() {
    const sections = stack ? STACK_SECTIONS : HOST_SECTIONS;
    const hostTiles = [
      { key: "cpu", label: "CPU", target: "chart-cpu" },
      { key: "mem", label: "Memory", target: "chart-mem" },
      { key: "disk", label: "Root disk", target: "chart-disk" },
      { key: "load", label: "Load (1 min)", target: "chart-load" },
      { key: "temp", label: "Hottest chip", target: "chart-temp" },
      { key: "drives", label: "Drives", target: "chart-drives" },
    ];
    const stackTiles = [
      { key: "cpu", label: "CPU", target: "chart-cpu" },
      { key: "mem", label: "Memory", target: "chart-mem" },
      { key: "disk", label: "Disk", target: "chart-disk" },
      { key: "apps", label: "Apps running", target: "chart-appcpu" },
      {
        key: "restarts",
        label: "Restarts (last hour)",
        target: "chart-restarts",
      },
    ];
    const strip = kpiStrip(stack ? stackTiles : hostTiles, {
      loading: true,
      label: stack ? `Stack ${stack} right now` : "The host right now",
      drive: { id: TILE },
    });
    const attention = attentionBand([]);
    const drivesTable = tableSlot();
    stops.push(drivesTable.stop);
    const annKey = h(
      "p",
      { class: "mk-annkey", "aria-label": "Chart markers" },
      h("span", null, dot("info"), "an operation the host ran"),
      h("span", null, dot("warn"), "an update"),
      h("span", null, dot("bad"), "a failure"),
      h(
        "span",
        { class: "mk-pointer-only" },
        "Point at a chart: every chart follows the same moment.",
      ),
      h(
        "span",
        { class: "mk-touch-only" },
        "Touch a chart: every chart follows the same moment.",
      ),
    );
    /** @type {Map<string, ReturnType<typeof card>>} */
    const cards = new Map();
    const blocks = sections.map((s) =>
      sect(
        s.title,
        s.desc,
        s.cards.map((c) => {
          const k = card({
            key: c.key,
            title: c.title,
            desc: fill(c.desc, { cores: current().fleet?.host.cores_total }),
            span: c.span,
          });
          cards.set(c.key, k);
          return k.el;
        }),
      ),
    );
    root.append(strip.el, attention.el, annKey, ...blocks);

    return async () => {
      const q = setParams("", { stack: stack || null, range });
      const r = await fetchJson(`/data/charts${q}`, "the charts", abort.signal);
      if (!r.ok) {
        failed.replaceChildren(errorBox(r.error));
        span.textContent = "";
        for (const c of cards.values()) notRead(c.body, "Not read: see above.");
        tilesNotRead(strip, attention);
        return;
      }
      failed.replaceChildren();
      const { from, to } = r.body;
      await readMarks(from, to);
      span.textContent = `${windowText(from, to)} · ${marks.length} ${marks.length === 1 ? "event" : "events"} marked · drag across a chart to zoom`;
      const P = byTitle(r.body.panels);
      destroyCharts();
      const fleet = current().fleet;
      const host = fleet?.host;
      const st = fleet?.stacks.find((s) => s.name === stack);
      for (const s of sections)
        for (const c of s.cards) {
          const k = cards.get(c.key);
          if (!k) continue;
          const p = P.get(c.panel);
          if (c.kind === "drives") {
            paintDrives(k, P);
            continue;
          }
          if (!p) {
            notRead(k.body, "This chart is not part of the answer.");
            continue;
          }
          if (p.error) {
            notRead(k.body, `Prometheus did not answer: ${p.error}`);
            continue;
          }
          if (!p.series.length || !p.series.some((x) => x.points.length)) {
            notRead(k.body, "No readings in this window yet.");
            continue;
          }
          const unit = c.unit ?? "count";
          const threshold =
            c.threshold === "committed"
              ? host?.ram_committed_mb
                ? host.ram_committed_mb * 1048576
                : undefined
              : c.threshold === "allowance"
                ? st?.ram_max_mb
                  ? st.ram_max_mb * 1048576
                  : undefined
                : c.threshold;
          draw(
            k.body,
            {
              key: c.key,
              label: c.title,
              unit,
              series: p.series.map((x) => ({
                label: x.label || c.title,
                points: x.points,
              })),
              ...(threshold != null ? { threshold } : {}),
              ...(c.yMax != null ? { yMax: c.yMax } : {}),
            },
            { from, to },
          );
          const now = nowValue(p, c.now);
          k.setNow(
            now == null
              ? null
              : unit === "percent"
                ? `${Math.round(now)}%`
                : fmtValue(now, unit),
          );
          const d = k.el.querySelector(".nx-card__head > p");
          if (d) d.textContent = fill(c.desc, { cores: host?.cores_total });
        }
      if (stack) paintStackTiles(P);
      else paintHostTiles(P);
      head.live?.set(Math.floor(Date.now() / 1000));
    };

    /**
     * The Drives card: one row per drive (invariant 27), and the attention
     * band for each failing one.
     * @param {ReturnType<typeof card>} k
     * @param {Map<string, Panel>} P
     */
    function paintDrives(k, P) {
      const list = drives(P);
      const bad = list.filter((d) => !d.ok);
      k.setNow(
        list.length ? `${bad.length || list.length} of ${list.length}` : null,
        bad.length ? "failing" : "healthy",
      );
      if (!list.length) {
        notRead(k.body, "Prometheus reports no SMART readings.");
        return;
      }
      const t = dataTable({
        cls: "mk-table",
        remember: "metrics-drives",
        caption: "Drives",
        columns: [
          { label: "Drive", sort: "text" },
          { label: "SMART", sort: "text" },
          { label: "Temp.", sort: "number", cls: "mk-n" },
          { label: "Trend", cls: "mk-hide-phone" },
          { label: "Pending", sort: "number", cls: "mk-n" },
          { label: "Trend", cls: "mk-hide-phone" },
          { label: "Reallocated", sort: "number", cls: "mk-n" },
          { label: "Powered on", sort: "number", cls: "mk-n mk-hide-phone" },
        ],
      });
      t.tbody.replaceChildren(
        ...list.map((d) =>
          h(
            "tr",
            { "data-kp-row-key": d.name },
            h("td", null, h("b", { class: "mono" }, d.name)),
            h(
              "td",
              { class: d.ok ? "" : "mk-bad" },
              h(
                "span",
                { class: "mk-state" },
                dot(d.ok ? "ok" : "bad"),
                h(
                  "span",
                  null,
                  d.ok ? "ok" : "not ok",
                  ...(!d.ok
                    ? [
                        h(
                          "span",
                          { class: "mk-hide-phone" },
                          d.since
                            ? `, since ${windowText(d.since, d.since).split(" → ")[0]}`
                            : ", for this whole window",
                        ),
                      ]
                    : []),
                ),
              ),
            ),
            h(
              "td",
              { class: "mk-n" },
              d.temp == null ? "—" : `${d.temp.toFixed(0)} °C`,
            ),
            h(
              "td",
              { class: "mk-hide-phone mk-trend" },
              d.tempTrend.length > 1
                ? sparkline(d.tempTrend, { colour: "var(--chart-4)" })
                : "",
            ),
            h(
              "td",
              { class: `mk-n${d.pending ? " mk-bad" : ""}` },
              d.pending == null ? "—" : String(d.pending),
            ),
            h(
              "td",
              { class: "mk-hide-phone mk-trend" },
              d.pendTrend.length > 1
                ? sparkline(d.pendTrend, {
                    colour: d.pending ? "var(--destructive)" : "var(--chart-3)",
                  })
                : "",
            ),
            h(
              "td",
              { class: `mk-n${d.realloc ? " mk-bad" : ""}` },
              d.realloc == null ? "—" : String(d.realloc),
            ),
            h("td", { class: "mk-n mk-hide-phone" }, poweredOn(d.hours)),
          ),
        ),
      );
      k.body.replaceChildren(t.wrap);
      k.foot(
        [
          "A rising pending or reallocated count is the early warning; temperature above 50 °C shortens a drive's life.",
        ],
        ["smartctl via node-exporter"],
      );
      drivesTable.attach(k.body);
      const seeDrive = (/** @type {string} */ name) =>
        drivable(
          h(
            "button",
            {
              type: "button",
              class: "kp-button kp-button--sm",
              title: "Scroll to the Drives table",
            },
            "See the drive",
          ),
          SEE_DRIVE,
          name,
        );
      attention.set(
        bad.map((d) => {
          const b = seeDrive(d.name);
          b.addEventListener("click", () =>
            k.el.scrollIntoView({ block: "start", behavior: "smooth" }),
          );
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
            tone: "bad",
            title: `Drive ${d.name} reports SMART not ok`,
            text: `${d.pending ?? 0} ${d.pending === 1 ? "sector waits" : "sectors wait"} to be re-read and ${d.realloc ?? 0} ${d.realloc === 1 ? "was" : "were"} already swapped for spares. Data on it is at risk.`,
            action: h("span", { class: "mk-acts" }, b, back),
          };
        }),
      );
    }

    /** @param {Map<string, Panel>} P */
    function paintHostTiles(P) {
      const host = current().fleet?.host;
      const T = strip.tiles;
      const cores = host?.cores_total ?? null;
      const cpu = P.get("CPU used")?.series[0]?.points ?? [];
      const pk = peak(cpu);
      T.get("cpu")?.set({
        value: String(host?.cpu_pct ?? Math.round(cpu.at(-1)?.[1] ?? 0)),
        unit: "%",
        ctx: `${cores ?? "?"} cores${pk ? ` · peak ${Math.round(pk.value)}% at ${hhmm(pk.at)}` : ""}`,
        spark: cpu.map((p) => p[1]),
        colour: "var(--chart-1)",
      });
      if (host) {
        const used = host.ram_used_mb;
        const total = host.ram_total_mb;
        const promised = host.ram_committed_mb ?? 0;
        T.get("mem")?.set({
          value: (used / 1024).toFixed(1),
          unit: `of ${(total / 1024).toFixed(0)} GiB`,
          ctx: `${Math.round((promised / total) * 100)}% promised to stacks`,
          meter: {
            pct: (used / total) * 100,
            mark: (promised / total) * 100,
            tone:
              used / total >= 0.9 ? "bad" : used / total >= 0.75 ? "warn" : "",
          },
        });
        const root = P.get("Disk used per filesystem")?.series.find(
          (s) => s.label === "/",
        )?.points;
        const moved =
          root && root.length > 1
            ? root[root.length - 1][1] - root[0][1]
            : null;
        const d = host.disk_detail;
        T.get("disk")?.set({
          value: String(host.disk_pct),
          unit: "%",
          // Short enough for one line of a wide tile (finding 14).
          ctx: `${d ? `${d.root_lv_size_gb} GB on ${d.root_disk_device.replace(/^\/dev\//, "")}` : "the host's root"}${
            moved == null
              ? ""
              : Math.abs(moved) < 0.5
                ? " · flat"
                : ` · ${moved > 0 ? "+" : "−"}${Math.abs(moved).toFixed(1)} pts`
          }`,
          meter: {
            pct: host.disk_pct,
            tone:
              host.disk_pct >= 90 ? "bad" : host.disk_pct >= 75 ? "warn" : "",
          },
        });
        const load = host.load1_x100 == null ? null : host.load1_x100 / 100;
        const loadPts = P.get("Load average (5 min)")?.series[0]?.points ?? [];
        T.get("load")?.set({
          value: load == null ? "—" : load.toFixed(2),
          ctx:
            load == null || cores == null
              ? "not reported"
              : load < cores / 2
                ? `well under ${cores} cores`
                : load < cores
                  ? `near ${cores} cores`
                  : `above ${cores} cores: work queues up`,
          tone: load != null && cores != null && load >= cores ? "warn" : "",
          spark: loadPts.map((p) => p[1]),
          colour: "var(--chart-2)",
        });
      }
      const temps =
        P.get("Temperature (hottest sensor per chip)")?.series ?? [];
      const hot = temps
        .map((s) => ({ s, v: s.points.at(-1)?.[1] ?? -Infinity }))
        .sort((a, b) => b.v - a.v)[0];
      T.get("temp")?.set(
        hot
          ? {
              value: hot.v.toFixed(0),
              unit: "°C",
              ctx: hot.s.label,
              tone: hot.v >= 85 ? "bad" : hot.v >= 75 ? "warn" : "",
              spark: hot.s.points.map((p) => p[1]),
              colour: "var(--chart-4)",
            }
          : { value: "—", ctx: "no sensor reported" },
      );
      const list = drives(P);
      const bad = list.filter((d) => !d.ok);
      const health = P.get("Drive health (SMART, 1 = ok)")?.series ?? [];
      const failing = (health[0]?.points ?? []).map((_, i) =>
        health.reduce((a, s) => a + ((s.points[i]?.[1] ?? 1) < 1 ? 1 : 0), 0),
      );
      T.get("drives")?.set(
        bad.length
          ? {
              value: String(bad.length),
              unit: `of ${list.length} failing`,
              ctx: `${bad.map((d) => d.name).join(", ")}: SMART not ok`,
              ctxTone: "bad",
              tone: "bad",
              spark: failing,
              colour: "var(--destructive)",
            }
          : {
              value: String(list.length),
              unit: list.length === 1 ? "drive" : "drives",
              ctx: list.length ? "all report SMART ok" : "no SMART readings",
              ctxTone: list.length ? "ok" : "",
              tone: "",
              spark: failing,
              colour: "var(--success-foreground)",
            },
      );
    }

    /** @param {Map<string, Panel>} P */
    function paintStackTiles(P) {
      const st = current().fleet?.stacks.find((s) => s.name === stack);
      const T = strip.tiles;
      const cpu = P.get("CPU (whole container)")?.series[0]?.points ?? [];
      const pk = peak(cpu);
      T.get("cpu")?.set({
        value: cpu.length ? (cpu.at(-1)?.[1] ?? 0).toFixed(0) : "—",
        unit: "%",
        ctx: pk
          ? `of its own cores · peak ${Math.round(pk.value)}% at ${hhmm(pk.at)}`
          : "not read",
        spark: cpu.map((p) => p[1]),
        colour: "var(--chart-1)",
      });
      const memPts =
        P.get("Memory used (whole container)")?.series[0]?.points ?? [];
      const usedMb = st?.ram_used_mb ?? (memPts.at(-1)?.[1] ?? 0) / 1048576;
      const maxMb = st?.ram_max_mb ?? null;
      T.get("mem")?.set({
        value:
          usedMb >= 1024
            ? (usedMb / 1024).toFixed(1)
            : String(Math.round(usedMb)),
        unit: `${usedMb >= 1024 ? "GiB" : "MiB"}${maxMb ? ` of ${maxMb >= 1024 ? `${(maxMb / 1024).toFixed(1)} GiB` : `${maxMb} MiB`}` : ""}`,
        ctx: maxMb
          ? `${Math.round((usedMb / maxMb) * 100)}% of its allowance`
          : "no allowance set",
        meter: maxMb
          ? {
              pct: (usedMb / maxMb) * 100,
              tone:
                usedMb / maxMb >= 0.9
                  ? "bad"
                  : usedMb / maxMb >= 0.75
                    ? "warn"
                    : "",
            }
          : null,
      });
      const disk =
        P.get("Disk used (root filesystem)")?.series[0]?.points ?? [];
      const dv = disk.at(-1)?.[1];
      T.get("disk")?.set({
        value: dv == null ? "—" : dv.toFixed(0),
        unit: "%",
        ctx: "of its own root disk",
        meter:
          dv == null
            ? null
            : { pct: dv, tone: dv >= 90 ? "bad" : dv >= 75 ? "warn" : "" },
      });
      T.get("apps")?.set({
        value: st ? String(st.apps_running ?? 0) : "—",
        unit: st ? `of ${st.apps_total ?? 0}` : "",
        ctx: st
          ? (st.apps_running ?? 0) === (st.apps_total ?? 0)
            ? "every app is up"
            : `${(st.apps_total ?? 0) - (st.apps_running ?? 0)} not running`
          : "not in the fleet",
        tone: st && (st.apps_running ?? 0) < (st.apps_total ?? 0) ? "warn" : "",
        meter: st?.apps_total
          ? { pct: ((st.apps_running ?? 0) / st.apps_total) * 100 }
          : null,
      });
      const restarts = P.get("Restarts (last hour)")?.series ?? [];
      const n = restarts.reduce((a, s) => a + (s.points.at(-1)?.[1] ?? 0), 0);
      T.get("restarts")?.set({
        value: String(Math.round(n)),
        ctx: n ? "an app is restarting" : "a healthy app reads 0",
        tone: n ? "warn" : "",
        spark: (restarts[0]?.points ?? []).map((_, i) =>
          restarts.reduce((a, s) => a + (s.points[i]?.[1] ?? 0), 0),
        ),
        colour: "var(--warning-foreground)",
      });
    }
  }

  // ── Traffic ─────────────────────────────────────────────────────────
  function traffic() {
    const strip = kpiStrip(
      [
        { key: "requests", label: "Requests", target: "chart-requests" },
        { key: "5xx", label: "Server errors (5xx)", target: "chart-requests" },
        { key: "4xx", label: "Client errors (4xx)", target: "chart-mix" },
        { key: "hosts", label: "Hostnames", target: "chart-hostnames" },
        { key: "clients", label: "Client addresses", target: "chart-clients" },
      ],
      { loading: true, label: "Traffic in this window", drive: { id: TILE } },
    );
    const over = card({
      key: "requests",
      title: "Requests over time",
      desc: "Requests per interval, stacked by status class; red is the server failing.",
      span: 2,
    });
    const mix = card({
      key: "mix",
      title: "Status mix",
      desc: "Share of all requests per status class in this window.",
      span: 1,
    });
    const perHost = card({
      key: "perhost",
      title: "Requests per hostname",
      desc: "Requests per interval for each hostname; click a row in Hostnames below to switch one on or off.",
      span: 3,
    });
    const hostsCard = card({
      key: "hostnames",
      title: "Hostnames",
      desc: "Requests per hostname, its trend and its share of the total. Click a row to switch it on or off in the chart; Show all resets.",
      span: 2,
      nowLabel: "active",
    });
    const clientsCard = card({
      key: "clients",
      title: "Busiest clients",
      desc: "The addresses that made the most requests.",
      span: 1,
    });
    for (const c of [mix, hostsCard, clientsCard])
      c.body.replaceChildren(skeletonBlock("168px", "Reading the access log"));
    const hostsTable = tableSlot();
    const clientsTable = tableSlot();
    stops.push(hostsTable.stop, clientsTable.stop);
    root.append(
      strip.el,
      h(
        "section",
        { class: "mk-section", "aria-label": "Traffic" },
        h(
          "div",
          { class: "mk-panels" },
          over.el,
          mix.el,
          perHost.el,
          hostsCard.el,
          clientsCard.el,
        ),
      ),
    );

    return async () => {
      const r = await fetchJson(
        `/data/traffic${setParams("", { range })}`,
        "the traffic",
        abort.signal,
      );
      if (!r.ok) {
        failed.replaceChildren(errorBox(r.error));
        span.textContent = "";
        for (const c of [over, mix, perHost, hostsCard, clientsCard])
          notRead(c.body, "Not read: see above.");
        tilesNotRead(strip, null);
        return;
      }
      failed.replaceChildren();
      const b = r.body;
      await readMarks(b.from, b.to);
      span.textContent = `${windowText(b.from, b.to)} · ${marks.length} ${marks.length === 1 ? "event" : "events"} marked · drag across a chart to zoom`;
      destroyCharts();
      const P = byTitle(b.panels ?? []);
      const status = P.get("Requests per status");
      const hostsP = P.get("Requests per hostname");
      const classes = statusClasses(status?.series ?? []);
      const f = trafficFigures({
        classes,
        prevTotal: b.prev_total ?? null,
        errors: b.errors?.rows ?? [],
      });
      const w = { from: b.from, to: b.to };

      // Requests over time, stacked by class.
      if (status?.error)
        notRead(over.body, `Loki did not answer: ${status.error}`);
      else if (!classes[0]?.points.length)
        notRead(over.body, "No requests in this window.");
      else
        draw(
          over.body,
          {
            key: "requests",
            label: "Requests over time",
            unit: "count",
            stacked: true,
            series: classes,
          },
          w,
        );

      // Status mix.
      mix.body.replaceChildren(
        h(
          "div",
          { class: "mk-mix" },
          ...CLASSES.map((c) =>
            h(
              "div",
              { class: "mk-mix__row" },
              h("b", null, c.label),
              shareBar(
                (f.shares[c.label] ?? 0) * 100,
                `${((f.shares[c.label] ?? 0) * 100).toFixed(1)}%`,
                c.colour,
              ),
            ),
          ),
        ),
        h(
          "p",
          { class: "mk-muted mk-sm" },
          f.top4xx
            ? `${f.top4xx.status} is the biggest 4xx (${f.top4xx.host}${f.top4xx.path}, ${count(f.top4xx.n)} times).`
            : b.errors?.error
              ? `The biggest error answers were not read: ${b.errors.error}`
              : "No 4xx answers in this window.",
        ),
      );

      // Requests per hostname: the legend's totals are the Hostnames
      // table's own numbers (one count, two places, never two sums).
      const rows = /** @type {[string, number][]} */ (b.hosts?.rows ?? []);
      const hostSeries = hostLegend(hostsP?.series ?? [], rows);
      /** The table's rows, pressed as the chart's sources are on. */
      const paintRows = (/** @type {number[]} */ on) => {
        for (const row of hostsCard.body.querySelectorAll(
          "tr[data-kp-row-key]",
        )) {
          const key = /** @type {HTMLElement} */ (row).dataset.kpRowKey ?? "";
          const j = hostSeries.findIndex(
            (s) => s.label === (key || "no hostname"),
          );
          const pressed = j >= 0 && on.includes(j);
          row.classList.toggle("is-on", pressed);
          row.setAttribute("aria-pressed", String(pressed));
        }
      };
      /** @type {Chart | null} */
      let hostChart = null;
      if (hostsP?.error)
        notRead(perHost.body, `Loki did not answer: ${hostsP.error}`);
      else if (!hostSeries.length)
        notRead(perHost.body, "No requests in this window.");
      else
        hostChart = draw(
          perHost.body,
          {
            key: "perhost",
            label: "Requests per hostname",
            unit: "count",
            series: hostSeries,
            onSelect: paintRows,
          },
          w,
        );

      // Hostnames table.
      const top = rows[0]?.[1] ?? 0;
      hostsCard.setNow(String(rows.length), "active");
      if (b.hosts?.error)
        notRead(hostsCard.body, `Loki did not answer: ${b.hosts.error}`);
      else {
        const t = dataTable({
          cls: "mk-table",
          remember: "metrics-hostnames",
          caption: "Hostnames",
          columns: [
            { label: "Hostname", sort: "text" },
            { label: "Trend", cls: "mk-hide-phone" },
            { label: "Requests", sort: "number", cls: "mk-n" },
            { label: "Share", sort: "number" },
          ],
        });
        t.tbody.replaceChildren(
          ...rows.map(([name, n]) => {
            const si = hostSeries.findIndex(
              (s) => s.label === (name || "no hostname"),
            );
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
                null,
                h("span", { class: "mono mk-host" }, name || "no hostname"),
              ),
              h(
                "td",
                { class: "mk-hide-phone mk-trend" },
                si >= 0 && hostSeries[si].points.length > 1
                  ? sparkline(
                      hostSeries[si].points.map((p) => p[1]),
                      { colour: `var(--chart-${(si % 5) + 1})` },
                    )
                  : "",
              ),
              h("td", { class: "mk-n" }, count(n)),
              h(
                "td",
                { class: "mk-share" },
                shareBar(
                  top ? (n / top) * 100 : 0,
                  `${f.total ? ((n / f.total) * 100).toFixed(1) : "0.0"}%`,
                ),
              ),
            );
            drivable(tr, HOSTNAME, name || "no hostname");
            // The chart owns the selection; its onSelect repaints the rows.
            const pickIt = () => {
              if (si >= 0) hostChart?.select(si);
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
        hostsCard.body.replaceChildren(t.wrap);
        hostsTable.attach(hostsCard.body);
      }

      // Busiest clients.
      const clients = /** @type {[string, number][]} */ (b.clients?.rows ?? []);
      const busiest = clients[0]?.[1] ?? 0;
      if (b.clients?.error)
        notRead(clientsCard.body, `Loki did not answer: ${b.clients.error}`);
      else {
        const t = dataTable({
          cls: "mk-table",
          remember: "metrics-clients",
          caption: "Busiest clients",
          columns: [
            { label: "Client", sort: "text" },
            { label: "Share", sort: "number" },
          ],
        });
        t.tbody.replaceChildren(
          ...clients.slice(0, 8).map(([c, n]) =>
            h(
              "tr",
              { "data-kp-row-key": c || "-" },
              c
                ? h("td", null, h("span", { class: "mono" }, c))
                : h(
                    "td",
                    {
                      class: "mk-muted",
                      title:
                        "The access log line carried no client address for these requests",
                    },
                    "unknown (not logged)",
                  ),
              h(
                "td",
                { class: "mk-share" },
                shareBar(
                  busiest ? (n / busiest) * 100 : 0,
                  `${f.total ? Math.round((n / f.total) * 100) : 0}%`,
                  "var(--chart-2)",
                ),
              ),
            ),
          ),
        );
        clientsCard.body.replaceChildren(t.wrap);
        clientsTable.attach(clientsCard.body);
      }

      // The tiles.
      const T = strip.tiles;
      const sum = classes[0]?.points.map((_, i) =>
        classes.reduce((a, c) => a + (c.points[i]?.[1] ?? 0), 0),
      );
      T.get("requests")?.set({
        value: count(f.total),
        ctx: changeText(f.change, range),
        spark: sum,
        colour: "var(--chart-1)",
      });
      const s5 = (f.shares["5xx"] ?? 0) * 100;
      T.get("5xx")?.set({
        value: s5.toFixed(1),
        unit: "%",
        ctx: `${count(f.counts["5xx"] ?? 0)} requests${f.spikeAt ? ` · spike at ${hhmm(f.spikeAt)}` : ""}`,
        ctxTone: s5 >= 1 ? "bad" : s5 > 0.1 ? "warn" : "",
        tone: s5 >= 1 ? "bad" : s5 > 0.1 ? "warn" : "",
        spark: classes[3]?.points.map((p) => p[1]),
        colour: "var(--destructive)",
      });
      const s4 = (f.shares["4xx"] ?? 0) * 100;
      T.get("4xx")?.set({
        value: s4.toFixed(1),
        unit: "%",
        ctx: f.top4xx
          ? `mostly ${f.top4xx.status} on ${f.top4xx.host}`
          : `${count(f.counts["4xx"] ?? 0)} requests`,
        spark: classes[2]?.points.map((p) => p[1]),
        colour: "var(--warning-foreground)",
      });
      T.get("hosts")?.set({
        value: String(rows.length),
        ctx: rows[0]
          ? `busiest: ${rows[0][0] || "no hostname"}`
          : "no requests",
        meter:
          f.total && rows[0] ? { pct: (rows[0][1] / f.total) * 100 } : null,
        title: "The bar is the busiest hostname's share of all requests",
      });
      const unknown = clients.find(([c]) => c === "")?.[1] ?? 0;
      T.get("clients")?.set({
        value:
          b.clients_total != null
            ? count(b.clients_total)
            : String(clients.length),
        ctx: f.total
          ? `${Math.round((unknown / f.total) * 100)}% without an address`
          : "no requests",
        meter: f.total && busiest ? { pct: (busiest / f.total) * 100 } : null,
        title: "The bar is the busiest address's share of all requests",
      });
      head.live?.set(Math.floor(Date.now() / 1000));
    };
  }
}
