// The Map (redesign-371-map, release 3.71.0; the Fleet view renamed —
// Kenny approved the demo 2026-10-03: ~/.local/share/homelab/redesign-3.71/
// fleetview.html + fleetview.js, implemented exactly). How the stacks
// connect, what they use and what is out of date, all derived from the
// stack files and the fleet's metrics, nothing typed by hand.
//
// Top to bottom: the header (freshness, "Show measured traffic"), four
// KPI tiles, the ONE topology (fix-215, invariant 24: the Firewall page
// links here) as a graph with a side panel or as a list, Capacity and
// Disk growth side by side, and Stale images with an Update per row
// (fix-231/232, invariant 34). Each block reads its own route and fails
// on its own, so a Prometheus outage only empties the two blocks that
// need it.
//
// Data: /data/topology, /data/fleet-traffic (only once the switch is on),
// /data/capacity, /data/disk-growth, /data/stale-images, the fleet store.

import { errorBox, fetchJson, h, slowRead } from "../dom.js";
import { declare, declareField, drivable } from "../drivable.js";
import { formatDateTime } from "../format.js";
import {
  EDGE_KINDS,
  capacityRows,
  firewallOf,
  hubOf,
  hues,
  isStack,
  mapFigures,
  meterTone,
  neighbours,
  shortDate,
  sortCapacity,
  sortGrowth,
  staleGroups,
  trafficByStack,
} from "../mapview.js";
import { openPinUpdate } from "../pinupdate.js";
import { stackHref } from "../router.js";
import { releaseUrl } from "../staleimages.js";
import { current, subscribe } from "../store.js";
import { bytes } from "../charts.js";
import {
  dataTable,
  dot,
  emptyState,
  ensureStyle,
  kpiStrip,
  meter,
  pageHeader,
  section,
  segSwitch,
  skeletonBlock,
  skeletonLines,
  tableSlot,
} from "../ui.js";
import { attachSwitches } from "/static/kp/js/forms.js";
import { mapGraph } from "./mapgraph.js";

// fix-239: Live view reaches every stale image's Update (`homelab ui click
// pin-update <stack>/<app>/<service>`) and every other control here.
const PIN_UPDATE = declare({
  id: "pin-update",
  page: "fleetview",
  // redesign-integrate-8: since redesign-flows-1 it opens the one Update
  // flow (an address), not a dialog; Live view waited for a dialog.
  opens: "view",
  row: "<stack>/<app>/<service>",
  what: "a stale image's Update: open the Update flow with that app picked (back up, move the pinned image, commit, deploy, verify)",
});
const VIEW = declare({
  id: "map-view",
  page: "fleetview",
  opens: "view",
  row: "graph|list",
  what: "show the topology as a graph or as a list",
});
const KIND = declare({
  id: "map-edge-kind",
  page: "fleetview",
  opens: "view",
  row: "declared|planned|open|named|route",
  what: "show or hide one kind of connection on the topology",
});
// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const FLEET_TRAFFIC = declareField({
  id: "fleetview-traffic",
  page: "fleetview",
  what: "show the measured traffic on the map",
});
const TRAFFIC = declare({
  id: "map-traffic",
  page: "fleetview",
  opens: "view",
  what: "size each stack's ring by its measured network traffic, or stop",
});
const TILE = declare({
  id: "map-kpi",
  page: "fleetview",
  opens: "view",
  row: "<tile>",
  what: "a KPI tile: scroll to the block it sums up",
});
const OPEN_STACK = declare({
  id: "map-open-stack",
  page: "fleetview",
  opens: "view",
  row: "<stack>",
  what: "open a stack's own page from the Map (side panel, list or capacity)",
});
const FW_RULES = declare({
  id: "map-firewall-rules",
  page: "fleetview",
  opens: "view",
  row: "<stack>",
  what: "open a stack's firewall rules from the Map's side panel",
  shows: "while a node is selected",
  reach: [{ do: "click", control: "map-node", row: "*" }],
});
const NOTES = declare({
  id: "map-release-notes",
  page: "fleetview",
  opens: "view",
  row: "<image>",
  what: "a stale image's release notes, in a new tab",
});
const CAP_SORT = declare({
  id: "map-capacity-sort",
  page: "fleetview",
  opens: "view",
  row: "cpu|memory|disk",
  what: "sort the Capacity card busiest first by CPU, memory or disk",
});

/**
 * @typedef {import("../topology.js").Topology} Topology
 * @typedef {import("../topology.js").TopoNode} TopoNode
 */

/**
 * fix-231: one stale row's Update — what it moves to, a MAJOR jump marked
 * on the button itself (the dialog then asks for the release notes first).
 * @param {import("../mapview.js").StaleGroup} g
 */
function updateButton(g) {
  const stack = g.stacks[0];
  const btn = h(
    "button",
    {
      type: "button",
      class: `kp-button kp-button--sm${g.major ? "" : " kp-button--primary"}`,
      "data-pin-update": "",
      "data-from": g.pinned,
      "data-to": g.latest,
      ...(g.major ? { "data-major": "" } : {}),
      title: `Back up ${stack}, move ${g.key} from ${g.pinned} to ${g.latest}, commit and deploy, in the Update flow${g.major ? "; a major version: the flow asks you to read the release notes first" : ""}`,
    },
    `Update to ${g.latest}${g.major ? "…" : ""}`,
  );
  drivable(btn, PIN_UPDATE, `${stack}/${g.key}`);
  btn.addEventListener("click", () => {
    void openPinUpdate({
      stack,
      container: g.container,
      key: /** @type {string} */ (g.key),
      pinned: g.pinned,
      latest: g.latest,
      upstream: g.upstream,
    });
  });
  return btn;
}

/**
 * A node's identity square in its topology colour (initials, or ↗ for an
 * address outside the fleet).
 * @param {TopoNode} n
 * @param {Map<string, number>} hue
 */
function mark(n, hue) {
  const m = h(
    "span",
    {
      class: `mp-mark${isStack(n) ? "" : " mp-mark--ext"}`,
      "aria-hidden": "true",
    },
    isStack(n) ? n.stack.slice(0, 2).toUpperCase() : "↗",
  );
  if (isStack(n))
    m.style.setProperty("--stack-hue", String(hue.get(n.stack) ?? 0));
  return m;
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/metrics.css");
  ensureStyle("/css/pages/map.css");
  root.classList.add("mk-page", "mp-page");
  const params = new URLSearchParams(location.search);
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];

  // ── header ────────────────────────────────────────────────────────────
  const input = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-switch__input",
      type: "checkbox",
      role: "switch",
      id: FLEET_TRAFFIC,
    })
  );
  input.checked = params.get("traffic") === "1";
  drivable(input, TRAFFIC);
  const trafficSwitch = h(
    "label",
    {
      class: "kp-switch mp-switch",
      title:
        "Size each stack's ring by its measured network traffic (bytes/s from Prometheus, received + transmitted): per stack, not per connection",
    },
    input,
    h("span", { class: "kp-switch__state", "aria-hidden": "true" }),
    h("span", null, "Show measured traffic"),
  );
  const head = pageHeader({
    title: "Map",
    desc: "How the stacks connect, what they use and what is out of date, all derived from the stack files and the fleet's metrics, nothing typed by hand.",
    live: "read",
    actions: [trafficSwitch],
  });

  // ── KPI tiles ─────────────────────────────────────────────────────────
  const strip = kpiStrip(
    [
      { key: "stacks", label: "Stacks in the graph", target: "topo" },
      { key: "connections", label: "Connections", target: "topo" },
      { key: "firewall", label: "Firewall", target: "topo" },
      { key: "stale", label: "Stale images", target: "stale" },
    ],
    { loading: true, label: "The fleet at a glance", drive: { id: TILE } },
  );

  // ── Topology ──────────────────────────────────────────────────────────
  const viewSeg = segSwitch({
    label: "View",
    items: [
      { value: "graph", label: "Graph", hint: "The topology as a picture" },
      {
        value: "list",
        label: "List",
        hint: "The topology as a table: one row per stack",
      },
    ],
    value: params.get("view") === "list" ? "list" : "graph",
    onChange: (v) => showView(v),
    drive: { id: VIEW },
  });
  const topo = section({
    id: "topo",
    title: "Topology",
    desc: "Which container talks to which, from the stack files and the firewall state the host enforces now.",
    tools: [viewSeg.el],
  });
  topo.el.setAttribute("aria-label", "Topology");
  topo.body.replaceChildren(skeletonBlock("30rem", "Reading the topology"));

  // ── Capacity and Disk growth ──────────────────────────────────────────
  /** @type {"cpu" | "mem" | "disk"} */
  let capBy = "cpu";
  /** @type {import("../mapview.js").CapRow[] | null} */
  let capRows = null;
  const capSeg = segSwitch({
    label: "Sort by",
    items: [
      { value: "cpu", label: "CPU", hint: "Busiest CPU first" },
      { value: "memory", label: "Memory", hint: "Fullest memory first" },
      { value: "disk", label: "Disk", hint: "Fullest disk first" },
    ],
    value: "cpu",
    onChange: (v) => {
      capBy = v === "memory" ? "mem" : v === "disk" ? "disk" : "cpu";
      paintCapacity();
    },
    drive: { id: CAP_SORT },
  });
  const cap = section({
    id: "capacity",
    title: "Capacity",
    desc: "CPU, memory and disk per stack against its own allowance, busiest first.",
    tools: [capSeg.el],
    foot: ["Prometheus · now", "amber from 75% · red from 90%"],
  });
  cap.el.setAttribute("aria-label", "Capacity");
  cap.el.classList.add("mp-half");
  cap.body.replaceChildren(skeletonLines(5, "Reading the capacity"));
  const growth = section({
    id: "growth",
    title: "Disk growth",
    desc: "Which filesystems fill up, and when each would be full at its current trend.",
  });
  const growthFoot = h("span", null, "warns before a disk is full");
  growth.foot.replaceChildren(
    h("span", null, "Prometheus · 7-day robust trend"),
    growthFoot,
  );
  growth.foot.hidden = false;
  growth.el.setAttribute("aria-label", "Disk growth");
  growth.el.classList.add("mp-half");
  growth.body.replaceChildren(skeletonLines(5, "Reading the disk trends"));

  // ── Stale images ──────────────────────────────────────────────────────
  const staleWhen = h("span", { class: "mk-muted mk-sm" }, "");
  const stale = section({
    id: "stale",
    title: "Stale images",
    desc: "Pinned images whose upstream has a newer release. Update backs the stack up, moves the pin and deploys it.",
    tools: [staleWhen],
  });
  stale.el.setAttribute("aria-label", "Stale images");
  stale.body.replaceChildren(skeletonLines(3, "Reading the stale images"));

  root.replaceChildren(
    head.el,
    strip.el,
    topo.el,
    h("div", { class: "mp-grid" }, cap.el, growth.el),
    stale.el,
  );
  stops.push(attachSwitches(root));

  /** @type {Topology | null} */
  let topology = null;
  /** @type {any} */
  let staleBody = null;
  /** @type {ReturnType<typeof mapGraph> | null} */
  let graph = null;
  /** @type {Map<string, number> | null} */
  let traffic = null;
  const side = h("aside", { class: "mp-side", "aria-live": "polite" });
  const listBox = h("div", { class: "mp-list", hidden: "" });
  const topoGrid = h("div", { class: "mp-topo" });
  const listTable = tableSlot();
  const staleTable = tableSlot();
  stops.push(listTable.stop, staleTable.stop);

  const paintTiles = () => {
    if (!topology) return;
    const f = mapFigures(topology, staleBody);
    const T = strip.tiles;
    T.get("stacks")?.set({
      value: String(f.stacks),
      ctx: f.outside
        ? `+ ${f.outside} ${f.outside === 1 ? "address" : "addresses"} outside the fleet`
        : "no address outside the fleet",
    });
    // The demo's colours: what the firewall enforces in green, what is
    // open in amber.
    T.get("connections")?.set({
      value: String(f.connections),
      ctxParts: [
        { text: `${f.enforced} enforced`, tone: "ok" },
        ...(f.planned ? [{ text: `${f.planned} not on yet` }] : []),
        { text: `${f.open} open`, tone: f.open ? "warn" : "" },
      ],
    });
    const off = [
      ...f.firewallNone.map((s) => `${s} none`),
      ...f.firewallDiffers.map((s) => `${s} differs`),
    ];
    T.get("firewall")?.set({
      value: String(f.firewallOn),
      unit: `of ${f.stacks} on`,
      ctx: off.length ? off.join(" · ") : "every stack's firewall is on",
      tone: off.length ? "warn" : "",
    });
    T.get("stale")?.set(
      f.stale == null
        ? { value: "…", ctx: "the fleet check is reading" }
        : {
            value: String(f.stale),
            ctx: f.stale
              ? `${f.major} major ${f.major === 1 ? "jump" : "jumps"} · ${f.updatable} you can update`
              : "every pin is current",
            tone: f.stale ? "warn" : "",
          },
    );
  };

  /** @param {string} v */
  function showView(v) {
    const list = v === "list";
    topoGrid.hidden = list;
    listBox.hidden = !list;
    const url = new URL(location.href);
    if (list) url.searchParams.set("view", "list");
    else url.searchParams.delete("view");
    history.replaceState(history.state, "", url);
    if (!list) graph?.draw();
  }

  /** @param {string[]} sel @param {string | null} hover */
  const paintSide = (sel, hover) => {
    if (!topology) return;
    const t = topology;
    const hue = hues(t);
    const ids = sel.length ? sel : hover ? [hover] : [];
    if (!ids.length) {
      const hub = hubOf(t);
      const f = mapFigures(t, null);
      side.replaceChildren(
        h("h3", null, "Nothing selected"),
        h(
          "p",
          { class: "mk-muted mk-sm mp-side__p" },
          "Hover a stack in the graph to see its connections; click one to add it to the selection (click again to remove it); select several to compare them side by side.",
        ),
        h(
          "dl",
          { class: "mp-kv" },
          ...(hub
            ? [
                h("dt", null, "Hub"),
                h(
                  "dd",
                  null,
                  `${hub} (${neighbours(t, hub).edges.length} connections)`,
                ),
              ]
            : []),
          h("dt", null, "No firewall"),
          h(
            "dd",
            null,
            ...(f.firewallNone.length
              ? f.firewallNone.map((s) =>
                  h("span", { class: "mp-pill" }, dot("bad"), s),
                )
              : ["none"]),
          ),
          h("dt", null, "Repository differs"),
          h(
            "dd",
            null,
            ...(f.firewallDiffers.length
              ? f.firewallDiffers.map((s) =>
                  h("span", { class: "mp-pill" }, dot("warn"), s),
                )
              : ["none"]),
          ),
        ),
      );
      return;
    }
    const fleet = current().fleet;
    side.replaceChildren(
      h(
        "p",
        { class: "mp-side__label" },
        sel.length
          ? `${sel.length} selected · Esc or Show all clears`
          : "hovering",
      ),
      ...ids.flatMap((id, i) => {
        const n = t.nodes.find((x) => x.stack === id);
        if (!n) return [];
        const nb = neighbours(t, id);
        const out = [];
        if (i) out.push(h("hr", { class: "mp-side__hr" }));
        if (!isStack(n)) {
          out.push(
            h("h3", null, mark(n, hue), n.stack),
            h(
              "p",
              { class: "mk-muted mk-sm mp-side__p" },
              "An address outside the fleet.",
            ),
            h(
              "dl",
              { class: "mp-kv" },
              h("dt", null, "Reached by"),
              h("dd", null, nb.dependedOnBy.join(", ") || "—"),
            ),
          );
          return out;
        }
        const fw = firewallOf(n);
        const st = fleet?.stacks.find((s) => s.name === n.stack);
        const running = st ? st.online !== false : null;
        out.push(
          h(
            "h3",
            null,
            mark(n, hue),
            n.stack,
            running == null
              ? ""
              : h(
                  "span",
                  { class: "mp-state" },
                  dot(running ? "ok" : "bad"),
                  running ? "running" : "offline",
                ),
          ),
          h(
            "dl",
            { class: "mp-kv" },
            h("dt", null, "vmid"),
            h("dd", { class: "mono" }, String(n.vmid)),
            h("dt", null, "Address"),
            h("dd", { class: "mono" }, n.ip),
            h("dt", null, "Firewall"),
            h(
              "dd",
              null,
              h("span", { class: "mp-pill" }, dot(fw.tone), fw.text),
            ),
            h("dt", null, "Depends on"),
            h("dd", null, nb.dependsOn.join(", ") || "—"),
            h("dt", null, "Depended on by"),
            h("dd", null, nb.dependedOnBy.join(", ") || "—"),
            ...(traffic
              ? [
                  h("dt", null, "Traffic"),
                  h("dd", null, `${bytes(traffic.get(n.stack) ?? 0)}/s`),
                ]
              : []),
          ),
          h(
            "ul",
            { class: "mp-feed" },
            ...nb.edges.map((e) =>
              h(
                "li",
                null,
                h("span", {
                  class: `mp-feed__kind mp-feed__kind--${e.kind}`,
                  "aria-hidden": "true",
                }),
                h(
                  "span",
                  null,
                  `${e.from} → ${e.to}`,
                  h("small", null, e.detail.join(", ") || e.kind),
                ),
              ),
            ),
          ),
          h(
            "div",
            { class: "mp-side__acts" },
            drivable(
              h(
                "a",
                {
                  class: "kp-button kp-button--sm",
                  href: stackHref(n.stack),
                  title: `Open ${n.stack}'s own page`,
                },
                "Open stack",
              ),
              OPEN_STACK,
              `side:${n.stack}`,
            ),
            drivable(
              h(
                "a",
                {
                  class: "kp-button kp-button--sm",
                  href: `${stackHref(n.stack, "settings")}?section=firewall`,
                  title: `${n.stack}'s firewall rules, in its settings`,
                },
                "Firewall rules",
              ),
              FW_RULES,
              n.stack,
            ),
          ),
        );
        return out;
      }),
    );
  };

  /** @param {string[]} sel @param {string | null} hover */
  const onGraph = (sel, hover) => {
    const url = new URL(location.href);
    if (sel.length) url.searchParams.set("select", sel.join(","));
    else url.searchParams.delete("select");
    if (url.search !== location.search)
      history.replaceState(history.state, "", url);
    paintSide(sel, hover);
  };

  const paintTopology = () => {
    if (!topology) return;
    const t = topology;
    if (!t.nodes.length) {
      topo.body.replaceChildren(
        emptyState({
          title: "No stacks to draw yet",
          text: "The topology is read from the working copy's stack files; once a stack has a working copy, it appears here.",
        }),
      );
      return;
    }
    graph?.stop();
    const want = (params.get("select") ?? "")
      .split(",")
      .filter((s) => t.nodes.some((n) => n.stack === s));
    graph = mapGraph(t, {
      traffic: input.checked ? traffic : null,
      selected: want,
      onChange: onGraph,
    });
    const kinds = EDGE_KINDS.filter((k) =>
      t.edges.some((e) => e.kind === k.kind),
    );
    const legend = h("div", {
      class: "mp-kinds",
      role: "group",
      "aria-label": "Connection kinds: click to hide or show",
    });
    for (const k of kinds) {
      const b = h(
        "button",
        {
          type: "button",
          class: "mp-kind",
          "data-kind": k.kind,
          "aria-pressed": "true",
          title: `${k.hint}; click to hide or show these lines`,
        },
        h("i", {
          class: `mp-kind__line mp-kind__line--${k.kind}`,
          "aria-hidden": "true",
        }),
        k.label,
      );
      drivable(b, KIND, k.kind);
      b.addEventListener("click", () => {
        const hide = b.getAttribute("aria-pressed") === "true";
        b.setAttribute("aria-pressed", String(!hide));
        graph?.hideKind(k.kind, hide);
      });
      legend.append(b);
    }
    graph.onKindsReset(() => {
      for (const b of legend.querySelectorAll("button"))
        b.setAttribute("aria-pressed", "true");
    });
    topoGrid.replaceChildren(graph.el, side);
    // The list view: one row per stack.
    const hue = hues(t);
    const tbl = dataTable({
      cls: "mk-table",
      remember: "map-topology-list",
      caption: "Topology as a list",
      columns: [
        { label: "Stack", sort: "text" },
        { label: "Address", sort: "text" },
        { label: "Firewall", sort: "text" },
        { label: "Depends on", sort: "text" },
        { label: "Depended on by", sort: "text" },
      ],
    });
    tbl.tbody.replaceChildren(
      ...t.nodes.filter(isStack).map((n) => {
        const nb = neighbours(t, n.stack);
        const fw = firewallOf(n);
        return h(
          "tr",
          { "data-kp-row-key": n.stack },
          h(
            "td",
            null,
            h(
              "span",
              { class: "mp-name" },
              mark(n, hue),
              drivable(
                h("a", { href: stackHref(n.stack) }, n.stack),
                OPEN_STACK,
                `list:${n.stack}`,
              ),
            ),
          ),
          h("td", { class: "mono" }, n.ip),
          h("td", null, h("span", { class: "mp-pill" }, dot(fw.tone), fw.text)),
          h("td", null, nb.dependsOn.join(", ") || "—"),
          h("td", null, nb.dependedOnBy.join(", ") || "—"),
        );
      }),
    );
    listBox.replaceChildren(tbl.wrap);
    topo.body.replaceChildren(
      h("div", { class: "mp-legends" }, legend),
      topoGrid,
      listBox,
    );
    listTable.attach(listBox);
    showView(viewSeg.value());
    graph.draw();
    onGraph(want, null);
  };

  const ensureTraffic = async () => {
    if (traffic) return;
    const r = await fetchJson(
      "/data/fleet-traffic",
      "the measured traffic",
      abort.signal,
    );
    if (abort.signal.aborted || !r.ok) return;
    traffic = trafficByStack(r.body.panels ?? []);
    graph?.setTraffic(input.checked ? traffic : null);
  };
  input.addEventListener("change", () => {
    const url = new URL(location.href);
    if (input.checked) url.searchParams.set("traffic", "1");
    else url.searchParams.delete("traffic");
    history.replaceState(history.state, "", url);
    if (input.checked) void ensureTraffic().catch(() => {});
    else graph?.setTraffic(null);
  });

  void (async () => {
    const r = await fetchJson("/data/topology", "the topology", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      topo.body.replaceChildren(errorBox(r.error));
      return;
    }
    topology = r.body.topology;
    head.live?.set(r.body.head?.at ?? Math.floor(Date.now() / 1000));
    paintTopology();
    paintTiles();
    if (input.checked) void ensureTraffic();
  })().catch(() => {});
  // A stack coming online or going down repaints the side panel.
  stops.push(
    subscribe(() => {
      if (graph) paintSide(graph.selected(), null);
    }),
  );

  // ── Capacity ──────────────────────────────────────────────────────────
  function paintCapacity() {
    if (!capRows) return;
    if (!capRows.length) {
      cap.body.replaceChildren(
        emptyState({
          title: "No stack has metrics yet",
          text: "Each stack's node-exporter reports CPU, memory and disk to Prometheus; a stack appears here at its first reading.",
        }),
      );
      return;
    }
    /** @param {number | null} v */
    const cell = (v) =>
      h(
        "span",
        { class: "mp-cap__cell" },
        v == null
          ? h("span", { class: "mk-muted" }, "—")
          : meter({ pct: v, tone: meterTone(v) }).el,
        h(
          "span",
          { class: "mp-cap__n" },
          v == null ? "not read" : `${Math.round(v)}%`,
        ),
      );
    cap.body.replaceChildren(
      h(
        "div",
        { class: "mp-caps", role: "table", "aria-label": "Capacity per stack" },
        h(
          "div",
          { class: "mp-cap mp-cap--head", role: "row" },
          ...["Stack", "CPU", "Memory", "Disk"].map((x) =>
            h("span", { role: "columnheader" }, x),
          ),
        ),
        ...sortCapacity(capRows, capBy).map((r) =>
          h(
            "div",
            { class: "mp-cap", role: "row", "data-stack": r.stack },
            h(
              "span",
              { role: "cell" },
              drivable(
                h("a", { href: stackHref(r.stack) }, r.stack),
                OPEN_STACK,
                `capacity:${r.stack}`,
              ),
            ),
            cell(r.cpu),
            cell(r.mem),
            cell(r.disk),
          ),
        ),
      ),
    );
  }
  void (async () => {
    const r = await fetchJson("/data/capacity", "the capacity", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      cap.body.replaceChildren(errorBox(r.error));
      return;
    }
    capRows = capacityRows(r.body.panels ?? []);
    paintCapacity();
  })().catch(() => {});

  // ── Disk growth ───────────────────────────────────────────────────────
  void (async () => {
    const r = await fetchJson("/data/disk-growth", "disk growth", abort.signal);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      growth.body.replaceChildren(errorBox(r.error));
      return;
    }
    const within = r.body.within_days;
    if (within != null)
      growthFoot.textContent = `warns ${within} days before full`;
    const rows = r.body.rows ?? [];
    if (!rows.length) {
      growth.body.replaceChildren(
        emptyState({
          art: growthArt(),
          title: "No filesystem has enough history to predict yet",
          text: "A trend needs 7 days of disk readings; until then the Capacity card shows how full each disk is now.",
        }),
      );
      return;
    }
    growth.body.replaceChildren(
      h(
        "div",
        {
          class: "mp-caps",
          role: "table",
          "aria-label": "Disk growth per filesystem",
        },
        h(
          "div",
          { class: "mp-growth mp-cap--head", role: "row" },
          ...["Where", "Now", "Trend", "Full in"].map((x) =>
            h("span", { role: "columnheader" }, x),
          ),
        ),
        // One fixed order whatever order the read brings (finding 4).
        ...sortGrowth(rows).map((/** @type {any} */ row) => {
          const days = row.fit.days_to_full;
          const now = row.fit.pct_now;
          // On a phone the head row is hidden: two lines per row (name and
          // "Full in", then the bar and the trend), each value with its
          // own inline label.
          const inl = (/** @type {string} */ t) =>
            h("span", { class: "mp-inl" }, `${t} `);
          return h(
            "div",
            { class: "mp-growth", role: "row" },
            h(
              "span",
              { role: "cell", class: "mp-g-name" },
              row.scope === "host" ? "Host" : "Stack",
              " ",
              h("b", { class: "mono" }, row.subject || "/"),
            ),
            h(
              "span",
              { class: "mp-cap__cell mp-g-bar", role: "cell" },
              meter({ pct: now, tone: meterTone(now) }).el,
              h("span", { class: "mp-cap__n" }, `${Math.round(now)}%`),
            ),
            h(
              "span",
              { role: "cell", class: "mp-cap__n mp-g-trend" },
              inl("Trend"),
              `${row.fit.pct_per_day_robust >= 0 ? "+" : "−"}${Math.abs(row.fit.pct_per_day_robust).toFixed(2)}%/day`,
            ),
            h(
              "span",
              {
                role: "cell",
                class: `mp-g-full${row.warning ? " mk-bad" : ""}`,
              },
              inl("Full in"),
              days == null
                ? "not growing"
                : days > 365
                  ? "more than a year"
                  : `${days.toFixed(1)} days`,
            ),
          );
        }),
      ),
    );
  })().catch(() => {});

  // ── Stale images ──────────────────────────────────────────────────────
  /** @param {any} body */
  const paintStale = (body) => {
    staleBody = body;
    paintTiles();
    if (body.measured_at)
      staleWhen.textContent = `from the fleet check · ${formatDateTime(body.measured_at)}`;
    const groups = staleGroups(body.images ?? []);
    if (!groups.length) {
      stale.body.replaceChildren(
        emptyState({
          title: "Every pinned image is current",
          text: "Every pinned image matches its upstream's latest release; the fleet check looks again on its own round.",
        }),
      );
      return;
    }
    const t = dataTable({
      cls: "mk-table",
      remember: "stale-images",
      caption: "Stale images",
      columns: [
        { label: "Image", sort: "text" },
        { label: "Stack", sort: "text" },
        { label: "Pinned → latest", sort: "text" },
        { label: "Released", sort: "text" },
        { label: "Upstream", sort: "none" },
        { label: "Action", cls: "mk-n" },
      ],
    });
    // On a phone each row is a small card (the demo's nx-table--cards):
    // the image first, then each cell with its column's name, the Update
    // button on its own line, never behind a sideways scroll.
    t.wrap.querySelector("table")?.classList.add("mk-cards");
    t.tbody.replaceChildren(
      ...groups.map((g) => {
        const notes = releaseUrl(g.upstream, g.latest);
        return h(
          "tr",
          { "data-kp-row-key": g.where.join(",") },
          h("td", { class: "id" }, h("b", null, g.image)),
          h("td", { "data-label": "Stack" }, g.stacks.join(", ")),
          h(
            "td",
            { "data-label": "Pinned → latest" },
            h(
              "span",
              { class: "mp-pin" },
              h("span", { class: "mono" }, g.pinned),
              " → ",
              h("span", { class: "mono" }, g.latest),
              h(
                "span",
                {
                  class: `mp-tag${g.major ? " mp-tag--warn" : ""}`,
                  title: g.major
                    ? "The first number changes: read the release notes first"
                    : "A minor or patch step",
                },
                g.major ? "major" : "minor",
              ),
            ),
          ),
          // The sort reads the cell's text: a hidden ISO date first makes
          // "28 Sep" sort before "1 Oct" (finding 13).
          h(
            "td",
            { "data-label": "Released" },
            h("span", { class: "mk-sortkey", hidden: "" }, g.released ?? "~"),
            shortDate(g.released),
          ),
          h(
            "td",
            { "data-label": "Upstream" },
            notes
              ? drivable(
                  h(
                    "a",
                    {
                      href: notes,
                      target: "_blank",
                      rel: "noopener noreferrer",
                      title: `The ${g.latest} release notes (${g.upstream}), in a new tab`,
                    },
                    "release notes ↗",
                  ),
                  NOTES,
                  g.where.join(","),
                )
              : h("span", { class: "mono" }, g.upstream),
          ),
          g.key
            ? h("td", { class: "mk-n row-actions" }, updateButton(g))
            : h(
                "td",
                {
                  class: "mk-n mk-muted mk-sm row-actions",
                  title:
                    "This pin lives in the homelab binary, not a stack file: it moves with a homelab release, not from here",
                },
                "with a homelab release",
              ),
        );
      }),
    );
    stale.body.replaceChildren(t.wrap);
    staleTable.attach(stale.body);
  };
  void (async () => {
    const r = await slowRead(
      "/data/stale-images",
      "stale images",
      abort.signal,
      (last) => paintStale(last),
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      stale.body.replaceChildren(errorBox(r.error));
      return;
    }
    paintStale(r.body);
  })().catch(() => {});

  return () => {
    abort.abort();
    graph?.stop();
    for (const s of stops) s();
    root.classList.remove("mk-page", "mp-page");
  };
}

/** The empty Disk growth card's small drawing: a line that will rise. */
function growthArt() {
  const NS = "http://www.w3.org/2000/svg";
  const s = document.createElementNS(NS, "svg");
  s.setAttribute("class", "mp-illus");
  s.setAttribute("width", "96");
  s.setAttribute("height", "56");
  s.setAttribute("viewBox", "0 0 96 56");
  s.setAttribute("aria-hidden", "true");
  s.innerHTML =
    '<path d="M4 50h88" stroke="currentColor" stroke-width="1.5" opacity=".5"/><path d="M8 44 C 24 43 30 41 40 40 S60 38 70 37 84 36 90 35" fill="none" stroke="var(--chart-1)" stroke-width="2"/><path d="M70 37 L90 22" stroke="var(--chart-1)" stroke-width="2" stroke-dasharray="3 3" opacity=".6"/><circle cx="90" cy="22" r="3" fill="var(--warning-foreground)"/>';
  return s;
}
