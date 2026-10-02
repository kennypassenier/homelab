// Fleet view (visuals milestone): the fleet-wide graphs that need no single
// stack open — feat-overview-7 (topology), feat-overview-11 (capacity map),
// feat-overview-12 (disk-growth prediction), feat-stacks-9 (dependencies)
// and feat-stacks-10 (stale images). Each section reads its own route and
// fails on its own, so a Prometheus outage only empties the two sections
// that need it.

import { formatValue } from "../charts.js";
import {
  badgeCell,
  errorBox,
  fetchJson,
  h,
  slowRead,
  tableBlock,
  td,
} from "../dom.js";
import { formatDateTime } from "../format.js";
import { topologyFigure } from "../topology.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const topoBox = h(
    "div",
    { class: "fleetview__topology" },
    h("p", null, "Loading…"),
  );
  const capacity = tableBlock({
    remember: "capacity-map",
    caption: "Capacity map",
    search: "Search stacks",
    state: "loading",
    nothing: "No stacks with metrics yet.",
    columns: [
      { label: "Stack", sort: "text" },
      { label: "CPU", sort: "number" },
      { label: "Memory", sort: "number" },
      { label: "Disk", sort: "number" },
    ],
  });
  const growth = tableBlock({
    remember: "disk-growth",
    caption: "Disk growth",
    search: "Search filesystems",
    state: "loading",
    nothing: "No growing filesystem has enough history yet.",
    columns: [
      { label: "Where", sort: "text" },
      { label: "Filesystem", sort: "text" },
      { label: "Now", sort: "number" },
      { label: "Trend (robust)", sort: "number" },
      { label: "Days to full", sort: "number" },
      { label: "Warning", sort: "text", filter: "choice" },
    ],
  });
  const deps = tableBlock({
    remember: "dependencies",
    caption: "Dependencies between stacks",
    search: "Search stacks",
    state: "loading",
    nothing: "No declared firewall rules to derive dependencies from yet.",
    columns: [
      { label: "Stack", sort: "text" },
      { label: "Depends on", sort: "text" },
      { label: "Depended on by", sort: "text" },
    ],
  });
  const stale = tableBlock({
    remember: "stale-images",
    caption: "Stale images",
    search: "Search images",
    state: "loading",
    nothing: "Every pinned image matches its upstream's latest release.",
    columns: [
      { label: "Where", sort: "text" },
      { label: "Pinned", sort: "text" },
      { label: "Upstream", sort: "text" },
      { label: "Latest", sort: "text" },
      { label: "Released", sort: "text" },
    ],
  });
  const staleStatus = h("p", { class: "measured" });

  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Fleet view")),
    h(
      "p",
      { class: "page-intro" },
      "Derived from the stack files and the fleet's metrics — nothing here is typed by hand.",
    ),
    h("section", { class: "kp-card", "aria-label": "Topology" }, topoBox),
    h(
      "section",
      { class: "kp-card", "aria-label": "Capacity map" },
      capacity.wrap,
    ),
    h(
      "section",
      { class: "kp-card", "aria-label": "Disk growth" },
      growth.wrap,
    ),
    h("section", { class: "kp-card", "aria-label": "Dependencies" }, deps.wrap),
    h(
      "section",
      { class: "kp-card", "aria-label": "Stale images" },
      stale.wrap,
      staleStatus,
    ),
  );
  const detach = attachDataTables(root);
  const capacityTable = dataTable(capacity.wrap);
  const growthTable = dataTable(growth.wrap);
  const depsTable = dataTable(deps.wrap);
  const staleTable = dataTable(stale.wrap);
  const abort = new AbortController();

  void (async () => {
    const r = await fetchJson("/data/topology", "the topology", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      topoBox.replaceChildren(errorBox(r.error));
      return;
    }
    topoBox.replaceChildren(
      topologyFigure(r.body.topology, { caption: "Topology" }),
    );
  })().catch(() => {});

  void (async () => {
    const r = await fetchJson(
      "/data/capacity",
      "the capacity map",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      capacity.failed(r.error);
      return;
    }
    /** @type {Map<string, {cpu?: number, mem?: number, disk?: number}>} */
    const byStack = new Map();
    for (const p of r.body.panels ?? []) {
      const key = p.panel.title.startsWith("CPU")
        ? "cpu"
        : p.panel.title.startsWith("Memory")
          ? "mem"
          : "disk";
      for (const s of p.series ?? []) {
        if (!s.points?.length) continue;
        const row = byStack.get(s.label) ?? {};
        row[key] = s.points[0][1];
        byStack.set(s.label, row);
      }
    }
    const rows = [...byStack.entries()].map(([stack, v]) =>
      h(
        "tr",
        { "data-kp-row-key": stack },
        td(stack),
        td(v.cpu != null ? formatValue(v.cpu, "percent") : "—", "num"),
        td(v.mem != null ? formatValue(v.mem, "percent") : "—", "num"),
        td(v.disk != null ? formatValue(v.disk, "percent") : "—", "num"),
      ),
    );
    capacity.tbody.replaceChildren(...rows);
    capacity.ready();
  })().catch(() => {});

  void (async () => {
    const r = await fetchJson("/data/disk-growth", "disk growth", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      growth.failed(r.error);
      return;
    }
    const rows = (r.body.rows ?? []).map((/** @type {any} */ row) => {
      const days = row.fit.days_to_full;
      return h(
        "tr",
        { "data-kp-row-key": `${row.scope}-${row.subject}` },
        td(row.scope === "host" ? "Host" : "Stack"),
        td(row.subject || "—"),
        td(formatValue(row.fit.pct_now, "percent"), "num"),
        td(
          `${row.fit.pct_per_day_robust >= 0 ? "+" : ""}${row.fit.pct_per_day_robust.toFixed(2)}%/day`,
          "num",
        ),
        td(days != null ? `${days.toFixed(1)} d` : "not growing", "num"),
        badgeCell({
          label: row.warning ? "warning" : "ok",
          tone: row.warning ? "bad" : "ok",
        }),
      );
    });
    growth.tbody.replaceChildren(...rows);
    if (rows.length) growth.ready();
    else
      growth.loading({
        words: "No growing filesystem has enough history yet.",
        overlay: false,
      });
  })().catch(() => {});

  void (async () => {
    const r = await fetchJson(
      "/data/dependencies",
      "dependencies",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      deps.failed(r.error);
      return;
    }
    const rows = (r.body.dependencies ?? []).map((/** @type {any} */ d) =>
      h(
        "tr",
        { "data-kp-row-key": d.stack },
        td(d.stack),
        td(d.depends_on.join(", ") || "—"),
        td(d.depended_on_by.join(", ") || "—"),
      ),
    );
    deps.tbody.replaceChildren(...rows);
    deps.ready();
  })().catch(() => {});

  /**
   * One `/data/stale-images` answer, painted — the dashboard's kept last
   * answer (fix-179: it used to go blank on every visit while a fresh run
   * was still on its way) or the finished new one, the same rows either way.
   * @param {any} body
   */
  const paintStale = (body) => {
    const rows = (body.images ?? []).map((/** @type {any} */ x) =>
      h(
        "tr",
        { "data-kp-row-key": x.where_ },
        td(x.where_),
        td(x.pinned),
        td(x.upstream),
        td(x.latest),
        td(x.released ?? "—"),
      ),
    );
    stale.tbody.replaceChildren(...rows);
    if (body.measured_at)
      staleStatus.textContent = `measured ${formatDateTime(body.measured_at)} (reuses the fleet check's own run)`;
  };

  void (async () => {
    const r = await slowRead(
      "/data/stale-images",
      "stale images",
      abort.signal,
      (last) => {
        paintStale(last);
        stale.ready({ refresh: false });
      },
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      stale.failed(r.error);
      return;
    }
    paintStale(r.body);
    stale.ready();
  })().catch(() => {});

  return () => {
    abort.abort();
    detach();
    void capacityTable;
    void growthTable;
    void depsTable;
    void staleTable;
  };
}
