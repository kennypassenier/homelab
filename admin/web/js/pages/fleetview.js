// Fleet view (visuals milestone): the fleet-wide graphs that need no single
// stack open — feat-overview-7 (topology), feat-overview-11 (capacity map),
// feat-overview-12 (disk-growth prediction), feat-stacks-9 (dependencies)
// and feat-stacks-10 (stale images). Each section reads its own route and
// fails on its own, so a Prometheus outage only empties the two sections
// that need it.
//
// [fix-215] (Kenny, Dutch: "wat is het verschil met topology en topology
// with measured traffic?"): there used to be a second, near-identical
// topology graph on the Firewall page, drawn from the same shape but with
// a traffic overlay always on. One graph that looked almost the same as
// this one, under a different heading, is exactly what he could not tell
// apart. Now there is ONE topology, here, with a "Show measured traffic"
// toggle; the Firewall page links to it (with the toggle on) instead of
// drawing its own.
//
// [fix-230] (Kenny, Dutch: "zet eens deftige titels per chunk zodat ik
// direct weet welke tabel hoort te tonen"): every block opens with the
// shared `sectionHeader` — its name and one sentence saying what it shows —
// and the tables' own tiny captions, which only repeated that name under
// it, are kept for screen readers alone (`captionHidden`).
//
// [fix-231] the stale images get an Update per row (pinupdate.js), and
// [fix-232] each row's upstream is an absolute link to the newer version's
// release page (staleimages.js `releaseUrl`), opening in its own tab.

import { formatValue } from "../charts.js";
import {
  badgeCell,
  errorBox,
  fetchJson,
  h,
  sectionHeader,
  slowRead,
  tableBlock,
  td,
} from "../dom.js";
import { formatDateTime } from "../format.js";
import { openPinUpdate } from "../pinupdate.js";
import { majorJump, releaseUrl } from "../staleimages.js";
import { topologyFigure } from "../topology.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";
import { attachSwitches } from "/static/kp/js/forms.js";
import { declare, declareField, drivable } from "../drivable.js";

// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const FLEET_TRAFFIC = declareField({
  id: "fleetview-traffic",
  page: "fleetview",
  what: "show the measured traffic on the map",
});

// fix-239: Live view reaches every stale image's Update (`homelab ui click
// pin-update <stack>/<app>/<service>`).
const PIN_UPDATE = declare({
  id: "pin-update",
  page: "fleetview",
  opens: "dialog",
  row: "<stack>/<app>/<service>",
  what: "a stale image's Update: back up the stack, move its pinned image to the newer release, commit and deploy",
});

/**
 * A kp switch (shared shape with notifications.js's own, kept local here
 * since neither page exports it yet): the checkbox plus its On/Off words.
 * @param {string} id
 * @param {string} label
 */
function switchEl(id, label) {
  const input = h("input", {
    class: "kp-switch__input",
    type: "checkbox",
    role: "switch",
    id,
  });
  const wrap = h(
    "label",
    { class: "kp-switch" },
    input,
    h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
    h("span", null, label),
  );
  return { wrap, input };
}

/**
 * fix-231: one stale row's Update — what it moves to, a MAJOR jump marked
 * on the button itself (the dialog then asks for the release notes first).
 * @param {any} x the `/data/stale-images` row
 * @param {string} stack
 * @param {string} container
 * @param {boolean} major
 */
function updateButton(x, stack, container, major) {
  const btn = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--sm",
      "data-pin-update": "",
      "data-from": x.pinned,
      "data-to": x.latest,
      ...(major ? { "data-major": "" } : {}),
      title: `Back up ${stack}, move ${x.key} from ${x.pinned} to ${x.latest}, commit and deploy`,
    },
    `Update to ${x.latest}`,
  );
  drivable(btn, PIN_UPDATE, `${stack}/${x.key}`);
  btn.addEventListener("click", () => {
    void openPinUpdate({
      stack,
      container,
      key: x.key,
      pinned: x.pinned,
      latest: x.latest,
      upstream: x.upstream,
    });
  });
  return btn;
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const params = new URLSearchParams(location.search);
  const trafficWanted = params.get("traffic") === "1";
  const topoBox = h(
    "div",
    { class: "fleetview__topology" },
    h("p", null, "Loading…"),
  );
  const trafficSwitch = switchEl(FLEET_TRAFFIC, "Show measured traffic");
  trafficSwitch.input.checked = trafficWanted;
  const capacity = tableBlock({
    remember: "capacity-map",
    captionHidden: true,
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
    captionHidden: true,
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
    captionHidden: true,
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
    captionHidden: true,
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
      { label: "Action", sort: "none" },
    ],
  });
  const staleStatus = h("p", { class: "measured" });

  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Map")),
    h(
      "p",
      { class: "page-intro" },
      "Derived from the stack files and the fleet's metrics — nothing here is typed by hand.",
    ),
    h(
      "section",
      { class: "kp-card fleetview__block", "aria-label": "Topology" },
      sectionHeader(
        "Topology",
        "Which container talks to which, from the stack files and the firewall state the host enforces now.",
      ),
      h(
        "p",
        { class: "measured" },
        'Turn on "Show measured traffic" to size each stack\'s ring by its own total network throughput (bytes/s from Prometheus, received + transmitted) — that ring is per stack, not per connection: the fleet has no per-neighbour flow metric.',
      ),
      trafficSwitch.wrap,
      topoBox,
    ),
    h(
      "section",
      { class: "kp-card fleetview__block", "aria-label": "Capacity map" },
      sectionHeader(
        "Capacity map",
        "CPU, memory and disk per stack, measured now from the fleet's metrics.",
      ),
      capacity.wrap,
    ),
    h(
      "section",
      { class: "kp-card fleetview__block", "aria-label": "Disk growth" },
      sectionHeader(
        "Disk growth",
        "Which filesystems fill up, and when each would be full at its current trend.",
      ),
      growth.wrap,
    ),
    h(
      "section",
      { class: "kp-card fleetview__block", "aria-label": "Dependencies" },
      sectionHeader(
        "Dependencies between stacks",
        "Which stack needs which, derived from the firewall rules each stack declares.",
      ),
      deps.wrap,
    ),
    h(
      "section",
      { class: "kp-card fleetview__block", "aria-label": "Stale images" },
      sectionHeader(
        "Stale images",
        "Pinned images whose upstream has a newer release; Update backs the stack up, moves the pin and deploys it.",
      ),
      stale.wrap,
      staleStatus,
    ),
  );
  const detachSwitches = attachSwitches(root);
  const detach = attachDataTables(root);
  const capacityTable = dataTable(capacity.wrap);
  const growthTable = dataTable(growth.wrap);
  const depsTable = dataTable(deps.wrap);
  const staleTable = dataTable(stale.wrap);
  const abort = new AbortController();

  /** @type {any} */
  let topo = null;
  /** @type {Map<string, number> | undefined} */
  let traffic;
  let trafficLoaded = false;

  const paintTopology = () => {
    if (!topo) return;
    topoBox.replaceChildren(
      topologyFigure(topo, {
        traffic: trafficSwitch.input.checked ? traffic : undefined,
        caption: "Topology",
      }),
    );
  };

  const ensureTraffic = async () => {
    if (trafficLoaded) return;
    trafficLoaded = true;
    const r = await fetchJson(
      "/data/fleet-traffic",
      "the measured traffic",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) return;
    traffic = new Map();
    for (const p of r.body.panels ?? []) {
      for (const s of p.series ?? []) {
        if (!s.points?.length) continue;
        traffic.set(s.label, (traffic.get(s.label) ?? 0) + s.points[0][1]);
      }
    }
    paintTopology();
  };

  trafficSwitch.input.addEventListener("change", () => {
    const url = new URL(location.href);
    if (trafficSwitch.input.checked) {
      url.searchParams.set("traffic", "1");
      void ensureTraffic();
    } else {
      url.searchParams.delete("traffic");
    }
    history.replaceState(null, "", url);
    paintTopology();
  });

  void (async () => {
    const r = await fetchJson("/data/topology", "the topology", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      topoBox.replaceChildren(errorBox(r.error));
      return;
    }
    topo = r.body.topology;
    paintTopology();
    if (trafficSwitch.input.checked) void ensureTraffic();
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
    // fix-230: no rows is an answer, not a load still on its way — kp's
    // empty box says "No growing filesystem has enough history yet."
    // (spec.nothing) instead of a spinner counting seconds for ever.
    growth.ready();
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
    const rows = (body.images ?? []).map((/** @type {any} */ x) => {
      const major = majorJump(x.pinned, x.latest);
      const notes = releaseUrl(x.upstream, x.latest);
      const [stack, container] = String(x.where_).split("/");
      return h(
        "tr",
        { "data-kp-row-key": x.where_ },
        td(x.where_),
        td(x.pinned),
        h(
          "td",
          null,
          notes
            ? h(
                "a",
                {
                  href: notes,
                  target: "_blank",
                  rel: "noopener noreferrer",
                  title: `The ${x.latest} release notes, in a new tab`,
                },
                x.upstream,
              )
            : x.upstream,
        ),
        h(
          "td",
          { class: "stale__latest" },
          x.latest,
          ...(major
            ? [
                " ",
                h(
                  "span",
                  { class: "state warn", title: "The first number changes" },
                  h("span", null, "major"),
                ),
              ]
            : []),
        ),
        td(x.released ?? "—"),
        x.key
          ? h(
              "td",
              { class: "stale__action" },
              updateButton(x, stack, container, major),
            )
          : h(
              "td",
              { class: "measured stale__action" },
              "updated with a homelab release, not from here",
            ),
      );
    });
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
    detachSwitches();
    void capacityTable;
    void growthTable;
    void depsTable;
    void staleTable;
  };
}
